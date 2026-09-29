use crate::text::TextEngine;
use pvg::ast::Color;
use pvg::draw_list::{
    BlendMode, DrawCmd, DrawList, DrawPathCommand, DrawPattern, DrawStyle, LineCap, LineJoin,
    Paint as PvgPaint, TextAlign,
};
use std::collections::HashMap;
use std::f64::consts::{PI, TAU as TAU_F64};
use std::sync::OnceLock;
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

/// Flat fallback for `Pattern` paints that never reach the sampler: patterns
/// referenced from inside a tile (cycle guard) or unknown names resolve to
/// neutral gray. Gradients return `None` here (handled per-pixel via `grad_cfg`).
fn solid_or_pattern_paint(paint: &PvgPaint, opacity: f64) -> Option<Paint<'static>> {
    match paint {
        PvgPaint::Color(c) => color_to_skia(c, opacity),
        PvgPaint::Pattern(_) => color_to_skia(&Color::Rgba(136, 136, 136, 255), opacity),
        _ => None,
    }
}

/// Backwards-compatible paint helper: solid colors map directly, unknown
/// patterns map to gray, gradients fall back to their middle stop's color.
/// Prefer `solid_or_pattern_paint` + `grad_cfg`/`paint_sampled` for real 0.2 output.
#[allow(dead_code)]
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

/// Sorted, clamped, opacity-folded gradient stops as straight RGBA.
///
/// Fully transparent stops (`#rrggbbaa` with `aa == 00`) are MEANINGFUL
/// (fade-outs) and must be kept (PVG 0.2 Section 9.4).
fn sampled_stops(stops: &[pvg::draw_list::GradientStop], opacity: f64) -> Vec<(f64, u8, u8, u8, u8)> {
    let mut sorted: Vec<&pvg::draw_list::GradientStop> = stops.iter().collect();
    sorted.sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Vec::with_capacity(sorted.len());
    for s in sorted {
        let (r, g, b, a) = match &s.color {
            Color::Rgba(r, g, b, a) => (*r, *g, *b, ((*a as f64) * opacity).clamp(0.0, 255.0).round() as u8),
            Color::None => (0, 0, 0, 0),
        };
        out.push((s.offset.clamp(0.0, 1.0), r, g, b, a));
    }
    out
}

/// Gradient geometry in PVG user space (spec Section 9).
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
                kind: GradKind::Radial { focal: focal.unwrap_or(*center), center: *center, radius: *radius },
            })
        }
        PvgPaint::Angular { center, start_angle, stops } => {
            let stops = sampled_stops(stops, opacity);
            if stops.is_empty() {
                return None;
            }
            Some(GradCfg { stops, kind: GradKind::Angular { center: *center, start_angle: *start_angle } })
        }
    }
}

/// Gradient parameter `t` in [0, 1] for a user-space point.
/// Degenerate linear (start == end) yields the last stop.
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
            if (vx * vx + vy * vy).sqrt() < 1e-9 {
                return ((dx * dx + dy * dy).sqrt() / radius).clamp(0.0, 1.0);
            }
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
            ((dv - disc.sqrt()) / denom).clamp(0.0, 1.0)
        }
        GradKind::Angular { center, start_angle } => {
            let mut t = ((u.1 - center.1).atan2(u.0 - center.0) - start_angle) / TAU_F64;
            t -= t.floor();
            t.clamp(0.0, 1.0)
        }
    }
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
// PVG 0.2 Section 10 FX: blur / shadow / glow
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
/// shadow offset, mapped through the FULL draw transform (scale AND letterbox
/// offset), then clamped to the target. Cropping makes blur, silhouette
/// and compositing proportional to the affected region instead of the canvas.
///
/// NOTE: mapping through `transform` (not `scale` alone) is required — the
/// letterbox offset is non-zero on non-square targets, and dropping it shifts
/// every FX layer up-left and clips its bottom/right tails.
fn fx_layer_rect(
    cmd: &DrawCmd,
    style: &DrawStyle,
    transform: Transform,
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
    let ux0 = (x0 - pad).min(x0 + ox.min(0.0));
    let uy0 = (y0 - pad).min(y0 + oy.min(0.0));
    let ux1 = (x1 + pad).max(x1 + ox.max(0.0));
    let uy1 = (y1 + pad).max(y1 + oy.max(0.0));
    // Map all four corners (general under rotation, cheap anyway).
    let mut pts = [
        Point::from_xy(ux0 as f32, uy0 as f32),
        Point::from_xy(ux1 as f32, uy0 as f32),
        Point::from_xy(ux0 as f32, uy1 as f32),
        Point::from_xy(ux1 as f32, uy1 as f32),
    ];
    transform.map_points(&mut pts);
    let (mut lx0, mut ly0, mut lx1, mut ly1) =
        (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for p in &pts {
        lx0 = lx0.min(p.x);
        ly0 = ly0.min(p.y);
        lx1 = lx1.max(p.x);
        ly1 = ly1.max(p.y);
    }
    let x0 = lx0.floor() as i32 - 1;
    let y0 = ly0.floor() as i32 - 1;
    let x1 = lx1.ceil() as i32 + 1;
    let y1 = ly1.ceil() as i32 + 1;
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
///
/// The second scanline buffer is a per-thread reusable scratch allocation:
/// a blurred 512x512 layer otherwise pays six ~1MB `vec!` allocs per shape
/// per frame.
use std::cell::RefCell;
thread_local! {
    static BLUR_SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

fn box_blur(pixmap: &mut PixmapMut, radius: u32) {
    if radius == 0 {
        return;
    }
    BLUR_SCRATCH.with(|s| {
        let mut scratch = s.borrow_mut();
        for _ in 0..3 {
            blur_horizontal(pixmap, radius, &mut scratch);
            blur_vertical(pixmap, radius, &mut scratch);
        }
    });
}

/// Render-thread pool width for data-parallel pixel loops (Android docs for
/// software rendering: do the minimum work per frame, and spread CPU raster
/// work across cores — a single software thread leaves 7/8 of a phone SoC
/// idle while the frame budget burns).
///
/// Implemented with `std::thread::scope`: workers borrow stack buffers, so no
/// `'static` bounds, no thread-pool crate, no persistent threads. Spawning
/// costs ~100µs per region, so narrow regions stay serial (see `par_chunks`).
static WORKERS: OnceLock<usize> = OnceLock::new();

fn worker_threads() -> usize {
    *WORKERS.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 8)
    })
}

/// Row/column chunk count for a `total`-element dimension, or 0 when the
/// serial path is cheaper (single core, or too few rows to matter).
/// Chunks scale with the workload (~64 rows per thread minimum).
fn par_chunks(total: usize) -> usize {
    let t = worker_threads();
    if t < 2 || total < 128 {
        0
    } else {
        (total / 64).clamp(2, t.min(total))
    }
}

/// One unit of pool work. `SendPtr` payloads are `'static`-compatible (raw
/// pointers carry no lifetime), so jobs can own all their inputs.
type Job = Box<dyn FnOnce() + Send + 'static>;

/// Fixed worker pool: `std::thread::scope` births ~100µs/thread on Windows
/// (~30µs on Android) and a big FX fans out to ~60 spawns/frame — pure
/// birth tax with zero work attached. Pool workers persist for the process
/// lifetime; dispatching a job is a channel send (~1µs).
struct Pool {
    tx: std::sync::mpsc::Sender<Job>,
    // Workers run until process exit; handles intentionally never joined.
    _workers: Vec<std::thread::JoinHandle<()>>,
}

static POOL: OnceLock<Pool> = OnceLock::new();

fn pool() -> &'static Pool {
    POOL.get_or_init(|| Pool::new(worker_threads()))
}

impl Pool {
    fn new(n: usize) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
        let mut workers = Vec::with_capacity(n.max(1));
        for _ in 0..n.max(1) {
            let rx = std::sync::Arc::clone(&rx);
            workers.push(std::thread::spawn(move || loop {
                match rx.lock().unwrap().recv() {
                    Ok(job) => job(),
                    Err(_) => break,
                }
            }));
        }
        Self { tx, _workers: workers }
    }

    /// Fans range `[0, total)` over pool workers as `f(ctx, lo, hi)` chunks
    /// and blocks until all finish. Falls back to a direct serial call when
    /// threading doesn't pay.
    fn par_rows<C>(&self, total: usize, ctx: C, f: fn(C, usize, usize))
    where
        C: Clone + Send + Sync + 'static,
    {
        let chunks = par_chunks(total);
        if chunks == 0 {
            f(ctx, 0, total);
            return;
        }
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let rows_per = (total + chunks - 1) / chunks;
        let mut n = 0u32;
        for c in 0..chunks {
            let lo = c * rows_per;
            let hi = (lo + rows_per).min(total);
            if lo >= hi {
                break;
            }
            let tx = done_tx.clone();
            let cctx = ctx.clone();
            let job: Job = Box::new(move || {
                f(cctx, lo, hi);
                let _ = tx.send(());
            });
            if self.tx.send(job).is_err() {
                // Pool gone (process teardown): finish inline.
                f(ctx.clone(), lo, hi);
                let _ = done_tx.clone().send(());
            }
            n += 1;
        }
        drop(done_tx);
        for _ in 0..n {
            let _ = done_rx.recv();
        }
    }
}

/// Raw buffer pointer shared across scoped threads. Sound because every user
/// partitions it into disjoint row/column ranges before spawning.
///
/// NOTE: users must read the pointer through [`SendPtr::as_const`] /
/// [`SendPtr::as_mut`]. Projecting the `.0` field directly inside a closure
/// makes Rust 2021 disjoint capture grab the bare `*mut T` (which is
/// `!Send`) instead of the wrapper.
#[derive(Clone, Copy)]
struct SendPtr<T>(*mut T);
// SAFETY: only shared inside `std::thread::scope` with disjoint ranges.
unsafe impl<T> Send for SendPtr<T> {}
unsafe impl<T> Sync for SendPtr<T> {}

impl<T> SendPtr<T> {
    #[inline]
    fn as_const(self) -> *const T {
        self.0
    }
    #[inline]
    fn as_mut(self) -> *mut T {
        self.0
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



/// One output row of the horizontal sliding-window pass.
#[inline]
fn blur_h_row(src: &[u8], dst: &mut [u8], w: usize, y: usize, r: usize, recip: u64) {
    let window = 2 * r + 1;
    let row = y * w * 4;
    // acc = sum over window [-r, +r] around x=0 (edges clamped).
    let mut acc = [0u32; 4];
    for i in 0..window {
        let sx = i.saturating_sub(r).min(w - 1);
        let base = row + sx * 4;
        for c in 0..4 {
            acc[c] += src[base + c] as u32;
        }
    }
    for x in 0..w {
        let dst_o = row + x * 4;
        for c in 0..4 {
            dst[dst_o + c] = avg_channel(acc[c], recip);
        }
        // Slide window from x to x+1: drop x-r, take in x+r+1.
        let leave = row + x.saturating_sub(r) * 4;
        let enter = row + (x + r + 1).min(w - 1) * 4;
        for c in 0..4 {
            acc[c] += src[enter + c] as u32;
            acc[c] -= src[leave + c] as u32;
        }
    }
}

/// One output column of the vertical sliding-window pass.
#[inline]
fn blur_v_col(src: &[u8], dst: &mut [u8], w: usize, h: usize, x: usize, r: usize, recip: u64) {
    let window = 2 * r + 1;
    // acc = sum over window [-r, +r] around y=0 (edges clamped).
    let mut acc = [0u32; 4];
    for i in 0..window {
        let sy = i.saturating_sub(r).min(h - 1);
        let base = (sy * w + x) * 4;
        for c in 0..4 {
            acc[c] += src[base + c] as u32;
        }
    }
    for y in 0..h {
        let dst_o = (y * w + x) * 4;
        for c in 0..4 {
            dst[dst_o + c] = avg_channel(acc[c], recip);
        }
        // Slide window from y to y+1: drop y-r, take in y+r+1.
        let leave = ((y.saturating_sub(r)) * w + x) * 4;
        let enter = (((y + r + 1).min(h - 1)) * w + x) * 4;
        for c in 0..4 {
            acc[c] += src[enter + c] as u32;
            acc[c] -= src[leave + c] as u32;
        }
    }
}

/// Pool chunk context for one blur pass. Slices are reconstructed per chunk
/// from these pointers; chunks own disjoint rows (H) or columns (V).
#[derive(Clone, Copy)]
struct BlurCtx {
    src: SendPtr<u8>,
    dst: SendPtr<u8>,
    len: usize,
    w: usize,
    h: usize,
    r: usize,
    recip: u64,
}

fn blur_h_chunk(ctx: BlurCtx, y0: usize, y1: usize) {
    // SAFETY: chunk owns rows [y0, y1), disjoint across pool jobs.
    let src = unsafe { std::slice::from_raw_parts(ctx.src.as_const(), ctx.len) };
    let dst = unsafe { std::slice::from_raw_parts_mut(ctx.dst.as_mut(), ctx.len) };
    for y in y0..y1 {
        blur_h_row(src, dst, ctx.w, y, ctx.r, ctx.recip);
    }
}

fn blur_v_chunk(ctx: BlurCtx, x0: usize, x1: usize) {
    // SAFETY: chunk owns columns [x0, x1), disjoint across pool jobs.
    let src = unsafe { std::slice::from_raw_parts(ctx.src.as_const(), ctx.len) };
    let dst = unsafe { std::slice::from_raw_parts_mut(ctx.dst.as_mut(), ctx.len) };
    for x in x0..x1 {
        blur_v_col(src, dst, ctx.w, ctx.h, x, ctx.r, ctx.recip);
    }
}

fn blur_ctx(pixmap: &PixmapMut, scratch: &mut Vec<u8>, radius: usize) -> Option<BlurCtx> {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let len = w * h * 4;
    scratch.resize(len, 0);
    Some(BlurCtx {
        src: SendPtr(pixmap.as_ref().data().as_ptr() as *mut u8),
        dst: SendPtr(scratch.as_mut_ptr()),
        len,
        w,
        h,
        r: radius,
        recip: reciprocal(2 * radius + 1),
    })
}

fn blur_horizontal(pixmap: &mut PixmapMut, radius: u32, scratch: &mut Vec<u8>) {
    let r = radius as usize;
    let Some(ctx) = blur_ctx(pixmap, scratch, r) else {
        return;
    };
    pool().par_rows(ctx.h, ctx, blur_h_chunk);
    let buf = scratch.as_slice();
    pixmap.data_mut().copy_from_slice(&buf[..ctx.len]);
}

fn blur_vertical(pixmap: &mut PixmapMut, radius: u32, scratch: &mut Vec<u8>) {
    let r = radius as usize;
    let Some(ctx) = blur_ctx(pixmap, scratch, r) else {
        return;
    };
    pool().par_rows(ctx.w, ctx, blur_v_chunk);
    let buf = scratch.as_slice();
    pixmap.data_mut().copy_from_slice(&buf[..ctx.len]);
}

/// One pixel range of the silhouette pass: solid-color premultiplied copy of
/// the source alpha (`out_a = src_a * color_a * opacity`).
#[inline]
fn silhouette_range(
    s: &[u8],
    d: &mut [u8],
    i0: usize,
    i1: usize,
    cr: u8,
    cg: u8,
    cb: u8,
    ca: u8,
) {
    let fa = ca as u32;
    for i in i0..i1 {
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
        let n = w as usize * h as usize;
        let ctx = SilCtx {
            src: SendPtr(src.as_ref().data().as_ptr() as *mut u8),
            dst: SendPtr(out.data_mut().as_mut_ptr()),
            len: n * 4,
            rw: w as usize,
            cr,
            cg,
            cb,
            ca,
        };
        pool().par_rows(h as usize, ctx, sil_chunk);
    }
    Some(out)
}

/// Pool chunk context for the silhouette pass.
#[derive(Clone, Copy)]
struct SilCtx {
    src: SendPtr<u8>,
    dst: SendPtr<u8>,
    len: usize,
    rw: usize,
    cr: u8,
    cg: u8,
    cb: u8,
    ca: u8,
}

fn sil_chunk(ctx: SilCtx, y0: usize, y1: usize) {
    // SAFETY: chunk owns rows [y0, y1), disjoint across pool jobs.
    let s = unsafe { std::slice::from_raw_parts(ctx.src.as_const(), ctx.len) };
    let d = unsafe { std::slice::from_raw_parts_mut(ctx.dst.as_mut(), ctx.len) };
    for y in y0..y1 {
        silhouette_range(s, d, y * ctx.rw, (y + 1) * ctx.rw, ctx.cr, ctx.cg, ctx.cb, ctx.ca);
    }
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
fn render_pattern_tiles(
    patterns: &[DrawPattern],
    k: f32,
    text: &mut TextEngine,
) -> PatternTiles {
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
            render_cmd_masked(t, &mut pix.as_mut(), xf, None, k, &empty, text);
        }
        out.insert(pat.name.clone(), PatternTile { pix, w_units: pat.width, h_units: pat.height });
    }
    out
}

/// One output row of pattern tile-wrap sampling. The inverse draw transform
/// is inlined as six floats (cheaper than `map_point` dispatch per pixel and
/// trivially shareable across scoped threads).
#[allow(clippy::too_many_arguments)]
#[inline]
fn pattern_sample_row(
    cov: &[u8],
    tdata: &[u8],
    out: &mut [u8],
    rw: usize,
    y: usize,
    isx: f32,
    ikx: f32,
    itx: f32,
    iky: f32,
    isy: f32,
    ity: f32,
    tw_px: u32,
    th_px: u32,
    w_units: f64,
    h_units: f64,
    op: f32,
) {
    for x in 0..rw {
        let ca = cov[(y * rw + x) * 4 + 3] as f32 / 255.0;
        if ca <= 0.0 {
            continue;
        }
        let ux = x as f32 * isx + y as f32 * ikx + itx;
        let uy = x as f32 * iky + y as f32 * isy + ity;
        // Canvas space -> tile texel (wrap; rem_euclid is negative-safe).
        let fx = ((ux as f64).rem_euclid(w_units) / w_units * tw_px as f64) as u32;
        let fy = ((uy as f64).rem_euclid(h_units) / h_units * th_px as f64) as u32;
        let ti = ((fy.min(th_px - 1) * tw_px + fx.min(tw_px - 1)) as usize) * 4;
        let ta = tdata[ti + 3] as f32 / 255.0;
        let ae = ta * ca * op;
        if ae <= 0.0 {
            continue;
        }
        let idx = (y * rw + x) * 4;
        // Tile data is premultiplied: scale by coverage x opacity.
        out[idx] = (tdata[ti] as f32 * ca * op).round() as u8;
        out[idx + 1] = (tdata[ti + 1] as f32 * ca * op).round() as u8;
        out[idx + 2] = (tdata[ti + 2] as f32 * ca * op).round() as u8;
        out[idx + 3] = (ae * 255.0).round() as u8;
    }
}

/// One output row of gradient sampling (same inlined-inverse convention).
#[inline]
fn gradient_sample_row(
    cov: &[u8],
    out: &mut [u8],
    cfg: &GradCfg,
    rw: usize,
    y: usize,
    isx: f32,
    ikx: f32,
    itx: f32,
    iky: f32,
    isy: f32,
    ity: f32,
) {
    for x in 0..rw {
        let ca = cov[(y * rw + x) * 4 + 3] as f32 / 255.0;
        if ca <= 0.0 {
            continue;
        }
        let ux = x as f32 * isx + y as f32 * ikx + itx;
        let uy = x as f32 * iky + y as f32 * isy + ity;
        let t = grad_t(&cfg.kind, (ux as f64, uy as f64));
        let (r, g, b, a) = sample_stops(&cfg.stops, t);
        let ae = a as f32 / 255.0 * ca;
        if ae <= 0.0 {
            continue;
        }
        let idx = (y * rw + x) * 4;
        out[idx] = (r as f32 * ae).round() as u8;
        out[idx + 1] = (g as f32 * ae).round() as u8;
        out[idx + 2] = (b as f32 * ae).round() as u8;
        out[idx + 3] = (ae * 255.0).round() as u8;
    }
}

/// Pool chunk context for pattern tile-wrap sampling.
#[derive(Clone, Copy)]
struct PatCtx {
    cov: SendPtr<u8>,
    tdata: SendPtr<u8>,
    out: SendPtr<u8>,
    len: usize,
    tlen: usize,
    rw: usize,
    inv: [f32; 6],
    tw_px: u32,
    th_px: u32,
    w_units: f64,
    h_units: f64,
    op: f32,
}

fn pattern_chunk(ctx: PatCtx, y0: usize, y1: usize) {
    // SAFETY: chunk owns rows [y0, y1), disjoint across pool jobs.
    let cov = unsafe { std::slice::from_raw_parts(ctx.cov.as_const(), ctx.len) };
    let tdata = unsafe { std::slice::from_raw_parts(ctx.tdata.as_const(), ctx.tlen) };
    let out = unsafe { std::slice::from_raw_parts_mut(ctx.out.as_mut(), ctx.len) };
    let [isx, ikx, itx, iky, isy, ity] = ctx.inv;
    for y in y0..y1 {
        pattern_sample_row(
            cov, tdata, out, ctx.rw, y, isx, ikx, itx, iky, isy, ity, ctx.tw_px,
            ctx.th_px, ctx.w_units, ctx.h_units, ctx.op,
        );
    }
}

/// Pool chunk context for gradient sampling. Stops ride an `Arc` so the job
/// is `'static` (pool jobs can't borrow the stack); `SendPtr` fields are
/// only touched on disjoint rows, as documented on [`SendPtr`].
#[derive(Clone)]
struct GradCtx {
    cov: SendPtr<u8>,
    out: SendPtr<u8>,
    len: usize,
    rw: usize,
    inv: [f32; 6],
    cfg: std::sync::Arc<GradCfg>,
}

fn gradient_chunk(ctx: GradCtx, y0: usize, y1: usize) {
    // SAFETY: chunk owns rows [y0, y1), disjoint across pool jobs.
    let cov = unsafe { std::slice::from_raw_parts(ctx.cov.as_const(), ctx.len) };
    let out = unsafe { std::slice::from_raw_parts_mut(ctx.out.as_mut(), ctx.len) };
    let [isx, ikx, itx, iky, isy, ity] = ctx.inv;
    for y in y0..y1 {
        gradient_sample_row(cov, out, &ctx.cfg, ctx.rw, y, isx, ikx, itx, iky, isy, ity);
    }
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
        let ctx = PatCtx {
            cov: SendPtr(coverage.data().as_ptr() as *mut u8),
            tdata: SendPtr(tile.pix.data().as_ptr() as *mut u8),
            out: SendPtr(layer.data_mut().as_mut_ptr()),
            len: rw as usize * rh as usize * 4,
            tlen: tw_px as usize * th_px as usize * 4,
            rw: rw as usize,
            inv: [inv.sx, inv.kx, inv.tx, inv.ky, inv.sy, inv.ty],
            tw_px,
            th_px,
            w_units: tile.w_units,
            h_units: tile.h_units,
            op,
        };
        pool().par_rows(rh as usize, ctx, pattern_chunk);
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

/// Unified per-pixel gradient fill/stroke (PVG 0.2 Section 9, ported from
/// `pvg_win_gui::software::paint_sampled`).
///
/// Renders `path` coverage (white fill, or white stroke so dash/cap/join are
/// honored) into a cropped temp, then evaluates the gradient in PVG user space
/// per covered pixel: `user = draw_transform⁻¹(pixel)`. Exact for
/// translated/cropped layers, any scale, and focal/conic geometries.
#[allow(clippy::too_many_arguments)]
fn paint_sampled(
    dst: &mut PixmapMut,
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    cfg: GradCfg,
    is_fill: bool,
    style: &DrawStyle,
    transform: Transform,
    blend: SkBlend,
    mask: Option<&Mask>,
) {
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
        // Stops ride an Arc so pool jobs own their inputs (`'static`).
        let ctx = GradCtx {
            cov: SendPtr(coverage.data().as_ptr() as *mut u8),
            out: SendPtr(layer.data_mut().as_mut_ptr()),
            len: rw as usize * rh as usize * 4,
            rw: rw as usize,
            inv: [inv.sx, inv.kx, inv.tx, inv.ky, inv.sy, inv.ty],
            cfg: std::sync::Arc::new(cfg),
        };
        pool().par_rows(rh as usize, ctx, gradient_chunk);
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

/// Fill helper: real pattern tiling on a tile hit, real per-pixel gradients
/// (Section 9), otherwise solid color. Unknown patterns fall back to gray.
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
        match tiles.get(name) {
            Some(tile) => {
                paint_pattern(dst, cmd, path, tile, true, style, transform, style_to_blend(style), mask);
                return;
            }
            None => {
                if let Some(mut p) = solid_or_pattern_paint(&style.fill, style.opacity) {
                    p.blend_mode = style_to_blend(style);
                    dst.fill_path(path, &p, FillRule::Winding, transform, mask);
                }
                return;
            }
        }
    }
    if let Some(cfg) = grad_cfg(&style.fill, style.opacity) {
        paint_sampled(dst, cmd, path, cfg, true, style, transform, style_to_blend(style), mask);
        return;
    }
    if let Some(mut fill_paint) = solid_or_pattern_paint(&style.fill, style.opacity) {
        fill_paint.blend_mode = style_to_blend(style);
        dst.fill_path(path, &fill_paint, FillRule::Winding, transform, mask);
    }
}

/// Stroke helper: same split as [`fill_shape_path`] for stroke paints.
/// Zero-width strokes paint nothing.
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
        match tiles.get(name) {
            Some(tile) => {
                paint_pattern(dst, cmd, path, tile, false, style, transform, style_to_blend(style), mask);
                return;
            }
            None => {
                if let Some(mut p) = solid_or_pattern_paint(&style.stroke, style.opacity) {
                    p.blend_mode = style_to_blend(style);
                    let stroke = style_to_stroke(style);
                    dst.stroke_path(path, &p, &stroke, transform, mask);
                }
                return;
            }
        }
    }
    if let Some(cfg) = grad_cfg(&style.stroke, style.opacity) {
        paint_sampled(dst, cmd, path, cfg, false, style, transform, style_to_blend(style), mask);
        return;
    }
    if let Some(mut stroke_paint) = solid_or_pattern_paint(&style.stroke, style.opacity) {
        stroke_paint.blend_mode = style_to_blend(style);
        let stroke = style_to_stroke(style);
        dst.stroke_path(path, &stroke_paint, &stroke, transform, mask);
    }
}

/// Renders a shape's fill + stroke with the style's own blend mode / mask.
///
/// Splines are stroke-only (spec Section 4.2 forces their fill to NONE) and lines
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
/// when the style carries Section 10 `blur`/`shadow`/`glow`.
///
/// The layer is cropped to [`fx_layer_rect`] in device pixels and its local
/// draw transform is the caller's `transform` shifted by the crop origin, so
/// the device-scale convention (letterbox offset + preview scale) is preserved
/// exactly for both the canvas and tile/pattern render paths.
///
/// Crispness contract: `blur` replaces the shape (reduced full-stack bake is
/// exact — the output is all blur). But `shadow`/`glow` keep a SHARP shape
/// on top, so they take the halo path: reduced halo layers underneath plus a
/// full-resolution repaint. Baking the sharp shape at reduced res (as an
/// earlier revision did) visibly softens dashes, plate edges and text-like
/// geometry on real panels.
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
    if style.blur <= 1e-9 {
        // Shadow/glow only: halos underneath (reduced), sharp shape on top
        // (full resolution).
        if let Some(halos) = build_halos(cmd, path, style, transform, scale, dst.width(), dst.height()) {
            for h in &halos {
                composite_scaled(dst, &h.pix, h.ox, h.oy, h.down, h.blend, mask);
            }
        }
        paint_shape(dst, cmd, path, style, transform, mask, tiles);
        return;
    }
    let (layer, (ox, oy), down) = match build_fx_layer(cmd, path, style, transform, scale, dst.width(), dst.height(), tiles) {
        Some(v) => v,
        None => return,
    };
    composite_scaled(dst, &layer, ox, oy, down, style_to_blend(style), mask);
}

/// One baked drop-shadow / glow halo: a tinted, blurred silhouette at
/// reduced resolution, composited under a full-res sharp repaint.
struct HaloLayer {
    pix: Pixmap,
    /// Device-space origin (shadow offset already applied).
    ox: i32,
    oy: i32,
    down: f32,
    /// Shadow → `SourceOver`, glow → additive `Plus`.
    blend: SkBlend,
}

/// Scalar alpha of a paint for halo coverage. Solid colors contribute their
/// real alpha (so `fill none` contributes nothing); patterns and gradients
/// count as opaque — an accepted approximation for per-pixel translucent
/// gradient stops, exact for all solid art.
fn paint_alpha(paint: &PvgPaint) -> u8 {
    match paint {
        PvgPaint::Color(c) => match c {
            Color::Rgba(_, _, _, a) => *a,
            Color::None => 0,
        },
        PvgPaint::Pattern(_) => 255,
        _ => 255,
    }
}

/// Builds the reduced halo layers for a shadow/glow-only style (no `blur`).
/// The SHARP shape is deliberately NOT included — callers repaint it at full
/// resolution on top, which is what keeps edges crisp. Returns the shadow
/// halo first, then glow (composite order). Only the silhouette alpha is
/// sampled, so gradient/pattern paints cost nothing here.
fn build_halos(
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    style: &DrawStyle,
    transform: Transform,
    scale: f32,
    canvas_w: u32,
    canvas_h: u32,
) -> Option<Vec<HaloLayer>> {
    let mut hr = 0.0f32;
    if let Some(sh) = &style.shadow {
        hr = hr.max(sh.radius as f32);
    }
    if let Some(gl) = &style.glow {
        hr = hr.max(gl.radius as f32);
    }
    if !(hr > 0.0) {
        return None;
    }
    // Halo content is pure blur: aggressive tiers are safe (layer-space
    // radius stays ≥ 3px, bilinear-smoothed at composite).
    let r_dev = hr * scale;
    let down: f32 = if r_dev > 48.0 {
        0.125
    } else if r_dev > 12.0 {
        0.25
    } else {
        0.5
    };
    let (ox, oy, lw, lh) = fx_layer_rect(cmd, style, transform, canvas_w, canvas_h)?;
    let hw = ((lw as f32 * down).round() as u32).max(1);
    let hh = ((lh as f32 * down).round() as u32).max(1);
    let mut cov = Pixmap::new(hw, hh)?;
    // half(p) = (transform(p) - origin) * down, component-wise.
    let hxf = Transform::from_row(
        transform.sx * down,
        transform.ky * down,
        transform.kx * down,
        transform.sy * down,
        (transform.tx - ox as f32) * down,
        (transform.ty - oy as f32) * down,
    );
    {
        // White coverage with the shape's alpha: silhouette() below only
        // reads alpha, so color work is skipped entirely.
        let mut cpm = cov.as_mut();
        let stroke_only = matches!(cmd, DrawCmd::Line { .. } | DrawCmd::Spline { .. });
        if !stroke_only {
            let mut white = Paint::default();
            let a = paint_alpha(&style.fill);
            white.set_color_rgba8(255, 255, 255, a);
            white.anti_alias = true;
            cpm.fill_path(path, &white, FillRule::Winding, hxf, None);
        }
        if style.width > 0.0 {
            let mut white = Paint::default();
            let a = paint_alpha(&style.stroke);
            white.set_color_rgba8(255, 255, 255, a);
            white.anti_alias = true;
            let stroke = style_to_stroke(style);
            cpm.stroke_path(path, &white, &stroke, hxf, None);
        }
    }
    let mut out = Vec::new();
    if let Some(sh) = &style.shadow {
        if let Some(mut px) = silhouette(&cov.as_mut(), &sh.color, style.opacity) {
            let r = ((sh.radius as f32 * scale * down).round().max(0.0)) as u32;
            box_blur(&mut px.as_mut(), r);
            let dx = (sh.offset.0 as f32 * scale).round() as i32;
            let dy = (sh.offset.1 as f32 * scale).round() as i32;
            out.push(HaloLayer {
                pix: px,
                ox: ox + dx,
                oy: oy + dy,
                down,
                blend: SkBlend::SourceOver,
            });
        }
    }
    if let Some(gl) = &style.glow {
        if let Some(mut px) = silhouette(&cov.as_mut(), &gl.color, style.opacity) {
            let r = ((gl.radius as f32 * scale * down).round().max(0.0)) as u32;
            box_blur(&mut px.as_mut(), r);
            out.push(HaloLayer {
                pix: px,
                ox,
                oy,
                down,
                blend: SkBlend::Plus,
            });
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// Blits a finished pre-blend layer onto `dst` with the style blend mode.
fn composite_layer(
    dst: &mut PixmapMut,
    layer: &Pixmap,
    ox: i32,
    oy: i32,
    blend: SkBlend,
    mask: Option<&Mask>,
) {
    dst.draw_pixmap(
        ox,
        oy,
        layer.as_ref(),
        &PixmapPaint { opacity: 1.0, blend_mode: blend, quality: FilterQuality::Nearest },
        Transform::identity(),
        mask,
    );
}

/// Composites a REDUCED-resolution FX layer (`down` = layer_px / device_px)
/// in one step: tiny-skia scales it up during the blit, so no full-res temp
/// is ever allocated and no manual upscale loop runs. The layer content is
/// low-frequency blur/glow by construction, so the scaled blit is visually
/// identical to a full-res build at a fraction of the cost (the old manual
/// bilinear upscale alone cost 12ms on a fullscreen `blur 45` layer).
fn composite_scaled(
    dst: &mut PixmapMut,
    half: &Pixmap,
    ox: i32,
    oy: i32,
    down: f32,
    blend: SkBlend,
    mask: Option<&Mask>,
) {
    if (down - 1.0).abs() < 1e-6 {
        composite_layer(dst, half, ox, oy, blend, mask);
        return;
    }
    let k = 1.0 / down;
    // dst = half * k + origin.
    let xf = Transform::from_scale(k, k).post_translate(ox as f32, oy as f32);
    dst.draw_pixmap(
        0,
        0,
        half.as_ref(),
        &PixmapPaint { opacity: 1.0, blend_mode: blend, quality: FilterQuality::Bilinear },
        xf,
        mask,
    );
}

/// Builds the pre-blend FX layer for one geometric command.
///
/// Device-measured policy (emulator + physical SoCs): the effect stack
/// renders REDUCED and [`composite_scaled`] stretches it back during the
/// blit — no full-res temp, no manual upscale loop. Tiers follow the filter
/// radius so the kernel stays sampled (layer-space radius ≈ 8–12px):
/// monster blurs go 1/8, large ones 1/4, small crisp glows stay 1/2 so
/// sharp shape edges don't soften. (Full-res FX was tried and measured
/// SLOWER on device: 27ms vs 15ms on the Tactical HUD — the blur's memory
/// traffic dominates, and the upscale it avoids is cheaper than the
/// full-res passes.)
///
/// Gradient and pattern sampling inside the layer evaluate in
/// resolution-independent PVG user space, so stops and tile wraps stay exact.
///
/// Returns the (possibly reduced) pixmap, its device-space origin, and the
/// `down` factor (`layer_px = device_px * down`).
#[allow(clippy::too_many_arguments)]
fn build_fx_layer(
    cmd: &DrawCmd,
    path: &tiny_skia::Path,
    style: &DrawStyle,
    transform: Transform,
    scale: f32,
    canvas_w: u32,
    canvas_h: u32,
    tiles: &PatternTiles,
) -> Option<(Pixmap, (i32, i32), f32)> {
    let (ox, oy, lw, lh) = fx_layer_rect(cmd, style, transform, canvas_w, canvas_h)?;
    // Largest filter radius in device px — drives the downscale choice.
    let mut filt_r = style.blur as f32 * scale;
    if let Some(gl) = &style.glow {
        filt_r = filt_r.max(gl.radius as f32 * scale);
    }
    if let Some(sh) = &style.shadow {
        filt_r = filt_r.max(sh.radius as f32 * scale);
    }
    let area = lw as u64 * lh as u64;
    let down: f32 = if filt_r > 60.0 {
        0.125
    } else if filt_r > 24.0 || area > 320 * 320 {
        0.25
    } else {
        0.5
    };
    let hw = ((lw as f32 * down).round() as u32).max(1);
    let hh = ((lh as f32 * down).round() as u32).max(1);
    let mut half = Pixmap::new(hw, hh)?;
    // half(p) = (transform(p) - origin) * down, component-wise.
    let hxf = Transform::from_row(
        transform.sx * down,
        transform.ky * down,
        transform.kx * down,
        transform.sy * down,
        (transform.tx - ox as f32) * down,
        (transform.ty - oy as f32) * down,
    );
    {
        let mut hpm = half.as_mut();
        render_fx_into(&mut hpm, cmd, path, style, hxf, scale * down, tiles);
    }
    Some((half, (ox, oy), down))
}

// ---------------------------------------------------------------------------
// Frame-persistent FX cache: static shapes render once, blit per frame
// ---------------------------------------------------------------------------

/// Cached pre-blend layer for one top-level draw command. Blur/sampled/clip
/// layers are stored REDUCED or full-res (`down`) and blitted in one step;
/// shadow/glow-only shapes instead cache their [`HaloLayer`]s while the
/// sharp shape repaints full-res every frame (crisp edges, cheap vector
/// paint — see [`render_shape`]).
struct FxSlot {
    cmd: Option<DrawCmd>,
    layer: Option<Pixmap>,
    rect: (i32, i32, u32, u32),
    down: f32,
    halos: Vec<HaloLayer>,
}

/// Frame-persistent cache of static pre-blend layers.
///
/// Animated scenes re-evaluate every frame, but most shapes are static:
/// their `DrawCmd` (world-space geometry + resolved style) is bit-identical
/// across frames, so the expensive FX stack (blur/shadow/glow layers,
/// gradient and pattern sampling) renders once and re-composites via a fast
/// `draw_pixmap` blit. Time-varying commands miss the cache and render as
/// usual — correctness is exact by construction (`DrawCmd: PartialEq`).
#[derive(Default)]
pub struct FxCache {
    target: (u32, u32),
    slots: Vec<FxSlot>,
}

/// Baked full-frame image of a scene's leading static run — the software
/// equivalent of Android's display-list / `LAYER_TYPE_HARDWARE` concept
/// (record once, re-issue per frame), applied to our CPU rasterizer.
///
/// Animation usually touches a few top-level commands (rotating prisms, sweep
/// lines) while the heavy stack beneath (big blurs, shadows, gradients,
/// clips) is bit-identical frame to frame. Baking that prefix once replaces
/// per-frame full-buffer fills plus N static blits with a single `memcpy`,
/// with zero pixel loss: validity requires the target size, canvas,
/// background, patterns AND every prefix command to compare equal. Anything
/// time-varying ends the prefix and renders normally.
#[derive(Clone, Default)]
pub struct StaticBg {
    target: (u32, u32),
    canvas_bits: (u64, u64),
    background: Option<Color>,
    patterns: Vec<DrawPattern>,
    prefix: Vec<DrawCmd>,
    pix: Option<Pixmap>,
}

/// Per-frame raster caches held by the engine across animation ticks:
/// static FX layers, the baked static-scene prefix, the previous frame's
/// items for static-run classification, plus the glyph cache.
#[derive(Default)]
pub struct FrameCache {
    pub fx: FxCache,
    pub text: TextEngine,
    pub static_bg: StaticBg,
    prev_items: Vec<DrawCmd>,
}

/// Bakes `prefix` (plus the base fills) into a full-frame pixmap.
/// Pixel-identical to the direct path: same fills, same per-command renderer.
#[allow(clippy::too_many_arguments)]
fn bake_static_bg(
    prefix: &[DrawCmd],
    draw_list: &DrawList,
    transform: Transform,
    scale: f32,
    tiles: &PatternTiles,
    text: &mut TextEngine,
    dst_w: u32,
    dst_h: u32,
    offset_x: f32,
    offset_y: f32,
    scaled_w: f32,
    scaled_h: f32,
) -> Option<Pixmap> {
    let mut bg = Pixmap::new(dst_w, dst_h)?;
    {
        let mut bpm = bg.as_mut();
        bpm.fill(tiny_skia::Color::from_rgba8(8, 9, 13, 255));
        if let Some(canvas_rect) = Rect::from_xywh(offset_x, offset_y, scaled_w, scaled_h) {
            if let Some(ref col) = draw_list.background {
                if let Some(bg_paint) = color_to_skia(col, 1.0) {
                    bpm.fill_rect(canvas_rect, &bg_paint, Transform::identity(), None);
                }
            } else {
                let mut black_paint = Paint::default();
                black_paint.set_color_rgba8(0, 0, 0, 255);
                bpm.fill_rect(canvas_rect, &black_paint, Transform::identity(), None);
            }
        }
        for cmd in prefix {
            render_cmd_masked(cmd, &mut bpm, transform, None, scale, tiles, text);
        }
    }
    Some(bg)
}

impl FxCache {
    /// Drops all entries (call when the surface size changes).
    pub fn clear(&mut self) {
        self.slots.clear();
        self.target = (0, 0);
    }

    fn reset_for_frame(&mut self, target: (u32, u32), items: usize) {
        if self.target != target || self.slots.len() != items {
            self.slots.clear();
            self.slots.resize_with(items, || FxSlot {
                cmd: None,
                layer: None,
                rect: (0, 0, 0, 0),
                down: 1.0,
                halos: Vec::new(),
            });
            self.target = target;
        }
    }
}

/// Style of geometric commands (fill/stroke owners). Text/Sprite/Clip have
/// no single style.
fn cmd_style(cmd: &DrawCmd) -> Option<&DrawStyle> {
    match cmd {
        DrawCmd::Circle { style, .. }
        | DrawCmd::Ellipse { style, .. }
        | DrawCmd::Rectangle { style, .. }
        | DrawCmd::Line { style, .. }
        | DrawCmd::Polygon { style, .. }
        | DrawCmd::Path { style, .. }
        | DrawCmd::Spline { style, .. } => Some(style),
        _ => None,
    }
}

/// True when a paint needs per-pixel sampling (gradient or pattern tile).
fn paint_sampled_paint(paint: &PvgPaint) -> bool {
    !matches!(paint, PvgPaint::Color(_))
}

/// True when every blend in the subtree is `Normal`, so baking the subtree
/// into one layer and compositing with SourceOver is pixel-exact
/// (SourceOver associativity over a transparent base).
fn subtree_all_normal(cmd: &DrawCmd) -> bool {
    match cmd {
        DrawCmd::Clip { content, .. } => content.iter().all(subtree_all_normal),
        DrawCmd::Text { .. } | DrawCmd::Sprite { .. } => true,
        _ => cmd_style(cmd).map(|s| s.blend == BlendMode::Normal).unwrap_or(true),
    }
}

/// True when the subtree contains anything worth baking: FX filters,
/// sampled paints, or nested clips.
fn subtree_has_cost(cmd: &DrawCmd) -> bool {
    match cmd {
        DrawCmd::Clip { content, .. } => content.iter().any(subtree_has_cost),
        DrawCmd::Text { .. } => false,
        DrawCmd::Sprite { .. } => false,
        _ => match cmd_style(cmd) {
            Some(s) => needs_fx(s) || paint_sampled_paint(&s.fill) || paint_sampled_paint(&s.stroke),
            None => false,
        },
    }
}

/// Top-level commands eligible for [`FxCache`]: static-detectable via
/// `DrawCmd` equality and expensive enough to bake (FX filters or sampled
/// paints) — or clips containing such content.
///
/// Baking is pixel-exact for ANY blend mode on single-style shapes: the
/// layer is rendered exactly as the direct path renders it (same inner
/// paints, all backdrop-independent), and the style blend applies at
/// composite time from the cached command. Clips need an all-`Normal`
/// subtree since children bake into one layer composited once with
/// SourceOver (exact by SourceOver associativity over transparency).
fn cmd_cacheable(cmd: &DrawCmd) -> bool {
    match cmd {
        DrawCmd::Clip { .. } => subtree_all_normal(cmd) && subtree_has_cost(cmd),
        _ => match cmd_style(cmd) {
            Some(s) => needs_fx(s) || paint_sampled_paint(&s.fill) || paint_sampled_paint(&s.stroke),
            None => false,
        },
    }
}

/// Builds the cacheable pre-blend layer for one top-level command: FX shapes
/// via [`build_fx_layer`] (reduced, with its `down` factor), clips and
/// sampled paints as full-res cropped temps (`down == 1`).
#[allow(clippy::too_many_arguments)]
fn build_cached_layer(
    cmd: &DrawCmd,
    transform: Transform,
    scale: f32,
    canvas_w: u32,
    canvas_h: u32,
    tiles: &PatternTiles,
    text: &mut TextEngine,
) -> Option<(Pixmap, (i32, i32, u32, u32), f32)> {
    match cmd {
        DrawCmd::Clip { mask, content } => {
            let (x0, y0, x1, y1) = geom_bbox(mask)?;
            let mut pts = [
                Point::from_xy(x0 as f32, y0 as f32),
                Point::from_xy(x1 as f32, y0 as f32),
                Point::from_xy(x0 as f32, y1 as f32),
                Point::from_xy(x1 as f32, y1 as f32),
            ];
            transform.map_points(&mut pts);
            let (mut lx0, mut ly0, mut lx1, mut ly1) =
                (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
            for p in &pts {
                lx0 = lx0.min(p.x);
                ly0 = ly0.min(p.y);
                lx1 = lx1.max(p.x);
                ly1 = ly1.max(p.y);
            }
            let ox = (lx0.floor() as i32 - 2).max(0).min(canvas_w as i32);
            let oy = (ly0.floor() as i32 - 2).max(0).min(canvas_h as i32);
            let ex = (lx1.ceil() as i32 + 2).max(0).min(canvas_w as i32);
            let ey = (ly1.ceil() as i32 + 2).max(0).min(canvas_h as i32);
            if ex <= ox || ey <= oy {
                return None;
            }
            let (lw, lh) = ((ex - ox) as u32, (ey - oy) as u32);
            let mut layer = Pixmap::new(lw, lh)?;
            let lxf = transform.post_translate(-(ox as f32), -(oy as f32));
            let mask_path = cmd_to_path(mask)?;
            let mut inner = Mask::new(lw, lh)?;
            inner.fill_path(&mask_path, FillRule::Winding, true, lxf);
            {
                let mut lpm = layer.as_mut();
                for item in content {
                    render_cmd_masked(item, &mut lpm, lxf, Some(&inner), scale, tiles, text);
                }
            }
            Some((layer, (ox, oy, lw, lh), 1.0))
        }
        _ => {
            let style = cmd_style(cmd)?;
            let path = cmd_to_path(cmd)?;
            // Non-FX sampled shapes bake via a direct paint into a cropped
            // temp; blur-case FX reuses the reduced-res layer builder.
            // Shadow/glow-only shapes return None here — they bake as halo
            // layers plus a per-frame sharp repaint in `render_cached_top`.
            if !needs_fx(style) {
                let (rx0, ry0, rx1, ry1) = loop_rect(cmd, transform, canvas_w, canvas_h);
                let (lw, lh) = (rx1 - rx0, ry1 - ry0);
                if lw == 0 || lh == 0 {
                    return None;
                }
                let mut layer = Pixmap::new(lw, lh)?;
                let lxf = transform.post_translate(-(rx0 as f32), -(ry0 as f32));
                {
                    let mut lpm = layer.as_mut();
                    paint_shape(&mut lpm, cmd, &path, style, lxf, None, tiles);
                }
                Some((layer, (rx0 as i32, ry0 as i32, lw, lh), 1.0))
            } else if style.blur > 1e-9 {
                let (layer, (ox, oy), down) =
                    build_fx_layer(cmd, &path, style, transform, scale, canvas_w, canvas_h, tiles)?;
                let (lw, lh) = (layer.width(), layer.height());
                Some((layer, (ox, oy, lw, lh), down))
            } else {
                None
            }
        }
    }
}

/// Renders one cacheable top-level command through [`FxCache`].
/// Returns true when handled (hit or freshly baked); false falls back to
/// the direct render path.
#[allow(clippy::too_many_arguments)]
fn render_cached_top(
    idx: usize,
    cmd: &DrawCmd,
    dst: &mut PixmapMut,
    transform: Transform,
    scale: f32,
    tiles: &PatternTiles,
    cache: &mut FrameCache,
) -> bool {
    // Shadow/glow-only styles (no `blur`) bake as halo layers + a per-frame
    // full-res sharp repaint (see `render_shape`); everything else with a
    // cacheable cost bakes as one blittable layer.
    let halo_only = cmd_style(cmd)
        .map(|s| s.blur <= 1e-9 && (s.shadow.is_some() || s.glow.is_some()))
        .unwrap_or(false);
    // Disjoint field borrows: FX slots for hit-lookup, text engine for bakes.
    let FrameCache { fx, text, .. } = cache;
    let slot = &mut fx.slots[idx];
    if slot.cmd.as_ref() == Some(cmd) {
        if halo_only {
            if !slot.halos.is_empty() {
                for h in &slot.halos {
                    composite_scaled(dst, &h.pix, h.ox, h.oy, h.down, h.blend, None);
                }
                if let (Some(style), Some(path)) = (cmd_style(cmd), cmd_to_path(cmd)) {
                    paint_shape(dst, cmd, &path, style, transform, None, tiles);
                }
                return true;
            }
        } else if let Some(layer) = &slot.layer {
            let (ox, oy, _, _) = slot.rect;
            let blend = match cmd {
                DrawCmd::Clip { .. } => SkBlend::SourceOver,
                _ => cmd_style(cmd).map(style_to_blend).unwrap_or(SkBlend::SourceOver),
            };
            composite_scaled(dst, layer, ox, oy, slot.down, blend, None);
            return true;
        }
    }
    if halo_only {
        if let (Some(style), Some(path)) = (cmd_style(cmd), cmd_to_path(cmd)) {
            if let Some(halos) =
                build_halos(cmd, &path, style, transform, scale, dst.width(), dst.height())
            {
                for h in &halos {
                    composite_scaled(dst, &h.pix, h.ox, h.oy, h.down, h.blend, None);
                }
                paint_shape(dst, cmd, &path, style, transform, None, tiles);
                let slot = &mut fx.slots[idx];
                slot.cmd = Some(cmd.clone());
                slot.layer = None;
                slot.halos = halos;
                return true;
            }
        }
        return false;
    }
    match build_cached_layer(cmd, transform, scale, dst.width(), dst.height(), tiles, text) {
        Some((layer, rect, down)) => {
            let (ox, oy, _, _) = rect;
            let blend = match cmd {
                DrawCmd::Clip { .. } => SkBlend::SourceOver,
                _ => cmd_style(cmd).map(style_to_blend).unwrap_or(SkBlend::SourceOver),
            };
            composite_scaled(dst, &layer, ox, oy, down, blend, None);
            let slot = &mut fx.slots[idx];
            slot.cmd = Some(cmd.clone());
            slot.layer = Some(layer);
            slot.rect = rect;
            slot.down = down;
            slot.halos = Vec::new();
            true
        }
        None => false,
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

/// Flat color for a text fill paint. Gradients resolve to their middle
/// stop's color (deterministic fallback); patterns resolve to neutral gray.
fn text_solid_color(paint: &PvgPaint) -> Color {
    match paint {
        PvgPaint::Color(c) => c.clone(),
        PvgPaint::Pattern(_) => Color::Rgba(136, 136, 136, 255),
        PvgPaint::Linear { stops, .. }
        | PvgPaint::Radial { stops, .. }
        | PvgPaint::Angular { stops, .. } => stops
            .get(stops.len() / 2)
            .map(|s| s.color.clone())
            .unwrap_or(Color::TRANSPARENT),
    }
}

/// Renders one `text` primitive: white ab_glyph coverage recolored to the
/// fill (with opacity), optional Section 10 `blur`/`shadow`/`glow` stack,
/// then composited once with the style blend mode and clip mask.
///
/// `pos` maps through the draw transform (letterbox offset included);
/// glyph size scales with the transform's uniform scale, so text stays
/// crisp at any buffer resolution.
#[allow(clippy::too_many_arguments)]
fn render_text(
    dst: &mut PixmapMut,
    pos: (f64, f64),
    content: &str,
    size: f64,
    family: &str,
    align: TextAlign,
    style: &DrawStyle,
    transform: Transform,
    mask: Option<&Mask>,
    scale: f32,
    text: &mut TextEngine,
) {
    if content.is_empty() || !(size > 0.0) {
        return;
    }
    let mut dev = Point::from_xy(pos.0 as f32, pos.1 as f32);
    transform.map_point(&mut dev);
    let unit = transform.sx.abs().max(transform.sy.abs());
    let k = if unit > 0.0 && unit.is_finite() {
        unit
    } else {
        scale
    };
    let mut layout = match text.layout(content, size, family, align, (dev.x, dev.y), k) {
        Some(l) => l,
        None => return,
    };
    let fill = text_solid_color(&style.fill);
    let sharp = match silhouette(&layout.pix.as_mut(), &fill, style.opacity) {
        Some(p) => p,
        None => return,
    };
    let mut layer = sharp.clone();
    // Section 10 stack (same order as `render_fx_into`: shadow under the
    // sharp text, additive glow under it, blur replacing it).
    if let Some(sh) = &style.shadow {
        if let Some(mut sh_px) = silhouette(&layer.as_mut(), &sh.color, style.opacity) {
            let r = ((sh.radius as f32 * scale).round().max(0.0)) as u32;
            box_blur(&mut sh_px.as_mut(), r);
            let dx = (sh.offset.0 as f32 * scale).round() as i32;
            let dy = (sh.offset.1 as f32 * scale).round() as i32;
            layer.as_mut().draw_pixmap(
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
        layer.as_mut().draw_pixmap(
            0,
            0,
            sharp.as_ref(),
            &PixmapPaint {
                opacity: 1.0,
                blend_mode: SkBlend::SourceOver,
                quality: FilterQuality::Nearest,
            },
            Transform::identity(),
            None,
        );
    }
    if let Some(gl) = &style.glow {
        if let Some(mut gl_px) = silhouette(&layer.as_mut(), &gl.color, style.opacity) {
            let r = ((gl.radius as f32 * scale).round().max(0.0)) as u32;
            box_blur(&mut gl_px.as_mut(), r);
            layer.as_mut().draw_pixmap(
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
        box_blur(&mut layer.as_mut(), r);
    }
    composite_layer(dst, &layer, layout.ox, layout.oy, style_to_blend(style), mask);
}

/// Renders one command through an optional clip mask (for `clip` content,
/// including nested clips). Pattern paints sample the per-frame `tiles` map;
/// unknown names fall back to flat gray via `paint_to_skia`.
#[allow(clippy::too_many_arguments)]
fn render_cmd_masked(
    cmd: &DrawCmd,
    pixmap: &mut PixmapMut,
    transform: Transform,
    mask: Option<&Mask>,
    scale: f32,
    tiles: &PatternTiles,
    text: &mut TextEngine,
) {
    if let DrawCmd::Text { pos, content, size, font_family, align, style } = cmd {
        render_text(
            pixmap, *pos, content, *size, font_family, *align, style, transform, mask, scale,
            text,
        );
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
                    render_cmd_masked(inner, pixmap, transform, Some(&nested), scale, tiles, text);
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

/// Single-pass background fill shared by the direct path and the bake fallback.
fn base_fills(
    pixmap: &mut PixmapMut,
    draw_list: &DrawList,
    offset_x: f32,
    offset_y: f32,
    scaled_w: f32,
    scaled_h: f32,
) {
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
}

/// High-performance in-place vector rasterizer with single-pass clearing and aspect-ratio alignment.
///
/// `cache` persists static pre-blend layers across frames (see [`FxCache`]);
/// pass a fresh `FxCache::default()` for one-shot renders.
pub fn rasterize_draw_list_into_pixmap_mut(
    draw_list: &DrawList,
    pixmap: &mut PixmapMut,
    target_width: u32,
    target_height: u32,
    cache: &mut FrameCache,
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

    // 2/3. Static-prefix fast path (pattern tiles pre-rendered once per
    // frame at device scale; shapes sample them per-pixel, unknown names
    // fall back to flat gray).
    let tiles = render_pattern_tiles(&draw_list.patterns, scale, &mut cache.text);
    cache
        .fx
        .reset_for_frame((target_width, target_height), draw_list.items.len());

    // Leading run of provably-static items: bit-identical to the previous
    // frame's items (`DrawCmd: PartialEq`). This covers EVERY command type —
    // solid shapes, static text, static sprites — not just FX/gradient ones.
    // Anything time-varying (rotating groups, noise paths, live counters)
    // ends the run and renders through the normal path below.
    let mut prefix_end = 0;
    while prefix_end < draw_list.items.len()
        && prefix_end < cache.prev_items.len()
        && draw_list.items[prefix_end] == cache.prev_items[prefix_end]
    {
        prefix_end += 1;
    }

    // Items before `start` are already on screen (baked blit / memcpy).
    let mut start = 0;
    if prefix_end > 0 {
        let (dst_w, dst_h) = (pixmap.width(), pixmap.height());
        let usable = {
            let b = &cache.static_bg;
            b.pix.is_some()
                && b.target == (dst_w, dst_h)
                && b.canvas_bits
                    == (
                        draw_list.canvas_width.to_bits(),
                        draw_list.canvas_height.to_bits(),
                    )
                && b.background == draw_list.background
                && b.patterns == draw_list.patterns
                && b.prefix == draw_list.items[..prefix_end]
        };
        if usable {
            // Steady state: one memcpy replaces fills + all static work.
            let bg = cache.static_bg.pix.as_ref().unwrap();
            pixmap.data_mut().copy_from_slice(bg.data());
            start = prefix_end;
        } else if let Some(pix) = {
            let FrameCache { text, .. } = &mut *cache;
            bake_static_bg(
                &draw_list.items[..prefix_end],
                draw_list,
                transform,
                scale,
                &tiles,
                text,
                dst_w,
                dst_h,
                offset_x,
                offset_y,
                scaled_w,
                scaled_h,
            )
        } {
            // Bake frame: blit the fresh bake now, reuse it henceforth.
            pixmap.draw_pixmap(
                0,
                0,
                pix.as_ref(),
                &PixmapPaint {
                    opacity: 1.0,
                    blend_mode: SkBlend::SourceOver,
                    quality: FilterQuality::Nearest,
                },
                Transform::identity(),
                None,
            );
            cache.static_bg = StaticBg {
                target: (dst_w, dst_h),
                canvas_bits: (
                    draw_list.canvas_width.to_bits(),
                    draw_list.canvas_height.to_bits(),
                ),
                background: draw_list.background.clone(),
                patterns: draw_list.patterns.clone(),
                prefix: draw_list.items[..prefix_end].to_vec(),
                pix: Some(pix),
            };
            start = prefix_end;
        } else {
            // Bake failed (OOM): fall through to the full direct render.
            base_fills(pixmap, draw_list, offset_x, offset_y, scaled_w, scaled_h);
        }
    } else {
        // Cold / fully-dynamic frame: classic single-pass background fill.
        base_fills(pixmap, draw_list, offset_x, offset_y, scaled_w, scaled_h);
    }

    for (idx, cmd) in draw_list.items.iter().enumerate().skip(start) {
        // Static expensive shapes (FX filters, sampled paints, rich clips)
        // bake once and blit per frame; animated commands miss and render.
        if cmd_cacheable(cmd) && render_cached_top(idx, cmd, pixmap, transform, scale, &tiles, cache) {
            continue;
        }
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

            DrawCmd::Text { pos, content, size, font_family, align, style } => {
                render_text(
                    pixmap,
                    *pos,
                    content,
                    *size,
                    font_family,
                    *align,
                    style,
                    transform,
                    None,
                    scale,
                    &mut cache.text,
                );
            }

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
                            render_cmd_masked(
                                inner,
                                pixmap,
                                transform,
                                Some(&clip_mask),
                                scale,
                                &tiles,
                                &mut cache.text,
                            );
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
                    render_shape(pixmap, cmd, &path, style, transform, None, scale, &tiles);
                }
            }
        }
    }

    // Snapshot for next frame's static-run classification (a few KB of
    // clones — negligible next to a megapixel raster).
    cache.prev_items = draw_list.items.clone();

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
            rasterize_draw_list_into_pixmap_mut(&dl, &mut pm, tw, th, &mut FrameCache::default());
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
            rasterize_draw_list_into_pixmap_mut(&dl, &mut pm, size, size, &mut FrameCache::default());
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

    /// PVG 0.2 Section 10 `blur`: the filtered shape must spread energy past the sharp
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

    /// PVG 0.2 Section 10 `shadow [dx, dy] r color`: a blurred silhouette in the shadow
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

    /// Shadow/glow-only FX must keep the sharp shape pixel-crisp: the region
    /// far from an offset shadow must be byte-identical to the unshadowed
    /// render (an earlier revision baked the sharp shape at reduced res and
    /// smeared these pixels on real panels).
    #[test]
    fn fx_keeps_sharp_edges_crisp() {
        let size = 120u32;
        let plain = render_pixels(
            "PVG 0.2\ncanvas 64 64\n  background #000000\nrectangle\n  pos [16, 16]\n  size [32, 20]\n  radius 4\n  fill #ffffff\n",
            size,
        );
        let shadowed = render_pixels(
            "PVG 0.2\ncanvas 64 64\n  background #000000\nrectangle\n  pos [16, 16]\n  size [32, 20]\n  radius 4\n  fill #ffffff\n  shadow [12, 12] 1 #ff0000\n",
            size,
        );
        // Left half (canvas x in [18, 26]) is far from the +12,+12 shadow
        // (blur r=1 bleeds ~1px): must match the plain render exactly, at
        // full white.
        for cx in [18.0, 22.0, 26.0] {
            let (pr, pg, pb, _) = probe(&plain.0, size, cx, 26.0);
            let (sr, sg, sb, _) = probe(&shadowed.0, size, cx, 26.0);
            assert!(
                pr > 200 && pg > 200 && pb > 200,
                "plain interior must be white at {cx}, got {pr},{pg},{pb}"
            );
            assert_eq!(
                (sr, sg, sb),
                (pr, pg, pb),
                "shadow must not touch the far edge at {cx}"
            );
        }
        // ...while the offset shadow itself is still painted red.
        let (sr, sg, sb, _) = probe(&shadowed.0, size, 40.0, 40.0);
        assert!(sr > 60 && sr > sg && sr > sb, "shadow color lost");
    }

    /// PVG 0.2 Section 10 `glow r color`: an additive blurred halo under the shape.
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

    /// A shape without Section 10 FX must rasterize byte-identically to the same shape
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

    /// Counts near-white pixels in a canvas-space rect (mapped to device px).
    fn bright_in(
        buf: &[u8],
        canvas_w: f64,
        canvas_h: f64,
        tw: u32,
        th: u32,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    ) -> u32 {
        let (ax, ay) = dev_pt(canvas_w, canvas_h, tw, th, x0, y0);
        let (bx, by) = dev_pt(canvas_w, canvas_h, tw, th, x1, y1);
        let mut n = 0u32;
        for y in ay..by.min(th) {
            for x in ax..bx.min(tw) {
                let (r, g, b, _) = px_at(buf, tw, x, y);
                if r > 100 && g > 100 && b > 100 {
                    n += 1;
                }
            }
        }
        n
    }

    fn render_text_pixels(src: &str, tw: u32, th: u32) -> Option<Vec<u8>> {
        // Skip vacuously where no system font exists (minimal CI images).
        let mut probe = TextEngine::default();
        if probe
            .layout("X", 16.0, "mono", TextAlign::Left, (10.0, 10.0), 1.0)
            .is_none()
        {
            return None;
        }
        let dl = pvg::compile(src).expect("source must compile");
        let mut buf = vec![0u8; (tw * th * 4) as usize];
        {
            let mut pm = PixmapMut::from_bytes(&mut buf, tw, th).unwrap();
            rasterize_draw_list_into_pixmap_mut(&dl, &mut pm, tw, th, &mut FrameCache::default());
        }
        Some(buf)
    }

    /// PVG 0.2 Section 6.6: `text` must rasterize system-font glyphs at the
    /// anchor position (hanging top baseline), not vanish.
    #[test]
    fn text_renders_at_anchor() {
        let src = "PVG 0.2\ncanvas 200 100\n  background #000000\ntext\n  pos [20, 30]\n  content \"HELLO\"\n  size 16\n  font \"mono\"\n  align \"left\"\n  fill #ffffff\n";
        let (tw, th) = (200u32, 100u32);
        let Some(buf) = render_text_pixels(src, tw, th) else {
            return;
        };
        let lit = bright_in(&buf, 200.0, 100.0, tw, th, 18.0, 28.0, 110.0, 52.0);
        assert!(lit > 50, "text band must contain glyph pixels, got {lit}");
        let away = bright_in(&buf, 200.0, 100.0, tw, th, 130.0, 60.0, 190.0, 90.0);
        assert_eq!(away, 0, "background must stay clean, got {away}");
    }

    /// `align` must move the glyph block: left-anchored text lives near x,
    /// right-anchored text ends at x.
    #[test]
    fn text_align_moves_block() {
        let l = "PVG 0.2\ncanvas 200 100\n  background #000000\ntext\n  pos [20, 30]\n  content \"HELLO\"\n  size 16\n  font \"mono\"\n  align \"left\"\n  fill #ffffff\n";
        let r = "PVG 0.2\ncanvas 200 100\n  background #000000\ntext\n  pos [180, 30]\n  content \"HELLO\"\n  size 16\n  font \"mono\"\n  align \"right\"\n  fill #ffffff\n";
        let (tw, th) = (200u32, 100u32);
        let (Some(lb), Some(rb)) = (render_text_pixels(l, tw, th), render_text_pixels(r, tw, th))
        else {
            return;
        };
        let l_near = bright_in(&lb, 200.0, 100.0, tw, th, 15.0, 28.0, 80.0, 52.0);
        let l_far = bright_in(&lb, 200.0, 100.0, tw, th, 125.0, 28.0, 185.0, 52.0);
        assert!(l_near > 50 && l_far == 0, "left align: near={l_near} far={l_far}");
        let r_far = bright_in(&rb, 200.0, 100.0, tw, th, 125.0, 28.0, 185.0, 52.0);
        let r_near = bright_in(&rb, 200.0, 100.0, tw, th, 15.0, 28.0, 80.0, 52.0);
        assert!(r_far > 50 && r_near == 0, "right align: far={r_far} near={r_near}");
    }

    /// Empty content draws nothing; unknown families fall back to a face.
    #[test]
    fn text_empty_and_fallback_family() {
        let empty = "PVG 0.2\ncanvas 200 100\n  background #000000\ntext\n  pos [20, 30]\n  content \"\"\n  size 16\n  font \"mono\"\n  align \"left\"\n  fill #ffffff\n";
        let weird = "PVG 0.2\ncanvas 200 100\n  background #000000\ntext\n  pos [20, 30]\n  content \"HI\"\n  size 16\n  font \"fictional-face\"\n  align \"left\"\n  fill #ffffff\n";
        let (tw, th) = (200u32, 100u32);
        if render_text_pixels(empty, tw, th).is_none() {
            return;
        }
        let eb = render_text_pixels(empty, tw, th).unwrap();
        assert_eq!(
            bright_in(&eb, 200.0, 100.0, tw, th, 0.0, 0.0, 200.0, 100.0),
            0,
            "empty content must draw nothing"
        );
        let wb = render_text_pixels(weird, tw, th).unwrap();
        assert!(
            bright_in(&wb, 200.0, 100.0, tw, th, 18.0, 28.0, 80.0, 52.0) > 10,
            "unknown family must fall back to a real face"
        );
    }

    /// Host uniforms (`param`, Section 18.1) must drive the Android evaluator exactly
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

    /// PVG 0.2 Section 9: linear gradients must interpolate along the axis,
    /// not collapse to the middle-stop fallback.
    #[test]
    fn linear_gradient_runs_along_axis() {
        let src = "PVG 0.2\ncanvas 64 64\n  background #000000\nrectangle\n  pos [0, 0]\n  size [64, 64]\n  fill linear [0, 0] [64, 0]\n    stop 0.0 #000000\n    stop 1.0 #ffffff\n";
        let (buf, size) = render_pixels(src, 120);
        let (r0, _, _, _) = probe(&buf, size, 8.0, 32.0);
        let (r1, _, _, _) = probe(&buf, size, 56.0, 32.0);
        assert!(r0 < 80, "left must be dark, got {}", r0);
        assert!(r1 > 170, "right must be bright, got {}", r1);
        assert!(r1 > r0 + 60, "gradient must vary ({} -> {})", r0, r1);
    }

    /// PVG 0.2 Section 9: radial gradients must be bright at the center and
    /// dark at the rim (focal == center case).
    #[test]
    fn radial_gradient_interpolates_stops() {
        let src = "PVG 0.2\ncanvas 64 64\n  background #000000\ncircle\n  center [32, 32]\n  radius 28\n  fill radial [32, 32] 28\n    stop 0.0 #00ffff\n    stop 0.6 #0033aa\n    stop 1.0 #07090e\n";
        let (buf, size) = render_pixels(src, 120);
        let (r, g, b, a) = probe(&buf, size, 32.0, 32.0);
        assert_eq!(a, 255);
        assert!(g > 200 && b > 200, "core must be bright cyan, got {},{},{}", r, g, b);
        let (r2, g2, b2, _) = probe(&buf, size, 32.0, 56.0);
        assert!(r2 < 60 && g2 < 80, "rim must be dark, got {},{},{}", r2, g2, b2);
    }

    fn render_at(src: &str, t: f64, size: u32, cache: &mut FrameCache) -> Vec<u8> {
        let dl = pvg::compile_at_time(src, t).expect("source must compile");
        let mut buf = vec![0u8; (size * size * 4) as usize];
        {
            let mut pm = PixmapMut::from_bytes(&mut buf, size, size).unwrap();
            rasterize_draw_list_into_pixmap_mut(&dl, &mut pm, size, size, cache);
        }
        buf
    }

    /// Static-prefix scene cache: a warmed cache (bake + memcpy path) must
    /// produce byte-identical frames to a fresh one-shot render, while
    /// time-varying shapes still animate (no frozen backdrop).
    #[test]
    fn static_prefix_cache_is_exact() {
        let size = 120u32;
        // Static blurred additive glow + static text/sprite + one time-varying
        // dot, mirroring the shield-core structure (heavy statics below,
        // dynamics above). Static text and sprites must join the baked
        // prefix exactly like static geometry.
        let src = "PVG 0.2\ncanvas 64 64\n  background #000000\ncircle\n  center [32, 32]\n  radius 20\n  fill #00aaff\n  blur 6\n  blend \"add\"\n  opacity 0.5\ntext\n  pos [8, 8]\n  content \"STATIC\"\n  size 12\n  font \"mono\"\n  align \"left\"\n  fill #ffffff\nsprite\n  pos [48, 48]\n  scale 1\n  palette [#00000000, #ff0000]\n  data \"11\"\n  data \"11\"\ncircle\n  center [32 + 10 * sin(time * 2.0), 32]\n  radius 6\n  fill #ffffff\n";
        let fresh = render_at(src, 1.0, size, &mut FrameCache::default());
        let mut cache = FrameCache::default();
        let _ = render_at(src, 0.0, size, &mut cache);
        let _ = render_at(src, 0.5, size, &mut cache);
        let cached = render_at(src, 1.0, size, &mut cache);
        assert_eq!(
            fresh, cached,
            "baked-prefix frame must match the one-shot render exactly"
        );
        // Animation must not freeze behind the cached backdrop.
        let later = render_at(src, 2.0, size, &mut cache);
        assert_ne!(cached, later, "time-varying shapes must keep animating");
    }
}
