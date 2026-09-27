//! Full-fidelity PVG 0.2 CPU software rasterizer (tiny-skia backend).
//!
//! Unlike the flat-fallback rasterizers elsewhere in the workspace, this module
//! implements every 0.2 visual feature for real:
//!
//! | Feature | Implementation |
//! | :--- | :--- |
//! | `linear` gradients | native `LinearGradient` shader (user-space, scale-baked) |
//! | `radial` gradients (+focal) | native two-point-conical `RadialGradient` shader |
//! | `angular` gradients | exact per-pixel conic evaluation into a layer |
//! | `cap`/`join`/`miter`/`dash` | native `Stroke` (+`StrokeDash`) |
//! | `blend` | native `Paint.blend_mode` + `PixmapPaint.blend_mode` on composites |
//! | `clip` | `Mask` from the mask shape, threaded through content |
//! | `blur`/`glow`/`shadow` | shape layer + 3-pass separable box blur (O(N)) |
//! | `pattern` fills | tile pre-rendered once per frame, per-pixel wrap sampling |
//!
//! Coordinate convention: PVG user-space geometry is rasterized with a uniform
//! `Transform::from_scale(scale, scale)` draw transform. Gradient control
//! points therefore have to be pre-scaled by `scale` (shader transform stays
//! identity) so shader space and device space agree.
//!
//! Premultiplied-alpha note: `Pixmap` stores premultiplied RGBA. All per-pixel
//! helpers below (`silhouette`, `box_blur`, angular fill) operate directly on
//! premultiplied data, which is the correct domain for filtering.

use pvg::ast::Color as PvgColor;
use pvg::draw_list::{
    BlendMode as PvgBlend, DrawCmd, DrawList, DrawPathCommand, DrawPattern, DrawStyle,
    LineCap as PvgCap, LineJoin as PvgJoin, Paint as PvgPaint,
};
use std::collections::HashMap;
use std::f64::consts::TAU as TAU_F64;
use tiny_skia::{
    BlendMode as SkBlend, FillRule, FilterQuality, LineCap as SkCap, LineJoin as SkJoin, Mask,
    Paint as SkPaint, Path, PathBuilder, Pixmap, PixmapPaint, PixmapRef, Point,
    Rect as SkRect, Stroke as SkStroke, Transform,
};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Renders a `DrawList` into a device-pixel `Pixmap` at uniform `scale`.
///
/// Text primitives are skipped (the Studio overlays them with real-font egui
/// text in correct screen position; see `renderer::draw_text_overlay`).
pub fn render_pixmap(draw_list: &DrawList, scale: f32) -> Result<Pixmap, String> {
    let scale = if scale <= 0.0 || !scale.is_finite() {
        1.0
    } else {
        scale
    };
    let width = ((draw_list.canvas_width as f32) * scale).round().max(1.0) as u32;
    let height = ((draw_list.canvas_height as f32) * scale).round().max(1.0) as u32;
    if width == 0 || height == 0 {
        return Err("Canvas dimensions must be greater than zero".into());
    }
    let mut pixmap =
        Pixmap::new(width, height).ok_or_else(|| format!("Failed to allocate {}x{}", width, height))?;
    let transform = Transform::from_scale(scale, scale);

    if let Some(ref bg) = draw_list.background {
        if let Some(bg_paint) = solid_paint(bg, 1.0) {
            if let Some(rect) = SkRect::from_xywh(0.0, 0.0, width as f32, height as f32) {
                pixmap.fill_rect(rect, &bg_paint, Transform::identity(), None);
            }
        }
    }

    let tiles = render_pattern_tiles(&draw_list.patterns, scale);
    for cmd in &draw_list.items {
        render_cmd(&mut pixmap, cmd, transform, scale, None, None, &tiles);
    }
    Ok(pixmap)
}

// ---------------------------------------------------------------------------
// Color / paint helpers
// ---------------------------------------------------------------------------

/// Straight (non-premultiplied) RGBA with style opacity folded into alpha.
fn straight_rgba(c: &PvgColor, opacity: f64) -> Option<(u8, u8, u8, u8)> {
    match c {
        PvgColor::Rgba(r, g, b, a) => {
            let alpha = ((*a as f64) * opacity).clamp(0.0, 255.0).round() as u8;
            if alpha == 0 {
                return None;
            }
            Some((*r, *g, *b, alpha))
        }
        PvgColor::None => None,
    }
}

fn solid_paint(c: &PvgColor, opacity: f64) -> Option<SkPaint<'static>> {
    let (r, g, b, a) = straight_rgba(c, opacity)?;
    let mut paint = SkPaint::default();
    paint.set_color_rgba8(r, g, b, a);
    paint.anti_alias = true;
    Some(paint)
}

/// Flat fallback for `Pattern` paints that never reach the sampler: patterns
/// referenced from *inside* a tile (cycle guard in [`render_pattern_tiles`])
/// or unknown names resolve to neutral gray. Gradients return `None` here
/// (handled per-pixel via `grad_cfg`).
fn solid_or_pattern_paint(paint: &PvgPaint, opacity: f64) -> Option<SkPaint<'static>> {
    match paint {
        PvgPaint::Color(c) => solid_paint(c, opacity),
        PvgPaint::Pattern(_) => solid_paint(&PvgColor::Rgba(136, 136, 136, 255), opacity),
        _ => None,
    }
}

/// Sorted, clamped, opacity-folded gradient stops as straight RGBA.
///
/// Fully transparent stops (`#rrggbbaa` with `aa == 00`) are MEANINGFUL
/// (fade-outs) and must be kept: dropping them collapses a fade like
/// `white -> transparent` into a single solid stop.
fn sampled_stops(
    stops: &[pvg::draw_list::GradientStop],
    opacity: f64,
) -> Vec<(f64, u8, u8, u8, u8)> {
    let mut sorted: Vec<&pvg::draw_list::GradientStop> = stops.iter().collect();
    sorted.sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Vec::with_capacity(sorted.len());
    for s in sorted {
        let (r, g, b, a) = match &s.color {
            PvgColor::Rgba(r, g, b, a) => {
                (*r, *g, *b, ((*a as f64) * opacity).clamp(0.0, 255.0).round() as u8)
            }
            PvgColor::None => (0, 0, 0, 0),
        };
        out.push((s.offset.clamp(0.0, 1.0), r, g, b, a));
    }
    out
}

/// Gradient geometry in PVG user space.
#[derive(Clone)]
enum GradKind {
    Linear { start: (f64, f64), end: (f64, f64) },
    Radial { focal: (f64, f64), center: (f64, f64), radius: f64 },
    Angular { center: (f64, f64), start_angle: f64 },
}

#[derive(Clone)]
struct GradCfg {
    stops: Vec<(f64, u8, u8, u8, u8)>,
    kind: GradKind,
}

fn grad_cfg(paint: &PvgPaint, opacity: f64) -> Option<GradCfg> {
    match paint {
        PvgPaint::Color(_) | PvgPaint::Pattern(_) => None,
        PvgPaint::Linear { start, end, stops } => {
            let stops = sampled_stops(stops, opacity);
            if stops.is_empty() {
                return None;
            }
            Some(GradCfg { stops, kind: GradKind::Linear { start: *start, end: *end } })
        }
        PvgPaint::Radial { center, radius, focal, stops } => {
            let stops = sampled_stops(stops, opacity);
            if stops.is_empty() {
                return None;
            }
            Some(GradCfg {
                stops,
                kind: GradKind::Radial {
                    focal: focal.unwrap_or(*center),
                    center: *center,
                    radius: *radius,
                },
            })
        }
        PvgPaint::Angular { center, start_angle, stops } => {
            let stops = sampled_stops(stops, opacity);
            if stops.is_empty() {
                return None;
            }
            Some(GradCfg {
                stops,
                kind: GradKind::Angular { center: *center, start_angle: *start_angle },
            })
        }
    }
}

/// Gradient parameter `t` in [0, 1] for a user-space point.
/// Degenerate linear (start == end) yields the last stop (Skia convention).
fn grad_t(kind: &GradKind, u: (f64, f64)) -> f64 {
    match *kind {
        GradKind::Linear { start, end } => {
            let dx = end.0 - start.0;
            let dy = end.1 - start.1;
            let len2 = dx * dx + dy * dy;
            if len2 < 1e-12 {
                return 1.0;
            }
            (((u.0 - start.0) * dx + (u.1 - start.1) * dy) / len2).clamp(0.0, 1.0)
        }
        GradKind::Radial { focal, center, radius } => {
            if radius <= 1e-9 {
                return 1.0;
            }
            let vx = center.0 - focal.0;
            let vy = center.1 - focal.1;
            let dx = u.0 - focal.0;
            let dy = u.1 - focal.1;
            let dc = (vx * vx + vy * vy).sqrt();
            if dc < 1e-9 {
                // Centered radial: plain distance.
                return ((dx * dx + dy * dy).sqrt() / radius).clamp(0.0, 1.0);
            }
            // Exact two-point conical (r0 = 0 at focal, r1 = radius):
            // |d - t·v|² = (t·R)².
            let vv = vx * vx + vy * vy;
            let rr = radius * radius;
            let denom = vv - rr;
            if denom.abs() < 1e-9 {
                return ((dx * dx + dy * dy).sqrt() / radius).clamp(0.0, 1.0);
            }
            let dv = dx * vx + dy * vy;
            let dd = dx * dx + dy * dy;
            let disc = dv * dv - denom * dd;
            if disc < 0.0 {
                return 1.0;
            }
            // Positive root (visible sheet): (dv - √disc) / denom.
            // Verified: focal -> 0, rim point c + R·v̂ -> exactly 1.
            ((dv - disc.sqrt()) / denom).clamp(0.0, 1.0)
        }
        GradKind::Angular { center, start_angle } => {
            let mut t = ((u.1 - center.1).atan2(u.0 - center.0) - start_angle) / TAU_F64;
            t -= t.floor();
            t.clamp(0.0, 1.0)
        }
    }
}

fn pvg_blend_to_skia(blend: PvgBlend) -> SkBlend {
    match blend {
        PvgBlend::Normal => SkBlend::SourceOver,
        PvgBlend::Add => SkBlend::Plus,
        PvgBlend::Multiply => SkBlend::Multiply,
        PvgBlend::Screen => SkBlend::Screen,
        PvgBlend::Overlay => SkBlend::Overlay,
    }
}

fn style_stroke(style: &DrawStyle) -> SkStroke {
    let mut stroke = SkStroke::default();
    stroke.width = style.width.max(0.0) as f32;
    stroke.line_cap = match style.cap {
        PvgCap::Butt => SkCap::Butt,
        PvgCap::Round => SkCap::Round,
        PvgCap::Square => SkCap::Square,
    };
    stroke.line_join = match style.join {
        PvgJoin::Miter => SkJoin::Miter,
        PvgJoin::Round => SkJoin::Round,
        PvgJoin::Bevel => SkJoin::Bevel,
    };
    stroke.miter_limit = style.miter.max(1.0) as f32;
    if !style.dash.is_empty() {
        let dashes: Vec<f32> = style.dash.iter().map(|v| *v as f32).collect();
        stroke.dash = tiny_skia::StrokeDash::new(dashes, 0.0);
    }
    stroke
}

/// User-space geometry bounding box `(x0, y0, x1, y1)` for layer cropping.
/// Conservative for paths (includes control points); `None` for text (which
/// never renders through the layer path).
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

/// Cropped device-space layer rect `(ox, oy, w, h)` for an effect command:
/// geometry bbox padded for stroke half-width, blur/glow/shadow radii and
/// shadow offset, clamped to the canvas. Cropping makes blur, silhouette and
/// compositing proportional to the affected region instead of the canvas.
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

/// Unified per-pixel gradient fill/stroke.
///
/// Renders `path` coverage (white fill, or white stroke for strokes, so dash
/// / cap / join are honored) into a temp, then evaluates the gradient in PVG
/// user space per covered pixel: `user = draw_transform⁻¹(pixel)`. This is
/// exact for translated/cropped layers, any preview scale, and focal/conic
/// geometries — independent of tiny-skia shader-transform quirks.
///
/// `is_fill` selects fill vs stroke coverage; `loop_bb` optionally bounds the
/// pixel loop to a device-space rect (pass the full target otherwise).
#[allow(clippy::too_many_arguments)]
fn paint_sampled(
    dst: &mut Pixmap,
    cmd: &DrawCmd,
    path: &Path,
    cfg: &GradCfg,
    is_fill: bool,
    style: &DrawStyle,
    transform: Transform,
    blend: PvgBlend,
    mask: Option<&Mask>,
) {
    let (w, h) = (dst.width(), dst.height());
    // Crop temps to the command's device-space bbox: plain gradient shapes
    // on a big canvas would otherwise pay two full-canvas allocs plus a
    // full-canvas composite every frame.
    let (rx0, ry0, rx1, ry1) = loop_rect(cmd, transform, w, h);
    let (rw, rh) = (rx1 - rx0, ry1 - ry0);
    let lxf = transform.post_translate(-(rx0 as f32), -(ry0 as f32));
    let mut coverage = match Pixmap::new(rw, rh) {
        Some(p) => p,
        None => return,
    };
    {
        let mut white = SkPaint::default();
        white.set_color_rgba8(255, 255, 255, 255);
        white.anti_alias = true;
        if is_fill {
            coverage.fill_path(path, &white, FillRule::Winding, lxf, None);
        } else {
            let stroke = style_stroke(style);
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
        let out = layer.data_mut();
        for y in 0..rh {
            for x in 0..rw {
                let ca = cov[(y as usize * rw as usize + x as usize) * 4 + 3] as f32 / 255.0;
                if ca <= 0.0 {
                    continue;
                }
                let mut pt = Point::from_xy(x as f32, y as f32);
                inv.map_point(&mut pt);
                let t = grad_t(&cfg.kind, (pt.x as f64, pt.y as f64));
                let (r, g, b, a) = sample_stops(&cfg.stops, t);
                let ae = a as f32 / 255.0 * ca;
                let idx = (y as usize * rw as usize + x as usize) * 4;
                out[idx] = (r as f32 * ae).round() as u8;
                out[idx + 1] = (g as f32 * ae).round() as u8;
                out[idx + 2] = (b as f32 * ae).round() as u8;
                out[idx + 3] = (ae * 255.0).round() as u8;
            }
        }
    }
    dst.draw_pixmap(
        rx0 as i32,
        ry0 as i32,
        layer.as_ref(),
        &PixmapPaint { opacity: 1.0, blend_mode: pvg_blend_to_skia(blend), quality: FilterQuality::Nearest },
        Transform::identity(),
        mask,
    );
}

/// A pre-rendered pattern tile: transparent-based premultiplied pixmap at
/// `k` device px per user unit, plus the tile size in user units.
struct PatternTile {
    pix: Pixmap,
    w_units: f64,
    h_units: f64,
}

/// Frame-scale pattern tiles, looked up by pattern name. Built once per
/// frame by [`render_pattern_tiles`] and threaded through the render path
/// (so every shape samples the same tile instead of re-rendering it).
type PatternTiles = HashMap<String, PatternTile>;

/// Renders every pattern tile once at device scale `k`.
///
/// Tiles render with an EMPTY tile map: a pattern referenced from inside a
/// tile falls back to neutral gray, which terminates any A-references-A
/// cycle by construction.
fn render_pattern_tiles(patterns: &[DrawPattern], k: f32) -> PatternTiles {
    let mut out = PatternTiles::new();
    let k = if k > 0.0 && k.is_finite() { k } else { 1.0 };
    let empty = PatternTiles::new();
    for pat in patterns {
        if pat.width <= 1e-9 || pat.height <= 1e-9 || pat.tiles.is_empty() {
            continue;
        }
        // Cap tile resolution: absurd tiles (huge user size x HiDPI scale)
        // degrade to the gray fallback instead of OOMing the frame.
        let tw = ((pat.width as f32 * k).ceil().max(1.0) as u32).min(1024);
        let th = ((pat.height as f32 * k).ceil().max(1.0) as u32).min(1024);
        let mut pix = match Pixmap::new(tw, th) {
            Some(p) => p,
            None => continue,
        };
        let xf = Transform::from_scale(k, k);
        for t in &pat.tiles {
            // No FX cache for tiles: cache keys are transform-blind, so a
            // tile-local layer could collide with an identical main-scene
            // command keyed under a different transform.
            render_cmd(&mut pix, t, xf, k, None, None, &empty);
        }
        out.insert(pat.name.clone(), PatternTile { pix, w_units: pat.width, h_units: pat.height });
    }
    out
}

/// Pattern fill/stroke via per-pixel tile-wrap sampling.
///
/// Mirrors [`paint_sampled`]: renders `path` coverage (white fill, or white
/// stroke so dash/cap/join are honored) into a cropped temp, then maps each
/// covered device pixel back to canvas space and wraps it into the tile with
/// Euclidean mod (negative-safe, seamless for any translate/scale draw
/// transform). `style.opacity` folds in at composite time.
#[allow(clippy::too_many_arguments)]
fn paint_pattern(
    dst: &mut Pixmap,
    cmd: &DrawCmd,
    path: &Path,
    tile: &PatternTile,
    is_fill: bool,
    style: &DrawStyle,
    transform: Transform,
    blend: PvgBlend,
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
        let mut white = SkPaint::default();
        white.set_color_rgba8(255, 255, 255, 255);
        white.anti_alias = true;
        if is_fill {
            coverage.fill_path(path, &white, FillRule::Winding, lxf, None);
        } else {
            let stroke = style_stroke(style);
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
                let fx = ((pt.x as f64).rem_euclid(tile.w_units) / tile.w_units * tw_px as f64) as u32;
                let fy = ((pt.y as f64).rem_euclid(tile.h_units) / tile.h_units * th_px as f64) as u32;
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
        &PixmapPaint { opacity: 1.0, blend_mode: pvg_blend_to_skia(blend), quality: FilterQuality::Nearest },
        Transform::identity(),
        mask,
    );
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

// ---------------------------------------------------------------------------
// Geometry -> tiny-skia paths
// ---------------------------------------------------------------------------

/// Palette character -> index (digits, then a-z/A-Z for 10+). `.`/space = skip.
/// Mirrors `transpilers::pvg_to_png::sprite_char_index`.
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

/// Smooth spline path (Catmull-Rom -> cubic Bezier), stroke geometry.
/// Mirrors `transpilers::pvg_to_png::spline_to_skia_path`.
fn spline_to_path(points: &[(f64, f64)]) -> Option<Path> {
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

fn cmd_to_path(cmd: &DrawCmd) -> Option<Path> {
    match cmd {
        DrawCmd::Circle { center, radius, .. } => {
            let mut pb = PathBuilder::new();
            pb.push_circle(center.0 as f32, center.1 as f32, *radius as f32);
            pb.finish()
        }
        DrawCmd::Ellipse { center, radius, .. } => {
            let rect = SkRect::from_xywh(
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
                let rect = SkRect::from_xywh(x, y, w, h)?;
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
        DrawCmd::Path { commands, .. } => path_commands_to_path(commands),
        DrawCmd::Text { .. } => None,
        DrawCmd::Sprite { pos, rows, scale, .. } => {
            let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as f32 * *scale as f32;
            let h = rows.len() as f32 * *scale as f32;
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            let rect = SkRect::from_xywh(pos.0 as f32, pos.1 as f32, w, h)?;
            let mut pb = PathBuilder::new();
            pb.push_rect(rect);
            pb.finish()
        }
        DrawCmd::Spline { points, .. } => spline_to_path(points),
        DrawCmd::Clip { mask, .. } => cmd_to_path(mask),
    }
}

fn path_commands_to_path(commands: &[DrawPathCommand]) -> Option<Path> {
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
                let steps =
                    (delta.abs() / (std::f64::consts::PI / 32.0)).ceil().max(16.0) as usize;
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

// ---------------------------------------------------------------------------
// Per-pixel helpers: separable box blur, silhouette, conic fill, sampling
// ---------------------------------------------------------------------------

/// 3-pass separable box blur approximating a gaussian, O(pixels) per pass.
/// Operates in premultiplied space (correct domain for filtering).
fn box_blur(pixmap: &mut Pixmap, radius: u32) {
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
/// runs 6 passes over the full canvas per filtered shape).
#[inline]
fn reciprocal(window: usize) -> u64 {
    ((1u64 << 32) + window as u64 - 1) / window as u64
}

#[inline]
fn avg_channel(acc: u32, recip: u64) -> u8 {
    ((acc as u64 * recip) >> 32) as u8
}

fn blur_horizontal(pixmap: &mut Pixmap, radius: u32) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let r = radius as usize;
    let window = 2 * r + 1;
    let recip = reciprocal(window);
    let mut buf = vec![0u8; w * h * 4];
    {
        let src = pixmap.data();
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

fn blur_vertical(pixmap: &mut Pixmap, radius: u32) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let r = radius as usize;
    let window = 2 * r + 1;
    let recip = reciprocal(window);
    let mut buf = vec![0u8; w * h * 4];
    {
        let src = pixmap.data();
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
/// `out_a = src_a * color_a * opacity`, `out_rgb = color_rgb * out_a`.
fn silhouette(src: &Pixmap, color: &PvgColor, opacity: f64) -> Option<Pixmap> {
    let (w, h) = (src.width(), src.height());
    let (cr, cg, cb, ca) = straight_rgba(color, opacity)?;
    let mut out = Pixmap::new(w, h)?;
    {
        let s = src.data();
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

/// Samples a stop list at fraction `t` (stops sorted by offset).
fn sample_stops(stops: &[(f64, u8, u8, u8, u8)], t: f64) -> (u8, u8, u8, u8) {
    if stops.is_empty() {
        return (0, 0, 0, 0);
    }
    if t <= stops[0].0 {
        let s = stops[0];
        return (s.1, s.2, s.3, s.4);
    }
    if t >= stops[stops.len() - 1].0 {
        let s = stops[stops.len() - 1];
        return (s.1, s.2, s.3, s.4);
    }
    for w in stops.windows(2) {
        let (o0, o1) = (w[0].0, w[1].0);
        if t >= o0 && t <= o1 {
            let span = (o1 - o0).max(1e-9);
            let f = ((t - o0) / span) as f32;
            let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * f).round() as u8;
            return (lerp(w[0].1, w[1].1), lerp(w[0].2, w[1].2), lerp(w[0].3, w[1].3), lerp(w[0].4, w[1].4));
        }
    }
    let s = stops[stops.len() - 1];
    (s.1, s.2, s.3, s.4)
}

// ---------------------------------------------------------------------------
// Command rendering (plain + FX-layered)
// ---------------------------------------------------------------------------

fn needs_fx(style: &DrawStyle) -> bool {
    style.blur > 1e-9 || style.shadow.is_some() || style.glow.is_some()
}

/// Renders fill+stroke of one geometric command onto `dst`.
/// `transform` positions user-space geometry in `dst`-local pixels; gradients
/// are evaluated per-pixel in user space (exact under cropping, any scale);
/// `blend`/`mask` apply to this paint only (layers pass `Normal`/`None` and
/// composite later).
#[allow(clippy::too_many_arguments)]
fn paint_shape(
    dst: &mut Pixmap,
    cmd: &DrawCmd,
    path: &Path,
    style: &DrawStyle,
    transform: Transform,
    blend: PvgBlend,
    mask: Option<&Mask>,
    tiles: &PatternTiles,
) {
    let is_line = matches!(cmd, DrawCmd::Line { .. });
    if !is_line {
        if let PvgPaint::Pattern(name) = &style.fill {
            match tiles.get(name) {
                Some(tile) => {
                    paint_pattern(dst, cmd, path, tile, true, style, transform, blend, mask)
                }
                // Unknown (or nested-in-tile) pattern: neutral gray fallback.
                None => {
                    if let Some(mut paint) = solid_or_pattern_paint(&style.fill, style.opacity) {
                        paint.blend_mode = pvg_blend_to_skia(blend);
                        dst.fill_path(path, &paint, FillRule::Winding, transform, mask);
                    }
                }
            }
        } else {
            match grad_cfg(&style.fill, style.opacity) {
                Some(cfg) => {
                    paint_sampled(dst, cmd, path, &cfg, true, style, transform, blend, mask)
                }
                None => {
                    // Solid color (or `none`, which yields no paint).
                    if let Some(mut paint) = solid_or_pattern_paint(&style.fill, style.opacity) {
                        paint.blend_mode = pvg_blend_to_skia(blend);
                        dst.fill_path(path, &paint, FillRule::Winding, transform, mask);
                    }
                }
            }
        }
    }
    if style.width > 0.0 && !style.stroke.is_none() {
        if let PvgPaint::Pattern(name) = &style.stroke {
            match tiles.get(name) {
                Some(tile) => {
                    paint_pattern(dst, cmd, path, tile, false, style, transform, blend, mask)
                }
                None => {
                    if let Some(mut sp) = solid_or_pattern_paint(&style.stroke, style.opacity) {
                        sp.blend_mode = pvg_blend_to_skia(blend);
                        let stroke = style_stroke(style);
                        dst.stroke_path(path, &sp, &stroke, transform, mask);
                    }
                }
            }
        } else {
            match grad_cfg(&style.stroke, style.opacity) {
                Some(cfg) => {
                    paint_sampled(dst, cmd, path, &cfg, false, style, transform, blend, mask)
                }
                None => {
                    if let Some(mut sp) = solid_or_pattern_paint(&style.stroke, style.opacity) {
                        sp.blend_mode = pvg_blend_to_skia(blend);
                        let stroke = style_stroke(style);
                        dst.stroke_path(path, &sp, &stroke, transform, mask);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pvg::compile;

    /// Premultiplied pixel read.
    fn px(pixmap: &Pixmap, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let d = pixmap.data();
        let i = ((y * pixmap.width() + x) * 4) as usize;
        (d[i], d[i + 1], d[i + 2], d[i + 3])
    }

    fn close(a: u8, b: u8, tol: u8) -> bool {
        a.abs_diff(b) <= tol
    }

    #[test]
    fn radial_gradient_interpolates_stops() {
        let src = "PVG 0.2\ncanvas 200 200\n  background #000000\ncircle\n  center [100, 100]\n  radius 90\n  fill radial [100, 100] 90\n    stop 0.0 #00ffff\n    stop 0.6 #0033aa\n    stop 1.0 #07090e\n";
        let dl = compile(src).unwrap();
        let pxm = render_pixmap(&dl, 1.0).unwrap();
        let (r, g, b, a) = px(&pxm, 100, 100);
        assert_eq!(a, 255);
        assert!(r < 12 && g > 240 && b > 240, "core must be bright cyan, got {},{},{}", r, g, b);
        // t = 81/90 = 0.9 -> between #0033aa and #07090e (dark navy).
        let (r, g, b, a) = px(&pxm, 100, 19);
        assert_eq!(a, 255);
        assert!(r < 30 && g < 60 && b > 20 && b < 120, "edge must be dark blue, got {},{},{}", r, g, b);
    }

    #[test]
    fn linear_gradient_runs_along_axis() {
        let src = "PVG 0.2\ncanvas 100 20\nrectangle\n  pos [0, 0]\n  size [100, 20]\n  fill linear [0, 0] [100, 0]\n    stop 0.0 #000000\n    stop 1.0 #ffffff\n";
        let dl = compile(src).unwrap();
        let pxm = render_pixmap(&dl, 1.0).unwrap();
        let (r0, _, _, _) = px(&pxm, 8, 10);
        let (r1, _, _, _) = px(&pxm, 92, 10);
        assert!(r0 < 60, "left must be dark, got {}", r0);
        assert!(r1 > 190, "right must be bright, got {}", r1);
    }

    #[test]
    fn clip_confines_content() {
        let src = "PVG 0.2\ncanvas 200 200\n  background #000000\nclip\n  circle\n    center [100, 100]\n    radius 50\n  rectangle\n    pos [0, 0]\n    size [200, 200]\n    fill #ff0000\n";
        let dl = compile(src).unwrap();
        let pxm = render_pixmap(&dl, 1.0).unwrap();
        let (r, g, b, _) = px(&pxm, 100, 100);
        assert!(r > 200 && g < 50 && b < 50, "inside mask must be red");
        let (r, g, b, _) = px(&pxm, 10, 10);
        assert!(r < 20 && g < 20 && b < 20, "outside mask must stay background");
    }

    #[test]
    fn dash_breaks_stroke() {
        let src = "PVG 0.2\ncanvas 200 200\n  background #000000\nline\n  from [0, 100]\n  to [200, 100]\n  stroke #ffffff\n  width 10\n  dash [20, 20]\n";
        let dl = compile(src).unwrap();
        let pxm = render_pixmap(&dl, 1.0).unwrap();
        // NOTE: gaps reveal the opaque black background, so compare color,
        // not alpha.
        let (r_on, g_on, b_on, _) = px(&pxm, 10, 100);
        let (r_off, g_off, b_off, _) = px(&pxm, 30, 100);
        assert!(r_on > 200 && g_on > 200 && b_on > 200, "dash-on must be white");
        assert!(r_off < 40 && g_off < 40 && b_off < 40, "dash gap must show background");
    }

    #[test]
    fn blur_softens_edges() {
        let src = "PVG 0.2\ncanvas 200 200\n  background #000000\ncircle\n  center [100, 100]\n  radius 20\n  fill #ffffff\n  blur 10\n";
        let dl = compile(src).unwrap();
        let pxm = render_pixmap(&dl, 1.0).unwrap();
        // 8px past the rim: sharp render would be pure background; blur spreads light.
        let (r, g, b, _) = px(&pxm, 128, 100);
        assert!(r > 10 || g > 10 || b > 10, "blur halo must reach past the rim");
        // Rim pixel itself must be dimmed (energy spread out).
        let (r, _, _, _) = px(&pxm, 120, 100);
        assert!(r < 255, "rim must be softened by blur");
        // Far away stays black.
        let (r, g, b, _) = px(&pxm, 180, 100);
        assert!(r < 8 && g < 8 && b < 8, "far field must stay black");
    }

    #[test]
    fn shadow_casts_offset_dark() {
        let src = "PVG 0.2\ncanvas 200 200\n  background #ffffff\nrectangle\n  pos [50, 50]\n  size [60, 60]\n  fill #ffffff\n  shadow [0, 18] 6 #000000\n";
        let dl = compile(src).unwrap();
        let pxm = render_pixmap(&dl, 1.0).unwrap();
        // Under the rect + offset (y=110+): shadow darkens the white background.
        let (r, g, b, _) = px(&pxm, 80, 122);
        assert!(r < 200 && close(r, g, 12) && close(g, b, 12), "shadow must darken below, got {},{},{}", r, g, b);
    }

    #[test]
    fn additive_blend_brightens_overlap() {
        let src = "PVG 0.2\ncanvas 200 200\n  background #000000\ncircle\n  center [80, 100]\n  radius 40\n  fill #ff0000\n  blend \"add\"\ncircle\n  center [120, 100]\n  radius 40\n  fill #00ff00\n  blend \"add\"\n";
        let dl = compile(src).unwrap();
        let pxm = render_pixmap(&dl, 1.0).unwrap();
        let (r, g, b, _) = px(&pxm, 100, 100);
        assert!(r > 200 && g > 200 && b < 80, "additive overlap must be yellow, got {},{},{}", r, g, b);
    }

    /// Regression: radial gradients inside cropped layers (clip content, fx
    /// shapes) must sample the stops, not collapse to the last stop.
    /// (tiny-skia 0.11 mis-samples translated shader transforms, so control
    /// points are baked into target pixels with an identity shader transform.)
    #[test]
    fn radial_in_cropped_layer_stays_lit() {
        let src = "PVG 0.2\ncanvas 512 512\n  background #07090e\nset cx = 256\nset cy = 256\nclip\n  circle\n    center [cx, cy]\n    radius 105\n  circle\n    center [cx, cy]\n    radius 90\n    fill radial [cx, cy] 90\n      stop 0.0 #00ffff\n      stop 0.6 #0033aa\n      stop 1.0 #07090e\n";
        let dl = pvg::compile(&src).unwrap();
        for (label, pxm) in [
            ("fresh", render_pixmap(&dl, 1.0).unwrap()),
            ("cached", FxCache::default().render_cached(&dl, 1.0).unwrap()),
        ] {
            let (r, g, b, a) = px(&pxm, 256, 256);
            assert_eq!(a, 255, "{}: core must be opaque", label);
            assert!(r < 30 && g > 220 && b > 220, "{}: core must be cyan, got {},{},{}", label, r, g, b);
            // t = 81/90 = 0.9 -> dark navy between #0033aa and #07090e.
            let (r, g, b, a) = px(&pxm, 256, 175);
            assert_eq!(a, 255, "{}: flare edge must be opaque", label);
            assert!(r < 30 && g < 70 && b > 15 && b < 130, "{}: flare edge must be dark blue, got {},{},{}", label, r, g, b);
        }
    }

    /// Regression: fully transparent gradient stops (`#rrggbbaa` with aa=00)
    /// are meaningful fade-outs and must be kept, not dropped.
    #[test]
    fn transparent_gradient_stop_fades() {
        let src = "PVG 0.2\ncanvas 200 60\n  background #101010\nrectangle\n  pos [0, 0]\n  size [200, 60]\n  fill linear [0, 0] [200, 0]\n    stop 0.0 #ffffff\n    stop 1.0 #0066ff00\n";
        let dl = pvg::compile(&src).unwrap();
        let pxm = render_pixmap(&dl, 1.0).unwrap();
        let (r0, g0, b0, _) = px(&pxm, 8, 30);
        assert!(r0 > 200 && g0 > 200 && b0 > 200, "start must be near-white");
        // Far end: transparent blue over #101010 background ≈ background.
        let (r1, g1, b1, _) = px(&pxm, 192, 30);
        assert!(r1 < 60 && g1 < 90 && b1 < 110, "faded end must show background, got {},{},{}", r1, g1, b1);
        // Middle: partial white over background (brighter than bg, dimmer than white).
        let (rm, gm, bm, _) = px(&pxm, 100, 30);
        assert!(rm > 40 && rm < 200, "middle must be a translucent blend, got {},{},{}", rm, gm, bm);
    }

    /// The 3-pass box blur must approximate a gaussian: smooth erf falloff,
    /// ~half response at the edge, ~zero at 3r.
    #[test]
    fn blur_kernel_is_gaussian() {
        use tiny_skia::Pixmap;
        let mut pxm = Pixmap::new(121, 31).unwrap();
        {
            let d = pxm.data_mut();
            // White left half-plane (step edge at x=60), full alpha.
            for y in 0..31 {
                for x in 0..60 {
                    let i = (y * 121 + x) * 4;
                    d[i] = 255;
                    d[i + 1] = 255;
                    d[i + 2] = 255;
                    d[i + 3] = 255;
                }
            }
        }
        box_blur(&mut pxm, 6);
        let at = |x: usize| pxm.data()[(15 * 121 + x) * 4];
        assert!(at(48) > 200, "unblurred side stays bright");
        assert!((100..=155).contains(&at(60)), "edge response ≈ half, got {}", at(60));
        assert!(at(66) > 20 && at(66) < 120, "1r out is mid-falloff, got {}", at(66));
        assert!(at(72) < 15, "2r out is faint tail, got {}", at(72));
        assert!(at(78) < 5, "3r out is ~zero (seamless), got {}", at(78));
        // Monotonic across the edge.
        let mut prev = 255;
        for x in (48..78).step_by(2) {
            assert!(at(x) <= prev, "falloff must be monotonic");
            prev = at(x);
        }
    }

    /// Regression: blur layers need ~3r padding, or the gaussian tails clip
    /// into a visible square seam around the shape.
    #[test]
    fn blur_has_no_square_seam() {
        let src = "PVG 0.2\ncanvas 200 200\n  background #000000\ncircle\n  center [100, 100]\n  radius 10\n  fill #ffffff\n  blur 6\n";
        let dl = pvg::compile(&src).unwrap();
        let mut cache = FxCache::default();
        let pxm = cache.render_cached(&dl, 1.0).unwrap();
        // Halo must extend past the old 1r crop (where it was exactly 0).
        let (r, _, _, _) = px(&pxm, 122, 100);
        assert!(r >= 1, "halo must extend past 1r pad, got {}", r);
        // …and die smoothly by 3r (no energy cut → no seam).
        let (r, _, _, _) = px(&pxm, 128, 100);
        assert!(r < 5, "tails must vanish by 3r, got {}", r);
        // Far field stays black.
        let (r, _, _, _) = px(&pxm, 170, 100);
        assert!(r < 6, "far field must stay black, got {}", r);
        // Smooth falloff: nearer sample brighter than farther sample.
        let (rn, _, _, _) = px(&pxm, 114, 100);
        let (rf, _, _, _) = px(&pxm, 126, 100);
        assert!(rn >= rf, "falloff must be monotonic, near={} far={}", rn, rf);
    }

    /// Perf guard: a warm effect-layer cache must beat uncached re-rastering
    /// (relative comparison, so it holds on any machine and in any profile).
    #[test]
    fn cached_preview_beats_fresh_raster() {
        let src = std::fs::read_to_string("../presets/shield_core.pvg").unwrap();
        let dl = pvg::compile(&src).unwrap();
        let n = 6;
        let t1 = std::time::Instant::now();
        for _ in 0..n {
            let _ = render_pixmap(&dl, 1.0).unwrap();
        }
        let uncached = t1.elapsed() / n;
        let mut cache = FxCache::default();
        let _ = cache.render_cached(&dl, 1.0).unwrap();
        let t2 = std::time::Instant::now();
        for _ in 0..n {
            let _ = cache.render_cached(&dl, 1.0).unwrap();
        }
        let warm = t2.elapsed() / n;
        println!("uncached {:?} vs cached-warm {:?}", uncached, warm);
        assert!(warm < uncached, "effect-layer cache must speed up repeat frames");
    }

    /// Regression: `fill pattern` must tile for real in the rasterizer, not
    /// collapse to the neutral-gray fallback.
    #[test]
    fn pattern_fill_tiles() {
        let src = "PVG 0.2\ncanvas 64 64\n  background #000000\npattern gridp 16 16\n  line\n    from [0, 0]\n    to [16, 0]\n    stroke #ffffff\n    width 2\nrectangle\n  pos [0, 0]\n  size [64, 64]\n  fill pattern gridp\n";
        let dl = compile(src).unwrap();
        assert_eq!(dl.patterns.len(), 1);
        assert_eq!(dl.patterns[0].tiles.len(), 1);
        for (label, pxm) in [
            ("fresh", render_pixmap(&dl, 1.0).unwrap()),
            ("cached", FxCache::default().render_cached(&dl, 1.0).unwrap()),
        ] {
            // y=0 row carries the horizontal tile line -> near-white.
            let (r, g, b, a) = px(&pxm, 32, 0);
            assert_eq!(a, 255, "{}: tile line must be opaque", label);
            assert!(r > 200 && g > 200 && b > 200, "{}: tile line must be white, got {},{},{}", label, r, g, b);
            // Mid-tile gap shows the black background, not gray fallback.
            let (r, g, b, _) = px(&pxm, 32, 8);
            assert!(r < 40 && g < 40 && b < 40, "{}: tile gap must be background, got {},{},{}", label, r, g, b);
        }
    }

    /// The tactical-HUD carbon mesh: near-transparent tile lines over a dark
    /// card must visibly lift the line pixels above the background (this is
    /// the case that rendered as flat gray under the old fallback).
    #[test]
    fn faint_pattern_lines_lift_over_dark_card() {
        let src = "PVG 0.2\ncanvas 64 64\n  background #06080d\npattern carbon_mesh 16 16\n  line\n    from [0, 0]\n    to [16, 16]\n    stroke #ffffff0a\n    width 1\nrectangle\n  pos [0, 0]\n  size [64, 64]\n  fill pattern carbon_mesh\n";
        let dl = compile(src).unwrap();
        let pxm = render_pixmap(&dl, 1.0).unwrap();
        // (8,8) sits on the tile diagonal; (8,4) sits in a gap.
        let (rl, _, _, _) = px(&pxm, 8, 8);
        let (rg, _, _, _) = px(&pxm, 8, 4);
        assert!(rg < 14, "gap must stay near background, got {}", rg);
        assert!(rl > rg + 2, "diagonal line must lift above gap, line={} gap={}", rl, rg);
    }

    #[test]
    fn shield_core_smoke() {
        let src = std::fs::read_to_string("../presets/shield_core.pvg").unwrap();
        let dl = compile(&src).unwrap();
        assert!(dl.len() >= 5);
        let pxm = render_pixmap(&dl, 1.0).unwrap();
        // Hub highlight near center is bright.
        let (r, g, b, _) = px(&pxm, 250, 250);
        assert!(r > 150 && g > 150 && b > 150, "hub must be bright, got {},{},{}", r, g, b);
        // Armor plate corner region carries the metallic gradient (not background).
        let (r, g, b, _) = px(&pxm, 130, 130);
        assert!(!(r == 7 && g == 9 && b == 14), "plate must not be flat background");
    }
}

/// Cache of fully-composited effect layers for the live preview.
///
/// Filtered shapes (`blur`/`shadow`/`glow`) cost milliseconds per frame, but
/// most of them are *static* (time-independent). This cache keys the finished
/// layer by the evaluated command so static glow/shadow/blur renders once and
/// is blitted on later frames. Animated layers simply miss and re-render.
///
/// Bounds: at most `MAX_FX_LAYERS` entries; cleared on canvas/scale change.
/// Stale animated keys are bounded by the same cap (insert stops when full).
/// A cached raster layer with its canvas-device offset.
type CachedLayer = (Pixmap, i32, i32);

pub struct FxCache {
    scale_bits: u32,
    canvas: (u32, u32),
    layers: std::collections::HashMap<String, CachedLayer>,
}

const MAX_FX_LAYERS: usize = 24;

impl Default for FxCache {
    fn default() -> Self {
        Self { scale_bits: 0, canvas: (0, 0), layers: std::collections::HashMap::new() }
    }
}

impl FxCache {
    /// Renders with effect-layer caching. Identical output to
    /// [`render_pixmap`]; much faster when filtered shapes are static.
    pub fn render_cached(&mut self, draw_list: &DrawList, scale: f32) -> Result<Pixmap, String> {
        let scale = if scale <= 0.0 || !scale.is_finite() {
            1.0
        } else {
            scale
        };
        let width = ((draw_list.canvas_width as f32) * scale).round().max(1.0) as u32;
        let height = ((draw_list.canvas_height as f32) * scale).round().max(1.0) as u32;
        if width == 0 || height == 0 {
            return Err("Canvas dimensions must be greater than zero".into());
        }
        if self.scale_bits != scale.to_bits() || self.canvas != (width, height) {
            self.layers.clear();
            self.scale_bits = scale.to_bits();
            self.canvas = (width, height);
        }
        let mut pixmap = Pixmap::new(width, height)
            .ok_or_else(|| format!("Failed to allocate {}x{}", width, height))?;
        let transform = Transform::from_scale(scale, scale);

        if let Some(ref bg) = draw_list.background {
            if let Some(bg_paint) = solid_paint(bg, 1.0) {
                if let Some(rect) = SkRect::from_xywh(0.0, 0.0, width as f32, height as f32) {
                    pixmap.fill_rect(rect, &bg_paint, Transform::identity(), None);
                }
            }
        }

        let tiles = render_pattern_tiles(&draw_list.patterns, scale);
        for cmd in &draw_list.items {
            render_cmd(&mut pixmap, cmd, transform, scale, None, Some(self), &tiles);
        }
        Ok(pixmap)
    }

    /// Returns the cached finished layer for a command, if present.
    fn get(&self, key: &str) -> Option<(PixmapRef<'_>, i32, i32)> {
        self.layers.get(key).map(|(p, ox, oy)| (p.as_ref(), *ox, *oy))
    }

    /// Stores a finished layer unless the cache is full (then the caller just
    /// renders fresh every frame — bounded staleness, no unbounded growth).
    fn put(&mut self, key: String, layer: CachedLayer) {
        if self.layers.len() < MAX_FX_LAYERS {
            self.layers.insert(key, layer);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_cmd(
    dst: &mut Pixmap,
    cmd: &DrawCmd,
    transform: Transform,
    scale: f32,
    mask: Option<&Mask>,
    mut cache: Option<&mut FxCache>,
    tiles: &PatternTiles,
) {
    match cmd {
        DrawCmd::Text { .. } => {
            // Text is overlaid by the Studio with real fonts (renderer module).
        }
        DrawCmd::Clip { mask: mask_cmd, content } => {
            // Fast path: cacheable clips (all-Normal-blend content, so a
            // SourceOver layer composite is exact) render once into a layer
            // cropped to the mask bbox.
            let key = if cache.is_some() && clip_cacheable(content) {
                Some(format!("{:?}", cmd))
            } else {
                None
            };
            if let (Some(k), Some(c)) = (key.as_ref(), cache.as_ref()) {
                if let Some((layer, ox, oy)) = c.get(k) {
                    dst.draw_pixmap(
                        ox,
                        oy,
                        layer,
                        &PixmapPaint {
                            opacity: 1.0,
                            blend_mode: SkBlend::SourceOver,
                            quality: FilterQuality::Nearest,
                        },
                        Transform::identity(),
                        mask,
                    );
                    return;
                }
            }
            if key.is_some() {
                if let Some((ox, oy, lw, lh)) =
                    layer_rect_for_bbox(geom_bbox(mask_cmd), 2.0, scale, dst.width(), dst.height())
                {
                    if let Some(mut layer) = Pixmap::new(lw, lh) {
                        let lxf = Transform::from_scale(scale, scale)
                            .post_translate(-(ox as f32), -(oy as f32));
                        if let Some(mask_path) = cmd_to_path(mask_cmd) {
                            if let Some(mut clip_mask) = Mask::new(lw, lh) {
                                clip_mask.fill_path(&mask_path, FillRule::Winding, true, lxf);
                                for inner in content {
                                    render_cmd(
                                        &mut layer,
                                        inner,
                                        lxf,
                                        scale,
                                        Some(&clip_mask),
                                        cache.as_mut().map(|c| &mut **c),
                                        tiles,
                                    );
                                }
                                dst.draw_pixmap(
                                    ox,
                                    oy,
                                    layer.as_ref(),
                                    &PixmapPaint {
                                        opacity: 1.0,
                                        blend_mode: SkBlend::SourceOver,
                                        quality: FilterQuality::Nearest,
                                    },
                                    Transform::identity(),
                                    mask,
                                );
                                if let (Some(k), Some(c)) = (key, cache) {
                                    c.put(k, (layer, ox, oy));
                                }
                                return;
                            }
                        }
                    }
                }
            }
            // Correct fallback (also used by uncached one-shot renders):
            // full-canvas mask, content drawn straight through it.
            if let Some(mask_path) = cmd_to_path(mask_cmd) {
                if let Some(mut clip_mask) = Mask::new(dst.width(), dst.height()) {
                    clip_mask.fill_path(&mask_path, FillRule::Winding, true, transform);
                    // tiny-skia accepts a single mask; nested clips render
                    // through the tighter inner mask (documented limitation).
                    for inner in content {
                        render_cmd(
                            dst,
                            inner,
                            transform,
                            scale,
                            Some(&clip_mask),
                            cache.as_mut().map(|c| &mut **c),
                            tiles,
                        );
                    }
                }
            }
        }
        DrawCmd::Sprite { pos, palette, rows, scale, style } => {
            // Pixel-art sprite: per-pixel crisp rects (mirrors
            // `transpilers::pvg_to_png` main + masked paths; `mask` threads
            // clip content through here).
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
                    if let Some(mut px_paint) = solid_paint(color, style.opacity) {
                        px_paint.blend_mode = pvg_blend_to_skia(style.blend);
                        let x = pos.0 as f32 + rx as f32 * *scale as f32;
                        let y = pos.1 as f32 + ry as f32 * *scale as f32;
                        if let Some(rect) = SkRect::from_xywh(x, y, *scale as f32, *scale as f32) {
                            dst.fill_rect(rect, &px_paint, transform, mask);
                        }
                    }
                }
            }
        }
        DrawCmd::Circle { style, .. }
        | DrawCmd::Ellipse { style, .. }
        | DrawCmd::Rectangle { style, .. }
        | DrawCmd::Line { style, .. }
        | DrawCmd::Polygon { style, .. }
        | DrawCmd::Path { style, .. }
        | DrawCmd::Spline { style, .. } => {
            if let Some(path) = cmd_to_path(cmd) {
                if needs_fx(style) {
                    // Cached path: static layers hit, animated layers re-render.
                    // `{:?}` of the evaluated command is deterministic for
                    // identical values (f64 shortest-roundtrip Debug).
                    let key = if cache.is_some() { Some(format!("{:?}", cmd)) } else { None };
                    if let (Some(k), Some(c)) = (key.as_ref(), cache.as_ref()) {
                        if let Some((layer, ox, oy)) = c.get(k) {
                            dst.draw_pixmap(
                                ox,
                                oy,
                                layer,
                                &PixmapPaint {
                                    opacity: 1.0,
                                    blend_mode: pvg_blend_to_skia(style.blend),
                                    quality: FilterQuality::Nearest,
                                },
                                Transform::identity(),
                                mask,
                            );
                            return;
                        }
                    }
                    // Miss (or uncached one-shot): render the full FX stack
                    // into a bbox-cropped layer and composite it.
                    if let Some((ox, oy, lw, lh)) =
                        fx_layer_rect(cmd, style, scale, dst.width(), dst.height())
                    {
                        if let Some(mut layer) = Pixmap::new(lw, lh) {
                            let lxf = Transform::from_scale(scale, scale)
                                .post_translate(-(ox as f32), -(oy as f32));
                            render_fx_into(&mut layer, cmd, &path, style, lxf, scale, tiles);
                            dst.draw_pixmap(
                                ox,
                                oy,
                                layer.as_ref(),
                                &PixmapPaint {
                                    opacity: 1.0,
                                    blend_mode: pvg_blend_to_skia(style.blend),
                                    quality: FilterQuality::Nearest,
                                },
                                Transform::identity(),
                                mask,
                            );
                            if let (Some(k), Some(c)) = (key, cache) {
                                c.put(k, (layer, ox, oy));
                            }
                            return;
                        }
                    }
                    // Degenerate rect or layer allocation failed (near-OOM):
                    // degrade to a direct paint without filter passes.
                    paint_shape(dst, cmd, &path, style, transform, style.blend, mask, tiles);
                } else {
                    paint_shape(dst, cmd, &path, style, transform, style.blend, mask, tiles);
                }
            }
        }
    }
}

/// Cropped device-space rect `(ox, oy, w, h)` for a user-space bbox with a
/// fixed device-pixel pad, clamped to the canvas.
fn layer_rect_for_bbox(
    bbox: Option<(f64, f64, f64, f64)>,
    pad_dev: f32,
    scale: f32,
    canvas_w: u32,
    canvas_h: u32,
) -> Option<(i32, i32, u32, u32)> {
    let (x0, y0, x1, y1) = bbox?;
    let pad = pad_dev as f64 / scale.max(1e-6) as f64;
    let x0c = ((x0 - pad) as f32 * scale).floor() as i32 - 1;
    let y0c = ((y0 - pad) as f32 * scale).floor() as i32 - 1;
    let x1c = ((x1 + pad) as f32 * scale).ceil() as i32 + 1;
    let y1c = ((y1 + pad) as f32 * scale).ceil() as i32 + 1;
    let x0c = x0c.max(0).min(canvas_w as i32);
    let y0c = y0c.max(0).min(canvas_h as i32);
    let x1c = x1c.max(0).min(canvas_w as i32);
    let y1c = y1c.max(0).min(canvas_h as i32);
    if x1c <= x0c || y1c <= y0c {
        return None;
    }
    Some((x0c, y0c, (x1c - x0c) as u32, (y1c - y0c) as u32))
}

/// A clip block is layer-cacheable when its rasterized content is all
/// `Normal`-blend: SourceOver compositing of the finished layer is then exact.
/// (Text is overlaid by the Studio and never enters layers.)
fn clip_cacheable(content: &[DrawCmd]) -> bool {
    fn normal(cmd: &DrawCmd) -> bool {
        match cmd {
        DrawCmd::Circle { style, .. }
            | DrawCmd::Ellipse { style, .. }
            | DrawCmd::Rectangle { style, .. }
            | DrawCmd::Line { style, .. }
            | DrawCmd::Polygon { style, .. }
            | DrawCmd::Path { style, .. }
            | DrawCmd::Sprite { style, .. }
            | DrawCmd::Spline { style, .. } => style.blend == PvgBlend::Normal,
            DrawCmd::Text { .. } => true,
            DrawCmd::Clip { content, .. } => content.iter().all(normal),
        }
    }
    content.iter().all(normal)
}

/// FX stack rendered into `layer` (no outer-mask, no final blend): shape,
/// shadow, glow, then blur-or-sharp. The caller composites the finished layer
/// with the style blend mode (and optional clip mask) and optionally caches it.
fn render_fx_into(
    layer: &mut Pixmap,
    cmd: &DrawCmd,
    path: &Path,
    style: &DrawStyle,
    transform: Transform,
    scale: f32,
    tiles: &PatternTiles,
) {
    paint_shape(layer, cmd, path, style, transform, PvgBlend::Normal, None, tiles);

    if let Some(sh) = &style.shadow {
        if let Some(mut sh_px) = silhouette(layer, &sh.color, style.opacity) {
            let r = ((sh.radius as f32 * scale).round().max(0.0)) as u32;
            box_blur(&mut sh_px, r);
            let dx = (sh.offset.0 as f32 * scale).round() as i32;
            let dy = (sh.offset.1 as f32 * scale).round() as i32;
            layer.draw_pixmap(
                dx,
                dy,
                sh_px.as_ref(),
                &PixmapPaint { opacity: 1.0, blend_mode: SkBlend::SourceOver, quality: FilterQuality::Nearest },
                Transform::identity(),
                None,
            );
        }
        // Shadow must sit UNDER the shape: re-paint the sharp shape on top.
        // (Blit order: layer currently holds shape; shadow was drawn over it,
        // so redraw sharp shape to restore correct stacking.)
        paint_shape(layer, cmd, path, style, transform, PvgBlend::Normal, None, tiles);
    }
    if let Some(gl) = &style.glow {
        // Glow halo from the CURRENT layer content (shape, or shape+shadow).
        // Snapshot first: silhouette reads alpha only, halo composites additively.
        if let Some(mut gl_px) = silhouette(layer, &gl.color, style.opacity) {
            let r = ((gl.radius as f32 * scale).round().max(0.0)) as u32;
            box_blur(&mut gl_px, r);
            layer.draw_pixmap(
                0,
                0,
                gl_px.as_ref(),
                &PixmapPaint { opacity: 1.0, blend_mode: SkBlend::Plus, quality: FilterQuality::Nearest },
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
