use crate::ast::Color;

#[derive(Debug, Clone, Copy)]
pub struct Transform2D {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Transform2D {
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

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

    pub fn transform_point(&self, p: (f64, f64)) -> (f64, f64) {
        (
            self.a * p.0 + self.c * p.1 + self.tx,
            self.b * p.0 + self.d * p.1 + self.ty,
        )
    }
}

#[derive(Debug, Clone)]
pub struct DrawStyle {
    pub fill: Color,
    pub stroke: Color,
    pub width: f64,
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

#[derive(Debug, Clone)]
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
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct DrawList {
    pub canvas_width: f64,
    pub canvas_height: f64,
    pub background: Option<Color>,
    pub items: Vec<DrawCmd>,
}