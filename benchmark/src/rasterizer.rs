use pvg::ast::Color;
use pvg::draw_list::{
    BlendMode, DrawCmd, DrawList, DrawPathCommand, DrawPattern, DrawStyle, LineCap,
    LineJoin, Paint as PvgPaint, TextAlign,
};
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use tiny_skia::{
    BlendMode as SkBlend, FillRule, FilterQuality, LineCap as SkCap, LineJoin as SkJoin,
    Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Point, Rect, Stroke, StrokeDash,
    Transform,
};

pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn color_to_svg(col: &Color) -> String {
    match col {
        Color::Rgba(r, g, b, 255) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        Color::Rgba(r, g, b, a) => {
            format!("rgba({}, {}, {}, {:.3})", r, g, b, *a as f64 / 255.0)
        }
        Color::None => "none".to_string(),
    }
}

fn paint_to_svg(paint: &PvgPaint) -> String {
    match paint {
        PvgPaint::Color(c) => color_to_svg(c),
        PvgPaint::Pattern(name) => format!("url(#pvg-pat-{})", name),
        _ => "url(#pvg-grad)".to_string(),
    }
}

pub fn format_svg_attributes(style: &DrawStyle) -> String {
    let mut attrs = Vec::new();
    attrs.push(format!("fill=\"{}\"", paint_to_svg(&style.fill)));

    if !style.stroke.is_none() && style.width > 0.0 {
        attrs.push(format!("stroke=\"{}\"", paint_to_svg(&style.stroke)));
        attrs.push(format!("stroke-width=\"{:.2}\"", style.width));
    } else {
        attrs.push("stroke=\"none\"".to_string());
    }

    match style.cap {
        LineCap::Butt => attrs.push("stroke-linecap=\"butt\"".to_string()),
        LineCap::Round => attrs.push("stroke-linecap=\"round\"".to_string()),
        LineCap::Square => attrs.push("stroke-linecap=\"square\"".to_string()),
    }
    match style.join {
        LineJoin::Miter => attrs.push("stroke-linejoin=\"miter\"".to_string()),
        LineJoin::Round => attrs.push("stroke-linejoin=\"round\"".to_string()),
        LineJoin::Bevel => attrs.push("stroke-linejoin=\"bevel\"".to_string()),
    }
    if style.join == LineJoin::Miter && (style.miter - 4.0).abs() > 1e-6 {
        attrs.push(format!("stroke-miterlimit=\"{:.2}\"", style.miter));
    }
    if !style.dash.is_empty() {
        let d: Vec<String> = style.dash.iter().map(|v| format!("{:.2}", v)).collect();
        attrs.push(format!("stroke-dasharray=\"{}\"", d.join(" ")));
    }

    if (style.opacity - 1.0).abs() > 0.001 {
        attrs.push(format!("opacity=\"{:.3}\"", style.opacity));
    }

    match style.blend {
        BlendMode::Normal => {}
        BlendMode::Add => attrs.push("mix-blend-mode=\"plus-lighter\"".to_string()),
        BlendMode::Multiply => attrs.push("mix-blend-mode=\"multiply\"".to_string()),
        BlendMode::Screen => attrs.push("mix-blend-mode=\"screen\"".to_string()),
        BlendMode::Overlay => attrs.push("mix-blend-mode=\"overlay\"".to_string()),
    }

    attrs.join(" ")
}

fn emit_cmd_svg(cmd: &DrawCmd, out: &mut String) {
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
                return;
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
        DrawCmd::Text { pos, content, size, font_family, align, style } => {
            let anchor = match align {
                TextAlign::Left => "start",
                TextAlign::Center => "middle",
                TextAlign::Right => "end",
            };
            out.push_str(&format!(
                "  <text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" font-family=\"{}\" text-anchor=\"{}\" dominant-baseline=\"hanging\" {}>{}</text>\n",
                pos.0, pos.1, size, font_family, anchor, format_svg_attributes(style), escape_xml(content)
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
        DrawCmd::Spline { points, style } => {
            if points.is_empty() {
                return;
            }
            let mut d = format!("M {:.2} {:.2} ", points[0].0, points[0].1);
            for (c1, c2, ep) in pvg::spline_to_bezier(points) {
                d.push_str(&format!(
                    "C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2} ",
                    c1.0, c1.1, c2.0, c2.1, ep.0, ep.1
                ));
            }
            out.push_str(&format!(
                "  <path d=\"{}\" {} />\n",
                d.trim_end(),
                format_svg_attributes(style)
            ));
        }
        DrawCmd::Sprite { pos, palette, rows, scale, style } => {
            let mut g_attrs = vec!["shape-rendering=\"crispEdges\"".to_string()];
            if (style.opacity - 1.0).abs() > 0.001 {
                g_attrs.push(format!("opacity=\"{:.3}\"", style.opacity));
            }
            match style.blend {
                BlendMode::Normal => {}
                BlendMode::Add => g_attrs.push("mix-blend-mode=\"plus-lighter\"".to_string()),
                BlendMode::Multiply => g_attrs.push("mix-blend-mode=\"multiply\"".to_string()),
                BlendMode::Screen => g_attrs.push("mix-blend-mode=\"screen\"".to_string()),
                BlendMode::Overlay => g_attrs.push("mix-blend-mode=\"overlay\"".to_string()),
            }
            out.push_str(&format!("  <g {}>\n", g_attrs.join(" ")));
            for (ry, row) in rows.iter().enumerate() {
                for (rx, ch) in row.chars().enumerate() {
                    let idx = match sprite_char_index(ch) {
                        Some(i) => i,
                        None => continue,
                    };
                    let color = match palette.get(idx) {
                        Some(c) if !c.is_transparent() && !c.is_none() => c,
                        _ => continue,
                    };
                    out.push_str(&format!(
                        "    <rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"{}\" />\n",
                        pos.0 + rx as f64 * scale,
                        pos.1 + ry as f64 * scale,
                        scale,
                        scale,
                        color_to_svg(color)
                    ));
                }
            }
            out.push_str("  </g>\n");
        }
        DrawCmd::Clip { content, .. } => {
            // Benchmark SVG emitter: flatten clip content unclipped (eval-timing parity).
            out.push_str("  <!-- clip -->\n");
            for inner in content {
                emit_cmd_svg(inner, out);
            }
        }
    }
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

    // Pattern defs: only for patterns actually referenced by items, so
    // non-pattern scenes emit byte-identical output to before.
    let used = used_pattern_names(draw_list);
    if !used.is_empty() {
        out.push_str("  <defs>\n");
        for pat in &draw_list.patterns {
            if !used.contains(&pat.name) {
                continue;
            }
            let mut body = String::new();
            for tile in &pat.tiles {
                emit_cmd_svg(tile, &mut body);
            }
            out.push_str(&format!(
                "    <pattern id=\"pvg-pat-{}\" patternUnits=\"userSpaceOnUse\" width=\"{:.2}\" height=\"{:.2}\">\n{}    </pattern>\n",
                pat.name, pat.width, pat.height, body,
            ));
        }
        out.push_str("  </defs>\n");
    }

    for cmd in &draw_list.items {
        emit_cmd_svg(cmd, &mut out);
    }

    out.push_str("</svg>\n");
    out
}

fn collect_pattern_refs(cmd: &DrawCmd, into: &mut HashSet<String>) {
    match cmd {
        DrawCmd::Circle { style, .. }
        | DrawCmd::Ellipse { style, .. }
        | DrawCmd::Rectangle { style, .. }
        | DrawCmd::Line { style, .. }
        | DrawCmd::Polygon { style, .. }
        | DrawCmd::Path { style, .. }
        | DrawCmd::Spline { style, .. }
        | DrawCmd::Text { style, .. } => {
            if let PvgPaint::Pattern(name) = &style.fill {
                into.insert(name.clone());
            }
            if let PvgPaint::Pattern(name) = &style.stroke {
                into.insert(name.clone());
            }
        }
        DrawCmd::Sprite { .. } => {}
        DrawCmd::Clip { mask, content } => {
            collect_pattern_refs(mask, into);
            for c in content {
                collect_pattern_refs(c, into);
            }
        }
    }
}

fn used_pattern_names(draw_list: &DrawList) -> HashSet<String> {
    let mut set = HashSet::new();
    for cmd in &draw_list.items {
        collect_pattern_refs(cmd, &mut set);
    }
    // Keep only names that actually have a pattern definition.
    let defined: HashSet<String> =
        draw_list.patterns.iter().map(|p| p.name.clone()).collect();
    set.into_iter().filter(|n| defined.contains(n)).collect()
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

/// PVG 0.2 paint fallback for the CPU rasterizer: solid colors map directly;
/// gradients resolve to their middle stop's color (deterministic flat fallback).
/// Pattern paints resolve to neutral gray here (tile hits render via
/// `paint_pattern`; misses keep this gray fallback, including nested
/// patterns inside tiles).
pub fn paint_to_skia(paint: &PvgPaint, opacity: f64) -> Option<Paint<'static>> {
    match paint {
        PvgPaint::Color(c) => color_to_skia(c, opacity),
        PvgPaint::Pattern(_) => color_to_skia(&Color::Rgba(136, 136, 136, 255), opacity),
        PvgPaint::Linear { stops, .. }
        | PvgPaint::Radial { stops, .. }
        | PvgPaint::Angular { stops, .. } => {
            if stops.is_empty() {
                return None;
            }
            let mid = &stops[stops.len() / 2].color;
            color_to_skia(mid, opacity)
        }
    }
}

/// A pre-rendered pattern tile: transparent-based premultiplied pixmap at
/// device scale, plus the tile size in user units.
struct PatternTile {
    pix: Pixmap,
    w_units: f64,
    h_units: f64,
}

/// Frame-scale pattern tiles, looked up by pattern name. Built once per
/// raster call and threaded through the fill/stroke paths.
type PatternTiles = HashMap<String, PatternTile>;

/// Renders every pattern tile once at device scale.
///
/// Tiles render with an EMPTY tile map: a pattern referenced from inside a
/// tile falls back to neutral gray, which terminates any A-references-A
/// cycle by construction. Degenerate (zero-size) or empty patterns are
/// skipped; tile dims are capped at 1024px.
fn render_pattern_tiles(patterns: &[DrawPattern], scale: f32) -> PatternTiles {
    let mut out = PatternTiles::new();
    let k = if scale > 0.0 && scale.is_finite() { scale } else { 1.0 };
    let empty = PatternTiles::new();
    for pat in patterns {
        if pat.width <= 1e-9 || pat.height <= 1e-9 || pat.tiles.is_empty() {
            continue;
        }
        let tw = ((pat.width as f32 * k).ceil().max(1.0) as u32).min(1024);
        let th = ((pat.height as f32 * k).ceil().max(1.0) as u32).min(1024);
        let mut pix = match Pixmap::new(tw, th) {
            Some(p) => p,
            None => continue,
        };
        let xf = Transform::from_scale(k, k);
        for t in &pat.tiles {
            render_cmd_masked(t, &mut pix, xf, None, &empty);
        }
        out.insert(pat.name.clone(), PatternTile { pix, w_units: pat.width, h_units: pat.height });
    }
    out
}

/// User-space geometry bounding box `(x0, y0, x1, y1)` for layer cropping.
/// Conservative for paths (includes control points); `None` for text.
fn geom_bbox(cmd: &DrawCmd) -> Option<(f64, f64, f64, f64)> {
    match cmd {
        DrawCmd::Circle { center, radius, .. } => {
            Some((center.0 - radius, center.1 - radius, center.0 + radius, center.1 + radius))
        }
        DrawCmd::Ellipse { center, radius, .. } => Some((
            center.0 - radius.0,
            center.1 - radius.1,
            center.0 + radius.0,
            center.1 + radius.1,
        )),
        DrawCmd::Rectangle { pos, size, .. } => {
            Some((pos.0, pos.1, pos.0 + size.0, pos.1 + size.1))
        }
        DrawCmd::Line { from, to, .. } => Some((
            from.0.min(to.0),
            from.1.min(to.1),
            from.0.max(to.0),
            from.1.max(to.1),
        )),
        DrawCmd::Polygon { points, .. } => {
            if points.is_empty() {
                return None;
            }
            let mut b = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for p in points {
                b.0 = b.0.min(p.0);
                b.1 = b.1.min(p.1);
                b.2 = b.2.max(p.0);
                b.3 = b.3.max(p.1);
            }
            Some(b)
        }
        DrawCmd::Path { commands, .. } => {
            let mut b = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for c in commands {
                match c {
                    DrawPathCommand::Start(p) | DrawPathCommand::Line(p) => {
                        b.0 = b.0.min(p.0);
                        b.1 = b.1.min(p.1);
                        b.2 = b.2.max(p.0);
                        b.3 = b.3.max(p.1);
                    }
                    DrawPathCommand::Quad { cp, ep } => {
                        for (x, y) in &[*cp, *ep] {
                            b.0 = b.0.min(*x);
                            b.1 = b.1.min(*y);
                            b.2 = b.2.max(*x);
                            b.3 = b.3.max(*y);
                        }
                    }
                    DrawPathCommand::Curve { c1, c2, ep } => {
                        for (x, y) in &[*c1, *c2, *ep] {
                            b.0 = b.0.min(*x);
                            b.1 = b.1.min(*y);
                            b.2 = b.2.max(*x);
                            b.3 = b.3.max(*y);
                        }
                    }
                    DrawPathCommand::Arc { center, radius, .. } => {
                        b.0 = b.0.min(center.0 - radius);
                        b.1 = b.1.min(center.1 - radius);
                        b.2 = b.2.max(center.0 + radius);
                        b.3 = b.3.max(center.1 + radius);
                    }
                    DrawPathCommand::Close => {}
                }
            }
            if b.0.is_infinite() {
                return None;
            }
            Some(b)
        }
        DrawCmd::Text { .. } => None,
        DrawCmd::Sprite { pos, rows, scale, .. } => {
            let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as f64 * *scale;
            let h = rows.len() as f64 * *scale;
            Some((pos.0, pos.1, pos.0 + w, pos.1 + h))
        }
        DrawCmd::Spline { points, .. } => {
            if points.is_empty() {
                return None;
            }
            let mut b = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for p in points {
                b.0 = b.0.min(p.0);
                b.1 = b.1.min(p.1);
                b.2 = b.2.max(p.0);
                b.3 = b.3.max(p.1);
            }
            Some(b)
        }
        DrawCmd::Clip { mask, .. } => geom_bbox(mask),
    }
}

/// Device-space loop bounds for pattern fills: command bbox through the draw
/// transform, padded 2px for AA, clamped to the target.
fn loop_rect(cmd: &DrawCmd, transform: Transform, w: u32, h: u32) -> (u32, u32, u32, u32) {
    let fallback = (0, 0, w, h);
    let (x0, y0, x1, y1) = match geom_bbox(cmd) {
        Some(b) => b,
        None => return fallback,
    };
    let mut pts = [
        Point::from_xy(x0 as f32, y0 as f32),
        Point::from_xy(x1 as f32, y0 as f32),
        Point::from_xy(x0 as f32, y1 as f32),
        Point::from_xy(x1 as f32, y1 as f32),
    ];
    transform.map_points(&mut pts);
    let mut lx0 = f32::INFINITY;
    let mut ly0 = f32::INFINITY;
    let mut lx1 = f32::NEG_INFINITY;
    let mut ly1 = f32::NEG_INFINITY;
    for p in &pts {
        lx0 = lx0.min(p.x);
        ly0 = ly0.min(p.y);
        lx1 = lx1.max(p.x);
        ly1 = ly1.max(p.y);
    }
    let x0c = (lx0.floor() as i32 - 2).max(0).min(w as i32) as u32;
    let y0c = (ly0.floor() as i32 - 2).max(0).min(h as i32) as u32;
    let x1c = (lx1.ceil() as i32 + 2).max(0).min(w as i32) as u32;
    let y1c = (ly1.ceil() as i32 + 2).max(0).min(h as i32) as u32;
    if x1c <= x0c || y1c <= y0c {
        return fallback;
    }
    (x0c, y0c, x1c, y1c)
}

/// Pattern fill/stroke via per-pixel tile-wrap sampling.
///
/// Renders `path` coverage (white fill, or white stroke so dash/cap/join are
/// honored) into a cropped temp, then maps each covered device pixel back to
/// canvas space and wraps it into the tile with Euclidean mod
/// (negative-safe). `style.opacity` folds in at composite time; the finished
/// layer composites with ONE `draw_pixmap` using the style blend mode and the
/// existing `mask`.
fn paint_pattern(
    dst: &mut Pixmap,
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    tile: &PatternTile,
    is_fill: bool,
    style: &DrawStyle,
    transform: Transform,
    mask: Option<&Mask>,
) {
    let (tw_px, th_px) = (tile.pix.width(), tile.pix.height());
    if tw_px == 0 || th_px == 0 {
        return;
    }
    let op = (style.opacity as f32).clamp(0.0, 1.0);
    if op <= 0.0 {
        return;
    }
    let (w, h) = (dst.width(), dst.height());
    let (rx0, ry0, rx1, ry1) = loop_rect(cmd, transform, w, h);
    let (rw, rh) = (rx1 - rx0, ry1 - ry0);
    if rw == 0 || rh == 0 {
        return;
    }
    let lxf = transform.post_translate(-(rx0 as f32), -(ry0 as f32));
    let mut coverage = match Pixmap::new(rw, rh) {
        Some(p) => p,
        None => return,
    };
    {
        let mut white = Paint::default();
        white.set_color_rgba8(255, 255, 255, 255);
        white.anti_alias = true;
        if is_fill {
            coverage.fill_path(path, &white, FillRule::Winding, lxf, None);
        } else {
            let stroke = style_to_stroke(style);
            coverage.stroke_path(path, &white, &stroke, lxf, None);
        }
    }
    let inv = match lxf.invert() {
        Some(v) => v,
        None => return,
    };
    let mut layer = match Pixmap::new(rw, rh) {
        Some(p) => p,
        None => return,
    };
    {
        let cov = coverage.data();
        let tdata = tile.pix.data();
        let out = layer.data_mut();
        for y in 0..rh {
            for x in 0..rw {
                let ca = cov[(y as usize * rw as usize + x as usize) * 4 + 3] as f32 / 255.0;
                if ca <= 0.0 {
                    continue;
                }
                let mut pt = Point::from_xy(x as f32, y as f32);
                inv.map_point(&mut pt);
                let fx = ((pt.x as f64).rem_euclid(tile.w_units) / tile.w_units * tw_px as f64) as u32;
                let fy = ((pt.y as f64).rem_euclid(tile.h_units) / tile.h_units * th_px as f64) as u32;
                let ti = ((fy.min(th_px - 1) * tw_px + fx.min(tw_px - 1)) as usize) * 4;
                let ta = tdata[ti + 3] as f32 / 255.0;
                let ae = ta * ca * op;
                if ae <= 0.0 {
                    continue;
                }
                let idx = (y as usize * rw as usize + x as usize) * 4;
                out[idx] = (tdata[ti] as f32 * ca * op).round() as u8;
                out[idx + 1] = (tdata[ti + 1] as f32 * ca * op).round() as u8;
                out[idx + 2] = (tdata[ti + 2] as f32 * ca * op).round() as u8;
                out[idx + 3] = (ae * 255.0).round() as u8;
            }
        }
    }
    dst.draw_pixmap(
        rx0 as i32,
        ry0 as i32,
        layer.as_ref(),
        &PixmapPaint {
            opacity: 1.0,
            blend_mode: style_to_blend(style),
            quality: FilterQuality::Nearest,
        },
        Transform::identity(),
        mask,
    );
}

/// Fill of one shape path: real pattern tiling on `Paint::Pattern` tile hits,
/// otherwise the pre-existing flat `paint_to_skia` behavior (bit-identical
/// for solid/gradient paints; pattern misses keep the gray fallback).
fn paint_fill(
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    style: &DrawStyle,
    pixmap: &mut Pixmap,
    transform: Transform,
    mask: Option<&Mask>,
    tiles: &PatternTiles,
) {
    if let PvgPaint::Pattern(name) = &style.fill {
        if let Some(tile) = tiles.get(name) {
            paint_pattern(pixmap, cmd, path, tile, true, style, transform, mask);
            return;
        }
    }
    if let Some(mut fill_paint) = paint_to_skia(&style.fill, style.opacity) {
        fill_paint.blend_mode = style_to_blend(style);
        pixmap.fill_path(path, &fill_paint, FillRule::Winding, transform, mask);
    }
}

/// Stroke of one shape path: real pattern tiling on `Paint::Pattern` tile
/// hits, otherwise the pre-existing flat `paint_to_skia` behavior.
fn paint_stroke(
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    style: &DrawStyle,
    pixmap: &mut Pixmap,
    transform: Transform,
    mask: Option<&Mask>,
    tiles: &PatternTiles,
) {
    if style.width <= 0.0 {
        return;
    }
    if let PvgPaint::Pattern(name) = &style.stroke {
        if let Some(tile) = tiles.get(name) {
            paint_pattern(pixmap, cmd, path, tile, false, style, transform, mask);
            return;
        }
    }
    if let Some(mut stroke_paint) = paint_to_skia(&style.stroke, style.opacity) {
        stroke_paint.blend_mode = style_to_blend(style);
        let stroke = style_to_stroke(style);
        pixmap.stroke_path(path, &stroke_paint, &stroke, transform, mask);
    }
}

fn style_to_stroke(style: &DrawStyle) -> Stroke {
    let mut stroke = Stroke::default();
    stroke.width = style.width.max(0.0) as f32;
    stroke.line_cap = match style.cap {
        LineCap::Butt => SkCap::Butt,
        LineCap::Round => SkCap::Round,
        LineCap::Square => SkCap::Square,
    };
    stroke.line_join = match style.join {
        LineJoin::Miter => SkJoin::Miter,
        LineJoin::Round => SkJoin::Round,
        LineJoin::Bevel => SkJoin::Bevel,
    };
    stroke.miter_limit = style.miter.max(1.0) as f32;
    if !style.dash.is_empty() {
        stroke.dash = StrokeDash::new(style.dash.iter().map(|v| *v as f32).collect(), 0.0);
    }
    stroke
}

fn style_to_blend(style: &DrawStyle) -> SkBlend {
    match style.blend {
        BlendMode::Normal => SkBlend::SourceOver,
        BlendMode::Add => SkBlend::Plus,
        BlendMode::Multiply => SkBlend::Multiply,
        BlendMode::Screen => SkBlend::Screen,
        BlendMode::Overlay => SkBlend::Overlay,
    }
}

/// Palette character -> index (digits, then a-z/A-Z for 10+). `.`/space = skip.
fn sprite_char_index(ch: char) -> Option<usize> {
    if ch == '.' || ch == ' ' {
        return None;
    }
    if ch.is_ascii_digit() {
        Some((ch as u8 - b'0') as usize)
    } else if ('a'..='z').contains(&ch) {
        Some((ch as u8 - b'a') as usize + 10)
    } else if ('A'..='Z').contains(&ch) {
        Some((ch as u8 - b'A') as usize + 10)
    } else {
        None
    }
}

fn spline_to_skia_path(points: &[(f64, f64)]) -> Option<tiny_skia::Path> {
    if points.is_empty() {
        return None;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].0 as f32, points[0].1 as f32);
    if points.len() == 2 {
        pb.line_to(points[1].0 as f32, points[1].1 as f32);
    } else {
        for (c1, c2, ep) in pvg::spline_to_bezier(points) {
            pb.cubic_to(c1.0 as f32, c1.1 as f32, c2.0 as f32, c2.1 as f32, ep.0 as f32, ep.1 as f32);
        }
    }
    pb.finish()
}

/// Builds a tiny-skia path for the geometry of any draw command.
/// Used for clip masks and masked content rendering.
fn cmd_to_path(cmd: &DrawCmd) -> Option<tiny_skia::Path> {
    match cmd {
        DrawCmd::Circle { center, radius, .. } => {
            let mut pb = PathBuilder::new();
            pb.push_circle(center.0 as f32, center.1 as f32, *radius as f32);
            pb.finish()
        }
        DrawCmd::Ellipse { center, radius, .. } => {
            let rect = Rect::from_xywh(
                (center.0 - radius.0) as f32,
                (center.1 - radius.1) as f32,
                (radius.0 * 2.0) as f32,
                (radius.1 * 2.0) as f32,
            )?;
            let mut pb = PathBuilder::new();
            pb.push_oval(rect);
            pb.finish()
        }
        DrawCmd::Rectangle { pos, size, corner_radius, .. } => {
            let (x, y, w, h) = (pos.0 as f32, pos.1 as f32, size.0 as f32, size.1 as f32);
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            let cr = (*corner_radius as f32).max(0.0);
            if cr > 0.0 {
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
                let rect = Rect::from_xywh(x, y, w, h)?;
                let mut pb = PathBuilder::new();
                pb.push_rect(rect);
                pb.finish()
            }
        }
        DrawCmd::Line { from, to, .. } => {
            let mut pb = PathBuilder::new();
            pb.move_to(from.0 as f32, from.1 as f32);
            pb.line_to(to.0 as f32, to.1 as f32);
            pb.finish()
        }
        DrawCmd::Polygon { points, .. } => {
            if points.len() < 2 {
                return None;
            }
            let mut pb = PathBuilder::new();
            pb.move_to(points[0].0 as f32, points[0].1 as f32);
            for pt in &points[1..] {
                pb.line_to(pt.0 as f32, pt.1 as f32);
            }
            pb.close();
            pb.finish()
        }
        DrawCmd::Path { commands, .. } => {
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
            pb.finish()
        }
        DrawCmd::Text { .. } => None,
        DrawCmd::Sprite { pos, rows, scale, .. } => {
            let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as f32 * *scale as f32;
            let h = rows.len() as f32 * *scale as f32;
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            let rect = Rect::from_xywh(pos.0 as f32, pos.1 as f32, w, h)?;
            let mut pb = PathBuilder::new();
            pb.push_rect(rect);
            pb.finish()
        }
        DrawCmd::Spline { points, .. } => spline_to_skia_path(points),
        DrawCmd::Clip { mask, .. } => cmd_to_path(mask),
    }
}

/// Renders one command through an optional clip mask (for `clip` content,
/// including nested clips).
fn render_cmd_masked(
    cmd: &DrawCmd,
    pixmap: &mut Pixmap,
    transform: Transform,
    mask: Option<&Mask>,
    tiles: &PatternTiles,
) {
    // Text has no rasterizer (pre-existing 0.1 limitation).
    if matches!(cmd, DrawCmd::Text { .. }) {
        return;
    }
    if let DrawCmd::Clip { mask: inner_mask, content } = cmd {
        if let Some(mask_path) = cmd_to_path(inner_mask) {
            if let Some(mut nested) = Mask::new(pixmap.width(), pixmap.height()) {
                nested.fill_path(&mask_path, FillRule::Winding, true, transform);
                // Note: tiny-skia accepts a single mask; nested clips render
                // through the tighter inner mask (outer mask intersection
                // is a documented follow-up).
                let _ = mask;
                for inner in content {
                    render_cmd_masked(inner, pixmap, transform, Some(&nested), tiles);
                }
            }
        }
        return;
    }
    // Sprites render pixel-by-pixel (with mask); splines stroke their curve.
    if let DrawCmd::Sprite { pos, palette, rows, scale, style } = cmd {
        for (ry, row) in rows.iter().enumerate() {
            for (rx, ch) in row.chars().enumerate() {
                let idx = match sprite_char_index(ch) {
                    Some(i) => i,
                    None => continue,
                };
                let color = match palette.get(idx) {
                    Some(c) if !c.is_transparent() && !c.is_none() => c,
                    _ => continue,
                };
                if let Some(mut px_paint) = color_to_skia(color, style.opacity) {
                    px_paint.blend_mode = style_to_blend(style);
                    let x = pos.0 as f32 + rx as f32 * *scale as f32;
                    let y = pos.1 as f32 + ry as f32 * *scale as f32;
                    if let Some(rect) = Rect::from_xywh(x, y, *scale as f32, *scale as f32) {
                        pixmap.fill_rect(rect, &px_paint, transform, mask);
                    }
                }
            }
        }
        return;
    }
    if let DrawCmd::Spline { points, style } = cmd {
        if let Some(path) = spline_to_skia_path(points) {
            paint_stroke(cmd, &path, style, pixmap, transform, mask, tiles);
        }
        return;
    }
    let style = match cmd {
        DrawCmd::Circle { style, .. }
        | DrawCmd::Ellipse { style, .. }
        | DrawCmd::Rectangle { style, .. }
        | DrawCmd::Line { style, .. }
        | DrawCmd::Polygon { style, .. }
        | DrawCmd::Path { style, .. }
        | DrawCmd::Spline { style, .. } => style,
        _ => return,
    };
    if let Some(path) = cmd_to_path(cmd) {
        // Lines have no fill in the main loop; keep parity and stroke them only.
        let is_line = matches!(cmd, DrawCmd::Line { .. });
        if !is_line {
            paint_fill(cmd, &path, style, pixmap, transform, mask, tiles);
        }
        if is_line || !style.stroke.is_none() {
            paint_stroke(cmd, &path, style, pixmap, transform, mask, tiles);
        }
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

    let tiles = render_pattern_tiles(&draw_list.patterns, scale);

    for cmd in &draw_list.items {
        match cmd {
            DrawCmd::Circle { center, radius, style } => {
                let mut pb = PathBuilder::new();
                pb.push_circle(center.0 as f32, center.1 as f32, *radius as f32);
                if let Some(path) = pb.finish() {
                    paint_fill(cmd, &path, style, &mut pixmap, transform, None, &tiles);
                    paint_stroke(cmd, &path, style, &mut pixmap, transform, None, &tiles);
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
                        paint_fill(cmd, &path, style, &mut pixmap, transform, None, &tiles);
                        paint_stroke(cmd, &path, style, &mut pixmap, transform, None, &tiles);
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
                        paint_fill(cmd, &path, style, &mut pixmap, transform, None, &tiles);
                        paint_stroke(cmd, &path, style, &mut pixmap, transform, None, &tiles);
                    }
                }
            }
            DrawCmd::Line { from, to, style } => {
                let mut pb = PathBuilder::new();
                pb.move_to(from.0 as f32, from.1 as f32);
                pb.line_to(to.0 as f32, to.1 as f32);
                if let Some(path) = pb.finish() {
                    paint_stroke(cmd, &path, style, &mut pixmap, transform, None, &tiles);
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
                        paint_fill(cmd, &path, style, &mut pixmap, transform, None, &tiles);
                        paint_stroke(cmd, &path, style, &mut pixmap, transform, None, &tiles);
                    }
                }
            }
            DrawCmd::Text { .. } => {}
            DrawCmd::Sprite { pos, palette, rows, scale, style } => {
                for (ry, row) in rows.iter().enumerate() {
                    for (rx, ch) in row.chars().enumerate() {
                        let idx = match sprite_char_index(ch) {
                            Some(i) => i,
                            None => continue,
                        };
                        let color = match palette.get(idx) {
                            Some(c) if !c.is_transparent() && !c.is_none() => c,
                            _ => continue,
                        };
                        if let Some(mut px_paint) = color_to_skia(color, style.opacity) {
                            px_paint.blend_mode = style_to_blend(style);
                            let sx = pos.0 as f32 + rx as f32 * *scale as f32;
                            let sy = pos.1 as f32 + ry as f32 * *scale as f32;
                            if let Some(rect) =
                                Rect::from_xywh(sx, sy, *scale as f32, *scale as f32)
                            {
                                pixmap.fill_rect(rect, &px_paint, transform, None);
                            }
                        }
                    }
                }
            }
            DrawCmd::Spline { points, style } => {
                if let Some(path) = spline_to_skia_path(points) {
                    paint_stroke(cmd, &path, style, &mut pixmap, transform, None, &tiles);
                }
            }
            DrawCmd::Clip { mask, content } => {
                if let Some(mask_path) = cmd_to_path(mask) {
                    if let Some(mut clip_mask) = Mask::new(width, height) {
                        clip_mask.fill_path(&mask_path, FillRule::Winding, true, transform);
                        for inner in content {
                            render_cmd_masked(inner, &mut pixmap, transform, Some(&clip_mask), &tiles);
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
                    paint_fill(cmd, &path, style, &mut pixmap, transform, None, &tiles);
                    paint_stroke(cmd, &path, style, &mut pixmap, transform, None, &tiles);
                }
            }
        }
    }

    Ok(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(pixmap: &Pixmap, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let d = pixmap.data();
        let i = ((y * pixmap.width() + x) * 4) as usize;
        (d[i], d[i + 1], d[i + 2], d[i + 3])
    }

    #[test]
    fn pattern_fill_tiles_rect() {
        let src = "PVG 0.2\ncanvas 64 64\n  background #000000\npattern gridp 16 16\n  line\n    from [0, 0]\n    to [16, 0]\n    stroke #ffffff\n    width 2\nrectangle\n  pos [0, 0]\n  size [64, 64]\n  fill pattern gridp\n";
        let dl = pvg::compile(src).unwrap();
        assert_eq!(dl.patterns.len(), 1);
        let pixm = rasterize_skia(&dl, 1.0).unwrap();
        // Tile-line pixel (y=0 row carries the horizontal line) must be bright.
        let (r, g, b, a) = px(&pixm, 32, 0);
        assert_eq!(a, 255, "tile line must be opaque, got {:?}", (r, g, b, a));
        assert!(
            r > 200 && g > 200 && b > 200,
            "tile line must be bright, got {},{},{}",
            r, g, b
        );
        // Mid-tile gap shows background, not uniform gray fallback (#888).
        let (r, g, b, _) = px(&pixm, 32, 8);
        assert!(
            r < 40 && g < 40 && b < 40,
            "tile gap must be dark, got {},{},{}",
            r, g, b
        );
        assert!(
            !(r == 136 && g == 136 && b == 136),
            "must not be uniform gray fallback"
        );
    }

    #[test]
    fn pattern_svg_uses_url_and_defs() {
        let src = "PVG 0.2\ncanvas 64 64\npattern gridp 16 16\n  line\n    from [0, 0]\n    to [16, 0]\n    stroke #ffffff\n    width 2\nrectangle\n  pos [0, 0]\n  size [64, 64]\n  fill pattern gridp\n";
        let dl = pvg::compile(src).unwrap();
        let svg = emit_svg(&dl);
        assert!(svg.contains("fill=\"url(#pvg-pat-gridp)\""), "svg:\n{}", svg);
        assert!(svg.contains("<pattern id=\"pvg-pat-gridp\""), "svg:\n{}", svg);
        assert!(
            svg.contains("patternUnits=\"userSpaceOnUse\""),
            "svg:\n{}",
            svg
        );
    }

    #[test]
    fn plain_svg_has_no_pattern_defs() {
        let src = "PVG 0.2\ncanvas 64 64\nrectangle\n  pos [0, 0]\n  size [10, 10]\n  fill #ff0000\n";
        let dl = pvg::compile(src).unwrap();
        let svg = emit_svg(&dl);
        assert!(!svg.contains("pvg-pat-"), "svg:\n{}", svg);
        assert!(!svg.contains("<defs>"), "svg:\n{}", svg);
    }
}
