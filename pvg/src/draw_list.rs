use crate::ast::{Color, PixelFilter};
use crate::svg::emit_svg;
use std::ops::Mul;

/// A 2D affine transformation matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Transform2D {
    /// The identity transformation.
    pub const fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Translation transformation.
    pub fn from_translation(tx: f64, ty: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx,
            ty,
        }
    }

    /// Rotation transformation around the origin in radians.
    pub fn from_rotation(angle_rad: f64) -> Self {
        let (sin, cos) = angle_rad.sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Scaling transformation.
    pub fn from_scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Multiplies this transformation matrix by another.
    pub fn mul(&self, o: &Transform2D) -> Self {
        Self {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            tx: self.a * o.tx + self.c * o.ty + self.tx,
            ty: self.b * o.tx + self.d * o.ty + self.ty,
        }
    }

    /// Transforms a 2D point coordinate `(x, y)`.
    pub fn transform_point(&self, p: (f64, f64)) -> (f64, f64) {
        (
            self.a * p.0 + self.c * p.1 + self.tx,
            self.b * p.0 + self.d * p.1 + self.ty,
        )
    }
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::identity()
    }
}

impl Mul for Transform2D {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        Transform2D::mul(&self, &rhs)
    }
}

/// Horizontal text alignment relative to the anchor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Stroke line-cap topology (PVG 0.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// Stroke line-join topology (PVG 0.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Stroke positioning relative to the geometric edge (PVG 0.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokeAlign {
    #[default]
    Center,
    Inside,
    Outside,
}

/// Per-pixel compositing blend mode (PVG 0.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
}

/// A single evaluated gradient color stop.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientStop {
    pub offset: f64,
    pub color: Color,
}

/// Evaluated paint: solid color or procedural gradient (PVG 0.2).
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Color(Color),
    Linear {
        start: (f64, f64),
        end: (f64, f64),
        stops: Vec<GradientStop>,
    },
    Radial {
        center: (f64, f64),
        radius: f64,
        focal: Option<(f64, f64)>,
        stops: Vec<GradientStop>,
    },
    Angular {
        center: (f64, f64),
        start_angle: f64,
        stops: Vec<GradientStop>,
    },
    /// Repeatable tile (`fill pattern name`). Resolved via `DrawList.patterns`.
    Pattern(String),
}

impl Default for Paint {
    fn default() -> Self {
        Paint::Color(Color::BLACK)
    }
}

impl From<Color> for Paint {
    fn from(c: Color) -> Self {
        Paint::Color(c)
    }
}

impl Paint {
    /// Returns the solid color if this paint is a flat color.
    pub fn as_color(&self) -> Option<&Color> {
        match self {
            Paint::Color(c) => Some(c),
            _ => None,
        }
    }

    /// Returns `true` when this paint is `none`/fully transparent.
    pub fn is_none(&self) -> bool {
        match self {
            Paint::Color(c) => c.is_none(),
            Paint::Pattern(_) => false,
            _ => false,
        }
    }

    /// SVG-compatible string for solid colors. Gradients resolve to
    /// `url(#...)` references filled in by the SVG emitter; this fallback
    /// keeps ad-hoc formatting total.
    pub fn to_svg_string(&self) -> String {
        match self {
            Paint::Color(c) => c.to_svg_string(),
            Paint::Pattern(name) => format!("url(#pvg-pat-{})", name),
            _ => "url(#pvg-grad)".to_string(),
        }
    }
}

/// Drop/cast shadow descriptor (PVG 0.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    pub offset: (f64, f64),
    pub radius: f64,
    pub color: Color,
}

/// Outer neon/energy glow descriptor (PVG 0.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Glow {
    pub radius: f64,
    pub color: Color,
}

/// Styling attributes applied to geometric and text primitives.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawStyle {
    /// Fill paint (solid color or gradient).
    pub fill: Paint,
    /// Stroke paint (solid color or gradient).
    pub stroke: Paint,
    /// Stroke width in pixels.
    pub width: f64,
    /// Multiplicative opacity in [0.0, 1.0].
    pub opacity: f64,
    /// Stroke line cap.
    pub cap: LineCap,
    /// Stroke line join.
    pub join: LineJoin,
    /// Miter limit for `miter` joins.
    pub miter: f64,
    /// Dash pattern (`[dash, gap, ...]`). Empty = solid.
    pub dash: Vec<f64>,
    /// Stroke positioning.
    pub stroke_align: StrokeAlign,
    /// Compositing blend mode.
    pub blend: BlendMode,
    /// Direct object blur radius in px (0 = none).
    pub blur: f64,
    /// Optional drop shadow.
    pub shadow: Option<Shadow>,
    /// Optional outer glow.
    pub glow: Option<Glow>,
}

impl Default for DrawStyle {
    fn default() -> Self {
        Self {
            fill: Paint::Color(Color::BLACK),
            stroke: Paint::Color(Color::None),
            width: 1.0,
            opacity: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter: 4.0,
            dash: Vec::new(),
            stroke_align: StrokeAlign::Center,
            blend: BlendMode::Normal,
            blur: 0.0,
            shadow: None,
            glow: None,
        }
    }
}

impl DrawStyle {
    pub fn with_fill(mut self, fill: Color) -> Self {
        self.fill = Paint::Color(fill);
        self
    }
    pub fn with_paint(mut self, fill: Paint) -> Self {
        self.fill = fill;
        self
    }
    pub fn with_stroke(mut self, stroke: Color) -> Self {
        self.stroke = Paint::Color(stroke);
        self
    }
    pub fn with_stroke_paint(mut self, stroke: Paint) -> Self {
        self.stroke = stroke;
        self
    }
    pub fn with_width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }
}

/// Individual 2D draw command emitted into the evaluated DrawList.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    Circle {
        center: (f64, f64),
        radius: f64,
        style: DrawStyle,
    },
    Ellipse {
        center: (f64, f64),
        radius: (f64, f64),
        style: DrawStyle,
    },
    Rectangle {
        pos: (f64, f64),
        size: (f64, f64),
        corner_radius: f64,
        style: DrawStyle,
    },
    Line {
        from: (f64, f64),
        to: (f64, f64),
        style: DrawStyle,
    },
    Polygon {
        points: Vec<(f64, f64)>,
        style: DrawStyle,
    },
    Path {
        commands: Vec<DrawPathCommand>,
        style: DrawStyle,
    },
    Text {
        pos: (f64, f64),
        content: String,
        size: f64,
        font_family: String,
        align: TextAlign,
        style: DrawStyle,
    },
    /// Pixel-art sprite: palette-indexed rows rasterized as crisp rects.
    Sprite {
        pos: (f64, f64),
        palette: Vec<Color>,
        rows: Vec<String>,
        scale: f64,
        style: DrawStyle,
    },
    /// Smooth Catmull-Rom spline through control points.
    Spline {
        points: Vec<(f64, f64)>,
        style: DrawStyle,
    },
    Clip {
        mask: Box<DrawCmd>,
        content: Vec<DrawCmd>,
    },
}

/// Evaluated repeatable pattern tile.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawPattern {
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub tiles: Vec<DrawCmd>,
}

/// Individual path drawing sub-command.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawPathCommand {
    Start((f64, f64)),
    Line((f64, f64)),
    Quad { cp: (f64, f64), ep: (f64, f64) },
    Curve { c1: (f64, f64), c2: (f64, f64), ep: (f64, f64) },
    Arc {
        center: (f64, f64),
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    },
    Close,
}

/// The evaluated flat 2D scene graph ready for rendering or exporting.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawList {
    /// Canvas width in pixels.
    pub canvas_width: f64,
    /// Canvas height in pixels.
    pub canvas_height: f64,
    /// Optional canvas background color.
    pub background: Option<Color>,
    /// Pixel snap grid in px (0 = off).
    pub snap: f64,
    /// Pixel sampling hint for raster backends.
    pub pixel_filter: PixelFilter,
    /// Evaluated pattern tiles referenced by `Paint::Pattern`.
    pub patterns: Vec<DrawPattern>,
    /// Flat list of 2D draw commands.
    pub items: Vec<DrawCmd>,
}

impl DrawList {
    /// Returns the number of visual primitives in this draw list.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if the draw list has no geometric primitives.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Serializes this draw list directly into a standalone W3C SVG XML string.
    pub fn to_svg(&self) -> String {
        emit_svg(self)
    }
}