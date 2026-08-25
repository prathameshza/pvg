use pvg::ast::Color;
use pvg::draw_list::{DrawCmd, DrawList, DrawPathCommand, DrawStyle};
use std::f64::consts::PI;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

pub fn color_to_svg(col: &Color) -> String {
    match col {
        Color::Rgba(r, g, b, 255) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        Color::Rgba(r, g, b, a) => {
            format!("rgba({}, {}, {}, {:.3})", r, g, b, *a as f64 / 255.0)
        }
        Color::None => "none".to_string(),
    }
}

pub fn format_svg_attributes(style: &DrawStyle) -> String {
    let mut attrs = Vec::new();
    attrs.push(format!("fill=\"{}\"", color_to_svg(&style.fill)));

    if style.stroke != Color::None {
        attrs.push(format!("stroke=\"{}\"", color_to_svg(&style.stroke)));
        attrs.push(format!("stroke-width=\"{:.2}\"", style.width));
    } else {
        attrs.push("stroke=\"none\"".to_string());
    }

    if (style.opacity - 1.0).abs() > 0.001 {
        attrs.push(format!("opacity=\"{:.3}\"", style.opacity));
    }

    attrs.join(" ")
}

pub fn emit_svg(draw_list: &DrawList) -> String {
    let mut out = String::with_capacity(1024 * 4);
    out.push_str(&format!(
        "<svg width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\">\n",
        draw_list.canvas_width, draw_list.canvas_height, draw_list.canvas_width, draw_list.canvas_height
    ));

    if let Some(ref bg) = draw_list.background {
        out.push_str(&format!(
            "  <rect width=\"100%\" height=\"100%\" fill=\"{}\" />\n",
            color_to_svg(bg)
        ));
    }

    for cmd in &draw_list.items {
        match cmd {
            DrawCmd::Circle { center, radius, style } => {
                out.push_str(&format!(
                    "  <circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" {} />\n",
                    center.0, center.1, radius, format_svg_attributes(style)
                ));
            }
            DrawCmd::Ellipse { center, radius, style } => {
                out.push_str(&format!(
                    "  <ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" {} />\n",
                    center.0, center.1, radius.0, radius.1, format_svg_attributes(style)
                ));
            }
            DrawCmd::Rectangle { pos, size, corner_radius, style } => {
                if *corner_radius > 0.0 {
                    out.push_str(&format!(
                        "  <rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" {} />\n",
                        pos.0, pos.1, size.0, size.1, corner_radius, corner_radius, format_svg_attributes(style)
                    ));
                } else {
                    out.push_str(&format!(
                        "  <rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" {} />\n",
                        pos.0, pos.1, size.0, size.1, format_svg_attributes(style)
                    ));
                }
            }
            DrawCmd::Line { from, to, style } => {
                out.push_str(&format!(
                    "  <line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" {} />\n",
                    from.0, from.1, to.0, to.1, format_svg_attributes(style)
                ));
            }
            DrawCmd::Polygon { points, style } => {
                if points.is_empty() {
                    continue;
                }
                let pts_str: Vec<String> = points
                    .iter()
                    .map(|p| format!("{:.2},{:.2}", p.0, p.1))
                    .collect();
                out.push_str(&format!(
                    "  <polygon points=\"{}\" {} />\n",
                    pts_str.join(" "),
                    format_svg_attributes(style)
                ));
            }
            DrawCmd::Path { commands, style } => {
                let mut d = Vec::new();
                for c in commands {
                    match c {
                        DrawPathCommand::Start(p) => d.push(format!("M {:.2} {:.2}", p.0, p.1)),
                        DrawPathCommand::Line(p) => d.push(format!("L {:.2} {:.2}", p.0, p.1)),
                        DrawPathCommand::Quad { cp, ep } => d.push(format!("Q {:.2} {:.2}, {:.2} {:.2}", cp.0, cp.1, ep.0, ep.1)),
                        DrawPathCommand::Curve { c1, c2, ep } => d.push(format!("C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2}", c1.0, c1.1, c2.0, c2.1, ep.0, ep.1)),
                        DrawPathCommand::Arc { center, radius, start_angle, end_angle } => {
                            let delta = end_angle - start_angle;
                            let end_x = center.0 + radius * end_angle.cos();
                            let end_y = center.1 + radius * end_angle.sin();
                            let sweep = if delta > 0.0 { 1 } else { 0 };
                            let large_arc = if delta.abs() > PI { 1 } else { 0 };
                            d.push(format!("A {:.2} {:.2} 0 {} {} {:.2} {:.2}", radius, radius, large_arc, sweep, end_x, end_y));
                        }
                        DrawPathCommand::Close => d.push("Z".into()),
                    }
                }
                out.push_str(&format!("  <path d=\"{}\" {} />\n", d.join(" "), format_svg_attributes(style)));
            }
        }
    }

    out.push_str("</svg>\n");
    out
}

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

pub fn rasterize_skia(draw_list: &DrawList, scale: f32) -> Result<Pixmap, String> {
    let scale = if scale <= 0.0 { 1.0 } else { scale };
    let width = ((draw_list.canvas_width as f32) * scale).round() as u32;
    let height = ((draw_list.canvas_height as f32) * scale).round() as u32;

    if width == 0 || height == 0 {
        return Err("Canvas dimensions must be greater than 0".into());
    }

    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| format!("Failed to allocate Pixmap {}x{}", width, height))?;
    let transform = Transform::from_scale(scale, scale);

    if let Some(ref bg) = draw_list.background {
        if let Some(bg_paint) = color_to_skia(bg, 1.0) {
            if let Some(rect) = Rect::from_xywh(0.0, 0.0, width as f32, height as f32) {
                pixmap.fill_rect(rect, &bg_paint, Transform::identity(), None);
            }
        }
    }

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
                    } else {
                        Rect::from_xywh(x, y, w, h).and_then(|r| {
                            let mut pb = PathBuilder::new();
                            pb.push_rect(r);
                            pb.finish()
                        })
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
                    for p in &points[1..] {
                        pb.line_to(p.0 as f32, p.1 as f32);
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
                for c in commands {
                    match c {
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
                            pb.cubic_to(c1.0 as f32, c1.1 as f32, c2.0 as f32, c2.1 as f32, ep.0 as f32, ep.0 as f32);
                        }
                        DrawPathCommand::Arc { center, radius, start_angle, end_angle } => {
                            let delta = end_angle - start_angle;
                            let steps = 16;
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