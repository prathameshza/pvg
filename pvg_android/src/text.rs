//! CPU text rasterization for the Android runtime (PVG 0.2 §6.6).
//!
//! tiny-skia has no text engine, so glyphs come from `ab_glyph` (pure-Rust
//! TrueType rasterizer, already in the workspace lockfile) reading the
//! on-device system fonts. No network fetches, no bundled font binaries,
//! no JNI round-trips:
//!
//! | PVG family | Android file | Host test fallback |
//! | :--- | :--- | :--- |
//! | `"mono"` | `DroidSansMono.ttf` | `consola.ttf` |
//! | `"sans"` (default) | `DroidSans.ttf` | `arial.ttf` |
//! | `"serif"` | `NotoSerif-Regular.ttf` | `times.ttf` |
//!
//! Layout follows the spec: `pos` is the TOP anchor
//! (`dominant-baseline: hanging`), `align` anchors horizontally
//! (`left`/`center`/`right`). Glyphs rasterize directly at device pixels
//! and are cached per `(face, char, size)` so animated labels (RPM counters,
//! clocks) only pay for changed characters.
//!
//! Known limits: single weight per family (no bold/italic synthesis),
//! gradient/pattern text fills fall back to a flat color, and text stroke
//! is ignored (fill-only, like the reference presets use).

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use pvg::draw_list::TextAlign;
use std::collections::HashMap;
use tiny_skia::Pixmap;

/// System font lookup order. `PVG_ANDROID_FONT_DIR` overrides the directory
/// (used by host tests); then the Android system directory, then common
/// desktop fallbacks so `cargo test` exercises the real raster path.
fn font_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(d) = std::env::var("PVG_ANDROID_FONT_DIR") {
        if !d.is_empty() {
            dirs.push(std::path::PathBuf::from(d));
        }
    }
    dirs.push(std::path::PathBuf::from("/system/fonts"));
    dirs.push(std::path::PathBuf::from("C:\\Windows\\Fonts"));
    dirs.push(std::path::PathBuf::from("/usr/share/fonts"));
    dirs.push(std::path::PathBuf::from("/usr/share/fonts/truetype/dejavu"));
    dirs
}

/// Face roles: 0 = mono, 1 = sans, 2 = serif.
fn candidates(role: u8) -> &'static [&'static str] {
    match role {
        0 => &[
            "DroidSansMono.ttf",
            "CutiveMono.ttf",
            "consola.ttf",
            "DejaVuSansMono.ttf",
        ],
        2 => &[
            "NotoSerif-Regular.ttf",
            "times.ttf",
            "DejaVuSerif.ttf",
            "RobotoSerif-Regular.ttf",
        ],
        _ => &[
            "DroidSans.ttf",
            "arial.ttf",
            "DejaVuSans.ttf",
            "Roboto-Regular.ttf",
        ],
    }
}

fn load_face(role: u8) -> Option<FontVec> {
    for dir in font_dirs() {
        for name in candidates(role) {
            if let Ok(bytes) = std::fs::read(dir.join(name)) {
                if let Ok(face) = FontVec::try_from_vec(bytes) {
                    return Some(face);
                }
            }
        }
    }
    None
}

/// Rasterized glyph coverage + metrics (device px).
#[derive(Clone)]
struct GlyphBitmap {
    w: u32,
    h: u32,
    /// Bounding-box minimum relative to the pen (x right, y DOWN from the
    /// baseline — `dy` is typically negative).
    dx: i32,
    dy: i32,
    /// Row-major coverage bytes (0–255).
    cover: Vec<u8>,
}

fn rasterize_glyph(
    face: &FontVec,
    cache: &mut HashMap<(u8, char, u32), GlyphBitmap>,
    role: u8,
    ch: char,
    px: f32,
) -> Option<GlyphBitmap> {
    let key = (role, ch, px.to_bits());
    if let Some(g) = cache.get(&key) {
        return Some(g.clone());
    }
    let scaled = face.as_scaled(PxScale::from(px));
    let og = scaled.outline_glyph(scaled.scaled_glyph(ch))?;
    let bb = og.px_bounds();
    let w = (bb.max.x - bb.min.x).max(0.0) as u32;
    let h = (bb.max.y - bb.min.y).max(0.0) as u32;
    if w == 0 || h == 0 || w > 512 || h > 512 {
        return None;
    }
    let mut cover = vec![0u8; (w * h) as usize];
    og.draw(|x, y, c| {
        if x < w && y < h {
            cover[(y * w + x) as usize] = (c * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    });
    let g = GlyphBitmap {
        w,
        h,
        dx: bb.min.x as i32,
        dy: bb.min.y as i32,
        cover,
    };
    cache.insert(key, g.clone());
    Some(g)
}

/// White coverage text block in device pixels + its device-space origin.
pub struct TextLayout {
    pub pix: Pixmap,
    pub ox: i32,
    pub oy: i32,
}

/// Position-independent rasterized block shared by [`TextEngine::layout`]
/// hits: the anchor is applied at composite time, so identical strings share
/// one block no matter where they are drawn.
#[derive(Clone)]
struct CachedLayout {
    pix: Pixmap,
    bw: u32,
}

/// Cache key for a laid-out block. Deliberately excludes the anchor position
/// (applied later) so static labels hit every frame.
#[derive(Hash, PartialEq, Eq)]
struct LayoutKey {
    content: String,
    px_bits: u32,
    role: u8,
    align: u8,
    scale_bits: u32,
}

/// Cap on cached blocks. Static labels (a handful per scene) stay cached
/// forever; rapidly-changing counters (RPM, clocks) miss and render
/// uncached — and when the table is full, new strings render uncached rather
/// than evicting the stable set (no thrash, bounded memory).
const MAX_LAYOUTS: usize = 24;

/// Persistent CPU text engine: three system faces, a glyph coverage cache,
/// and a laid-out block cache. Lives in the frame cache across frames; safe
/// to hold for the app lifetime.
pub struct TextEngine {
    faces: [Option<FontVec>; 3],
    faces_tried: [bool; 3],
    glyphs: HashMap<(u8, char, u32), GlyphBitmap>,
    layouts: HashMap<LayoutKey, CachedLayout>,
}

impl Default for TextEngine {
    fn default() -> Self {
        Self {
            faces: [None, None, None],
            faces_tried: [false; 3],
            glyphs: HashMap::new(),
            layouts: HashMap::new(),
        }
    }
}

impl TextEngine {
    /// Resolve a PVG `font` family name to a loaded face role.
    /// Falls back across roles so text never vanishes when one file is absent.
    fn resolve_role(&mut self, family: &str) -> Option<u8> {
        let f = family.to_lowercase();
        let want: u8 = if f.contains("mono") {
            0
        } else if f.contains("serif") {
            2
        } else {
            1
        };
        let mut seq = vec![want];
        for r in [1u8, 0, 2] {
            if r != want {
                seq.push(r);
            }
        }
        for role in seq {
            let idx = role as usize;
            if !self.faces_tried[idx] {
                self.faces_tried[idx] = true;
                self.faces[idx] = load_face(role);
            }
            if self.faces[idx].is_some() {
                return Some(role);
            }
        }
        None
    }

    /// Lay out `content` (supports `\n` lines, `\t` → 4 spaces) as white
    /// coverage.
    ///
    /// `dev_pos` is the device-space TOP anchor (the caller maps `pos`
    /// through the draw transform). Returns the coverage pixmap + origin.
    pub fn layout(
        &mut self,
        content: &str,
        size_user: f64,
        family: &str,
        align: TextAlign,
        dev_pos: (f32, f32),
        scale: f32,
    ) -> Option<TextLayout> {
        if !(size_user > 0.0) || !(scale > 0.0) || !scale.is_finite() {
            return None;
        }
        let mut text = content.replace("\r", "").replace('\t', "    ");
        text.retain(|c| !c.is_control() || c == '\n');
        if text.is_empty() {
            return None;
        }
        let px = (size_user as f32 * scale).clamp(1.0, 256.0);
        let role = self.resolve_role(family)?;
        // Fast path: static labels (and repeated counter values) reuse the
        // rasterized block; only the anchor is recomputed.
        let key = LayoutKey {
            content: text.clone(),
            px_bits: px.to_bits(),
            role,
            align: align_tag(align),
            scale_bits: scale.to_bits(),
        };
        if let Some(hit) = self.layouts.get(&key) {
            let (pix, bw) = (hit.pix.clone(), hit.bw);
            let base_x = match align {
                TextAlign::Left => dev_pos.0,
                TextAlign::Center => dev_pos.0 - bw as f32 * 0.5,
                TextAlign::Right => dev_pos.0 - bw as f32,
            };
            return Some(TextLayout {
                pix,
                ox: base_x.round() as i32,
                oy: dev_pos.1.round() as i32,
            });
        }
        // Disjoint field borrows: faces (read) + glyph cache (write).
        let faces = &self.faces;
        let cache = &mut self.glyphs;
        let face = faces[role as usize].as_ref()?;
        let scaled = face.as_scaled(PxScale::from(px));
        let ascent = scaled.ascent();
        let line_h = (ascent - scaled.descent() + scaled.line_gap())
            .ceil()
            .max(1.0);

        // Pass 1: measure lines and collect advances (kern pairs included).
        struct MeasuredLine {
            width: f32,
            pens: Vec<(char, f32)>, // (char, pen_x at glyph start)
        }
        let mut lines: Vec<MeasuredLine> = Vec::new();
        for line in text.split('\n') {
            let mut pens = Vec::new();
            let mut pen = 0.0f32;
            let mut prev: Option<ab_glyph::GlyphId> = None;
            for ch in line.chars() {
                let id = face.glyph_id(ch);
                if let Some(p) = prev {
                    pen += scaled.kern(p, id);
                }
                pens.push((ch, pen));
                pen += scaled.h_advance(id);
                prev = Some(id);
            }
            lines.push(MeasuredLine { width: pen, pens });
        }
        let block_w = lines
            .iter()
            .map(|l| l.width)
            .fold(0.0f32, f32::max)
            .ceil()
            .max(1.0);
        let block_h = (lines.len() as f32 * line_h).ceil().max(1.0);
        if block_w > 2048.0 || block_h > 2048.0 {
            return None;
        }
        // Anchor the block: left starts at x, center straddles it, right ends
        // at it (per-line shifts below align multi-line blocks internally).
        let base_x = match align {
            TextAlign::Left => dev_pos.0,
            TextAlign::Center => dev_pos.0 - block_w * 0.5,
            TextAlign::Right => dev_pos.0 - block_w,
        };
        let (bw, bh) = (block_w as u32, block_h as u32);
        let mut pix = Pixmap::new(bw, bh)?;

        // Pass 2: blit cached coverage.
        for (li, line) in lines.iter().enumerate() {
            let x_shift = match align {
                TextAlign::Left => 0.0,
                TextAlign::Center => ((block_w - line.width) * 0.5).floor(),
                TextAlign::Right => (block_w - line.width).floor(),
            };
            let baseline = li as f32 * line_h + ascent;
            for (ch, pen_x) in &line.pens {
                if *ch == ' ' {
                    continue;
                }
                let g = match rasterize_glyph(face, cache, role, *ch, px) {
                    Some(g) => g,
                    None => continue, // no outline (e.g. emoji): advance kept
                };
                let gx = (x_shift + pen_x + g.dx as f32).round() as i32;
                let gy = (baseline + g.dy as f32).round() as i32;
                let data = pix.data_mut();
                for row in 0..g.h {
                    let dy = gy + row as i32;
                    if dy < 0 || dy >= bh as i32 {
                        continue;
                    }
                    for col in 0..g.w {
                        let c = g.cover[(row * g.w + col) as usize];
                        if c == 0 {
                            continue;
                        }
                        let dx = gx + col as i32;
                        if dx < 0 || dx >= bw as i32 {
                            continue;
                        }
                        // White premultiplied: (c, c, c, c).
                        let o = ((dy as u32 * bw + dx as u32) * 4) as usize;
                        let cur = data[o + 3] as u16;
                        let add = c as u16;
                        // Max-blend overlapping glyph coverage (same color).
                        let a = cur.max(add) as u8;
                        data[o] = a;
                        data[o + 1] = a;
                        data[o + 2] = a;
                        data[o + 3] = a;
                    }
                }
            }
        }
        // Remember the block for identical strings (bounded: rapid counters
        // render uncached rather than evicting the stable label set).
        if self.layouts.len() < MAX_LAYOUTS {
            self.layouts.insert(key, CachedLayout { pix: pix.clone(), bw });
        }
        Some(TextLayout {
            pix,
            ox: base_x.round() as i32,
            oy: dev_pos.1.round() as i32,
        })
    }
}

#[inline]
fn align_tag(align: TextAlign) -> u8 {
    match align {
        TextAlign::Left => 0,
        TextAlign::Center => 1,
        TextAlign::Right => 2,
    }
}
