use crate::ast::Color;
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

/// Styling attributes applied to geometric and text primitives.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawStyle {
    /// Fill color.
    pub fill: Color,
    /// Stroke color.
    pub stroke: Color,
    /// Stroke width in pixels.
    pub width: f64,
    /// Multiplicative opacity in [0.0, 1.0].
    pub opacity: f64,
}

impl Default for DrawStyle {
    fn default() -> Self {
        Self {
            fill: Color::BLACK,
            stroke: Color::None,
            width: 1.0,
            opacity: 1.0,
        }
    }
}

impl DrawStyle {
    pub fn with_fill(mut self, fill: Color) -> Self {
        self.fill = fill;
        self
    }
    pub fn with_stroke(mut self, stroke: Color) -> Self {
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