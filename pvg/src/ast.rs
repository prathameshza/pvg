use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Color {
    Rgba(u8, u8, u8, u8),
    None,
}

impl Color {
    pub const BLACK: Color = Color::Rgba(0, 0, 0, 255);
    pub const WHITE: Color = Color::Rgba(255, 255, 255, 255);
    pub const RED: Color = Color::Rgba(255, 0, 0, 255);
    pub const GREEN: Color = Color::Rgba(0, 255, 0, 255);
    pub const BLUE: Color = Color::Rgba(0, 0, 255, 255);
    pub const YELLOW: Color = Color::Rgba(255, 255, 0, 255);
    pub const CYAN: Color = Color::Rgba(0, 255, 255, 255);
    pub const MAGENTA: Color = Color::Rgba(255, 0, 255, 255);
    pub const TRANSPARENT: Color = Color::Rgba(0, 0, 0, 0);

    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Color::Rgba(r, g, b, 255)
    }

    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color::Rgba(r, g, b, a)
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Color::None)
    }

    pub fn is_transparent(&self) -> bool {
        match self {
            Color::None => true,
            Color::Rgba(_, _, _, a) => *a == 0,
        }
    }

    pub fn to_rgba_tuple(&self) -> Option<(u8, u8, u8, u8)> {
        match self {
            Color::Rgba(r, g, b, a) => Some((*r, *g, *b, *a)),
            Color::None => None,
        }
    }

    pub fn from_hex(hex: &str) -> Option<Color> {
        let s = hex.trim_start_matches('#');
        match s.len() {
            3 => {
                let r = u8::from_str_radix(&s[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&s[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&s[2..3].repeat(2), 16).ok()?;
                Some(Color::Rgba(r, g, b, 255))
            }
            6 => {
                let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                Some(Color::Rgba(r, g, b, 255))
            }
            8 => {
                let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                let a = u8::from_str_radix(&s[6..8], 16).ok()?;
                Some(Color::Rgba(r, g, b, a))
            }
            _ => None,
        }
    }

    pub fn to_svg_string(&self) -> String {
        match self {
            Color::Rgba(r, g, b, 255) => format!("#{:02x}{:02x}{:02x}", r, g, b),
            Color::Rgba(r, g, b, a) => {
                format!("rgba({}, {}, {}, {:.3})", r, g, b, *a as f64 / 255.0)
            }
            Color::None => "none".to_string(),
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::BLACK
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_svg_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    String(String),
    Bool(bool),
    Color(Color),
    Vec2(Box<Expr>, Box<Expr>),
    Ident(String),
    Unary(UnaryOp, Box<Expr>),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanvasDecl {
    pub width: f64,
    pub height: f64,
    pub background: Option<Color>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathCommand {
    Set(String, Expr),
    Start(Expr),
    Line(Expr),
    Quad(Expr, Expr),
    Curve(Expr, Expr, Expr),
    Arc {
        center: Expr,
        radius: Expr,
        start_angle: Expr,
        end_angle: Expr,
    },
    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathNode {
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
    pub commands: Vec<PathCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CircleNode {
    pub center: Expr,
    pub radius: Expr,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EllipseNode {
    pub center: Expr,
    pub radius: Expr,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RectNode {
    pub pos: Expr,
    pub size: Expr,
    pub radius: Option<Expr>,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineNode {
    pub from: Expr,
    pub to: Expr,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolygonNode {
    pub points: Vec<Expr>,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextNode {
    pub pos: Expr,
    pub content: Expr,
    pub size: Option<Expr>,
    pub font: Option<Expr>,
    pub align: Option<Expr>,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupNode {
    pub pos: Option<Expr>,
    pub rot: Option<Expr>,
    pub scale: Option<Expr>,
    pub opacity: Option<Expr>,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Set(String, Expr),
    For {
        var: String,
        from: Expr,
        to: Expr,
        step: Option<Expr>,
        body: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    Def(FunctionDef),
    Call(String, Vec<Expr>),
    Return(Expr),
    Seed(u64),
    Circle(CircleNode),
    Ellipse(EllipseNode),
    Rectangle(RectNode),
    Line(LineNode),
    Polygon(PolygonNode),
    Path(PathNode),
    Text(TextNode),
    Group(GroupNode),
}

/// The top-level parsed AST representation of a PVG document.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// Document version (major, minor).
    pub version: (u32, u32),
    /// Canvas declaration containing width, height, and optional background color.
    pub canvas: CanvasDecl,
    /// Root statements of the document.
    pub statements: Vec<Stmt>,
}

impl Document {
    /// Returns the canvas dimensions as `(width, height)`.
    pub fn canvas_size(&self) -> (f64, f64) {
        (self.canvas.width, self.canvas.height)
    }

    /// Checks if this AST references the timeline clock variables (`time` or `t`).
    pub fn is_animated(&self) -> bool {
        fn expr_has_time(e: &Expr) -> bool {
            match e {
                Expr::Ident(name) => name == "time" || name == "t",
                Expr::Vec2(x, y) => expr_has_time(x) || expr_has_time(y),
                Expr::Unary(_, inner) => expr_has_time(inner),
                Expr::Binary(l, _, r) => expr_has_time(l) || expr_has_time(r),
                Expr::Ternary(c, t, f) => expr_has_time(c) || expr_has_time(t) || expr_has_time(f),
                Expr::Call(_, args) => args.iter().any(expr_has_time),
                _ => false,
            }
        }

        fn stmt_has_time(s: &Stmt) -> bool {
            match s {
                Stmt::Set(_, e) | Stmt::Return(e) => expr_has_time(e),
                Stmt::For { from, to, step, body, .. } => {
                    expr_has_time(from)
                        || expr_has_time(to)
                        || step.as_ref().map_or(false, expr_has_time)
                        || body.iter().any(stmt_has_time)
                }
                Stmt::While { cond, body } => expr_has_time(cond) || body.iter().any(stmt_has_time),
                Stmt::If { cond, then_body, else_body } => {
                    expr_has_time(cond)
                        || then_body.iter().any(stmt_has_time)
                        || else_body.iter().any(stmt_has_time)
                }
                Stmt::Def(f) => f.body.iter().any(stmt_has_time),
                Stmt::Call(_, args) => args.iter().any(expr_has_time),
                Stmt::Circle(c) => {
                    expr_has_time(&c.center)
                        || expr_has_time(&c.radius)
                        || c.fill.as_ref().map_or(false, expr_has_time)
                        || c.stroke.as_ref().map_or(false, expr_has_time)
                        || c.width.as_ref().map_or(false, expr_has_time)
                        || c.opacity.as_ref().map_or(false, expr_has_time)
                }
                Stmt::Ellipse(e) => {
                    expr_has_time(&e.center)
                        || expr_has_time(&e.radius)
                        || e.fill.as_ref().map_or(false, expr_has_time)
                        || e.stroke.as_ref().map_or(false, expr_has_time)
                        || e.width.as_ref().map_or(false, expr_has_time)
                        || e.opacity.as_ref().map_or(false, expr_has_time)
                }
                Stmt::Rectangle(r) => {
                    expr_has_time(&r.pos)
                        || expr_has_time(&r.size)
                        || r.radius.as_ref().map_or(false, expr_has_time)
                        || r.fill.as_ref().map_or(false, expr_has_time)
                        || r.stroke.as_ref().map_or(false, expr_has_time)
                        || r.width.as_ref().map_or(false, expr_has_time)
                        || r.opacity.as_ref().map_or(false, expr_has_time)
                }
                Stmt::Line(l) => {
                    expr_has_time(&l.from)
                        || expr_has_time(&l.to)
                        || l.stroke.as_ref().map_or(false, expr_has_time)
                        || l.width.as_ref().map_or(false, expr_has_time)
                        || l.opacity.as_ref().map_or(false, expr_has_time)
                }
                Stmt::Polygon(p) => {
                    p.points.iter().any(expr_has_time)
                        || p.fill.as_ref().map_or(false, expr_has_time)
                        || p.stroke.as_ref().map_or(false, expr_has_time)
                        || p.width.as_ref().map_or(false, expr_has_time)
                        || p.opacity.as_ref().map_or(false, expr_has_time)
                }
                Stmt::Path(p) => {
                    p.fill.as_ref().map_or(false, expr_has_time)
                        || p.stroke.as_ref().map_or(false, expr_has_time)
                        || p.width.as_ref().map_or(false, expr_has_time)
                        || p.opacity.as_ref().map_or(false, expr_has_time)
                        || p.commands.iter().any(|cmd| match cmd {
                            PathCommand::Set(_, e) | PathCommand::Start(e) | PathCommand::Line(e) => expr_has_time(e),
                            PathCommand::Quad(cp, ep) => expr_has_time(cp) || expr_has_time(ep),
                            PathCommand::Curve(c1, c2, ep) => expr_has_time(c1) || expr_has_time(c2) || expr_has_time(ep),
                            PathCommand::Arc { center, radius, start_angle, end_angle } => {
                                expr_has_time(center)
                                    || expr_has_time(radius)
                                    || expr_has_time(start_angle)
                                    || expr_has_time(end_angle)
                            }
                            PathCommand::Close => false,
                        })
                }
                Stmt::Text(t) => {
                    expr_has_time(&t.pos)
                        || expr_has_time(&t.content)
                        || t.size.as_ref().map_or(false, expr_has_time)
                        || t.font.as_ref().map_or(false, expr_has_time)
                        || t.align.as_ref().map_or(false, expr_has_time)
                        || t.fill.as_ref().map_or(false, expr_has_time)
                        || t.stroke.as_ref().map_or(false, expr_has_time)
                        || t.width.as_ref().map_or(false, expr_has_time)
                        || t.opacity.as_ref().map_or(false, expr_has_time)
                }
                Stmt::Group(g) => {
                    g.pos.as_ref().map_or(false, expr_has_time)
                        || g.rot.as_ref().map_or(false, expr_has_time)
                        || g.scale.as_ref().map_or(false, expr_has_time)
                        || g.opacity.as_ref().map_or(false, expr_has_time)
                        || g.fill.as_ref().map_or(false, expr_has_time)
                        || g.stroke.as_ref().map_or(false, expr_has_time)
                        || g.body.iter().any(stmt_has_time)
                }
                Stmt::Seed(_) => false,
            }
        }

        self.statements.iter().any(stmt_has_time)
    }
}