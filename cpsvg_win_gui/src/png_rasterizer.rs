use crate::ast::Color;
use crate::compile_pvg_at_time;
use crate::draw_list::*;
use std::path::Path;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

/// Converts a PVG Color and opacity multiplier into a tiny-skia Paint
pub fn color_to_skia(col: &Color, opacity: f64) -> Option<Paint<'static>> {
    match col {
        Color::Rgba(r, g, b, a) => {
            let final_a = ((*a as f64) * opacity).clamp(0.0, 255.0).round() as u8;
            if final_a == 0 {
                return None;
            }
            let mut paint = Paint::default();
            paint.set_color_rgba8(*r, *g, *b, final_a);
            paint.anti_alias = true;
            Some(paint)
        }
        Color::None => None,
    }
}

/// Rasterizes a PVG DrawList into a tiny-skia Pixmap at a given resolution scale factor (e.g. 1.0, 2.0)
pub fn rasterize_draw_list(draw_list: &DrawList, scale: f32) -> Result<Pixmap, String> {
    let scale = if scale <= 0.0 { 1.0 } else { scale };
    let width = ((draw_list.canvas_width as f32) * scale).round() as u32;
    let height = ((draw_list.canvas_height as f32) * scale).round() as u32;

    if width == 0 || height == 0 {
        return Err("Canvas dimensions must be greater than zero".to_string());
    }

    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| format!("Failed to allocate Pixmap buffer of size {}x{}", width, height))?;

    let transform = Transform::from_scale(scale, scale);

    // 1. Canvas Background
    if let Some(ref bg) = draw_list.background {
        if let Some(bg_paint) = color_to_skia(bg, 1.0) {
            if let Some(rect) = Rect::from_xywh(0.0, 0.0, width as f32, height as f32) {
                pixmap.fill_rect(rect, &bg_paint, Transform::identity(), None);
            }
        }
    }

    // 2. Render DrawList Primitives
    for cmd in &draw_list.items {
        match cmd {
            DrawCmd::Circle { center, radius, style } => {
                let mut pb = PathBuilder::new();
                pb.push_circle(center.0 as f32, center.1 as f32, *radius as f32);
                if let Some(path) = pb.finish() {
                    if let Some(fill_paint) = color_to_skia(&style.fill, style.opacity) {
                        pixmap.fill_path(&path, &fill_paint, FillRule::Winding, transform, None);
                    }
                    if let Some(stroke_paint) = color_to_skia(&style.stroke, style.opacity) {
                        if style.width > 0.0 {
                            let mut stroke = Stroke::default();
                            stroke.width = style.width as f32;
                            pixmap.stroke_path(&path, &stroke_paint, &stroke, transform, None);
                        }
                    }
                }
            }

            DrawCmd::Ellipse { center, radius, style } => {
                let x = (center.0 - radius.0) as f32;
                let y = (center.1 - radius.1) as f32;
                let w = (radius.0 * 2.0) as f32;
                let h = (radius.1 * 2.0) as f32;

                if let Some(rect) = Rect::from_xywh(x, y, w, h) {
                    let mut pb = PathBuilder::new();
                    pb.push_oval(rect);
                    if let Some(path) = pb.finish() {
                        if let Some(fill_paint) = color_to_skia(&style.fill, style.opacity) {
                            pixmap.fill_path(&path, &fill_paint, FillRule::Winding, transform, None);
                        }
                        if let Some(stroke_paint) = color_to_skia(&style.stroke, style.opacity) {
                            if style.width > 0.0 {
                                let mut stroke = Stroke::default();
                                stroke.width = style.width as f32;
                                pixmap.stroke_path(&path, &stroke_paint, &stroke, transform, None);
                            }
                        }
                    }
                }
            }

            DrawCmd::Rectangle { pos, size, corner_radius, style } => {
                let x = pos.0 as f32;
                let y = pos.1 as f32;
                let w = size.0 as f32;
                let h = size.1 as f32;
                let cr = (*corner_radius as f32).max(0.0);

                if w > 0.0 && h > 0.0 {
                    let path = if cr > 0.0 {
                        let r = cr.min(w / 2.0).min(h / 2.0);
                        let mut pb = PathBuilder::new();
                        pb.move_to(x + r, y);
                        pb.line_to(x + w - r, y);
                        pb.quad_to(x + w, y, x + w, y + r);
                        pb.line_to(x + w, y + h - r);
                        pb.quad_to(x + w, y + h, x + w - r, y + h);
                        pb.line_to(x + r, y + h);
                        pb.quad_to(x, y + h, x, y + h - r);
                        pb.line_to(x, y + r);
                        pb.quad_to(x, y, x + r, y);
                        pb.close();
                        pb.finish()
                    } else if let Some(rect) = Rect::from_xywh(x, y, w, h) {
                        let mut pb = PathBuilder::new();
                        pb.push_rect(rect);
                        pb.finish()
                    } else {
                        None
                    };

                    if let Some(path) = path {
                        if let Some(fill_paint) = color_to_skia(&style.fill, style.opacity) {
                            pixmap.fill_path(&path, &fill_paint, FillRule::Winding, transform, None);
                        }
                        if let Some(stroke_paint) = color_to_skia(&style.stroke, style.opacity) {
                            if style.width > 0.0 {
                                let mut stroke = Stroke::default();
                                stroke.width = style.width as f32;
                                pixmap.stroke_path(&path, &stroke_paint, &stroke, transform, None);
                            }
                        }
                    }
                }
            }

            DrawCmd::Line { from, to, style } => {
                let mut pb = PathBuilder::new();
                pb.move_to(from.0 as f32, from.1 as f32);
                pb.line_to(to.0 as f32, to.1 as f32);
                if let Some(path) = pb.finish() {
                    if let Some(stroke_paint) = color_to_skia(&style.stroke, style.opacity) {
                        if style.width > 0.0 {
                            let mut stroke = Stroke::default();
                            stroke.width = style.width as f32;
                            pixmap.stroke_path(&path, &stroke_paint, &stroke, transform, None);
                        }
                    }
                }
            }

            DrawCmd::Polygon { points, style } => {
                if points.len() >= 2 {
                    let mut pb = PathBuilder::new();
                    pb.move_to(points[0].0 as f32, points[0].1 as f32);
                    for pt in &points[1..] {
                        pb.line_to(pt.0 as f32, pt.1 as f32);
                    }
                    pb.close();

                    if let Some(path) = pb.finish() {
                        if let Some(fill_paint) = color_to_skia(&style.fill, style.opacity) {
                            pixmap.fill_path(&path, &fill_paint, FillRule::Winding, transform, None);
                        }
                        if let Some(stroke_paint) = color_to_skia(&style.stroke, style.opacity) {
                            if style.width > 0.0 {
                                let mut stroke = Stroke::default();
                                stroke.width = style.width as f32;
                                pixmap.stroke_path(&path, &stroke_paint, &stroke, transform, None);
                            }
                        }
                    }
                }
            }

            DrawCmd::Path { commands, style } => {
                let mut pb = PathBuilder::new();
                let mut has_commands = false;

                for cmd in commands {
                    match cmd {
                        DrawPathCommand::Start(p) => {
                            pb.move_to(p.0 as f32, p.1 as f32);
                            has_commands = true;
                        }
                        DrawPathCommand::Line(p) => {
                            if !has_commands {
                                pb.move_to(p.0 as f32, p.1 as f32);
                                has_commands = true;
                            } else {
                                pb.line_to(p.0 as f32, p.1 as f32);
                            }
                        }
                        DrawPathCommand::Quad { cp, ep } => {
                            if !has_commands {
                                pb.move_to(cp.0 as f32, cp.1 as f32);
                                has_commands = true;
                            }
                            pb.quad_to(cp.0 as f32, cp.1 as f32, ep.0 as f32, ep.1 as f32);
                        }
                        DrawPathCommand::Curve { c1, c2, ep } => {
                            if !has_commands {
                                pb.move_to(c1.0 as f32, c1.1 as f32);
                                has_commands = true;
                            }
                            pb.cubic_to(
                                c1.0 as f32, c1.1 as f32,
                                c2.0 as f32, c2.1 as f32,
                                ep.0 as f32, ep.1 as f32,
                            );
                        }
                        DrawPathCommand::Arc { center, radius, start_angle, end_angle } => {
                            let delta = end_angle - start_angle;
                            let steps = (delta.abs() / (std::f64::consts::PI / 32.0)).ceil().max(16.0) as usize;
                            for step in 0..=steps {
                                let t = step as f64 / steps as f64;
                                let angle = start_angle + t * delta;
                                let px = center.0 + radius * angle.cos();
                                let py = center.1 + radius * angle.sin();
                                if !has_commands && step == 0 {
                                    pb.move_to(px as f32, py as f32);
                                    has_commands = true;
                                } else {
                                    pb.line_to(px as f32, py as f32);
                                }
                            }
                        }
                        DrawPathCommand::Close => {
                            pb.close();
                        }
                    }
                }

                if let Some(path) = pb.finish() {
                    if let Some(fill_paint) = color_to_skia(&style.fill, style.opacity) {
                        pixmap.fill_path(&path, &fill_paint, FillRule::Winding, transform, None);
                    }
                    if let Some(stroke_paint) = color_to_skia(&style.stroke, style.opacity) {
                        if style.width > 0.0 {
                            let mut stroke = Stroke::default();
                            stroke.width = style.width as f32;
                            pixmap.stroke_path(&path, &stroke_paint, &stroke, transform, None);
                        }
                    }
                }
            }
        }
    }

    Ok(pixmap)
}

/// Rasterizes a DrawList into an encoded PNG byte vector
pub fn rasterize_draw_list_to_png(draw_list: &DrawList, scale: f32) -> Result<Vec<u8>, String> {
    let pixmap = rasterize_draw_list(draw_list, scale)?;
    pixmap.encode_png().map_err(|e| format!("PNG encoding failed: {}", e))
}

/// Compiles PVG source text at a given timestamp and encodes it to PNG bytes
pub fn rasterize_pvg_to_png(source: &str, time: f64, scale: f32) -> Result<Vec<u8>, String> {
    let dl = compile_pvg_at_time(source, time)?;
    rasterize_draw_list_to_png(&dl, scale)
}

/// Compiles PVG source text and writes the PNG directly to a file path
pub fn save_pvg_to_png<P: AsRef<Path>>(source: &str, time: f64, scale: f32, path: P) -> Result<(), String> {
    let bytes = rasterize_pvg_to_png(source, time, scale)?;
    std::fs::write(path.as_ref(), bytes)
        .map_err(|e| format!("Failed to save PNG to '{}': {}", path.as_ref().display(), e))
}