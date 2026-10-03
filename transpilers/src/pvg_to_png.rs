use pvg::ast::Color;
use pvg::compile_pvg_at_time;
use pvg::draw_list::{BlendMode, DrawCmd, DrawList, DrawPathCommand, DrawPattern, DrawStyle, LineCap, LineJoin, Paint as PvgPaint};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tiny_skia::{BlendMode as SkBlend, FillRule, FilterQuality, LineCap as SkCap, LineJoin as SkJoin, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Point, Rect, Stroke, Transform};

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
/// `Pattern` paints that miss the per-frame tile map (unknown names, or
/// patterns referenced from *inside* a tile) resolve to neutral gray.
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
/// `rasterize_draw_list` call by [`render_pattern_tiles`] and threaded through
/// the fill/stroke paths (so every shape samples the same tile instead of
/// re-rendering it).
type PatternTiles = HashMap<String, PatternTile>;

/// Renders every pattern tile once at device scale `k`.
///
/// Tiles render with an EMPTY tile map: a pattern referenced from inside a
/// tile misses the map and falls back to neutral gray, which terminates any
/// A-references-A cycle by construction.
fn render_pattern_tiles(patterns: &[DrawPattern], k: f32) -> PatternTiles {
    let mut out = PatternTiles::new();
    let k = if k > 0.0 && k.is_finite() { k } else { 1.0 };
    let empty = PatternTiles::new();
    for pat in patterns {
        if pat.width <= 1e-9 || pat.height <= 1e-9 || pat.tiles.is_empty() {
            continue;
        }
        // Cap tile resolution: absurd tiles (huge user size x export scale)
        // degrade to the gray fallback instead of OOMing the export.
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
/// Conservative for paths (includes control points); `None` for text (which
/// has no rasterizer here).
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
                        for p in [cp, ep] {
                            b.0 = b.0.min(p.0);
                            b.1 = b.1.min(p.1);
                            b.2 = b.2.max(p.0);
                            b.3 = b.3.max(p.1);
                        }
                    }
                    DrawPathCommand::Curve { c1, c2, ep } => {
                        for p in [c1, c2, ep] {
                            b.0 = b.0.min(p.0);
                            b.1 = b.1.min(p.1);
                            b.2 = b.2.max(p.0);
                            b.3 = b.3.max(p.1);
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

/// Device-space loop bounds for sampled fills: command bbox through the draw
/// transform, padded 2px for AA, clamped to the target.
fn loop_rect(cmd: &DrawCmd, transform: Transform, w: u32, h: u32) -> (u32, u32, u32, u32) {
    let fallback = (0, 0, w, h);
    let (x0, y0, x1, y1) = match geom_bbox(cmd) {
        Some(b) => b,
        None => return fallback,
    };
    // Map all four corners (general under rotation, cheap anyway).
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
/// canvas space (inverse transform; the transforms here are scale-only but
/// this is written generally) and wraps it into the tile with Euclidean mod
/// (negative-safe). `style.opacity` and the coverage alpha fold into the
/// premultiplied output, which composites with ONE `draw_pixmap` using the
/// style blend mode and the caller's `mask`.
#[allow(clippy::too_many_arguments)]
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
                // Canvas space -> tile texel (wrap; rem_euclid is negative-safe).
                let fx =
                    ((pt.x as f64).rem_euclid(tile.w_units) / tile.w_units * tw_px as f64) as u32;
                let fy =
                    ((pt.y as f64).rem_euclid(tile.h_units) / tile.h_units * th_px as f64) as u32;
                let ti = ((fy.min(th_px - 1) * tw_px + fx.min(tw_px - 1)) as usize) * 4;
                let ta = tdata[ti + 3] as f32 / 255.0;
                let ae = ta * ca * op;
                if ae <= 0.0 {
                    continue;
                }
                let idx = (y as usize * rw as usize + x as usize) * 4;
                // Tile data is premultiplied: scale by coverage x opacity.
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

/// Fills one shape path, sampling the pattern tile on `Paint::Pattern` hits.
/// Misses (unknown names) and all solid/gradient paints go through the
/// existing `paint_to_skia` path bit-identically.
fn fill_shape(
    dst: &mut Pixmap,
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    style: &DrawStyle,
    transform: Transform,
    mask: Option<&Mask>,
    tiles: &PatternTiles,
) {
    if let PvgPaint::Pattern(name) = &style.fill {
        if let Some(tile) = tiles.get(name) {
            paint_pattern(dst, cmd, path, tile, true, style, transform, mask);
            return;
        }
    }
    if let Some(mut fill_paint) = paint_to_skia(&style.fill, style.opacity) {
        fill_paint.blend_mode = style_to_blend(style);
        dst.fill_path(path, &fill_paint, FillRule::Winding, transform, mask);
    }
}

/// Strokes one shape path, sampling the pattern tile on `Paint::Pattern` hits.
/// Misses (unknown names) and all solid/gradient paints go through the
/// existing `paint_to_skia` path bit-identically.
fn stroke_shape(
    dst: &mut Pixmap,
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    style: &DrawStyle,
    transform: Transform,
    mask: Option<&Mask>,
    tiles: &PatternTiles,
) {
    if let PvgPaint::Pattern(name) = &style.stroke {
        if let Some(tile) = tiles.get(name) {
            paint_pattern(dst, cmd, path, tile, false, style, transform, mask);
            return;
        }
    }
    if let Some(mut stroke_paint) = paint_to_skia(&style.stroke, style.opacity) {
        stroke_paint.blend_mode = style_to_blend(style);
        if style.width > 0.0 {
            let stroke = style_to_stroke(style);
            dst.stroke_path(path, &stroke_paint, &stroke, transform, mask);
        }
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
        let dashes: Vec<f32> = style.dash.iter().map(|v| *v as f32).collect();
        stroke.dash = tiny_skia::StrokeDash::new(dashes, 0.0);
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
            stroke_shape(pixmap, cmd, &path, style, transform, mask, tiles);
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
        fill_shape(pixmap, cmd, &path, style, transform, mask, tiles);
        // Lines have no fill in the main loop; keep parity and stroke them only.
    // (fill above is a no-op for open line paths.)
    let is_line = matches!(cmd, DrawCmd::Line { .. });
    if is_line || !style.stroke.is_none() {
        stroke_shape(pixmap, cmd, &path, style, transform, mask, tiles);
    }
    }
}

pub fn rasterize_draw_list(draw_list: &DrawList, scale: f32) -> Result<Pixmap, String> {
    let scale = if scale <= 0.0 { 1.0 } else { scale };
    let width = ((draw_list.canvas_width as f32) * scale).round() as u32;
    let height = ((draw_list.canvas_height as f32) * scale).round() as u32;

    if width == 0 || height == 0 {
        return Err("Canvas dimensions must be greater than zero".into());
    }

    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| format!("Failed to allocate Pixmap of size {}x{}", width, height))?;

    let transform = Transform::from_scale(scale, scale);

    // 1. Background
    if let Some(ref bg) = draw_list.background {
        if let Some(bg_paint) = color_to_skia(bg, 1.0) {
            if let Some(rect) = Rect::from_xywh(0.0, 0.0, width as f32, height as f32) {
                pixmap.fill_rect(rect, &bg_paint, Transform::identity(), None);
            }
        }
    }

    // 2. Shapes (pattern tiles rendered once per frame above).
    let tiles = render_pattern_tiles(&draw_list.patterns, scale);
    for cmd in &draw_list.items {
        match cmd {
            DrawCmd::Circle { center, radius, style } => {
                let mut pb = PathBuilder::new();
                pb.push_circle(center.0 as f32, center.1 as f32, *radius as f32);
                if let Some(path) = pb.finish() {
                    fill_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
                    stroke_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
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
                        fill_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
                        stroke_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
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
                        fill_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
                        stroke_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
                    }
                }
            }

            DrawCmd::Line { from, to, style } => {
                let mut pb = PathBuilder::new();
                pb.move_to(from.0 as f32, from.1 as f32);
                pb.line_to(to.0 as f32, to.1 as f32);
                if let Some(path) = pb.finish() {
                    stroke_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
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
                        fill_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
                        stroke_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
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
                            // Canvas-space rect; `transform` applies export scale.
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
                    stroke_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
                }
            }

            // PVG 0.2 clip: build a pixel mask from the mask shape and
            // render content through it (true clipping, not a fallback).
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

                for path_cmd in commands {
                    match path_cmd {
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
                    fill_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
                    stroke_shape(&mut pixmap, cmd, &path, style, transform, None, &tiles);
                }
            }
        }
    }

    Ok(pixmap)
}

pub fn rasterize_draw_list_to_png(draw_list: &DrawList, scale: f32) -> Result<Vec<u8>, String> {
    let pixmap = rasterize_draw_list(draw_list, scale)?;
    pixmap.encode_png().map_err(|e| format!("PNG encoding failed: {}", e))
}

fn print_usage() {
    println!("\n===============================================================================");
    println!("             PVG 0.2 TO PNG RASTERIZER & EXPORTER CLI                          ");
    println!("===============================================================================");
    println!("Usage:");
    println!("  cargo run --bin pvg_to_png -- <input.pvg> [output.png] [options]");
    println!("  cargo run --bin pvg_to_png -- --all [options]");
    println!();
    println!("Options:");
    println!("  -t, --time <sec>       Capture static frame at specific timestamp (default: 0.0)");
    println!("  -s, --scale <factor>   Resolution scale multiplier: 1.0, 2.0, 4.0 (default: 1.0)");
    println!("  --frames <count>       Export animation sequence frames (e.g. --frames 30)");
    println!("  --duration <sec>       Animation sequence duration in seconds (default: 3.0)");
    println!("  --static               Force single static export even if file is animated");
    println!("  -h, --help             Show this help message");
    println!();
    println!("Examples:");
    println!("  cargo run --bin pvg_to_png -- presets/dial.pvg");
    println!("  cargo run --bin pvg_to_png -- presets/radar.pvg radar.png --scale 2.0");
    println!("  cargo run --bin pvg_to_png -- presets/radar.pvg --frames 30");
    println!("  cargo run --bin pvg_to_png -- --all --scale 2.0");
    println!("===============================================================================\n");
}

fn resolve_file_path(filename: &str) -> Option<PathBuf> {
    let p = PathBuf::from(filename);
    if p.exists() {
        return Some(p);
    }
    let search_candidates = [
        format!("presets/{}", filename),
        format!("../presets/{}", filename),
        format!("../../presets/{}", filename),
    ];
    for candidate in &search_candidates {
        let cp = PathBuf::from(candidate);
        if cp.exists() {
            return Some(cp);
        }
    }
    None
}

fn find_presets_dir() -> Option<PathBuf> {
    let candidates = ["presets", "../presets", "../../presets"];
    for candidate in &candidates {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn rasterize_file(
    input_path: &Path,
    output_path: &Path,
    time: f64,
    scale: f32,
    frames_count: Option<usize>,
    duration: f64,
    force_static: bool,
) -> Result<(), String> {
    let source = fs::read_to_string(input_path)
        .map_err(|e| format!("Failed to read '{}': {}", input_path.display(), e))?;

    let is_animated = !force_static && (source.contains("time") || source.contains(" t ") || source.contains("(t)") || source.contains("* t"));

    if is_animated && frames_count.is_some() {
        let total_frames = frames_count.unwrap().max(1);
        let start = Instant::now();
        let stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("frame");
        let parent = output_path.parent().unwrap_or_else(|| Path::new("."));

        for f in 0..total_frames {
            let t = (f as f64 / total_frames as f64) * duration;
            let dl = compile_pvg_at_time(&source, t)
                .map_err(|e| format!("Runtime Error at t={:.2}: {}", t, e))?;
            let png_bytes = rasterize_draw_list_to_png(&dl, scale)?;
            let frame_filename = format!("{}_{:03}.png", stem, f);
            let frame_path = parent.join(frame_filename);
            fs::write(&frame_path, png_bytes)
                .map_err(|e| format!("Failed to write '{}': {}", frame_path.display(), e))?;
        }

        let elapsed = start.elapsed().as_micros();
        println!(
            "  ✓ [SUCCESS] {:<20} -> {} frames (Sequence @ {:.1}x | {:.3} ms total)",
            input_path.file_name().unwrap().to_str().unwrap_or(""),
            total_frames,
            scale,
            elapsed as f64 / 1000.0
        );
        return Ok(());
    }

    let start = Instant::now();
    let dl = compile_pvg_at_time(&source, time)?;
    let png_bytes = rasterize_draw_list_to_png(&dl, scale)?;
    fs::write(output_path, &png_bytes)
        .map_err(|e| format!("Failed to write '{}': {}", output_path.display(), e))?;

    let elapsed = start.elapsed().as_micros();
    let size_kb = png_bytes.len() as f64 / 1024.0;

    println!(
        "  ✓ [SUCCESS] {:<20} -> {:<20} ({:>5.1} KB | {:.1}x Scale | {:.3} ms)",
        input_path.file_name().unwrap().to_str().unwrap_or(""),
        output_path.file_name().unwrap().to_str().unwrap_or(""),
        size_kb,
        scale,
        elapsed as f64 / 1000.0
    );

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 || args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_usage();
        return;
    }

    let mut scale: f32 = 1.0;
    let mut time: f64 = 0.0;
    let mut frames_count: Option<usize> = None;
    let mut duration: f64 = 3.0;
    let mut force_static = false;
    let mut output_path = None;

    let is_batch = args[1] == "--all";
    let mut i = if is_batch { 2 } else { 1 };

    while i < args.len() {
        match args[i].as_str() {
            "-s" | "--scale" => {
                if i + 1 < args.len() {
                    scale = args[i + 1].parse::<f32>().unwrap_or(1.0);
                    i += 1;
                }
            }
            "-t" | "--time" => {
                if i + 1 < args.len() {
                    time = args[i + 1].parse::<f64>().unwrap_or(0.0);
                    i += 1;
                }
            }
            "--frames" => {
                if i + 1 < args.len() {
                    frames_count = args[i + 1].parse::<usize>().ok();
                    i += 1;
                }
            }
            "--duration" => {
                if i + 1 < args.len() {
                    duration = args[i + 1].parse::<f64>().unwrap_or(3.0);
                    i += 1;
                }
            }
            "--static" => {
                force_static = true;
            }
            arg if !arg.starts_with('-') && !is_batch && i > 1 && output_path.is_none() => {
                output_path = Some(PathBuf::from(arg));
            }
            _ => {}
        }
        i += 1;
    }

    if is_batch {
        let presets_dir = match find_presets_dir() {
            Some(d) => d,
            None => {
                eprintln!("\n❌ Could not locate the 'presets' directory.\n");
                return;
            }
        };

        println!("\n===============================================================================");
        println!("              PVG TO PNG BATCH RASTERIZER                                      ");
        println!("              Scanning: {} (Scale: {:.1}x)                                     ", presets_dir.display(), scale);
        println!("===============================================================================\n");

        let entries = match fs::read_dir(&presets_dir) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Failed to read directory: {}", e);
                return;
            }
        };

        let mut count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("pvg") {
                let mut out_path = path.clone();
                out_path.set_extension("png");
                if let Err(e) = rasterize_file(&path, &out_path, time, scale, frames_count, duration, force_static) {
                    eprintln!("  ✗ Failed '{}': {}", path.display(), e);
                } else {
                    count += 1;
                }
            }
        }

        println!("\n-------------------------------------------------------------------------------");
        println!(" ✨ Successfully rasterized {} file(s) into PNG in '{}'!", count, presets_dir.display());
        println!("===============================================================================\n");
        return;
    }

    let input_name = &args[1];
    let input_path = match resolve_file_path(input_name) {
        Some(p) => p,
        None => {
            eprintln!("\n❌ Could not find file '{}'. Check the filename or path.\n", input_name);
            return;
        }
    };

    let final_output = output_path.unwrap_or_else(|| {
        let mut p = PathBuf::from(input_path.file_name().unwrap());
        p.set_extension("png");
        p
    });

    println!("\n⚡ Rasterizing PVG -> PNG (Scale: {:.1}x)...", scale);
    match rasterize_file(&input_path, &final_output, time, scale, frames_count, duration, force_static) {
        Ok(_) => {
            println!("✨ Done! Created image at '{}'.\n", final_output.display());
        }
        Err(e) => {
            eprintln!("\n❌ Rasterization failed: {}\n", e);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Premultiplied pixel read.
    fn px(pixmap: &Pixmap, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let d = pixmap.data();
        let i = ((y * pixmap.width() + x) * 4) as usize;
        (d[i], d[i + 1], d[i + 2], d[i + 3])
    }

    /// Regression: `fill pattern` must tile for real in the PNG exporter, not
    /// collapse to the neutral-gray fallback.
    #[test]
    fn pattern_fill_tiles_rect() {
        let src = "PVG 0.2\ncanvas 64 64\n  background #000000\npattern gridp 16 16\n  line\n    from [0, 0]\n    to [16, 0]\n    stroke #ffffff\n    width 2\nrectangle\n  pos [0, 0]\n  size [64, 64]\n  fill pattern gridp\n";
        let dl = compile_pvg_at_time(src, 0.0).unwrap();
        assert_eq!(dl.patterns.len(), 1);
        assert_eq!(dl.patterns[0].tiles.len(), 1);
        let pxm = rasterize_draw_list(&dl, 1.0).unwrap();
        // y=0 row carries the horizontal tile line -> near-white.
        let (r, g, b, a) = px(&pxm, 32, 0);
        assert_eq!(a, 255, "tile line must be opaque");
        assert!(r > 200 && g > 200 && b > 200, "tile line must be white, got {},{},{}", r, g, b);
        // Mid-tile gap shows the black background, not gray fallback.
        let (r, g, b, _) = px(&pxm, 32, 8);
        assert!(r < 40 && g < 40 && b < 40, "tile gap must be background, got {},{},{}", r, g, b);
    }
}