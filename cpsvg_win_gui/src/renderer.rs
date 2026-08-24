use crate::ast::Color as CpsvgColor;
use crate::draw_list::*;
use eframe::egui::{Color32, Painter, Pos2, Rect, Stroke};
use eframe::epaint::PathShape;

pub fn to_egui_color(col: &CpsvgColor, opacity: f64) -> Color32 {
    match col {
        CpsvgColor::Rgba(r, g, b, a) => {
            let final_a = ((*a as f64) * opacity).clamp(0.0, 255.0) as u8;
            Color32::from_rgba_unmultiplied(*r, *g, *b, final_a)
        }
        CpsvgColor::None => Color32::TRANSPARENT,
    }
}

pub fn render_draw_list(painter: &Painter, draw_list: &DrawList, origin: Pos2, zoom: f32) {
    let to_screen = |p: (f64, f64)| -> Pos2 {
        Pos2::new(
            origin.x + (p.0 as f32) * zoom,
            origin.y + (p.1 as f32) * zoom,
        )
    };

    // 1. Canvas Background
    if let Some(ref bg) = draw_list.background {
        let bg_color = to_egui_color(bg, 1.0);
        let canvas_rect = Rect::from_min_size(
            origin,
            eframe::egui::vec2(
                (draw_list.canvas_width as f32) * zoom,
                (draw_list.canvas_height as f32) * zoom,
            ),
        );
        painter.rect_filled(canvas_rect, 0.0, bg_color);
    }

    // 2. Render Primitives
    for cmd in &draw_list.items {
        match cmd {
            DrawCmd::Circle { center, radius, style } => {
                let c = to_screen(*center);
                let r = (*radius as f32) * zoom;
                let fill_c = to_egui_color(&style.fill, style.opacity);
                let stroke_c = to_egui_color(&style.stroke, style.opacity);
                let stroke = Stroke::new((style.width as f32) * zoom, stroke_c);
                painter.circle(c, r, fill_c, stroke);
            }
            DrawCmd::Ellipse { center, radius, style } => {
                let c = to_screen(*center);
                let rx = (radius.0 as f32) * zoom;
                let ry = (radius.1 as f32) * zoom;
                let fill_c = to_egui_color(&style.fill, style.opacity);
                let stroke_c = to_egui_color(&style.stroke, style.opacity);
                let stroke = Stroke::new((style.width as f32) * zoom, stroke_c);

                let mut pts = Vec::with_capacity(32);
                for i in 0..32 {
                    let theta = (i as f32 / 32.0) * std::f32::consts::TAU;
                    pts.push(Pos2::new(c.x + rx * theta.cos(), c.y + ry * theta.sin()));
                }
                if fill_c != Color32::TRANSPARENT {
                    painter.add(PathShape::convex_polygon(pts.clone(), fill_c, Stroke::NONE));
                }
                if stroke.width > 0.0 && stroke.color != Color32::TRANSPARENT {
                    painter.add(PathShape::closed_line(pts, stroke));
                }
            }
            DrawCmd::Rectangle { pos, size, corner_radius, style } => {
                let min = to_screen(*pos);
                let rect = Rect::from_min_size(
                    min,
                    eframe::egui::vec2((size.0 as f32) * zoom, (size.1 as f32) * zoom),
                );
                let cr = (*corner_radius as f32) * zoom;
                let fill_c = to_egui_color(&style.fill, style.opacity);
                let stroke_c = to_egui_color(&style.stroke, style.opacity);
                let stroke = Stroke::new((style.width as f32) * zoom, stroke_c);
                painter.rect(rect, cr, fill_c, stroke);
            }
            DrawCmd::Line { from, to, style } => {
                let p1 = to_screen(*from);
                let p2 = to_screen(*to);
                let stroke_c = to_egui_color(&style.stroke, style.opacity);
                let stroke = Stroke::new((style.width as f32) * zoom, stroke_c);
                painter.line_segment([p1, p2], stroke);
            }
            DrawCmd::Polygon { points, style } => {
                if points.is_empty() {
                    continue;
                }
                let screen_pts: Vec<Pos2> = points.iter().map(|p| to_screen(*p)).collect();
                let fill_c = to_egui_color(&style.fill, style.opacity);
                let stroke_c = to_egui_color(&style.stroke, style.opacity);
                let stroke = Stroke::new((style.width as f32) * zoom, stroke_c);

                if fill_c != Color32::TRANSPARENT {
                    painter.add(PathShape::convex_polygon(screen_pts.clone(), fill_c, Stroke::NONE));
                }
                if stroke.width > 0.0 && stroke.color != Color32::TRANSPARENT {
                    painter.add(PathShape::closed_line(screen_pts, stroke));
                }
            }
            DrawCmd::Path { commands, style } => {
                let fill_c = to_egui_color(&style.fill, style.opacity);
                let stroke_c = to_egui_color(&style.stroke, style.opacity);
                let stroke = Stroke::new((style.width as f32) * zoom, stroke_c);

                let mut current_pt = Pos2::ZERO;
                let mut poly_pts = Vec::new();
                let mut is_closed = false;

                for cmd in commands {
                    match cmd {
                        DrawPathCommand::Start(p) => {
                            current_pt = to_screen(*p);
                            poly_pts.push(current_pt);
                        }
                        DrawPathCommand::Line(p) => {
                            current_pt = to_screen(*p);
                            poly_pts.push(current_pt);
                        }
                        DrawPathCommand::Quad { cp, ep } => {
                            let p0 = current_pt;
                            let p1 = to_screen(*cp);
                            let p2 = to_screen(*ep);
                            for step in 1..=16 {
                                let t = step as f32 / 16.0;
                                let inv = 1.0 - t;
                                let x = inv * inv * p0.x + 2.0 * inv * t * p1.x + t * t * p2.x;
                                let y = inv * inv * p0.y + 2.0 * inv * t * p1.y + t * t * p2.y;
                                poly_pts.push(Pos2::new(x, y));
                            }
                            current_pt = p2;
                        }
                        DrawPathCommand::Curve { c1, c2, ep } => {
                            let p0 = current_pt;
                            let p1 = to_screen(*c1);
                            let p2 = to_screen(*c2);
                            let p3 = to_screen(*ep);
                            for step in 1..=20 {
                                let t = step as f32 / 20.0;
                                let inv = 1.0 - t;
                                let x = inv.powi(3) * p0.x
                                    + 3.0 * inv.powi(2) * t * p1.x
                                    + 3.0 * inv * t.powi(2) * p2.x
                                    + t.powi(3) * p3.x;
                                let y = inv.powi(3) * p0.y
                                    + 3.0 * inv.powi(2) * t * p1.y
                                    + 3.0 * inv * t.powi(2) * p2.y
                                    + t.powi(3) * p3.y;
                                poly_pts.push(Pos2::new(x, y));
                            }
                            current_pt = p3;
                        }
                        DrawPathCommand::Arc { center, radius, start_angle, end_angle } => {
                            let c = to_screen(*center);
                            let r = (*radius as f32) * zoom;
                            let steps = 32;
                            for step in 0..=steps {
                                let t = step as f64 / steps as f64;
                                let angle = start_angle + t * (end_angle - start_angle);
                                let x = c.x + r * (angle.cos() as f32);
                                let y = c.y + r * (angle.sin() as f32);
                                poly_pts.push(Pos2::new(x, y));
                            }
                            if let Some(last) = poly_pts.last() {
                                current_pt = *last;
                            }
                        }
                        DrawPathCommand::Close => {
                            is_closed = true;
                        }
                    }
                }

                if !poly_pts.is_empty() {
                    if is_closed && fill_c != Color32::TRANSPARENT {
                        painter.add(PathShape::convex_polygon(poly_pts.clone(), fill_c, Stroke::NONE));
                    }
                    if stroke.width > 0.0 && stroke.color != Color32::TRANSPARENT {
                        if is_closed {
                            painter.add(PathShape::closed_line(poly_pts, stroke));
                        } else {
                            painter.add(PathShape::line(poly_pts, stroke));
                        }
                    }
                }
            }
        }
    }
}