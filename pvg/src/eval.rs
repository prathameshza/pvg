use crate::ast::*;
use crate::draw_list::*;
use crate::error::PvgError;
use std::collections::HashMap;

/// Runtime dynamically typed value representation.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    String(String),
    Bool(bool),
    Color(Color),
    Vec2(f64, f64),
    None,
}

impl Value {
    pub fn as_f64(&self) -> Result<f64, PvgError> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            _ => Err(PvgError::runtime(format!("Expected number, got {:?}", self))),
        }
    }

    pub fn as_vec2(&self) -> Result<(f64, f64), PvgError> {
        match self {
            Value::Vec2(x, y) => Ok((*x, *y)),
            _ => Err(PvgError::runtime(format!("Expected [x, y] vector, got {:?}", self))),
        }
    }

    pub fn as_color(&self) -> Result<Color, PvgError> {
        match self {
            Value::Color(c) => Ok(c.clone()),
            _ => Err(PvgError::runtime(format!("Expected color, got {:?}", self))),
        }
    }

    pub fn as_string(&self) -> Result<String, PvgError> {
        match self {
            Value::String(s) => Ok(s.clone()),
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    Ok(format!("{}", *n as i64))
                } else {
                    Ok(format!("{}", n))
                }
            }
            Value::Bool(b) => Ok(format!("{}", b)),
            _ => Err(PvgError::runtime(format!("Expected string or displayable value, got {:?}", self))),
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::None => false,
            _ => true,
        }
    }
}

/// The procedural evaluator and runtime environment for PVG documents.
pub struct Evaluator {
    globals: HashMap<String, Value>,
    functions: HashMap<String, FunctionDef>,
    rng_state: u64,
    loop_limit: usize,
    loop_count: usize,
    draw_list: Vec<DrawCmd>,
    transform_stack: Vec<Transform2D>,
    style_stack: Vec<DrawStyle>,
}

impl Evaluator {
    /// Creates a new evaluator initialized at timeline clock `time = 0.0`.
    pub fn new() -> Self {
        Self::new_with_time(0.0)
    }

    /// Creates a new evaluator initialized with a specific timeline clock value in seconds.
    pub fn new_with_time(time: f64) -> Self {
        let mut globals = HashMap::new();
        globals.insert("PI".into(), Value::Number(std::f64::consts::PI));
        globals.insert("TAU".into(), Value::Number(std::f64::consts::TAU));
        globals.insert("time".into(), Value::Number(time));
        globals.insert("t".into(), Value::Number(time));

        Self {
            globals,
            functions: HashMap::new(),
            rng_state: 88172645463325252,
            loop_limit: 100_000,
            loop_count: 0,
            draw_list: Vec::new(),
            transform_stack: vec![Transform2D::identity()],
            style_stack: vec![DrawStyle::default()],
        }
    }

    /// Sets the maximum allowable loop iterations across the entire evaluation to prevent DoS hangs.
    pub fn with_loop_limit(mut self, limit: usize) -> Self {
        self.loop_limit = limit;
        self
    }

    /// Sets the initial 64-bit seed for deterministic Xorshift pseudorandom generation.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.rng_state = if seed == 0 { 88172645463325252 } else { seed };
        self
    }

    /// Injects or overrides a global variable in the execution scope.
    pub fn with_global(mut self, name: impl Into<String>, value: Value) -> Self {
        self.globals.insert(name.into(), value);
        self
    }

    fn current_transform(&self) -> Transform2D {
        *self.transform_stack.last().unwrap()
    }

    fn current_style(&self) -> DrawStyle {
        self.style_stack.last().unwrap().clone()
    }

    fn next_random(&mut self) -> f64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        (self.rng_state as f64) / (u64::MAX as f64)
    }

    /// Evaluates a parsed `Document` AST into a flat 2D `DrawList`.
    pub fn evaluate_document(mut self, doc: &Document) -> Result<DrawList, PvgError> {
        let mut locals = HashMap::new();
        for stmt in &doc.statements {
            self.eval_stmt(stmt, &mut locals)?;
        }

        Ok(DrawList {
            canvas_width: doc.canvas.width,
            canvas_height: doc.canvas.height,
            background: doc.canvas.background.clone(),
            items: self.draw_list,
        })
    }

    fn eval_stmt(&mut self, stmt: &Stmt, locals: &mut HashMap<String, Value>) -> Result<Option<Value>, PvgError> {
        match stmt {
            Stmt::Set(name, expr) => {
                let val = self.eval_expr(expr, locals)?;
                if locals.contains_key(name) {
                    locals.insert(name.clone(), val);
                } else {
                    self.globals.insert(name.clone(), val);
                }
                Ok(None)
            }
            Stmt::Seed(s) => {
                self.rng_state = if *s == 0 { 88172645463325252 } else { *s };
                Ok(None)
            }
            Stmt::Def(func) => {
                self.functions.insert(func.name.clone(), func.clone());
                Ok(None)
            }
            Stmt::Return(expr) => {
                let val = self.eval_expr(expr, locals)?;
                Ok(Some(val))
            }
            Stmt::For { var, from, to, step, body } => {
                let start_val = self.eval_expr(from, locals)?.as_f64()?;
                let end_val = self.eval_expr(to, locals)?.as_f64()?;
                let step_val = if let Some(s) = step {
                    self.eval_expr(s, locals)?.as_f64()?
                } else if end_val >= start_val {
                    1.0
                } else {
                    -1.0
                };

                if step_val == 0.0 {
                    return Err(PvgError::runtime("For loop step cannot be 0"));
                }

                let mut current = start_val;
                while (step_val > 0.0 && current <= end_val) || (step_val < 0.0 && current >= end_val) {
                    self.loop_count += 1;
                    if self.loop_count > self.loop_limit {
                        return Err(PvgError::safety_limit(format!(
                            "Exceeded loop safety limit of {} iterations",
                            self.loop_limit
                        )));
                    }
                    locals.insert(var.clone(), Value::Number(current));
                    for b_stmt in body {
                        if let Some(ret) = self.eval_stmt(b_stmt, locals)? {
                            return Ok(Some(ret));
                        }
                    }
                    current += step_val;
                }
                Ok(None)
            }
            Stmt::While { cond, body } => {
                while self.eval_expr(cond, locals)?.is_truthy() {
                    self.loop_count += 1;
                    if self.loop_count > self.loop_limit {
                        return Err(PvgError::safety_limit(format!(
                            "Exceeded loop safety limit of {} iterations",
                            self.loop_limit
                        )));
                    }
                    for b_stmt in body {
                        if let Some(ret) = self.eval_stmt(b_stmt, locals)? {
                            return Ok(Some(ret));
                        }
                    }
                }
                Ok(None)
            }
            Stmt::If { cond, then_body, else_body } => {
                if self.eval_expr(cond, locals)?.is_truthy() {
                    for b_stmt in then_body {
                        if let Some(ret) = self.eval_stmt(b_stmt, locals)? {
                            return Ok(Some(ret));
                        }
                    }
                } else {
                    for b_stmt in else_body {
                        if let Some(ret) = self.eval_stmt(b_stmt, locals)? {
                            return Ok(Some(ret));
                        }
                    }
                }
                Ok(None)
            }
            Stmt::Call(name, args) => {
                let mut evaluated_args = Vec::new();
                for a in args {
                    evaluated_args.push(self.eval_expr(a, locals)?);
                }
                self.invoke_function(name, evaluated_args)?;
                Ok(None)
            }
            Stmt::Circle(c) => {
                let center_raw = self.eval_expr(&c.center, locals)?.as_vec2()?;
                let radius = self.eval_expr(&c.radius, locals)?.as_f64()?;
                let mut style = self.current_style();
                if let Some(ref f) = c.fill { style.fill = self.eval_expr(f, locals)?.as_color()?; }
                if let Some(ref s) = c.stroke { style.stroke = self.eval_expr(s, locals)?.as_color()?; }
                if let Some(ref w) = c.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = c.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }

                let trans = self.current_transform();
                let center = trans.transform_point(center_raw);
                self.draw_list.push(DrawCmd::Circle { center, radius, style });
                Ok(None)
            }
            Stmt::Ellipse(e) => {
                let center_raw = self.eval_expr(&e.center, locals)?.as_vec2()?;
                let radius_raw = self.eval_expr(&e.radius, locals)?.as_vec2()?;
                let mut style = self.current_style();
                if let Some(ref f) = e.fill { style.fill = self.eval_expr(f, locals)?.as_color()?; }
                if let Some(ref s) = e.stroke { style.stroke = self.eval_expr(s, locals)?.as_color()?; }
                if let Some(ref w) = e.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = e.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }

                let trans = self.current_transform();
                let center = trans.transform_point(center_raw);
                self.draw_list.push(DrawCmd::Ellipse { center, radius: radius_raw, style });
                Ok(None)
            }
            Stmt::Rectangle(r) => {
                let pos_raw = self.eval_expr(&r.pos, locals)?.as_vec2()?;
                let size_raw = self.eval_expr(&r.size, locals)?.as_vec2()?;
                let corner_radius = if let Some(ref cr) = r.radius { self.eval_expr(cr, locals)?.as_f64()? } else { 0.0 };
                let mut style = self.current_style();
                if let Some(ref f) = r.fill { style.fill = self.eval_expr(f, locals)?.as_color()?; }
                if let Some(ref s) = r.stroke { style.stroke = self.eval_expr(s, locals)?.as_color()?; }
                if let Some(ref w) = r.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = r.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }

                let trans = self.current_transform();
                let pos = trans.transform_point(pos_raw);
                self.draw_list.push(DrawCmd::Rectangle { pos, size: size_raw, corner_radius, style });
                Ok(None)
            }
            Stmt::Line(l) => {
                let from_raw = self.eval_expr(&l.from, locals)?.as_vec2()?;
                let to_raw = self.eval_expr(&l.to, locals)?.as_vec2()?;
                let mut style = self.current_style();
                if let Some(ref s) = l.stroke { style.stroke = self.eval_expr(s, locals)?.as_color()?; }
                if let Some(ref w) = l.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = l.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }

                let trans = self.current_transform();
                let from = trans.transform_point(from_raw);
                let to = trans.transform_point(to_raw);
                self.draw_list.push(DrawCmd::Line { from, to, style });
                Ok(None)
            }
            Stmt::Polygon(p) => {
                let mut points = Vec::new();
                let trans = self.current_transform();
                for pt_expr in &p.points {
                    let pt_raw = self.eval_expr(pt_expr, locals)?.as_vec2()?;
                    points.push(trans.transform_point(pt_raw));
                }
                let mut style = self.current_style();
                if let Some(ref f) = p.fill { style.fill = self.eval_expr(f, locals)?.as_color()?; }
                if let Some(ref s) = p.stroke { style.stroke = self.eval_expr(s, locals)?.as_color()?; }
                if let Some(ref w) = p.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = p.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }

                self.draw_list.push(DrawCmd::Polygon { points, style });
                Ok(None)
            }
            Stmt::Path(p) => {
                let mut style = self.current_style();
                if let Some(ref f) = p.fill { style.fill = self.eval_expr(f, locals)?.as_color()?; }
                if let Some(ref s) = p.stroke { style.stroke = self.eval_expr(s, locals)?.as_color()?; }
                if let Some(ref w) = p.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = p.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }

                let trans = self.current_transform();
                let mut draw_commands = Vec::new();

                for cmd in &p.commands {
                    match cmd {
                        PathCommand::Set(name, expr) => {
                            let val = self.eval_expr(expr, locals)?;
                            locals.insert(name.clone(), val);
                        }
                        PathCommand::Start(e) => {
                            let pt = trans.transform_point(self.eval_expr(e, locals)?.as_vec2()?);
                            draw_commands.push(DrawPathCommand::Start(pt));
                        }
                        PathCommand::Line(e) => {
                            let pt = trans.transform_point(self.eval_expr(e, locals)?.as_vec2()?);
                            draw_commands.push(DrawPathCommand::Line(pt));
                        }
                        PathCommand::Quad(cp, ep) => {
                            let cp_pt = trans.transform_point(self.eval_expr(cp, locals)?.as_vec2()?);
                            let ep_pt = trans.transform_point(self.eval_expr(ep, locals)?.as_vec2()?);
                            draw_commands.push(DrawPathCommand::Quad { cp: cp_pt, ep: ep_pt });
                        }
                        PathCommand::Curve(c1, c2, ep) => {
                            let c1_pt = trans.transform_point(self.eval_expr(c1, locals)?.as_vec2()?);
                            let c2_pt = trans.transform_point(self.eval_expr(c2, locals)?.as_vec2()?);
                            let ep_pt = trans.transform_point(self.eval_expr(ep, locals)?.as_vec2()?);
                            draw_commands.push(DrawPathCommand::Curve { c1: c1_pt, c2: c2_pt, ep: ep_pt });
                        }
                        PathCommand::Arc { center, radius, start_angle, end_angle } => {
                            let c_pt = trans.transform_point(self.eval_expr(center, locals)?.as_vec2()?);
                            let r = self.eval_expr(radius, locals)?.as_f64()?;
                            let sa = self.eval_expr(start_angle, locals)?.as_f64()?;
                            let ea = self.eval_expr(end_angle, locals)?.as_f64()?;
                            draw_commands.push(DrawPathCommand::Arc { center: c_pt, radius: r, start_angle: sa, end_angle: ea });
                        }
                        PathCommand::Close => {
                            draw_commands.push(DrawPathCommand::Close);
                        }
                    }
                }

                self.draw_list.push(DrawCmd::Path { commands: draw_commands, style });
                Ok(None)
            }
            Stmt::Text(t) => {
                let pos_raw = self.eval_expr(&t.pos, locals)?.as_vec2()?;
                let content = self.eval_expr(&t.content, locals)?.as_string()?;
                let size = if let Some(ref s) = t.size {
                    self.eval_expr(s, locals)?.as_f64()?
                } else {
                    16.0
                };
                let font_family = if let Some(ref f) = t.font {
                    self.eval_expr(f, locals)?.as_string()?
                } else {
                    "sans-serif".to_string()
                };
                let align = if let Some(ref a) = t.align {
                    match self.eval_expr(a, locals)?.as_string()?.to_lowercase().as_str() {
                        "center" => TextAlign::Center,
                        "right" => TextAlign::Right,
                        _ => TextAlign::Left,
                    }
                } else {
                    TextAlign::Left
                };

                let mut style = self.current_style();
                if let Some(ref f) = t.fill { style.fill = self.eval_expr(f, locals)?.as_color()?; }
                if let Some(ref s) = t.stroke { style.stroke = self.eval_expr(s, locals)?.as_color()?; }
                if let Some(ref w) = t.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = t.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }

                let trans = self.current_transform();
                let pos = trans.transform_point(pos_raw);
                self.draw_list.push(DrawCmd::Text {
                    pos,
                    content,
                    size,
                    font_family,
                    align,
                    style,
                });
                Ok(None)
            }
            Stmt::Group(g) => {
                let mut local_trans = Transform2D::identity();
                if let Some(ref p) = g.pos {
                    let (tx, ty) = self.eval_expr(p, locals)?.as_vec2()?;
                    local_trans.tx = tx;
                    local_trans.ty = ty;
                }
                if let Some(ref r) = g.rot {
                    let angle = self.eval_expr(r, locals)?.as_f64()?;
                    let (sin_a, cos_a) = angle.sin_cos();
                    let rot_t = Transform2D { a: cos_a, b: sin_a, c: -sin_a, d: cos_a, tx: 0.0, ty: 0.0 };
                    local_trans = local_trans.mul(&rot_t);
                }
                if let Some(ref s) = g.scale {
                    let (sx, sy) = self.eval_expr(s, locals)?.as_vec2()?;
                    let scale_t = Transform2D { a: sx, b: 0.0, c: 0.0, d: sy, tx: 0.0, ty: 0.0 };
                    local_trans = local_trans.mul(&scale_t);
                }

                let new_trans = self.current_transform().mul(&local_trans);
                self.transform_stack.push(new_trans);

                let mut style = self.current_style();
                if let Some(ref f) = g.fill { style.fill = self.eval_expr(f, locals)?.as_color()?; }
                if let Some(ref s) = g.stroke { style.stroke = self.eval_expr(s, locals)?.as_color()?; }
                if let Some(ref o) = g.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }
                self.style_stack.push(style);

                for b_stmt in &g.body {
                    self.eval_stmt(b_stmt, locals)?;
                }

                self.style_stack.pop();
                self.transform_stack.pop();
                Ok(None)
            }
        }
    }

    fn invoke_function(&mut self, name: &str, args: Vec<Value>) -> Result<Option<Value>, PvgError> {
        let func = self.functions.get(name).cloned().ok_or_else(|| {
            PvgError::runtime(format!("Undefined function '{}'", name))
        })?;
        if func.params.len() != args.len() {
            return Err(PvgError::runtime(format!(
                "Function '{}' expects {} arguments, got {}",
                name,
                func.params.len(),
                args.len()
            )));
        }

        let mut locals = HashMap::new();
        for (param, val) in func.params.iter().zip(args) {
            locals.insert(param.clone(), val);
        }

        for stmt in &func.body {
            if let Some(ret) = self.eval_stmt(stmt, &mut locals)? {
                return Ok(Some(ret));
            }
        }

        Ok(None)
    }

    fn eval_expr(&mut self, expr: &Expr, locals: &HashMap<String, Value>) -> Result<Value, PvgError> {
        match expr {
            Expr::Number(n) => Ok(Value::Number(*n)),
            Expr::String(s) => Ok(Value::String(s.clone())),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Color(c) => Ok(Value::Color(c.clone())),
            Expr::Vec2(x, y) => {
                let xv = self.eval_expr(x, locals)?.as_f64()?;
                let yv = self.eval_expr(y, locals)?.as_f64()?;
                Ok(Value::Vec2(xv, yv))
            }
            Expr::Ident(name) => {
                if let Some(v) = locals.get(name) {
                    Ok(v.clone())
                } else if let Some(v) = self.globals.get(name) {
                    Ok(v.clone())
                } else {
                    Err(PvgError::runtime(format!("Undefined variable '{}'", name)))
                }
            }
            Expr::Unary(op, inner) => {
                let val = self.eval_expr(inner, locals)?;
                match op {
                    UnaryOp::Neg => Ok(Value::Number(-val.as_f64()?)),
                    UnaryOp::Not => Ok(Value::Bool(!val.is_truthy())),
                }
            }
            Expr::Binary(left, op, right) => {
                let l_val = self.eval_expr(left, locals)?;
                let r_val = self.eval_expr(right, locals)?;
                match op {
                    BinaryOp::Add => {
                        match (l_val, r_val) {
                            (Value::String(s1), Value::String(s2)) => Ok(Value::String(format!("{}{}", s1, s2))),
                            (Value::String(s1), Value::Number(n2)) => {
                                if n2.fract() == 0.0 && n2.abs() < 1e15 {
                                    Ok(Value::String(format!("{}{}", s1, n2 as i64)))
                                } else {
                                    Ok(Value::String(format!("{}{}", s1, n2)))
                                }
                            }
                            (Value::Number(n1), Value::String(s2)) => {
                                if n1.fract() == 0.0 && n1.abs() < 1e15 {
                                    Ok(Value::String(format!("{}{}", n1 as i64, s2)))
                                } else {
                                    Ok(Value::String(format!("{}{}", n1, s2)))
                                }
                            }
                            (Value::String(s1), Value::Bool(b2)) => Ok(Value::String(format!("{}{}", s1, b2))),
                            (l, r) => Ok(Value::Number(l.as_f64()? + r.as_f64()?)),
                        }
                    }
                    BinaryOp::Sub => Ok(Value::Number(l_val.as_f64()? - r_val.as_f64()?)),
                    BinaryOp::Mul => Ok(Value::Number(l_val.as_f64()? * r_val.as_f64()?)),
                    BinaryOp::Div => {
                        let denom = r_val.as_f64()?;
                        if denom == 0.0 {
                            Ok(Value::Number(0.0))
                        } else {
                            Ok(Value::Number(l_val.as_f64()? / denom))
                        }
                    }
                    BinaryOp::Mod => Ok(Value::Number(l_val.as_f64()? % r_val.as_f64()?)),
                    BinaryOp::Pow => Ok(Value::Number(l_val.as_f64()?.powf(r_val.as_f64()?))),
                    BinaryOp::Eq => Ok(Value::Bool(l_val.as_f64()? == r_val.as_f64()?)),
                    BinaryOp::Ne => Ok(Value::Bool(l_val.as_f64()? != r_val.as_f64()?)),
                    BinaryOp::Lt => Ok(Value::Bool(l_val.as_f64()? < r_val.as_f64()?)),
                    BinaryOp::Le => Ok(Value::Bool(l_val.as_f64()? <= r_val.as_f64()?)),
                    BinaryOp::Gt => Ok(Value::Bool(l_val.as_f64()? > r_val.as_f64()?)),
                    BinaryOp::Ge => Ok(Value::Bool(l_val.as_f64()? >= r_val.as_f64()?)),
                    BinaryOp::And => Ok(Value::Bool(l_val.is_truthy() && r_val.is_truthy())),
                    BinaryOp::Or => Ok(Value::Bool(l_val.is_truthy() || r_val.is_truthy())),
                }
            }
            Expr::Ternary(cond, t_expr, f_expr) => {
                if self.eval_expr(cond, locals)?.is_truthy() {
                    self.eval_expr(t_expr, locals)
                } else {
                    self.eval_expr(f_expr, locals)
                }
            }
            Expr::Call(name, args) => {
                let mut evaluated_args = Vec::new();
                for a in args {
                    evaluated_args.push(self.eval_expr(a, locals)?);
                }

                match name.as_str() {
                    "sin" => Ok(Value::Number(evaluated_args[0].as_f64()?.sin())),
                    "cos" => Ok(Value::Number(evaluated_args[0].as_f64()?.cos())),
                    "tan" => Ok(Value::Number(evaluated_args[0].as_f64()?.tan())),
                    "sqrt" => Ok(Value::Number(evaluated_args[0].as_f64()?.sqrt())),
                    "abs" => Ok(Value::Number(evaluated_args[0].as_f64()?.abs())),
                    "floor" => Ok(Value::Number(evaluated_args[0].as_f64()?.floor())),
                    "ceil" => Ok(Value::Number(evaluated_args[0].as_f64()?.ceil())),
                    "round" => Ok(Value::Number(evaluated_args[0].as_f64()?.round())),
                    "min" => Ok(Value::Number(evaluated_args[0].as_f64()?.min(evaluated_args[1].as_f64()?))),
                    "max" => Ok(Value::Number(evaluated_args[0].as_f64()?.max(evaluated_args[1].as_f64()?))),
                    "pow" => Ok(Value::Number(evaluated_args[0].as_f64()?.powf(evaluated_args[1].as_f64()?))),
                    "random" => {
                        let min = evaluated_args[0].as_f64()?;
                        let max = evaluated_args[1].as_f64()?;
                        let r = self.next_random();
                        Ok(Value::Number(min + r * (max - min)))
                    }
                    _ => {
                        if let Some(val) = self.invoke_function(name, evaluated_args)? {
                            Ok(val)
                        } else {
                            Ok(Value::None)
                        }
                    }
                }
            }
        }
    }
}