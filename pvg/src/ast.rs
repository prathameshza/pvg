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
pub struct GradientStop {
    pub offset: Expr,
    pub color: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShadowExpr {
    pub offset: Expr,
    pub radius: Expr,
    pub color: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlowExpr {
    pub radius: Expr,
    pub color: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    String(String),
    Bool(bool),
    Color(Color),
    Vec2(Box<Expr>, Box<Expr>),
    /// Lightweight 1D array literal (length != 2; 2-element brackets stay Vec2).
    /// Use `array(a, b)` builtin for explicit 2-element arrays.
    Array(Vec<Expr>),
    /// Reference to a named `pattern` tile (`fill pattern name`).
    Pattern(String),
    Ident(String),
    Unary(UnaryOp, Box<Expr>),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Linear {
        start: Box<Expr>,
        end: Box<Expr>,
        stops: Vec<GradientStop>,
    },
    Radial {
        center: Box<Expr>,
        radius: Box<Expr>,
        focal: Option<Box<Expr>>,
        stops: Vec<GradientStop>,
    },
    Angular {
        center: Box<Expr>,
        start_angle: Box<Expr>,
        stops: Vec<GradientStop>,
    },
}

/// Pixel-art sampling for `canvas filter` (sprite / retro mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PixelFilter {
    #[default]
    Linear,
    Nearest,
}

/// Host-overridable uniform declared via `param name: default`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamDecl {
    pub name: String,
    pub default: Expr,
}

/// Repeatable tile declared via `pattern name w h` + body block.
#[derive(Debug, Clone, PartialEq)]
pub struct PatternDef {
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanvasDecl {
    pub width: f64,
    pub height: f64,
    pub background: Option<Color>,
    /// Snap grid in px (0 = off). `snap 1.0` forces integer pixel boundaries.
    pub snap: f64,
    /// Image smoothing for raster backends (`"nearest"` = crisp pixels).
    pub pixel_filter: PixelFilter,
}

impl CanvasDecl {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height, background: None, snap: 0.0, pixel_filter: PixelFilter::Linear }
    }
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
    /// Control flow inside `path` bodies (shares the path's locals scope).
    /// Lets loops/conditionals emit points procedurally, e.g. noise-wobbled
    /// perimeters with `if deg == 0` start/line selection.
    For {
        var: String,
        from: Expr,
        to: Expr,
        step: Option<Expr>,
        body: Vec<PathCommand>,
    },
    While {
        cond: Expr,
        body: Vec<PathCommand>,
    },
    If {
        cond: Expr,
        then_body: Vec<PathCommand>,
        else_body: Vec<PathCommand>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathNode {
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
    pub cap: Option<Expr>,
    pub join: Option<Expr>,
    pub miter: Option<Expr>,
    pub dash: Option<Vec<Expr>>,
    pub align: Option<Expr>,
    pub blur: Option<Expr>,
    pub shadow: Option<ShadowExpr>,
    pub glow: Option<GlowExpr>,
    pub blend: Option<Expr>,
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
    pub cap: Option<Expr>,
    pub join: Option<Expr>,
    pub miter: Option<Expr>,
    pub dash: Option<Vec<Expr>>,
    pub align: Option<Expr>,
    pub blur: Option<Expr>,
    pub shadow: Option<ShadowExpr>,
    pub glow: Option<GlowExpr>,
    pub blend: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EllipseNode {
    pub center: Expr,
    pub radius: Expr,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
    pub cap: Option<Expr>,
    pub join: Option<Expr>,
    pub miter: Option<Expr>,
    pub dash: Option<Vec<Expr>>,
    pub align: Option<Expr>,
    pub blur: Option<Expr>,
    pub shadow: Option<ShadowExpr>,
    pub glow: Option<GlowExpr>,
    pub blend: Option<Expr>,
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
    pub cap: Option<Expr>,
    pub join: Option<Expr>,
    pub miter: Option<Expr>,
    pub dash: Option<Vec<Expr>>,
    pub align: Option<Expr>,
    pub blur: Option<Expr>,
    pub shadow: Option<ShadowExpr>,
    pub glow: Option<GlowExpr>,
    pub blend: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineNode {
    pub from: Expr,
    pub to: Expr,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
    pub cap: Option<Expr>,
    pub join: Option<Expr>,
    pub miter: Option<Expr>,
    pub dash: Option<Vec<Expr>>,
    pub align: Option<Expr>,
    pub blur: Option<Expr>,
    pub shadow: Option<ShadowExpr>,
    pub glow: Option<GlowExpr>,
    pub blend: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolygonNode {
    pub points: Vec<Expr>,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
    pub cap: Option<Expr>,
    pub join: Option<Expr>,
    pub miter: Option<Expr>,
    pub dash: Option<Vec<Expr>>,
    pub align: Option<Expr>,
    pub blur: Option<Expr>,
    pub shadow: Option<ShadowExpr>,
    pub glow: Option<GlowExpr>,
    pub blend: Option<Expr>,
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
    pub blur: Option<Expr>,
    pub shadow: Option<ShadowExpr>,
    pub glow: Option<GlowExpr>,
    pub blend: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupNode {
    pub pos: Option<Expr>,
    pub rot: Option<Expr>,
    pub scale: Option<Expr>,
    pub opacity: Option<Expr>,
    pub fill: Option<Expr>,
    pub stroke: Option<Expr>,
    pub blend: Option<Expr>,
    pub blur: Option<Expr>,
    pub shadow: Option<ShadowExpr>,
    pub glow: Option<GlowExpr>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpriteNode {
    pub pos: Expr,
    /// Palette entries (color expressions, index 0+; `.` / space = transparent).
    pub palette: Vec<Expr>,
    /// Pixel rows: each char maps to a palette index (`0`-`9`), `.`/space = skip.
    pub rows: Vec<String>,
    pub scale: Option<Expr>,
    pub opacity: Option<Expr>,
    pub blend: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SplineNode {
    /// Array of Vec2 control points, or array of Numbers (y-values, x = index).
    pub points: Expr,
    /// Origin for Number-series auto-layout (default [0, 0]).
    pub pos: Option<Expr>,
    /// Extent for Number-series auto-layout (default [n-1, 1] mapping).
    pub size: Option<Expr>,
    pub stroke: Option<Expr>,
    pub width: Option<Expr>,
    pub opacity: Option<Expr>,
    pub cap: Option<Expr>,
    pub join: Option<Expr>,
    pub miter: Option<Expr>,
    pub dash: Option<Vec<Expr>>,
    pub blur: Option<Expr>,
    pub shadow: Option<ShadowExpr>,
    pub glow: Option<GlowExpr>,
    pub blend: Option<Expr>,
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
    Sprite(SpriteNode),
    Spline(SplineNode),
    Clip {
        mask: Box<Stmt>,
        content: Vec<Stmt>,
    },
}

/// The top-level parsed AST representation of a PVG document.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// Document version (major, minor).
    pub version: (u32, u32),
    /// Canvas declaration containing width, height, and optional background color.
    pub canvas: CanvasDecl,
    /// Host-overridable uniforms (`param name: default`).
    pub params: Vec<ParamDecl>,
    /// Named repeatable tiles (`pattern name w h` + body).
    pub patterns: Vec<PatternDef>,
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
        fn stop_has_time(s: &GradientStop) -> bool {
            expr_has_time(&s.offset) || expr_has_time(&s.color)
        }
        fn shadow_has_time(s: &ShadowExpr) -> bool {
            expr_has_time(&s.offset) || expr_has_time(&s.radius) || expr_has_time(&s.color)
        }
        fn glow_has_time(g: &GlowExpr) -> bool {
            expr_has_time(&g.radius) || expr_has_time(&g.color)
        }
        fn opt_has_time(o: &Option<Expr>) -> bool {
            o.as_ref().map_or(false, expr_has_time)
        }
        fn dash_has_time(d: &Option<Vec<Expr>>) -> bool {
            d.as_ref().map_or(false, |v| v.iter().any(expr_has_time))
        }
        fn expr_has_time(e: &Expr) -> bool {
            match e {
                Expr::Ident(name) => name == "time" || name == "t",
                Expr::Vec2(x, y) => expr_has_time(x) || expr_has_time(y),
                Expr::Array(items) => items.iter().any(expr_has_time),
                Expr::Pattern(_) => false,
                Expr::Unary(_, inner) => expr_has_time(inner),
                Expr::Binary(l, _, r) => expr_has_time(l) || expr_has_time(r),
                Expr::Ternary(c, t, f) => expr_has_time(c) || expr_has_time(t) || expr_has_time(f),
                Expr::Call(_, args) => args.iter().any(expr_has_time),
                Expr::Linear { start, end, stops } => {
                    expr_has_time(start)
                        || expr_has_time(end)
                        || stops.iter().any(stop_has_time)
                }
                Expr::Radial { center, radius, focal, stops } => {
                    expr_has_time(center)
                        || expr_has_time(radius)
                        || focal.as_ref().map_or(false, |f| expr_has_time(f))
                        || stops.iter().any(stop_has_time)
                }
                Expr::Angular { center, start_angle, stops } => {
                    expr_has_time(center)
                        || expr_has_time(start_angle)
                        || stops.iter().any(stop_has_time)
                }
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
                        || opt_has_time(&c.fill)
                        || opt_has_time(&c.stroke)
                        || opt_has_time(&c.width)
                        || opt_has_time(&c.opacity)
                        || opt_has_time(&c.cap)
                        || opt_has_time(&c.join)
                        || opt_has_time(&c.miter)
                        || dash_has_time(&c.dash)
                        || opt_has_time(&c.align)
                        || opt_has_time(&c.blur)
                        || c.shadow.as_ref().map_or(false, shadow_has_time)
                        || c.glow.as_ref().map_or(false, glow_has_time)
                        || opt_has_time(&c.blend)
                }
                Stmt::Ellipse(e) => {
                    expr_has_time(&e.center)
                        || expr_has_time(&e.radius)
                        || opt_has_time(&e.fill)
                        || opt_has_time(&e.stroke)
                        || opt_has_time(&e.width)
                        || opt_has_time(&e.opacity)
                        || opt_has_time(&e.cap)
                        || opt_has_time(&e.join)
                        || opt_has_time(&e.miter)
                        || dash_has_time(&e.dash)
                        || opt_has_time(&e.align)
                        || opt_has_time(&e.blur)
                        || e.shadow.as_ref().map_or(false, shadow_has_time)
                        || e.glow.as_ref().map_or(false, glow_has_time)
                        || opt_has_time(&e.blend)
                }
                Stmt::Rectangle(r) => {
                    expr_has_time(&r.pos)
                        || expr_has_time(&r.size)
                        || opt_has_time(&r.radius)
                        || opt_has_time(&r.fill)
                        || opt_has_time(&r.stroke)
                        || opt_has_time(&r.width)
                        || opt_has_time(&r.opacity)
                        || opt_has_time(&r.cap)
                        || opt_has_time(&r.join)
                        || opt_has_time(&r.miter)
                        || dash_has_time(&r.dash)
                        || opt_has_time(&r.align)
                        || opt_has_time(&r.blur)
                        || r.shadow.as_ref().map_or(false, shadow_has_time)
                        || r.glow.as_ref().map_or(false, glow_has_time)
                        || opt_has_time(&r.blend)
                }
                Stmt::Line(l) => {
                    expr_has_time(&l.from)
                        || expr_has_time(&l.to)
                        || opt_has_time(&l.stroke)
                        || opt_has_time(&l.width)
                        || opt_has_time(&l.opacity)
                        || opt_has_time(&l.cap)
                        || opt_has_time(&l.join)
                        || opt_has_time(&l.miter)
                        || dash_has_time(&l.dash)
                        || opt_has_time(&l.align)
                        || opt_has_time(&l.blur)
                        || l.shadow.as_ref().map_or(false, shadow_has_time)
                        || l.glow.as_ref().map_or(false, glow_has_time)
                        || opt_has_time(&l.blend)
                }
                Stmt::Polygon(p) => {
                    p.points.iter().any(expr_has_time)
                        || opt_has_time(&p.fill)
                        || opt_has_time(&p.stroke)
                        || opt_has_time(&p.width)
                        || opt_has_time(&p.opacity)
                        || opt_has_time(&p.cap)
                        || opt_has_time(&p.join)
                        || opt_has_time(&p.miter)
                        || dash_has_time(&p.dash)
                        || opt_has_time(&p.align)
                        || opt_has_time(&p.blur)
                        || p.shadow.as_ref().map_or(false, shadow_has_time)
                        || p.glow.as_ref().map_or(false, glow_has_time)
                        || opt_has_time(&p.blend)
                }
                Stmt::Path(p) => {
                    opt_has_time(&p.fill)
                        || opt_has_time(&p.stroke)
                        || opt_has_time(&p.width)
                        || opt_has_time(&p.opacity)
                        || opt_has_time(&p.cap)
                        || opt_has_time(&p.join)
                        || opt_has_time(&p.miter)
                        || dash_has_time(&p.dash)
                        || opt_has_time(&p.align)
                        || opt_has_time(&p.blur)
                        || p.shadow.as_ref().map_or(false, shadow_has_time)
                        || p.glow.as_ref().map_or(false, glow_has_time)
                        || opt_has_time(&p.blend)
                        || p.commands.iter().any(|cmd| {
                            fn pc_has_time(c: &PathCommand) -> bool {
                                match c {
                                    PathCommand::Set(_, e)
                                    | PathCommand::Start(e)
                                    | PathCommand::Line(e) => expr_has_time(e),
                                    PathCommand::Quad(cp, ep) => {
                                        expr_has_time(cp) || expr_has_time(ep)
                                    }
                                    PathCommand::Curve(c1, c2, ep) => {
                                        expr_has_time(c1) || expr_has_time(c2) || expr_has_time(ep)
                                    }
                                    PathCommand::Arc { center, radius, start_angle, end_angle } => {
                                        expr_has_time(center)
                                            || expr_has_time(radius)
                                            || expr_has_time(start_angle)
                                            || expr_has_time(end_angle)
                                    }
                                    PathCommand::Close => false,
                                    PathCommand::For { from, to, step, body, .. } => {
                                        expr_has_time(from)
                                            || expr_has_time(to)
                                            || step.as_ref().map_or(false, expr_has_time)
                                            || body.iter().any(pc_has_time)
                                    }
                                    PathCommand::While { cond, body } => {
                                        expr_has_time(cond) || body.iter().any(pc_has_time)
                                    }
                                    PathCommand::If { cond, then_body, else_body } => {
                                        expr_has_time(cond)
                                            || then_body.iter().any(pc_has_time)
                                            || else_body.iter().any(pc_has_time)
                                    }
                                }
                            }
                            pc_has_time(cmd)
                        })
                }
                Stmt::Text(t) => {
                    expr_has_time(&t.pos)
                        || expr_has_time(&t.content)
                        || opt_has_time(&t.size)
                        || opt_has_time(&t.font)
                        || opt_has_time(&t.align)
                        || opt_has_time(&t.fill)
                        || opt_has_time(&t.stroke)
                        || opt_has_time(&t.width)
                        || opt_has_time(&t.opacity)
                        || opt_has_time(&t.blur)
                        || t.shadow.as_ref().map_or(false, shadow_has_time)
                        || t.glow.as_ref().map_or(false, glow_has_time)
                        || opt_has_time(&t.blend)
                }
                Stmt::Group(g) => {
                    opt_has_time(&g.pos)
                        || opt_has_time(&g.rot)
                        || opt_has_time(&g.scale)
                        || opt_has_time(&g.opacity)
                        || opt_has_time(&g.fill)
                        || opt_has_time(&g.stroke)
                        || opt_has_time(&g.blend)
                        || opt_has_time(&g.blur)
                        || g.shadow.as_ref().map_or(false, shadow_has_time)
                        || g.glow.as_ref().map_or(false, glow_has_time)
                        || g.body.iter().any(stmt_has_time)
                }
                Stmt::Sprite(s) => {
                    expr_has_time(&s.pos)
                        || s.palette.iter().any(expr_has_time)
                        || opt_has_time(&s.scale)
                        || opt_has_time(&s.opacity)
                        || opt_has_time(&s.blend)
                }
                Stmt::Spline(s) => {
                    expr_has_time(&s.points)
                        || opt_has_time(&s.pos)
                        || opt_has_time(&s.size)
                        || opt_has_time(&s.stroke)
                        || opt_has_time(&s.width)
                        || opt_has_time(&s.opacity)
                        || opt_has_time(&s.cap)
                        || opt_has_time(&s.join)
                        || opt_has_time(&s.miter)
                        || dash_has_time(&s.dash)
                        || opt_has_time(&s.blur)
                        || s.shadow.as_ref().map_or(false, shadow_has_time)
                        || s.glow.as_ref().map_or(false, glow_has_time)
                        || opt_has_time(&s.blend)
                }
                Stmt::Clip { mask, content } => {
                    stmt_has_time(mask) || content.iter().any(stmt_has_time)
                }
                Stmt::Seed(_) => false,
            }
        }

        self.statements.iter().any(stmt_has_time)
            || self.params.iter().any(|p| expr_has_time(&p.default))
            || self.patterns.iter().any(|p| p.body.iter().any(stmt_has_time))
    }

    /// Names of declared host uniforms (`param`).
    pub fn param_names(&self) -> Vec<&str> {
        self.params.iter().map(|p| p.name.as_str()).collect()
    }

    /// Names of declared pattern tiles.
    pub fn pattern_names(&self) -> Vec<&str> {
        self.patterns.iter().map(|p| p.name.as_str()).collect()
    }
}