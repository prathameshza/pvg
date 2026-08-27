use pvg::ast::Color;
use pvg::draw_list::{DrawCmd, DrawList, DrawPathCommand};
use std::f64::consts::PI;
use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Rect, Stroke, Transform};

#[inline]
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

/// High-performance in-place vector rasterizer with single-pass clearing and aspect-ratio alignment
pub fn rasterize_draw_list_into_pixmap_mut(
    draw_list: &DrawList,
    pixmap: &mut PixmapMut,
    target_width: u32,
    target_height: u32,
) {
    if draw_list.canvas_width <= 0.0 || draw_list.canvas_height <= 0.0 {
        pixmap.fill(tiny_skia::Color::from_rgba8(8, 9, 13, 255));
        return;
    }

    // 1. Calculate aesthetic letterbox dimensions with 4% padding margin
    let margin = 0.94_f32;
    let avail_w = (target_width as f32) * margin;
    let avail_h = (target_height as f32) * margin;

    let scale_x = avail_w / draw_list.canvas_width as f32;
    let scale_y = avail_h / draw_list.canvas_height as f32;
    let scale = scale_x.min(scale_y);

    let scaled_w = (draw_list.canvas_width as f32 * scale).round();
    let scaled_h = (draw_list.canvas_height as f32 * scale).round();

    let offset_x = ((target_width as f32 - scaled_w) / 2.0).round();
    let offset_y = ((target_height as f32 - scaled_h) / 2.0).round();

    let transform = Transform::from_row(scale, 0.0, 0.0, scale, offset_x, offset_y);

    // 2. Single-pass background fill
    let container_bg = tiny_skia::Color::from_rgba8(8, 9, 13, 255);
    pixmap.fill(container_bg);

    if let Some(canvas_rect) = Rect::from_xywh(offset_x, offset_y, scaled_w, scaled_h) {
        if let Some(ref bg) = draw_list.background {
            if let Some(bg_paint) = color_to_skia(bg, 1.0) {
                pixmap.fill_rect(canvas_rect, &bg_paint, Transform::identity(), None);
            }
        } else {
            let mut black_paint = Paint::default();
            black_paint.set_color_rgba8(0, 0, 0, 255);
            pixmap.fill_rect(canvas_rect, &black_paint, Transform::identity(), None);
        }
    }

    // 3. Render visual primitives
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

            DrawCmd::Text { .. } => {}

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
                            let steps = (delta.abs() / (PI / 32.0)).ceil().max(16.0) as usize;
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

    // 4. Draw clean canvas border outline
    if let Some(canvas_rect) = Rect::from_xywh(offset_x, offset_y, scaled_w, scaled_h) {
        let mut border_paint = Paint::default();
        border_paint.set_color_rgba8(31, 35, 51, 255);
        let mut border_stroke = Stroke::default();
        border_stroke.width = 1.5;
        let mut border_pb = PathBuilder::new();
        border_pb.push_rect(canvas_rect);
        if let Some(border_path) = border_pb.finish() {
            pixmap.stroke_path(&border_path, &border_paint, &border_stroke, Transform::identity(), None);
        }
    }
}