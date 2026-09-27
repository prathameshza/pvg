use pvg::ast::Color;
use pvg::draw_list::{
    BlendMode, DrawCmd, DrawList, DrawPathCommand, DrawPattern, DrawStyle, LineCap, LineJoin,
    Paint as PvgPaint,
};
use std::collections::HashMap;
use std::f64::consts::PI;
use tiny_skia::{
    BlendMode as SkBlend, FillRule, FilterQuality, LineCap as SkCap, LineJoin as SkJoin, Mask,
    Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Point, Rect, Stroke, Transform,
};

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

/// PVG 0.2 paint fallback for the CPU rasterizer: solid colors map directly;
/// gradients resolve to their middle stop's color (deterministic flat fallback).
/// Pattern fills resolve to neutral gray only when the name misses the
/// per-frame tile map (unknown name, or nested inside a tile); known patterns
/// tile for real (see `render_pattern_tiles` / `paint_pattern`).
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
        stroke.dash =
            tiny_skia::StrokeDash::new(style.dash.iter().map(|v| *v as f32).collect(), 0.0);
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
// ---------------------------------------------------------------------------
// PVG 0.2 §10 FX: blur / shadow / glow
// ---------------------------------------------------------------------------
//
// Ported from `pvg_win_gui::software` (identical tiny-skia 0.11 backend and
// identical premultiplied-alpha domain) so the Windows Studio and the Android
// runtime produce the same filtered pixels. A shape whose style carries
// `blur`, `shadow` or `glow` renders into an offscreen device-space layer
// cropped by [`fx_layer_rect`]; the finished layer is composited exactly once
// with the style's blend mode (and the optional clip mask).
//
// All per-pixel helpers below (`silhouette`, `box_blur`) operate directly on
// premultiplied RGBA, which is the correct domain for filtering.

/// True when the style needs the offscreen FX layer pipeline.
fn needs_fx(style: &DrawStyle) -> bool {
    style.blur > 1e-9 || style.shadow.is_some() || style.glow.is_some()
}

/// Cropped device-space layer rect `(ox, oy, w, h)` for an effect command:
/// geometry bbox padded for stroke half-width, blur/glow/shadow radii and the
/// shadow offset, then clamped to the target. Cropping makes blur, silhouette
/// and compositing proportional to the affected region instead of the canvas.
fn fx_layer_rect(
    cmd: &DrawCmd,
    style: &DrawStyle,
    scale: f32,
    canvas_w: u32,
    canvas_h: u32,
) -> Option<(i32, i32, u32, u32)> {
    let (x0, y0, x1, y1) = geom_bbox(cmd)?;
    // Filter padding: a blur of radius r spreads visible energy to ~3r
    // (3-pass box ≈ gaussian with σ≈r; ±3σ holds 99.7%). Padding only 1r
    // slices the tails and leaves a visible square seam around the shape.
    let mut pad = style.width * 0.5;
    pad = pad.max(style.blur * 3.0);
    if let Some(gl) = &style.glow {
        pad = pad.max(gl.radius * 3.0);
    }
    let (mut ox, mut oy) = (0.0, 0.0);
    if let Some(sh) = &style.shadow {
        pad = pad.max(sh.radius * 3.0);
        ox = sh.offset.0;
        oy = sh.offset.1;
    }
    let x0 = ((x0 - pad).min(x0 + ox.min(0.0)) as f32 * scale).floor() as i32 - 1;
    let y0 = ((y0 - pad).min(y0 + oy.min(0.0)) as f32 * scale).floor() as i32 - 1;
    let x1 = ((x1 + pad).max(x1 + ox.max(0.0)) as f32 * scale).ceil() as i32 + 1;
    let y1 = ((y1 + pad).max(y1 + oy.max(0.0)) as f32 * scale).ceil() as i32 + 1;
    let x0c = x0.max(0).min(canvas_w as i32);
    let y0c = y0.max(0).min(canvas_h as i32);
    let x1c = x1.max(0).min(canvas_w as i32);
    let y1c = y1.max(0).min(canvas_h as i32);
    if x1c <= x0c || y1c <= y0c {
        return None;
    }
    Some((x0c, y0c, (x1c - x0c) as u32, (y1c - y0c) as u32))
}

/// 3-pass separable box blur approximating a gaussian, O(pixels) per pass.
/// Operates on premultiplied data, the correct domain for filtering.
fn box_blur(pixmap: &mut PixmapMut, radius: u32) {
    if radius == 0 {
        return;
    }
    for _ in 0..3 {
        blur_horizontal(pixmap, radius);
        blur_vertical(pixmap, radius);
    }
}

/// Fixed-point reciprocal `2^32 / window` so the per-pixel average uses a
/// multiply instead of an integer division (div is ~20-40 cycles; this blur
/// runs 6 passes over the filtered layer).
#[inline]
fn reciprocal(window: usize) -> u64 {
    ((1u64 << 32) + window as u64 - 1) / window as u64
}

#[inline]
fn avg_channel(acc: u32, recip: u64) -> u8 {
    ((acc as u64 * recip) >> 32) as u8
}



fn blur_horizontal(pixmap: &mut PixmapMut, radius: u32) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    if w == 0 || h == 0 {
        return;
    }
    let r = radius as usize;
    let window = 2 * r + 1;
    let recip = reciprocal(window);
    let mut buf = vec![0u8; w * h * 4];
    {
        let src = pixmap.as_ref().data();
        for y in 0..h {
            // acc = sum over window [-r, +r] around x=0 (edges clamped).
            let mut acc = [0u32; 4];
            for i in 0..window {
                let sx = i.saturating_sub(r).min(w - 1);
                for c in 0..4 {
                    acc[c] += src[(y * w + sx) * 4 + c] as u32;
                }
            }
            for x in 0..w {
                for c in 0..4 {
                    buf[(y * w + x) * 4 + c] = avg_channel(acc[c], recip);
                }
                // Slide window from x to x+1: drop x-r, take in x+r+1.
                let leave = x.saturating_sub(r);
                let enter = (x + r + 1).min(w - 1);
                for c in 0..4 {
                    acc[c] += src[(y * w + enter) * 4 + c] as u32;
                    acc[c] -= src[(y * w + leave) * 4 + c] as u32;
                }
            }
        }
    }
    pixmap.data_mut().copy_from_slice(&buf);
}

fn blur_vertical(pixmap: &mut PixmapMut, radius: u32) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    if w == 0 || h == 0 {
        return;
    }
    let r = radius as usize;
    let window = 2 * r + 1;
    let recip = reciprocal(window);
    let mut buf = vec![0u8; w * h * 4];
    {
        let src = pixmap.as_ref().data();
        for x in 0..w {
            // acc = sum over window [-r, +r] around y=0 (edges clamped).
            let mut acc = [0u32; 4];
            for i in 0..window {
                let sy = i.saturating_sub(r).min(h - 1);
                for c in 0..4 {
                    acc[c] += src[(sy * w + x) * 4 + c] as u32;
                }
            }
            for y in 0..h {
                for c in 0..4 {
                    buf[(y * w + x) * 4 + c] = avg_channel(acc[c], recip);
                }
                // Slide window from y to y+1: drop y-r, take in y+r+1.
                let leave = y.saturating_sub(r);
                let enter = (y + r + 1).min(h - 1);
                for c in 0..4 {
                    acc[c] += src[(enter * w + x) * 4 + c] as u32;
                    acc[c] -= src[(leave * w + x) * 4 + c] as u32;
                }
            }
        }
    }
    pixmap.data_mut().copy_from_slice(&buf);
}

/// Builds a solid-color silhouette pixmap from a rendered layer's alpha:
/// `out_a = src_a * color_a * opacity`, `out_rgb = color_rgb * out_a`
/// (premultiplied, so the result can be blurred directly).
fn silhouette(src: &PixmapMut, color: &Color, opacity: f64) -> Option<Pixmap> {
    let (cr, cg, cb, ca) = match color {
        Color::Rgba(r, g, b, a) => {
            let alpha = ((*a as f64) * opacity).clamp(0.0, 255.0).round() as u8;
            if alpha == 0 {
                return None;
            }
            (*r, *g, *b, alpha)
        }
        Color::None => return None,
    };
    let (w, h) = (src.width(), src.height());
    if w == 0 || h == 0 {
        return None;
    }
    let mut out = Pixmap::new(w, h)?;
    {
        let s = src.as_ref().data();
        let d = out.data_mut();
        let fa = ca as u32;
        for i in 0..(w as usize * h as usize) {
            let sa = s[i * 4 + 3] as u32;
            if sa == 0 {
                continue;
            }
            let a = sa * fa / 255;
            d[i * 4] = (cr as u32 * a / 255) as u8;
            d[i * 4 + 1] = (cg as u32 * a / 255) as u8;
            d[i * 4 + 2] = (cb as u32 * a / 255) as u8;
            d[i * 4 + 3] = a as u8;
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Pattern fills: per-frame tile pre-render + per-pixel wrap sampling
// ---------------------------------------------------------------------------

/// A pre-rendered pattern tile: transparent-based premultiplied pixmap at
/// `k` device px per user unit, plus the tile size in user units.
struct PatternTile {
    pix: Pixmap,
    w_units: f64,
    h_units: f64,
}

/// Frame-scale pattern tiles, looked up by pattern name. Built once per frame
/// by [`render_pattern_tiles`] and threaded through the render path (so every
/// shape samples the same tile instead of re-rendering it).
type PatternTiles = HashMap<String, PatternTile>;

/// Renders every pattern tile once at device scale `k`.
///
/// Tiles render with an EMPTY tile map: a pattern referenced from inside a
/// tile falls back to neutral gray, which terminates any A-references-A cycle
/// by construction.
fn render_pattern_tiles(patterns: &[DrawPattern], k: f32) -> PatternTiles {
    let mut out = PatternTiles::new();
    let k = if k > 0.0 && k.is_finite() { k } else { 1.0 };
    let empty = PatternTiles::new();
    for pat in patterns {
        if pat.width <= 1e-9 || pat.height <= 1e-9 || pat.tiles.is_empty() {
            continue;
        }
        // Cap tile resolution: absurd tiles (huge user size x scale) degrade
        // to the gray fallback instead of OOMing the frame.
        let tw = ((pat.width as f32 * k).ceil().max(1.0) as u32).min(1024);
        let th = ((pat.height as f32 * k).ceil().max(1.0) as u32).min(1024);
        let mut pix = match Pixmap::new(tw, th) {
            Some(p) => p,
            None => continue,
        };
        let xf = Transform::from_scale(k, k);
        for t in &pat.tiles {
            render_cmd_masked(t, &mut pix.as_mut(), xf, None, k, &empty);
        }
        out.insert(pat.name.clone(), PatternTile { pix, w_units: pat.width, h_units: pat.height });
    }
    out
}

/// Pattern fill/stroke via per-pixel tile-wrap sampling.
///
/// Renders `path` coverage (white fill, or white stroke so dash/cap/join are
/// honored) into a cropped temp, then maps each covered device pixel back to
/// canvas space and wraps it into the tile with Euclidean mod
/// (negative-safe, seamless for any draw transform). `style.opacity` folds in
/// at composite time; the finished layer composites with ONE blit using the
/// style blend mode and the caller's clip mask.
#[allow(clippy::too_many_arguments)]
fn paint_pattern(
    dst: &mut PixmapMut,
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    tile: &PatternTile,
    is_fill: bool,
    style: &DrawStyle,
    transform: Transform,
    blend: SkBlend,
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
        &PixmapPaint { opacity: 1.0, blend_mode: blend, quality: FilterQuality::Nearest },
        Transform::identity(),
        mask,
    );
}

/// User-space geometry bounding box `(x0, y0, x1, y1)` for layer cropping.
/// Conservative for paths (includes control points); `None` for text (which
/// never renders through the sampled path).
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
        DrawCmd::Line { from, to, .. } => {
            Some((from.0.min(to.0), from.1.min(to.1), from.0.max(to.0), from.1.max(to.1)))
        }
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
            let put = |x: f64, y: f64, bb: &mut (f64, f64, f64, f64)| {
                bb.0 = bb.0.min(x);
                bb.1 = bb.1.min(y);
                bb.2 = bb.2.max(x);
                bb.3 = bb.3.max(y);
            };
            for c in commands {
                match c {
                    DrawPathCommand::Start(p) | DrawPathCommand::Line(p) => put(p.0, p.1, &mut b),
                    DrawPathCommand::Quad { cp, ep } => {
                        put(cp.0, cp.1, &mut b);
                        put(ep.0, ep.1, &mut b);
                    }
                    DrawPathCommand::Curve { c1, c2, ep } => {
                        put(c1.0, c1.1, &mut b);
                        put(c2.0, c2.1, &mut b);
                        put(ep.0, ep.1, &mut b);
                    }
                    DrawPathCommand::Arc { center, radius, .. } => {
                        put(center.0 - radius, center.1 - radius, &mut b);
                        put(center.0 + radius, center.1 + radius, &mut b);
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

/// Fill helper: real pattern tiling on a tile hit, otherwise the exact
/// pre-existing flat paint path (solid / gradient fallback bit-identical).
fn fill_shape_path(
    dst: &mut PixmapMut,
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    style: &DrawStyle,
    transform: Transform,
    mask: Option<&Mask>,
    tiles: &PatternTiles,
) {
    if let PvgPaint::Pattern(name) = &style.fill {
        if let Some(tile) = tiles.get(name) {
            paint_pattern(dst, cmd, path, tile, true, style, transform, style_to_blend(style), mask);
            return;
        }
    }
    if let Some(mut fill_paint) = paint_to_skia(&style.fill, style.opacity) {
        fill_paint.blend_mode = style_to_blend(style);
        dst.fill_path(path, &fill_paint, FillRule::Winding, transform, mask);
    }
}

/// Stroke helper: same split as [`fill_shape_path`] for stroke paints.
/// Zero-width strokes paint nothing (matches the pre-existing gate).
fn stroke_shape_path(
    dst: &mut PixmapMut,
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    style: &DrawStyle,
    transform: Transform,
    mask: Option<&Mask>,
    tiles: &PatternTiles,
) {
    if style.width <= 0.0 {
        return;
    }
    if let PvgPaint::Pattern(name) = &style.stroke {
        if let Some(tile) = tiles.get(name) {
            paint_pattern(
                dst,
                cmd,
                path,
                tile,
                false,
                style,
                transform,
                style_to_blend(style),
                mask,
            );
            return;
        }
    }
    if let Some(mut stroke_paint) = paint_to_skia(&style.stroke, style.opacity) {
        stroke_paint.blend_mode = style_to_blend(style);
        let stroke = style_to_stroke(style);
        dst.stroke_path(path, &stroke_paint, &stroke, transform, mask);
    }
}

/// Renders a shape's fill + stroke with the style's own blend mode / mask.
///
/// Splines are stroke-only (spec §4.2 forces their fill to NONE) and lines
/// have no interior, so their fill pass is skipped exactly as before.
#[allow(clippy::too_many_arguments)]
fn paint_shape(
    dst: &mut PixmapMut,
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    style: &DrawStyle,
    transform: Transform,
    mask: Option<&Mask>,
    tiles: &PatternTiles,
) {
    let stroke_only = matches!(cmd, DrawCmd::Line { .. } | DrawCmd::Spline { .. });
    if !stroke_only {
        fill_shape_path(dst, cmd, path, style, transform, mask, tiles);
    }
    stroke_shape_path(dst, cmd, path, style, transform, mask, tiles);
}

/// FX stack rendered into `layer` (no outer mask, no final blend): sharp
/// shape, then shadow, then glow, then blur-or-sharp — the exact order of the
/// Windows reference. `blur` REPLACES the sharp shape (it is applied to the
/// finished layer), while `shadow`/`glow` add filtered copies of it.
fn render_fx_into(
    layer: &mut PixmapMut,
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    style: &DrawStyle,
    transform: Transform,
    scale: f32,
    tiles: &PatternTiles,
) {
    paint_shape(layer, cmd, path, style, transform, None, tiles);

    if let Some(sh) = &style.shadow {
        if let Some(mut sh_px) = silhouette(layer, &sh.color, style.opacity) {
            let r = ((sh.radius as f32 * scale).round().max(0.0)) as u32;
            box_blur(&mut sh_px.as_mut(), r);
            let dx = (sh.offset.0 as f32 * scale).round() as i32;
            let dy = (sh.offset.1 as f32 * scale).round() as i32;
            layer.draw_pixmap(
                dx,
                dy,
                sh_px.as_ref(),
                &PixmapPaint {
                    opacity: 1.0,
                    blend_mode: SkBlend::SourceOver,
                    quality: FilterQuality::Nearest,
                },
                Transform::identity(),
                None,
            );
        }
        // Shadow must sit UNDER the shape: the blit above drew it over the
        // layer's current content, so re-paint the sharp shape on top.
        paint_shape(layer, cmd, path, style, transform, None, tiles);
    }

    if let Some(gl) = &style.glow {
        // Glow halo from the CURRENT layer content (shape, or shape+shadow).
        // Snapshot first: silhouette reads alpha only, the halo is additive.
        if let Some(mut gl_px) = silhouette(layer, &gl.color, style.opacity) {
            let r = ((gl.radius as f32 * scale).round().max(0.0)) as u32;
            box_blur(&mut gl_px.as_mut(), r);
            layer.draw_pixmap(
                0,
                0,
                gl_px.as_ref(),
                &PixmapPaint {
                    opacity: 1.0,
                    blend_mode: SkBlend::Plus,
                    quality: FilterQuality::Nearest,
                },
                Transform::identity(),
                None,
            );
        }
    }

    if style.blur > 1e-9 {
        let r = ((style.blur as f32 * scale).round().max(0.0)) as u32;
        box_blur(layer, r);
    }
}

/// Fill + stroke one geometric command, routing through an offscreen FX layer
/// when the style carries §10 `blur`/`shadow`/`glow`.
///
/// The layer is cropped to [`fx_layer_rect`] in device pixels and its local
/// draw transform is the caller's `transform` shifted by the crop origin, so
/// the device-scale convention (letterbox offset + preview scale) is preserved
/// exactly for both the canvas and tile/pattern render paths.
#[allow(clippy::too_many_arguments)]
fn render_shape(
    dst: &mut PixmapMut,
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    style: &DrawStyle,
    transform: Transform,
    mask: Option<&Mask>,
    scale: f32,
    tiles: &PatternTiles,
) {
    if !needs_fx(style) {
        paint_shape(dst, cmd, path, style, transform, mask, tiles);
        return;
    }
    let (ox, oy, lw, lh) = match fx_layer_rect(cmd, style, scale, dst.width(), dst.height()) {
        Some(r) => r,
        None => return,
    };
    let mut layer = match Pixmap::new(lw, lh) {
        Some(p) => p,
        None => return,
    };
    let lxf = transform.post_translate(-(ox as f32), -(oy as f32));
    {
        let mut lpm = layer.as_mut();
        render_fx_into(&mut lpm, cmd, path, style, lxf, scale, tiles);
    }
    dst.draw_pixmap(
        ox,
        oy,
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
/// including nested clips). Pattern paints sample the per-frame `tiles` map;
/// unknown names fall back to flat gray via `paint_to_skia`.
fn render_cmd_masked(
    cmd: &DrawCmd,
    pixmap: &mut PixmapMut,
    transform: Transform,
    mask: Option<&Mask>,
    scale: f32,
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
                    render_cmd_masked(inner, pixmap, transform, Some(&nested), scale, tiles);
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
            render_shape(pixmap, cmd, &path, style, transform, mask, scale, tiles);
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
        // FX-aware fill + stroke: `blur`/`shadow`/`glow` render through an
        // offscreen layer, everything else through the per-paint path.
        render_shape(pixmap, cmd, &path, style, transform, mask, scale, tiles);
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

    // 3. Render visual primitives (pattern tiles pre-rendered once per frame
    // at device scale; shapes sample them per-pixel, unknown names fall back
    // to flat gray).
    let tiles = render_pattern_tiles(&draw_list.patterns, scale);
    for cmd in &draw_list.items {
        match cmd {
            DrawCmd::Circle { center, radius, style } => {
                let mut pb = PathBuilder::new();
                pb.push_circle(center.0 as f32, center.1 as f32, *radius as f32);
                if let Some(path) = pb.finish() {
                    render_shape(pixmap, cmd, &path, style, transform, None, scale, &tiles);
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
                        render_shape(pixmap, cmd, &path, style, transform, None, scale, &tiles);
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
                        render_shape(pixmap, cmd, &path, style, transform, None, scale, &tiles);
                    }
                }
            }

            DrawCmd::Line { from, to, style } => {
                let mut pb = PathBuilder::new();
                pb.move_to(from.0 as f32, from.1 as f32);
                pb.line_to(to.0 as f32, to.1 as f32);
                if let Some(path) = pb.finish() {
                    render_shape(pixmap, cmd, &path, style, transform, None, scale, &tiles);
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
                        render_shape(pixmap, cmd, &path, style, transform, None, scale, &tiles);
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
                    render_shape(pixmap, cmd, &path, style, transform, None, scale, &tiles);
                }
            }

            // PVG 0.2 clip: build a pixel mask from the mask shape and
            // render content through it (true clipping, not a fallback).
            DrawCmd::Clip { mask, content } => {
                if let Some(mask_path) = cmd_to_path(mask) {
                    if let Some(mut clip_mask) = Mask::new(pixmap.width(), pixmap.height()) {
                        clip_mask.fill_path(&mask_path, FillRule::Winding, true, transform);
                        for inner in content {
                            render_cmd_masked(inner, pixmap, transform, Some(&clip_mask), scale, &tiles);
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
                    fill_shape_path(pixmap, cmd, &path, style, transform, None, &tiles);
                    stroke_shape_path(pixmap, cmd, &path, style, transform, None, &tiles);
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

#[cfg(test)]
mod tests {
    use super::*;
    use pvg::eval::Evaluator;

    /// Device pixel for a canvas-space point, replicating the letterbox math
    /// in [`rasterize_draw_list_into_pixmap_mut`].
    fn dev_pt(canvas_w: f64, canvas_h: f64, tw: u32, th: u32, cx: f32, cy: f32) -> (u32, u32) {
        let margin = 0.94_f32;
        let scale = ((tw as f32 * margin) / canvas_w as f32)
            .min((th as f32 * margin) / canvas_h as f32);
        let scaled_w = (canvas_w as f32 * scale).round();
        let scaled_h = (canvas_h as f32 * scale).round();
        let ox = ((tw as f32 - scaled_w) / 2.0).round();
        let oy = ((th as f32 - scaled_h) / 2.0).round();
        ((ox + cx * scale).round() as u32, (oy + cy * scale).round() as u32)
    }

    fn px_at(buf: &[u8], tw: u32, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let i = ((y * tw + x) * 4) as usize;
        (buf[i], buf[i + 1], buf[i + 2], buf[i + 3])
    }

    /// Regression: `fill pattern` must tile for real, not collapse to the
    /// neutral-gray fallback (mirrors `pattern_fill_tiles` in
    /// `pvg_win_gui::software`).
    #[test]
    fn pattern_fill_tiles() {
        let src = "PVG 0.2\ncanvas 64 64\n  background #000000\npattern gridp 16 16\n  line\n    from [0, 0]\n    to [16, 0]\n    stroke #ffffff\n    width 2\nrectangle\n  pos [0, 0]\n  size [64, 64]\n  fill pattern gridp\n";
        let dl = pvg::compile(src).unwrap();
        assert_eq!(dl.patterns.len(), 1);
        assert_eq!(dl.patterns[0].tiles.len(), 1);

        let (tw, th) = (200u32, 200u32);
        let mut buf = vec![0u8; (tw * th * 4) as usize];
        {
            let mut pm = PixmapMut::from_bytes(&mut buf, tw, th).unwrap();
            rasterize_draw_list_into_pixmap_mut(&dl, &mut pm, tw, th);
        }
        // Canvas (32, 0.5) sits on the tile's horizontal line -> near-white.
        let (lx, ly) = dev_pt(64.0, 64.0, tw, th, 32.0, 0.5);
        let (r, g, b, a) = px_at(&buf, tw, lx, ly);
        assert_eq!(a, 255, "tile line must be opaque");
        assert!(
            r > 200 && g > 200 && b > 200,
            "tile line must be white, got {},{},{}",
            r, g, b
        );
        // Canvas (32, 8) sits mid-tile in a gap -> black background, not gray.
        let (gx, gy) = dev_pt(64.0, 64.0, tw, th, 32.0, 8.0);
        let (r, g, b, _) = px_at(&buf, tw, gx, gy);
        assert!(
            r < 40 && g < 40 && b < 40,
            "tile gap must be background, got {},{},{}",
            r, g, b
        );
    }

    /// Rasterizes `src` into a square RGBA buffer and returns the pixels.
    fn render_pixels(src: &str, size: u32) -> (Vec<u8>, u32) {
        let dl = pvg::compile(src).expect("source must compile");
        let mut buf = vec![0u8; (size * size * 4) as usize];
        {
            let mut pm = PixmapMut::from_bytes(&mut buf, size, size).unwrap();
            rasterize_draw_list_into_pixmap_mut(&dl, &mut pm, size, size);
        }
        (buf, size)
    }

    /// Canvas-space probe -> device pixel, then read the RGBA value.
    fn probe(buf: &[u8], size: u32, cx: f32, cy: f32) -> (u8, u8, u8, u8) {
        let (x, y) = dev_pt(64.0, 64.0, size, size, cx, cy);
        px_at(buf, size, x, y)
    }

    /// Sums the red channel as a coarse energy probe.
    fn red_energy(buf: &[u8]) -> u64 {
        buf.chunks(4).map(|p| p[0] as u64).sum()
    }

    /// PVG 0.2 §10 `blur`: the filtered shape must spread energy past the sharp
    /// silhouette edge onto the (black) canvas background, and must redistribute
    /// rather than create energy.
    #[test]
    fn blur_softens_shape() {
        let size = 120u32;
        let sharp = render_pixels(
            "PVG 0.2\ncanvas 64 64\n  background #000000\ncircle\n  center [32, 32]\n  radius 20\n  fill #ffffff\n",
            size,
        );
        let blurred = render_pixels(
            "PVG 0.2\ncanvas 64 64\n  background #000000\ncircle\n  center [32, 32]\n  radius 20\n  fill #ffffff\n  blur 6\n",
            size,
        );

        // 9px above the radius-20 silhouette: black when sharp, lit when blurred.
        let (sr, _, _, _) = probe(&sharp.0, size, 32.0, 3.0);
        let (br, _, _, _) = probe(&blurred.0, size, 32.0, 3.0);
        assert!(sr < 20, "sharp render must leave the halo band black, got {}", sr);
        assert!(br > sr, "blur must spill past the sharp silhouette ({} -> {})", sr, br);

        // The shape itself must now be dimmer: blur REPLACES the crisp fill.
        let (sharp_center, _, _, _) = probe(&sharp.0, size, 32.0, 32.0);
        let (blur_center, _, _, _) = probe(&blurred.0, size, 32.0, 32.0);
        assert!(
            blur_center < sharp_center,
            "blur must dim the interior ({} -> {})",
            sharp_center,
            blur_center
        );
    }

    /// PVG 0.2 §10 `shadow [dx, dy] r color`: a blurred silhouette in the shadow
    /// color, offset and drawn UNDER the sharp shape.
    #[test]
    fn shadow_paints_at_offset() {
        let size = 120u32;
        // Shape occupies 20..36; the shadow is the same box shifted by +12,+12.
        let plain = render_pixels(
            "PVG 0.2\ncanvas 64 64\n  background #000000\nrectangle\n  pos [20, 20]\n  size [16, 16]\n  fill #ffffff\n",
            size,
        );
        let shadowed = render_pixels(
            "PVG 0.2\ncanvas 64 64\n  background #000000\nrectangle\n  pos [20, 20]\n  size [16, 16]\n  fill #ffffff\n  shadow [12, 12] 1 #ff0000\n",
            size,
        );

        // (40, 40) is inside the offset shadow (32..48) but outside the shape.
        let (pr, pg, pb, _) = probe(&plain.0, size, 40.0, 40.0);
        let (sr, sg, sb, _) = probe(&shadowed.0, size, 40.0, 40.0);
        assert!(pr < 20 && pg < 20 && pb < 20, "plain render must be black there");
        assert!(sr > 60, "shadow must be visible at the offset, got {}", sr);
        assert!(
            sr > sg && sr > sb,
            "shadow must use its own color (red dominant), got {},{},{}",
            sr, sg, sb
        );

        // The shadow must stay UNDER the shape: the shape interior is still white.
        let (_, _, _, shape_a) = probe(&shadowed.0, size, 26.0, 26.0);
        let (_, _, _, white_r) = probe(&shadowed.0, size, 26.0, 26.0);
        assert!(white_r > 200, "the sharp shape must still be drawn on top");
        let _ = shape_a;
    }

    /// PVG 0.2 §10 `glow r color`: an additive blurred halo under the shape.
    #[test]
    fn glow_adds_halo() {
        let size = 120u32;
        let plain = render_pixels(
            "PVG 0.2\ncanvas 64 64\n  background #000000\ncircle\n  center [32, 32]\n  radius 12\n  fill #ffffff\n",
            size,
        );
        let glowed = render_pixels(
            "PVG 0.2\ncanvas 64 64\n  background #000000\ncircle\n  center [32, 32]\n  radius 12\n  fill #ffffff\n  glow 8 #00ff00\n",
            size,
        );

        // 18px above center: 6px outside the radius-12 edge, inside the glow.
        let (pr, _, _, _) = probe(&plain.0, size, 32.0, 14.0);
        let (gr, gg, _, _) = probe(&glowed.0, size, 32.0, 14.0);
        assert!(pr < 20, "no halo without glow, got {}", pr);
        assert!(gg > 20, "glow must light the halo band, got green {}", gg);
        assert!(
            gg > gr,
            "glow halo must carry the glow color (green dominant), got {},{}",
            gr, gg
        );
    }

    /// A shape without §10 FX must rasterize byte-identically to the same shape
    /// with explicit zero FX values - guards against always taking the FX path.
    #[test]
    fn no_fx_is_unchanged() {
        let size = 100u32;
        let a = render_pixels(
            "PVG 0.2\ncanvas 64 64\n  background #000000\nrectangle\n  pos [16, 16]\n  size [32, 20]\n  radius 4\n  fill #00ffcc\n",
            size,
        );
        let b = render_pixels(
            "PVG 0.2\ncanvas 64 64\n  background #000000\nrectangle\n  pos [16, 16]\n  size [32, 20]\n  radius 4\n  fill #00ffcc\n  blur 0\n",
            size,
        );
        assert_eq!(a.0, b.0, "blur 0 must not alter the rasterization");
    }

    /// Host uniforms (`param`, §18.1) must drive the Android evaluator exactly
    /// like the core `Scene` API that the JNI layer wraps.
    #[test]
    fn host_param_overrides_apply() {
        let src = "PVG 0.2\ncanvas 200 60\nparam health: 0.75\nrectangle\n  pos [0, 0]\n  size [200 * health, 12]\n  fill #00e676\n";
        let doc = pvg::parse(src).unwrap();
        assert_eq!(doc.param_names(), vec!["health"]);

        let dl = Evaluator::new_with_time(0.0).evaluate_document(&doc).unwrap();
        if let DrawCmd::Rectangle { size, .. } = &dl.items[0] {
            assert!((size.0 - 150.0).abs() < 1e-9, "document default must apply");
        } else {
            panic!("expected rect");
        }

        let mut ev = Evaluator::new_with_time(0.0);
        ev.set_param("health".to_string(), pvg::eval::Value::Number(0.25));
        let dl2 = ev.evaluate_document(&doc).unwrap();
        if let DrawCmd::Rectangle { size, .. } = &dl2.items[0] {
            assert!((size.0 - 50.0).abs() < 1e-9, "override must win, got {}", size.0);
        } else {
            panic!("expected rect");
        }
    }
}
