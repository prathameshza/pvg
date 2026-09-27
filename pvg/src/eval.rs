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
    /// 1D data array (charts, waves, spline control values).
    Array(Vec<Value>),
    Paint(Paint),
    None,
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Number(v)
    }
}
impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::Number(v as f64)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Number(v as f64)
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::Number(v as f64)
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}
impl From<Color> for Value {
    fn from(v: Color) -> Self {
        Value::Color(v)
    }
}
impl From<(f64, f64)> for Value {
    fn from(v: (f64, f64)) -> Self {
        Value::Vec2(v.0, v.1)
    }
}
impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::Array(v)
    }
}
impl From<Vec<f64>> for Value {
    fn from(v: Vec<f64>) -> Self {
        Value::Array(v.into_iter().map(Value::Number).collect())
    }
}

// ---------------------------------------------------------------------------
// Deterministic value noise (integer-hash based, no tables, no RNG state).
// Returns [-1, 1]. Pure function of coordinates => identical across platforms.
// ---------------------------------------------------------------------------

fn hash_lattice_2d(ix: i64, iy: i64) -> f64 {
    let mut h = (ix as u64).wrapping_mul(0x9E3779B97F4A7C15);
    h ^= (iy as u64).wrapping_mul(0xBF58476D1CE4E5B9);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58476D1CE4E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D049BB133111EB);
    h ^= h >> 31;
    (h as f64) / (u64::MAX as f64)
}

fn hash_lattice_3d(ix: i64, iy: i64, iz: i64) -> f64 {
    let mut h = (ix as u64).wrapping_mul(0x9E3779B97F4A7C15);
    h ^= (iy as u64).wrapping_mul(0xBF58476D1CE4E5B9);
    h ^= (iz as u64).wrapping_mul(0x94D049BB133111EB);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58476D1CE4E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D049BB133111EB);
    h ^= h >> 31;
    (h as f64) / (u64::MAX as f64)
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Deterministic 2D value noise in [-1, 1].
pub fn pvg_noise2(x: f64, y: f64) -> f64 {
    if !x.is_finite() || !y.is_finite() {
        return 0.0;
    }
    let x0 = x.floor() as i64;
    let y0 = y.floor() as i64;
    let fx = x - x0 as f64;
    let fy = y - y0 as f64;
    let a = hash_lattice_2d(x0, y0);
    let b = hash_lattice_2d(x0 + 1, y0);
    let c = hash_lattice_2d(x0, y0 + 1);
    let d = hash_lattice_2d(x0 + 1, y0 + 1);
    let ux = smooth(fx);
    let uy = smooth(fy);
    let v = a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy;
    v * 2.0 - 1.0
}

/// Deterministic 3D value noise in [-1, 1].
pub fn pvg_noise3(x: f64, y: f64, z: f64) -> f64 {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return 0.0;
    }
    let x0 = x.floor() as i64;
    let y0 = y.floor() as i64;
    let z0 = z.floor() as i64;
    let fx = x - x0 as f64;
    let fy = y - y0 as f64;
    let fz = z - z0 as f64;
    let ux = smooth(fx);
    let uy = smooth(fy);
    let uz = smooth(fz);
    let c000 = hash_lattice_3d(x0, y0, z0);
    let c100 = hash_lattice_3d(x0 + 1, y0, z0);
    let c010 = hash_lattice_3d(x0, y0 + 1, z0);
    let c110 = hash_lattice_3d(x0 + 1, y0 + 1, z0);
    let c001 = hash_lattice_3d(x0, y0, z0 + 1);
    let c101 = hash_lattice_3d(x0 + 1, y0, z0 + 1);
    let c011 = hash_lattice_3d(x0, y0 + 1, z0 + 1);
    let c111 = hash_lattice_3d(x0 + 1, y0 + 1, z0 + 1);
    let x00 = c000 + (c100 - c000) * ux;
    let x10 = c010 + (c110 - c010) * ux;
    let x01 = c001 + (c101 - c001) * ux;
    let x11 = c011 + (c111 - c011) * ux;
    let y0v = x00 + (x10 - x00) * uy;
    let y1v = x01 + (x11 - x01) * uy;
    let v = y0v + (y1v - y0v) * uz;
    v * 2.0 - 1.0
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
            Value::Paint(Paint::Color(c)) => Ok(c.clone()),
            _ => Err(PvgError::runtime(format!("Expected color, got {:?}", self))),
        }
    }

    pub fn as_paint(&self) -> Result<Paint, PvgError> {
        match self {
            Value::Color(c) => Ok(Paint::Color(c.clone())),
            Value::Paint(p) => Ok(p.clone()),
            _ => Err(PvgError::runtime(format!("Expected color or gradient paint, got {:?}", self))),
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
            Value::Array(items) => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|v| v.as_string().unwrap_or_else(|_| "?".into()))
                    .collect();
                Ok(format!("[{}]", parts.join(", ")))
            }
            _ => Err(PvgError::runtime(format!("Expected string or displayable value, got {:?}", self))),
        }
    }

    pub fn as_array(&self) -> Result<&Vec<Value>, PvgError> {
        match self {
            Value::Array(items) => Ok(items),
            _ => Err(PvgError::runtime(format!("Expected array, got {:?}", self))),
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Array(items) => !items.is_empty(),
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
    /// Pixel snap grid in px (0 = off). Set from `canvas snap`.
    snap: f64,
    /// Pattern names available for `fill pattern name` (set per document).
    pattern_names: Vec<String>,
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
        // Helpers so organic-shape examples read naturally:
        // `deg * deg_to_rad` == radians(deg).
        globals.insert("deg_to_rad".into(), Value::Number(std::f64::consts::PI / 180.0));
        globals.insert("rad_to_deg".into(), Value::Number(180.0 / std::f64::consts::PI));

        Self {
            globals,
            functions: HashMap::new(),
            rng_state: 88172645463325252,
            loop_limit: 100_000,
            loop_count: 0,
            draw_list: Vec::new(),
            transform_stack: vec![Transform2D::identity()],
            style_stack: vec![DrawStyle::default()],
            snap: 0.0,
            pattern_names: Vec::new(),
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

    /// Injects or overrides a host uniform (`param`) for this evaluation.
    /// Values set here win over the document's declared defaults.
    pub fn with_param(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        self.globals.insert(name.into(), value.into());
        self
    }

    /// Mutably sets a host uniform after construction (game-loop friendly).
    pub fn set_param(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.globals.insert(name.into(), value.into());
    }

    /// Sets the timeline clock without rebuilding the evaluator.
    pub fn set_time(&mut self, time: f64) {
        self.globals.insert("time".into(), Value::Number(time));
        self.globals.insert("t".into(), Value::Number(time));
    }

    /// Snap helper: rounds to the active pixel grid (0 = off).
    fn snap_val(&self, v: f64) -> f64 {
        if self.snap > 0.0 && v.is_finite() {
            (v / self.snap).round() * self.snap
        } else {
            v
        }
    }

    fn snap_point(&self, p: (f64, f64)) -> (f64, f64) {
        (self.snap_val(p.0), self.snap_val(p.1))
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

    /// Resolves stroke line-cap from a string expression (`butt`/`round`/`square`).
    fn eval_cap(&mut self, expr: &Expr, locals: &HashMap<String, Value>) -> Result<LineCap, PvgError> {
        match self.eval_expr(expr, locals)?.as_string()?.to_lowercase().as_str() {
            "round" => Ok(LineCap::Round),
            "square" => Ok(LineCap::Square),
            _ => Ok(LineCap::Butt),
        }
    }

    /// Resolves stroke line-join (`miter`/`round`/`bevel`).
    fn eval_join(&mut self, expr: &Expr, locals: &HashMap<String, Value>) -> Result<LineJoin, PvgError> {
        match self.eval_expr(expr, locals)?.as_string()?.to_lowercase().as_str() {
            "round" => Ok(LineJoin::Round),
            "bevel" => Ok(LineJoin::Bevel),
            _ => Ok(LineJoin::Miter),
        }
    }

    /// Resolves stroke align (`center`/`inside`/`outside`).
    fn eval_stroke_align(&mut self, expr: &Expr, locals: &HashMap<String, Value>) -> Result<StrokeAlign, PvgError> {
        match self.eval_expr(expr, locals)?.as_string()?.to_lowercase().as_str() {
            "inside" => Ok(StrokeAlign::Inside),
            "outside" => Ok(StrokeAlign::Outside),
            _ => Ok(StrokeAlign::Center),
        }
    }

    /// Resolves blend mode (`normal`/`add`/`multiply`/`screen`/`overlay`).
    fn eval_blend(&mut self, expr: &Expr, locals: &HashMap<String, Value>) -> Result<BlendMode, PvgError> {
        match self.eval_expr(expr, locals)?.as_string()?.to_lowercase().as_str() {
            "add" => Ok(BlendMode::Add),
            "multiply" => Ok(BlendMode::Multiply),
            "screen" => Ok(BlendMode::Screen),
            "overlay" => Ok(BlendMode::Overlay),
            _ => Ok(BlendMode::Normal),
        }
    }

    fn eval_dash(&mut self, dash: &[Expr], locals: &HashMap<String, Value>) -> Result<Vec<f64>, PvgError> {
        let mut out = Vec::with_capacity(dash.len());
        for d in dash {
            let v = self.eval_expr(d, locals)?.as_f64()?;
            if v > 0.0 && v.is_finite() {
                out.push(v);
            }
        }
        Ok(out)
    }

    fn eval_shadow(&mut self, s: &ShadowExpr, locals: &HashMap<String, Value>) -> Result<Shadow, PvgError> {
        let offset = self.eval_expr(&s.offset, locals)?.as_vec2()?;
        let radius = self.eval_expr(&s.radius, locals)?.as_f64()?.max(0.0);
        let color = self.eval_expr(&s.color, locals)?.as_color()?;
        Ok(Shadow { offset, radius, color })
    }

    fn eval_glow(&mut self, g: &GlowExpr, locals: &HashMap<String, Value>) -> Result<Glow, PvgError> {
        let radius = self.eval_expr(&g.radius, locals)?.as_f64()?.max(0.0);
        let color = self.eval_expr(&g.color, locals)?.as_color()?;
        Ok(Glow { radius, color })
    }

    /// Applies shared PVG 0.2 style properties (cap/join/miter/dash/align/blur/shadow/glow/blend).
    fn apply_fx_props(
        &mut self,
        style: &mut DrawStyle,
        cap: &Option<Expr>,
        join: &Option<Expr>,
        miter: &Option<Expr>,
        dash: &Option<Vec<Expr>>,
        align: &Option<Expr>,
        blur: &Option<Expr>,
        shadow: &Option<ShadowExpr>,
        glow: &Option<GlowExpr>,
        blend: &Option<Expr>,
        locals: &HashMap<String, Value>,
    ) -> Result<(), PvgError> {
        if let Some(c) = cap {
            style.cap = self.eval_cap(c, locals)?;
        }
        if let Some(j) = join {
            style.join = self.eval_join(j, locals)?;
        }
        if let Some(m) = miter {
            style.miter = self.eval_expr(m, locals)?.as_f64()?.max(1.0);
        }
        if let Some(d) = dash {
            style.dash = self.eval_dash(d, locals)?;
        }
        if let Some(a) = align {
            style.stroke_align = self.eval_stroke_align(a, locals)?;
        }
        if let Some(b) = blur {
            style.blur = self.eval_expr(b, locals)?.as_f64()?.max(0.0);
        }
        if let Some(s) = shadow {
            style.shadow = Some(self.eval_shadow(s, locals)?);
        }
        if let Some(g) = glow {
            style.glow = Some(self.eval_glow(g, locals)?);
        }
        if let Some(b) = blend {
            style.blend = self.eval_blend(b, locals)?;
        }
        Ok(())
    }

    /// Evaluates a single shape statement into one `DrawCmd` without pushing it.
    /// Used for `clip` masks so the mask geometry is resolved in isolation.
    fn eval_single_shape(&mut self, stmt: &Stmt, locals: &mut HashMap<String, Value>) -> Result<DrawCmd, PvgError> {
        let base = self.draw_list.len();
        // Evaluate with a scratch locals scope sharing globals; `set` side
        // effects inside mask shapes are discarded for determinism.
        let mut scratch = locals.clone();
        self.eval_stmt(stmt, &mut scratch)?;
        if self.draw_list.len() != base + 1 {
            self.draw_list.truncate(base);
            return Err(PvgError::runtime("Clip mask must produce exactly one shape."));
        }
        Ok(self.draw_list.pop().unwrap())
    }

    /// Evaluates a parsed `Document` AST into a flat 2D `DrawList`.
    pub fn evaluate_document(mut self, doc: &Document) -> Result<DrawList, PvgError> {
        self.snap = doc.canvas.snap;
        self.pattern_names = doc.patterns.iter().map(|p| p.name.clone()).collect();

        // 1. Host uniforms: declared defaults apply unless the host already
        //    overrode them via `with_param` / `set_param`.
        {
            let locals = HashMap::new();
            for param in &doc.params {
                if !self.globals.contains_key(&param.name) {
                    let v = self.eval_expr(&param.default, &locals)?;
                    self.globals.insert(param.name.clone(), v);
                }
            }
        }

        // 2. Pre-evaluate pattern tiles (identity transform, default style)
        //    so `fill pattern name` has resolved tile content per frame.
        let mut evaluated_patterns = Vec::with_capacity(doc.patterns.len());
        for pat in &doc.patterns {
            let saved_draw = std::mem::take(&mut self.draw_list);
            let saved_trans = self.transform_stack.clone();
            let saved_style = self.style_stack.clone();
            self.transform_stack = vec![Transform2D::identity()];
            self.style_stack = vec![DrawStyle::default()];
            let mut locals = HashMap::new();
            for stmt in &pat.body {
                self.eval_stmt(stmt, &mut locals)?;
            }
            let tiles = std::mem::take(&mut self.draw_list);
            self.draw_list = saved_draw;
            self.transform_stack = saved_trans;
            self.style_stack = saved_style;
            evaluated_patterns.push(crate::draw_list::DrawPattern {
                name: pat.name.clone(),
                width: pat.width,
                height: pat.height,
                tiles,
            });
        }

        // 3. Main scene body.
        let mut locals = HashMap::new();
        for stmt in &doc.statements {
            self.eval_stmt(stmt, &mut locals)?;
        }

        Ok(DrawList {
            canvas_width: doc.canvas.width,
            canvas_height: doc.canvas.height,
            background: doc.canvas.background.clone(),
            snap: doc.canvas.snap,
            pixel_filter: doc.canvas.pixel_filter,
            patterns: evaluated_patterns,
            items: self.draw_list,
        })
    }

    /// Evaluates one path-body item (draw command, `set`, or control flow),
    /// appending geometry to `draw_commands`. Control nodes share the path's
    /// `locals` scope, exactly like statement-level loops share function scope.
    fn eval_path_command(
        &mut self,
        cmd: &PathCommand,
        locals: &mut HashMap<String, Value>,
        trans: Transform2D,
        draw_commands: &mut Vec<DrawPathCommand>,
    ) -> Result<(), PvgError> {
        match cmd {
            PathCommand::Set(name, expr) => {
                let val = self.eval_expr(expr, locals)?;
                locals.insert(name.clone(), val);
            }
            PathCommand::Start(e) => {
                let raw = self.eval_expr(e, locals)?.as_vec2()?;
                draw_commands.push(DrawPathCommand::Start(self.snap_point(trans.transform_point(raw))));
            }
            PathCommand::Line(e) => {
                let raw = self.eval_expr(e, locals)?.as_vec2()?;
                draw_commands.push(DrawPathCommand::Line(self.snap_point(trans.transform_point(raw))));
            }
            PathCommand::Quad(cp, ep) => {
                let cp_raw = self.eval_expr(cp, locals)?.as_vec2()?;
                let ep_raw = self.eval_expr(ep, locals)?.as_vec2()?;
                draw_commands.push(DrawPathCommand::Quad {
                    cp: self.snap_point(trans.transform_point(cp_raw)),
                    ep: self.snap_point(trans.transform_point(ep_raw)),
                });
            }
            PathCommand::Curve(c1, c2, ep) => {
                let c1_raw = self.eval_expr(c1, locals)?.as_vec2()?;
                let c2_raw = self.eval_expr(c2, locals)?.as_vec2()?;
                let ep_raw = self.eval_expr(ep, locals)?.as_vec2()?;
                draw_commands.push(DrawPathCommand::Curve {
                    c1: self.snap_point(trans.transform_point(c1_raw)),
                    c2: self.snap_point(trans.transform_point(c2_raw)),
                    ep: self.snap_point(trans.transform_point(ep_raw)),
                });
            }
            PathCommand::Arc { center, radius, start_angle, end_angle } => {
                let c_raw = self.eval_expr(center, locals)?.as_vec2()?;
                let r = self.eval_expr(radius, locals)?.as_f64()?;
                let sa = self.eval_expr(start_angle, locals)?.as_f64()?;
                let ea = self.eval_expr(end_angle, locals)?.as_f64()?;
                draw_commands.push(DrawPathCommand::Arc {
                    center: self.snap_point(trans.transform_point(c_raw)),
                    radius: r,
                    start_angle: sa,
                    end_angle: ea,
                });
            }
            PathCommand::Close => {
                draw_commands.push(DrawPathCommand::Close);
            }
            PathCommand::For { var, from, to, step, body } => {
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
                    for b_cmd in body {
                        self.eval_path_command(b_cmd, locals, trans, draw_commands)?;
                    }
                    current += step_val;
                }
            }
            PathCommand::While { cond, body } => {
                while self.eval_expr(cond, locals)?.is_truthy() {
                    self.loop_count += 1;
                    if self.loop_count > self.loop_limit {
                        return Err(PvgError::safety_limit(format!(
                            "Exceeded loop safety limit of {} iterations",
                            self.loop_limit
                        )));
                    }
                    for b_cmd in body {
                        self.eval_path_command(b_cmd, locals, trans, draw_commands)?;
                    }
                }
            }
            PathCommand::If { cond, then_body, else_body } => {
                let branch = if self.eval_expr(cond, locals)?.is_truthy() {
                    then_body
                } else {
                    else_body
                };
                for b_cmd in branch {
                    self.eval_path_command(b_cmd, locals, trans, draw_commands)?;
                }
            }
        }
        Ok(())
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
                if let Some(ref f) = c.fill { style.fill = self.eval_expr(f, locals)?.as_paint()?; }
                if let Some(ref s) = c.stroke { style.stroke = self.eval_expr(s, locals)?.as_paint()?; }
                if let Some(ref w) = c.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = c.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }
                self.apply_fx_props(&mut style, &c.cap, &c.join, &c.miter, &c.dash, &c.align, &c.blur, &c.shadow, &c.glow, &c.blend, locals)?;

                let trans = self.current_transform();
                let center = self.snap_point(trans.transform_point(center_raw));
                self.draw_list.push(DrawCmd::Circle { center, radius, style });
                Ok(None)
            }
            Stmt::Ellipse(e) => {
                let center_raw = self.eval_expr(&e.center, locals)?.as_vec2()?;
                let radius_raw = self.eval_expr(&e.radius, locals)?.as_vec2()?;
                let mut style = self.current_style();
                if let Some(ref f) = e.fill { style.fill = self.eval_expr(f, locals)?.as_paint()?; }
                if let Some(ref s) = e.stroke { style.stroke = self.eval_expr(s, locals)?.as_paint()?; }
                if let Some(ref w) = e.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = e.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }
                self.apply_fx_props(&mut style, &e.cap, &e.join, &e.miter, &e.dash, &e.align, &e.blur, &e.shadow, &e.glow, &e.blend, locals)?;

                let trans = self.current_transform();
                let center = self.snap_point(trans.transform_point(center_raw));
                self.draw_list.push(DrawCmd::Ellipse { center, radius: radius_raw, style });
                Ok(None)
            }
            Stmt::Rectangle(r) => {
                let pos_raw = self.eval_expr(&r.pos, locals)?.as_vec2()?;
                let size_raw = self.eval_expr(&r.size, locals)?.as_vec2()?;
                let corner_radius = if let Some(ref cr) = r.radius { self.eval_expr(cr, locals)?.as_f64()? } else { 0.0 };
                let mut style = self.current_style();
                if let Some(ref f) = r.fill { style.fill = self.eval_expr(f, locals)?.as_paint()?; }
                if let Some(ref s) = r.stroke { style.stroke = self.eval_expr(s, locals)?.as_paint()?; }
                if let Some(ref w) = r.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = r.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }
                self.apply_fx_props(&mut style, &r.cap, &r.join, &r.miter, &r.dash, &r.align, &r.blur, &r.shadow, &r.glow, &r.blend, locals)?;

                let trans = self.current_transform();
                let pos = self.snap_point(trans.transform_point(pos_raw));
                self.draw_list.push(DrawCmd::Rectangle { pos, size: size_raw, corner_radius, style });
                Ok(None)
            }
            Stmt::Line(l) => {
                let from_raw = self.eval_expr(&l.from, locals)?.as_vec2()?;
                let to_raw = self.eval_expr(&l.to, locals)?.as_vec2()?;
                let mut style = self.current_style();
                if let Some(ref s) = l.stroke { style.stroke = self.eval_expr(s, locals)?.as_paint()?; }
                if let Some(ref w) = l.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = l.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }
                self.apply_fx_props(&mut style, &l.cap, &l.join, &l.miter, &l.dash, &l.align, &l.blur, &l.shadow, &l.glow, &l.blend, locals)?;

                let trans = self.current_transform();
                let from = self.snap_point(trans.transform_point(from_raw));
                let to = self.snap_point(trans.transform_point(to_raw));
                self.draw_list.push(DrawCmd::Line { from, to, style });
                Ok(None)
            }
            Stmt::Polygon(p) => {
                let mut points = Vec::new();
                let trans = self.current_transform();
                for pt_expr in &p.points {
                    let pt_raw = self.eval_expr(pt_expr, locals)?.as_vec2()?;
                    points.push(self.snap_point(trans.transform_point(pt_raw)));
                }
                let mut style = self.current_style();
                if let Some(ref f) = p.fill { style.fill = self.eval_expr(f, locals)?.as_paint()?; }
                if let Some(ref s) = p.stroke { style.stroke = self.eval_expr(s, locals)?.as_paint()?; }
                if let Some(ref w) = p.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = p.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }
                self.apply_fx_props(&mut style, &p.cap, &p.join, &p.miter, &p.dash, &p.align, &p.blur, &p.shadow, &p.glow, &p.blend, locals)?;

                self.draw_list.push(DrawCmd::Polygon { points, style });
                Ok(None)
            }
            Stmt::Path(p) => {
                let mut style = self.current_style();
                if let Some(ref f) = p.fill { style.fill = self.eval_expr(f, locals)?.as_paint()?; }
                if let Some(ref s) = p.stroke { style.stroke = self.eval_expr(s, locals)?.as_paint()?; }
                if let Some(ref w) = p.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = p.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }
                self.apply_fx_props(&mut style, &p.cap, &p.join, &p.miter, &p.dash, &p.align, &p.blur, &p.shadow, &p.glow, &p.blend, locals)?;

                let trans = self.current_transform();
                let mut draw_commands = Vec::new();

                for cmd in &p.commands {
                    self.eval_path_command(cmd, locals, trans, &mut draw_commands)?;
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
                if let Some(ref f) = t.fill { style.fill = self.eval_expr(f, locals)?.as_paint()?; }
                if let Some(ref s) = t.stroke { style.stroke = self.eval_expr(s, locals)?.as_paint()?; }
                if let Some(ref w) = t.width { style.width = self.eval_expr(w, locals)?.as_f64()?; }
                if let Some(ref o) = t.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }
                if let Some(ref b) = t.blur {
                    style.blur = self.eval_expr(b, locals)?.as_f64()?.max(0.0);
                }
                if let Some(ref s) = t.shadow {
                    style.shadow = Some(self.eval_shadow(s, locals)?);
                }
                if let Some(ref g) = t.glow {
                    style.glow = Some(self.eval_glow(g, locals)?);
                }
                if let Some(ref b) = t.blend {
                    style.blend = self.eval_blend(b, locals)?;
                }

                let trans = self.current_transform();
                let pos = self.snap_point(trans.transform_point(pos_raw));
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
                if let Some(ref f) = g.fill { style.fill = self.eval_expr(f, locals)?.as_paint()?; }
                if let Some(ref s) = g.stroke { style.stroke = self.eval_expr(s, locals)?.as_paint()?; }
                if let Some(ref o) = g.opacity { style.opacity *= self.eval_expr(o, locals)?.as_f64()?; }
                if let Some(ref b) = g.blend {
                    style.blend = self.eval_blend(b, locals)?;
                }
                if let Some(ref b) = g.blur {
                    style.blur = self.eval_expr(b, locals)?.as_f64()?.max(0.0);
                }
                if let Some(ref s) = g.shadow {
                    style.shadow = Some(self.eval_shadow(s, locals)?);
                }
                if let Some(ref gl) = g.glow {
                    style.glow = Some(self.eval_glow(gl, locals)?);
                }
                self.style_stack.push(style);

                for b_stmt in &g.body {
                    self.eval_stmt(b_stmt, locals)?;
                }

                self.style_stack.pop();
                self.transform_stack.pop();
                Ok(None)
            }
            Stmt::Sprite(s) => {
                let pos_raw = self.eval_expr(&s.pos, locals)?.as_vec2()?;
                let mut palette = Vec::with_capacity(s.palette.len());
                for entry in &s.palette {
                    palette.push(self.eval_expr(entry, locals)?.as_color()?);
                }
                let scale = if let Some(ref sc) = s.scale {
                    self.eval_expr(sc, locals)?.as_f64()?.max(0.01)
                } else {
                    1.0
                };
                let mut style = self.current_style();
                // Sprites are crisp pixel fills: no stroke, pixel opacity only.
                style.fill = Paint::Color(Color::WHITE);
                style.stroke = Paint::Color(Color::None);
                if let Some(ref o) = s.opacity {
                    style.opacity *= self.eval_expr(o, locals)?.as_f64()?;
                }
                if let Some(ref b) = s.blend {
                    style.blend = self.eval_blend(b, locals)?;
                }
                let trans = self.current_transform();
                let pos = self.snap_point(trans.transform_point(pos_raw));
                self.draw_list.push(DrawCmd::Sprite {
                    pos,
                    palette,
                    rows: s.rows.clone(),
                    scale,
                    style,
                });
                Ok(None)
            }
            Stmt::Spline(sp) => {
                let raw = self.eval_expr(&sp.points, locals)?;
                let items = raw.as_array()?.clone();
                // Resolve control points: Vec2 items used directly; Numbers
                // auto-layout across pos/size.
                let mut ctrl: Vec<(f64, f64)> = Vec::with_capacity(items.len());
                let has_vec = items.iter().any(|v| matches!(v, Value::Vec2(..)));
                if has_vec {
                    for v in &items {
                        match v {
                            Value::Vec2(x, y) => ctrl.push((*x, *y)),
                            Value::Number(n) => ctrl.push((ctrl.len() as f64, *n)),
                            _ => {
                                return Err(PvgError::runtime(
                                    "Spline points must be [x, y] vectors or numbers.",
                                ));
                            }
                        }
                    }
                } else {
                    let n = items.len();
                    if n == 0 {
                        return Err(PvgError::runtime("Spline requires at least one point."));
                    }
                    let mut nums = Vec::with_capacity(n);
                    for v in &items {
                        nums.push(v.as_f64()?);
                    }
                    let (ox, oy) = if let Some(ref p) = sp.pos {
                        self.eval_expr(p, locals)?.as_vec2()?
                    } else {
                        (0.0, 0.0)
                    };
                    let (sw, sh) = if let Some(ref sz) = sp.size {
                        self.eval_expr(sz, locals)?.as_vec2()?
                    } else {
                        ((n.max(2) - 1) as f64, 1.0)
                    };
                    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                    for v in &nums {
                        lo = lo.min(*v);
                        hi = hi.max(*v);
                    }
                    if !(hi > lo) {
                        hi = lo + 1.0;
                    }
                    for (i, v) in nums.iter().enumerate() {
                        let t = if n > 1 { i as f64 / (n - 1) as f64 } else { 0.0 };
                        // y down-positive: max value at top.
                        let y = oy + (1.0 - (v - lo) / (hi - lo)) * sh;
                        ctrl.push((ox + t * sw, y));
                    }
                }
                if ctrl.len() == 1 {
                    ctrl.push(ctrl[0]);
                }
                let mut style = self.current_style();
                if let Some(ref st) = sp.stroke {
                    style.stroke = self.eval_expr(st, locals)?.as_paint()?;
                } else {
                    style.stroke = Paint::Color(Color::WHITE);
                }
                style.fill = Paint::Color(Color::None);
                if let Some(ref w) = sp.width {
                    style.width = self.eval_expr(w, locals)?.as_f64()?;
                }
                if let Some(ref o) = sp.opacity {
                    style.opacity *= self.eval_expr(o, locals)?.as_f64()?;
                }
                self.apply_fx_props(
                    &mut style,
                    &sp.cap,
                    &sp.join,
                    &sp.miter,
                    &sp.dash,
                    &None,
                    &sp.blur,
                    &sp.shadow,
                    &sp.glow,
                    &sp.blend,
                    locals,
                )?;
                let trans = self.current_transform();
                let points: Vec<(f64, f64)> =
                    ctrl.into_iter().map(|p| self.snap_point(trans.transform_point(p))).collect();
                self.draw_list.push(DrawCmd::Spline { points, style });
                Ok(None)
            }
            Stmt::Clip { mask, content } => {
                let mask_cmd = self.eval_single_shape(mask, locals)?;
                let content_base = self.draw_list.len();
                for s in content {
                    self.eval_stmt(s, locals)?;
                }
                let content_cmds: Vec<DrawCmd> = self.draw_list.drain(content_base..).collect();
                self.draw_list.push(DrawCmd::Clip { mask: Box::new(mask_cmd), content: content_cmds });
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
            Expr::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    out.push(self.eval_expr(it, locals)?);
                }
                Ok(Value::Array(out))
            }
            Expr::Pattern(name) => {
                if self.pattern_names.iter().any(|n| n == name) {
                    Ok(Value::Paint(Paint::Pattern(name.clone())))
                } else {
                    Err(PvgError::runtime(format!("Unknown pattern '{}'", name)))
                }
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
                            (Value::String(s1), other) => {
                                Ok(Value::String(format!("{}{}", s1, other.as_string()?)))
                            }
                            (other, Value::String(s2)) => {
                                Ok(Value::String(format!("{}{}", other.as_string()?, s2)))
                            }
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
                    "radians" => Ok(Value::Number(evaluated_args[0].as_f64()?.to_radians())),
                    "degrees" => Ok(Value::Number(evaluated_args[0].as_f64()?.to_degrees())),
                    "deg_to_rad" => Ok(Value::Number(evaluated_args[0].as_f64()? * std::f64::consts::PI / 180.0)),
                    "noise2d" => {
                        if evaluated_args.len() != 2 {
                            return Err(PvgError::runtime("noise2d(x, y) needs 2 arguments"));
                        }
                        let x = evaluated_args[0].as_f64()?;
                        let y = evaluated_args[1].as_f64()?;
                        Ok(Value::Number(pvg_noise2(x, y)))
                    }
                    "noise3d" => {
                        if evaluated_args.len() != 3 {
                            return Err(PvgError::runtime("noise3d(x, y, z) needs 3 arguments"));
                        }
                        let x = evaluated_args[0].as_f64()?;
                        let y = evaluated_args[1].as_f64()?;
                        let z = evaluated_args[2].as_f64()?;
                        Ok(Value::Number(pvg_noise3(x, y, z)))
                    }
                    "array" => Ok(Value::Array(evaluated_args)),
                    "len" => {
                        if evaluated_args.is_empty() {
                            return Err(PvgError::runtime("len(arr) needs 1 argument"));
                        }
                        match &evaluated_args[0] {
                            Value::Array(items) => Ok(Value::Number(items.len() as f64)),
                            Value::String(s) => Ok(Value::Number(s.chars().count() as f64)),
                            _ => Err(PvgError::runtime("len() expects an array or string")),
                        }
                    }
                    "get" => {
                        if evaluated_args.len() != 2 {
                            return Err(PvgError::runtime("get(arr, i) needs 2 arguments"));
                        }
                        let idx = evaluated_args[1].as_f64()? as i64;
                        match &evaluated_args[0] {
                            Value::Array(items) => {
                                let n = items.len() as i64;
                                if n == 0 {
                                    return Err(PvgError::runtime("get() from empty array"));
                                }
                                // Negative indices wrap (Python-style).
                                let j = ((idx % n) + n) % n;
                                Ok(items[j as usize].clone())
                            }
                            _ => Err(PvgError::runtime("get() expects an array")),
                        }
                    }
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
            Expr::Linear { start, end, stops } => {
                let s = self.eval_expr(start, locals)?.as_vec2()?;
                let e = self.eval_expr(end, locals)?.as_vec2()?;
                // Apply current transform to gradient geometry so gradients
                // follow group transforms deterministically.
                let trans = self.current_transform();
                let mut eval_stops = Vec::with_capacity(stops.len());
                for st in stops {
                    let offset = self.eval_expr(&st.offset, locals)?.as_f64()?.clamp(0.0, 1.0);
                    let color = self.eval_expr(&st.color, locals)?.as_color()?;
                    eval_stops.push(crate::draw_list::GradientStop { offset, color });
                }
                Ok(Value::Paint(Paint::Linear {
                    start: trans.transform_point(s),
                    end: trans.transform_point(e),
                    stops: eval_stops,
                }))
            }
            Expr::Radial { center, radius, focal, stops } => {
                let c = self.eval_expr(center, locals)?.as_vec2()?;
                let r = self.eval_expr(radius, locals)?.as_f64()?.max(0.0);
                let trans = self.current_transform();
                let f = match focal {
                    Some(fx) => Some(trans.transform_point(self.eval_expr(fx, locals)?.as_vec2()?)),
                    None => None,
                };
                let mut eval_stops = Vec::with_capacity(stops.len());
                for st in stops {
                    let offset = self.eval_expr(&st.offset, locals)?.as_f64()?.clamp(0.0, 1.0);
                    let color = self.eval_expr(&st.color, locals)?.as_color()?;
                    eval_stops.push(crate::draw_list::GradientStop { offset, color });
                }
                Ok(Value::Paint(Paint::Radial {
                    center: trans.transform_point(c),
                    radius: r,
                    focal: f,
                    stops: eval_stops,
                }))
            }
            Expr::Angular { center, start_angle, stops } => {
                let c = self.eval_expr(center, locals)?.as_vec2()?;
                let sa = self.eval_expr(start_angle, locals)?.as_f64()?;
                let trans = self.current_transform();
                let mut eval_stops = Vec::with_capacity(stops.len());
                for st in stops {
                    let offset = self.eval_expr(&st.offset, locals)?.as_f64()?.clamp(0.0, 1.0);
                    let color = self.eval_expr(&st.color, locals)?.as_color()?;
                    eval_stops.push(crate::draw_list::GradientStop { offset, color });
                }
                Ok(Value::Paint(Paint::Angular {
                    center: trans.transform_point(c),
                    start_angle: sa,
                    stops: eval_stops,
                }))
            }
        }
    }
}

/// Converts Catmull-Rom control points into cubic Bezier segments.
/// Shared by SVG / raster backends so splines render identically.
pub fn spline_to_bezier(points: &[(f64, f64)]) -> Vec<((f64, f64), (f64, f64), (f64, f64))> {
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }
    if n == 2 {
        // Straight line as degenerate cubic.
        return vec![(points[0], points[0], points[1])];
    }
    let mut out = Vec::with_capacity(n - 1);
    for i in 0..n - 1 {
        let p0 = if i == 0 { points[0] } else { points[i - 1] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < n { points[i + 2] } else { points[n - 1] };
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        out.push((c1, c2, p2));
    }
    out
}

/// Host-driven scene handle: parse once, push uniforms per frame, evaluate.
///
/// ```rust
/// use pvg::Scene;
/// let mut scene = Scene::from_source("PVG 0.2\ncanvas 200 20\nparam health: 0.75\nrectangle\n  pos [20, 20]\n  size [200 * health, 12]\n").unwrap();
/// scene.set_param("health", 0.2);
/// let dl = scene.evaluate().unwrap();
/// assert_eq!(dl.len(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct Scene {
    doc: Document,
    params: HashMap<String, Value>,
    time: f64,
    loop_limit: usize,
}

impl Scene {
    /// Parses source once; per-frame work is just [`Scene::evaluate`].
    pub fn from_source(source: &str) -> Result<Self, PvgError> {
        Ok(Self { doc: crate::parse(source)?, params: HashMap::new(), time: 0.0, loop_limit: 100_000 })
    }

    /// Parses from an already-parsed document.
    pub fn from_document(doc: Document) -> Self {
        Self { doc, params: HashMap::new(), time: 0.0, loop_limit: 100_000 }
    }

    /// Overrides a `param` uniform (takes effect on next [`Scene::evaluate`]).
    pub fn set_param(&mut self, name: impl Into<String>, value: impl Into<Value>) {
        self.params.insert(name.into(), value.into());
    }

    /// Removes a host override so the document default applies again.
    pub fn clear_param(&mut self, name: &str) {
        self.params.remove(name);
    }

    /// Sets the timeline clock (`time` / `t`).
    pub fn set_time(&mut self, time: f64) {
        self.time = time;
    }

    /// Sets the loop-iteration safety cap (default 100,000).
    pub fn with_loop_limit(mut self, limit: usize) -> Self {
        self.loop_limit = limit;
        self
    }

    /// Declared uniform names.
    pub fn param_names(&self) -> Vec<&str> {
        self.doc.param_names()
    }

    /// Current value of a uniform (override or evaluated default at t=0).
    pub fn param_value(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.params.get(name) {
            return Some(v.clone());
        }
        let decl = self.doc.params.iter().find(|p| p.name == name)?;
        let mut ev = Evaluator::new_with_time(self.time);
        let locals = HashMap::new();
        // Defaults are pure expressions in practice; fall back to None on error.
        ev.eval_expr(&decl.default, &locals).ok()
    }

    /// Evaluates the cached document with current params + time (<50µs typical).
    pub fn evaluate(&self) -> Result<DrawList, PvgError> {
        let mut ev = Evaluator::new_with_time(self.time).with_loop_limit(self.loop_limit);
        for (k, v) in &self.params {
            ev.set_param(k.clone(), v.clone());
        }
        ev.evaluate_document(&self.doc)
    }

    /// Borrows the parsed document.
    pub fn document(&self) -> &Document {
        &self.doc
    }
}