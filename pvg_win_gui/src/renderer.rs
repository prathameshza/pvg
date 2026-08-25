use eframe::egui::{Color32, Painter, Pos2, Rect, Stroke};
use eframe::epaint::PathShape;
use pvg::ast::Color as PvgColor;
use pvg::draw_list::{DrawCmd, DrawList, DrawPathCommand, DrawStyle};
use std::f32::consts::PI as PI_F32;
use std::f64::consts::PI as PI_F64;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect as SkiaRect, Stroke as SkiaStroke, Transform};

#[inline]
pub fn to_egui_color(col: &PvgColor, opacity: f64) -> Color32 {
    match col {
        PvgColor::Rgba(r, g, b, a) => {
            let final_a = ((*a as f64) * opacity).clamp(0.0, 255.0) as u8;
            Color32::from_rgba_unmultiplied(*r, *g, *b, final_a)
        }
        PvgColor::None => Color32::TRANSPARENT,
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

    // 2. Render Primitives with Screen-Space Adaptive Resolution
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

                // Ramanujan ellipse circumference approximation for adaptive step count
                let perimeter_est = PI_F32 * (3.0 * (rx + ry) - ((3.0 * rx + ry) * (rx + 3.0 * ry)).sqrt());
                let steps = ((perimeter_est / 2.0).ceil().clamp(64.0, 512.0)) as usize;

                let mut pts = Vec::with_capacity(steps);
                for i in 0..steps {
                    let theta = (i as f32 / steps as f32) * std::f32::consts::TAU;
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
                if points.len() < 2 {
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

                struct SubPath {
                    pts: Vec<Pos2>,
                    closed: bool,
                }

                let mut subpaths: Vec<SubPath> = Vec::new();
                let mut current_subpath = SubPath {
                    pts: Vec::new(),
                    closed: false,
                };
                let mut current_pt = Pos2::ZERO;

                for cmd in commands {
                    match cmd {
                        DrawPathCommand::Start(p) => {
                            if !current_subpath.pts.is_empty() {
                                subpaths.push(current_subpath);
                                current_subpath = SubPath {
                                    pts: Vec::new(),
                                    closed: false,
                                };
                            }
                            current_pt = to_screen(*p);
                            current_subpath.pts.push(current_pt);
                        }

                        DrawPathCommand::Line(p) => {
                            if current_subpath.pts.is_empty() {
                                current_subpath.pts.push(current_pt);
                            }
                            current_pt = to_screen(*p);
                            current_subpath.pts.push(current_pt);
                        }

                        DrawPathCommand::Quad { cp, ep } => {
                            let p0 = current_pt;
                            let p1 = to_screen(*cp);
                            let p2 = to_screen(*ep);

                            if current_subpath.pts.is_empty() {
                                current_subpath.pts.push(p0);
                            }

                            // Adaptive subdivision based on screen-space chord length
                            let chord_len = (p1.x - p0.x).hypot(p1.y - p0.y) + (p2.x - p1.x).hypot(p2.y - p1.y);
                            let steps = ((chord_len / 1.5).ceil().clamp(32.0, 512.0)) as usize;

                            for step in 1..=steps {
                                let t = step as f32 / steps as f32;
                                let inv = 1.0 - t;
                                let x = inv * inv * p0.x + 2.0 * inv * t * p1.x + t * t * p2.x;
                                let y = inv * inv * p0.y + 2.0 * inv * t * p1.y + t * t * p2.y;
                                current_subpath.pts.push(Pos2::new(x, y));
                            }
                            current_pt = p2;
                        }

                        DrawPathCommand::Curve { c1, c2, ep } => {
                            let p0 = current_pt;
                            let p1 = to_screen(*c1);
                            let p2 = to_screen(*c2);
                            let p3 = to_screen(*ep);

                            if current_subpath.pts.is_empty() {
                                current_subpath.pts.push(p0);
                            }

                            // Adaptive subdivision based on screen-space control polygon length
                            let chord_len = (p1.x - p0.x).hypot(p1.y - p0.y)
                                + (p2.x - p1.x).hypot(p2.y - p1.y)
                                + (p3.x - p2.x).hypot(p3.y - p2.y);
                            let steps = ((chord_len / 1.5).ceil().clamp(48.0, 768.0)) as usize;

                            for step in 1..=steps {
                                let t = step as f32 / steps as f32;
                                let inv = 1.0 - t;
                                let inv2 = inv * inv;
                                let inv3 = inv2 * inv;
                                let t2 = t * t;
                                let t3 = t2 * t;
                                let x = inv3 * p0.x + 3.0 * inv2 * t * p1.x + 3.0 * inv * t2 * p2.x + t3 * p3.x;
                                let y = inv3 * p0.y + 3.0 * inv2 * t * p1.y + 3.0 * inv * t2 * p2.y + t3 * p3.y;
                                current_subpath.pts.push(Pos2::new(x, y));
                            }
                            current_pt = p3;
                        }

                        DrawPathCommand::Arc { center, radius, start_angle, end_angle } => {
                            let c = to_screen(*center);
                            let r = (*radius as f32) * zoom;
                            let delta = (*end_angle - *start_angle) as f32;
                            let arc_len = r * delta.abs();
                            let steps = ((arc_len / 1.5).ceil().clamp(32.0, 512.0)) as usize;

                            for step in 0..=steps {
                                let t = step as f64 / steps as f64;
                                let angle = *start_angle + t * (*end_angle - *start_angle);
                                let x = c.x + r * (angle.cos() as f32);
                                let y = c.y + r * (angle.sin() as f32);
                                current_subpath.pts.push(Pos2::new(x, y));
                            }
                            if let Some(last) = current_subpath.pts.last() {
                                current_pt = *last;
                            }
                        }

                        DrawPathCommand::Close => {
                            current_subpath.closed = true;
                        }
                    }
                }

                if !current_subpath.pts.is_empty() {
                    subpaths.push(current_subpath);
                }

                // Render filled and stroked subpaths
                for sp in &subpaths {
                    if sp.pts.len() < 2 {
                        continue;
                    }
                    if sp.closed && fill_c != Color32::TRANSPARENT {
                        painter.add(PathShape::convex_polygon(sp.pts.clone(), fill_c, Stroke::NONE));
                    }
                    if stroke.width > 0.0 && stroke.color != Color32::TRANSPARENT {
                        if sp.closed {
                            painter.add(PathShape::closed_line(sp.pts.clone(), stroke));
                        } else {
                            painter.add(PathShape::line(sp.pts.clone(), stroke));
                        }
                    }
                }
            }
        }
    }
}

pub fn export_svg(draw_list: &DrawList) -> String {
    let mut out = String::with_capacity(1024 * 4);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<svg width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\">\n",
        draw_list.canvas_width, draw_list.canvas_height, draw_list.canvas_width, draw_list.canvas_height
    ));

    let format_color = |c: &PvgColor| -> String {
        match c {
            PvgColor::Rgba(r, g, b, 255) => format!("#{:02x}{:02x}{:02x}", r, g, b),
            PvgColor::Rgba(r, g, b, a) => format!("rgba({}, {}, {}, {:.3})", r, g, b, *a as f64 / 255.0),
            PvgColor::None => "none".to_string(),
        }
    };

    let format_style = |s: &DrawStyle| -> String {
        let mut attrs = Vec::new();
        attrs.push(format!("fill=\"{}\"", format_color(&s.fill)));
        if s.stroke != PvgColor::None && s.width > 0.0 {
            attrs.push(format!("stroke=\"{}\"", format_color(&s.stroke)));
            attrs.push(format!("stroke-width=\"{:.2}\"", s.width));
        } else {
            attrs.push("stroke=\"none\"".to_string());
        }
        if (s.opacity - 1.0).abs() > 0.001 {
            attrs.push(format!("opacity=\"{:.3}\"", s.opacity));
        }
        attrs.join(" ")
    };

    if let Some(ref bg) = draw_list.background {
        out.push_str(&format!(
            "  <rect width=\"100%\" height=\"100%\" fill=\"{}\" />\n",
            format_color(bg)
        ));
    }

    for cmd in &draw_list.items {
        match cmd {
            DrawCmd::Circle { center, radius, style } => {
                out.push_str(&format!(
                    "  <circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" {} />\n",
                    center.0, center.1, radius, format_style(style)
                ));
            }
            DrawCmd::Ellipse { center, radius, style } => {
                out.push_str(&format!(
                    "  <ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" {} />\n",
                    center.0, center.1, radius.0, radius.1, format_style(style)
                ));
            }
            DrawCmd::Rectangle { pos, size, corner_radius, style } => {
                if *corner_radius > 0.0 {
                    out.push_str(&format!(
                        "  <rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" {} />\n",
                        pos.0, pos.1, size.0, size.1, corner_radius, corner_radius, format_style(style)
                    ));
                } else {
                    out.push_str(&format!(
                        "  <rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" {} />\n",
                        pos.0, pos.1, size.0, size.1, format_style(style)
                    ));
                }
            }
            DrawCmd::Line { from, to, style } => {
                out.push_str(&format!(
                    "  <line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" {} />\n",
                    from.0, from.1, to.0, to.1, format_style(style)
                ));
            }
            DrawCmd::Polygon { points, style } => {
                if !points.is_empty() {
                    let pts: Vec<String> = points.iter().map(|p| format!("{:.2},{:.2}", p.0, p.1)).collect();
                    out.push_str(&format!(
                        "  <polygon points=\"{}\" {} />\n",
                        pts.join(" "),
                        format_style(style)
                    ));
                }
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
                            let large_arc = if delta.abs() > PI_F64 { 1 } else { 0 };
                            d.push(format!("A {:.2} {:.2} 0 {} {} {:.2} {:.2}", radius, radius, large_arc, sweep, end_x, end_y));
                        }
                        DrawPathCommand::Close => d.push("Z".into()),
                    }
                }
                out.push_str(&format!("  <path d=\"{}\" {} />\n", d.join(" "), format_style(style)));
            }
        }
    }

    out.push_str("</svg>\n");
    out
}

fn color_to_skia(col: &PvgColor, opacity: f64) -> Option<Paint<'static>> {
    match col {
        PvgColor::Rgba(r, g, b, a) => {
            let final_a = ((*a as f64) * opacity).clamp(0.0, 255.0).round() as u8;
            if final_a == 0 {
                return None;
            }
            let mut paint = Paint::default();
            paint.set_color_rgba8(*r, *g, *b, final_a);
            paint.anti_alias = true;
            Some(paint)
        }
        PvgColor::None => None,
    }
}

pub fn rasterize_png(draw_list: &DrawList, scale: f32) -> Result<Vec<u8>, String> {
    let scale = if scale <= 0.0 { 1.0 } else { scale };
    let width = ((draw_list.canvas_width as f32) * scale).round() as u32;
    let height = ((draw_list.canvas_height as f32) * scale).round() as u32;

    if width == 0 || height == 0 {
        return Err("Canvas dimensions must be greater than 0".into());
    }

    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| format!("Failed to allocate Pixmap of size {}x{}", width, height))?;
    let transform = Transform::from_scale(scale, scale);

    if let Some(ref bg) = draw_list.background {
        if let Some(bg_paint) = color_to_skia(bg, 1.0) {
            if let Some(rect) = SkiaRect::from_xywh(0.0, 0.0, width as f32, height as f32) {
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
                            let mut stroke = SkiaStroke::default();
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
                if let Some(rect) = SkiaRect::from_xywh(x, y, w, h) {
                    let mut pb = PathBuilder::new();
                    pb.push_oval(rect);
                    if let Some(path) = pb.finish() {
                        if let Some(fill_paint) = color_to_skia(&style.fill, style.opacity) {
                            pixmap.fill_path(&path, &fill_paint, FillRule::Winding, transform, None);
                        }
                        if let Some(stroke_paint) = color_to_skia(&style.stroke, style.opacity) {
                            if style.width > 0.0 {
                                let mut stroke = SkiaStroke::default();
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
                        SkiaRect::from_xywh(x, y, w, h).and_then(|r| {
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
                                let mut stroke = SkiaStroke::default();
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
                            let mut stroke = SkiaStroke::default();
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
                                let mut stroke = SkiaStroke::default();
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
                            pb.cubic_to(c1.0 as f32, c1.1 as f32, c2.0 as f32, c2.1 as f32, ep.0 as f32, ep.1 as f32);
                        }
                        DrawPathCommand::Arc { center, radius, start_angle, end_angle } => {
                            let delta = end_angle - start_angle;
                            let steps = ((delta.abs() / (PI_F64 / 16.0)).ceil().max(8.0)) as usize;
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
                            let mut stroke = SkiaStroke::default();
                            stroke.width = style.width as f32;
                            pixmap.stroke_path(&path, &stroke_paint, &stroke, transform, None);
                        }
                    }
                }
            }
        }
    }

    pixmap.encode_png().map_err(|e| format!("PNG encode error: {}", e))
}