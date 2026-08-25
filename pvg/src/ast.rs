#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone)]
pub struct CanvasDecl {
    pub width: f64,
    pub height: f64,
    pub background: Option<Color>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct PathNode {
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
    pub commands: Vec<PathCommand>,
}

#[derive(Debug, Clone)]
pub struct CircleNode {
    pub center: Expr,
    pub radius: Expr,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct EllipseNode {
    pub center: Expr,
    pub radius: Expr,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct RectNode {
    pub pos: Expr,
    pub size: Expr,
    pub radius: Option<Expr>,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct LineNode {
    pub from: Expr,
    pub to: Expr,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct PolygonNode {
    pub points: Vec<Expr>,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct GroupNode {
    pub pos: Option<Expr>,
    pub rot: Option<Expr>,
    pub scale: Option<Expr>,
    pub opacity: Option<Expr>,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
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
    Group(GroupNode),
}

#[derive(Debug, Clone)]
pub struct Document {
    pub version: (u32, u32),
    pub canvas: CanvasDecl,
    pub statements: Vec<Stmt>,
}