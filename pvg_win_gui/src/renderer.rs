use eframe::egui::{self, Color32, ColorImage, Painter, Pos2, Rect, TextureOptions};
use pvg::ast::Color as PvgColor;
use pvg::draw_list::{DrawCmd, DrawList, Paint as PvgPaint, TextAlign};

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

/// Flat fallback from `Paint` to a solid egui color (used for the text
/// overlay; vector fills go through the software rasterizer with real
/// gradients). Gradients use the middle stop's color.
pub fn paint_to_egui(paint: &PvgPaint, opacity: f64) -> Color32 {
    match paint {
        PvgPaint::Color(c) => to_egui_color(c, opacity),
        // Pattern tiles need the full rasterizer; the text overlay uses a
        // neutral gray so patterned text stays visible.
        PvgPaint::Pattern(_) => to_egui_color(&PvgColor::Rgba(136, 136, 136, 255), opacity),
        PvgPaint::Linear { stops, .. }
        | PvgPaint::Radial { stops, .. }
        | PvgPaint::Angular { stops, .. } => {
            if stops.is_empty() {
                Color32::TRANSPARENT
            } else {
                let mid = &stops[stops.len() / 2].color;
                to_egui_color(mid, opacity)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Live preview: full-fidelity software raster -> GPU texture + text overlay
// ---------------------------------------------------------------------------

/// Frame cache for the live preview: skips both CPU rasterization *and* GPU
/// texture upload when nothing changed since the last frame.
///
/// Keyed by `(doc_rev, time, render_scale, canvas)`: typing new code bumps
/// `doc_rev`, animation advances `time`, zoom changes `render_scale`. Pan does
/// not affect the key (the same texture is just drawn at a new offset).
/// Effect layers inside the rasterizer have their own sub-cache
/// (`software::FxCache`), so animated scenes with static glow/shadow only
/// re-rasterize what actually moves.
#[derive(Default)]
pub struct PreviewCache {
    fx: crate::software::FxCache,
    last_key: Option<(u64, u64, u32, u32, u32)>,
    texture: Option<egui::TextureHandle>,
}

impl PreviewCache {
    /// Renders the preview: all vector geometry (with real PVG 0.2 gradients,
    /// dash, clip, blend, blur, glow, shadow) is rasterized on the CPU by
    /// [`crate::software`] and uploaded as a texture; text is then overlaid
    /// with real egui fonts at the exact screen positions.
    ///
    /// Text limitation (documented): overlay text always draws on top of the
    /// vector texture, so a `text` primitive ordered *under* a later shape will
    /// still appear above it, and `clip` masking does not apply to overlay text.
    /// All shipped presets author text-on-top, where this is exact.
    pub fn render_draw_list(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        draw_list: &DrawList,
        origin: Pos2,
        zoom: f32,
        doc_rev: u64,
        time: f64,
    ) {
        // Preview resolution (clamped so extreme zoom doesn't explode the
        // pixmap); the texture is min/mag-filtered onto the canvas rect.
        let render_scale = zoom.clamp(0.25, 3.0);
        let key = (
            doc_rev,
            time.to_bits(),
            render_scale.to_bits(),
            ((draw_list.canvas_width as f32) * render_scale).round() as u32,
            ((draw_list.canvas_height as f32) * render_scale).round() as u32,
        );
        if self.last_key != Some(key) {
            if let Ok(pixmap) = self.fx.render_cached(draw_list, render_scale) {
                let image = pixmap_to_color_image(&pixmap);
                let texture = ctx.load_texture("pvg-preview", image, TextureOptions::LINEAR);
                self.texture = Some(texture);
                self.last_key = Some(key);
            }
        }

        if let Some(ref texture) = self.texture {
            let dest = Rect::from_min_size(
                origin,
                egui::vec2(
                    (draw_list.canvas_width as f32) * zoom,
                    (draw_list.canvas_height as f32) * zoom,
                ),
            );
            painter.image(
                texture.id(),
                dest,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        let to_screen = |p: (f64, f64)| -> Pos2 {
            Pos2::new(origin.x + (p.0 as f32) * zoom, origin.y + (p.1 as f32) * zoom)
        };
        let mut texts = Vec::new();
        for cmd in &draw_list.items {
            collect_text(cmd, &mut texts);
        }
        for t in texts {
            draw_text_overlay(painter, t, &to_screen, zoom);
        }
    }
}

fn collect_text<'a>(cmd: &'a DrawCmd, out: &mut Vec<&'a DrawCmd>) {
    match cmd {
        // Note: `group` nodes are flattened into transformed primitives at
        // eval time, so only `Text` and `Clip` content need walking here.
        DrawCmd::Text { .. } => out.push(cmd),
        DrawCmd::Clip { content, .. } => {
            for sub in content {
                collect_text(sub, out);
            }
        }
        _ => {}
    }
}

/// tiny-skia premultiplied RGBA -> egui unpremultiplied `ColorImage`.
fn pixmap_to_color_image(pixmap: &tiny_skia::Pixmap) -> ColorImage {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let src = pixmap.data();
    let mut pixels = Vec::with_capacity(w * h);
    for px in src.chunks_exact(4) {
        let (r, g, b, a) = (px[0] as u32, px[1] as u32, px[2] as u32, px[3] as u32);
        if a == 0 {
            pixels.push(Color32::TRANSPARENT);
        } else {
            pixels.push(Color32::from_rgba_unmultiplied(
                ((r * 255 / a).min(255)) as u8,
                ((g * 255 / a).min(255)) as u8,
                ((b * 255 / a).min(255)) as u8,
                a as u8,
            ));
        }
    }
    ColorImage { size: [w, h], pixels }
}

fn draw_text_overlay(
    painter: &Painter,
    cmd: &DrawCmd,
    to_screen: &impl Fn((f64, f64)) -> Pos2,
    zoom: f32,
) {
    if let DrawCmd::Text { pos, content, size, font_family, align, style } = cmd {
        let screen_pos = to_screen(*pos);
        // PVG 0.2 note: stroke/blur/shadow/glow on text are not applied to the
        // overlay (fill + opacity only); vector-side FX never touches text.
        let fill_c = paint_to_egui(&style.fill, style.opacity);
        let egui_align = match align {
            TextAlign::Left => egui::Align2::LEFT_TOP,
            TextAlign::Center => egui::Align2::CENTER_TOP,
            TextAlign::Right => egui::Align2::RIGHT_TOP,
        };
        let font_fam = match font_family.to_lowercase().as_str() {
            "mono" | "monospace" | "code" => egui::FontFamily::Monospace,
            _ => egui::FontFamily::Proportional,
        };
        let font_id = egui::FontId::new((*size as f32) * zoom, font_fam);
        painter.text(screen_pos, egui_align, content, font_id, fill_c);
    }
}

// ---------------------------------------------------------------------------
// Exports (delegate to the core / software rasterizer - single source of truth)
// ---------------------------------------------------------------------------

/// SVG export via the core 0.2 emitter (gradients, clip, dash, blend, filters).
pub fn export_svg(draw_list: &DrawList) -> String {
    draw_list.to_svg()
}

/// PNG export via the full-fidelity 0.2 software rasterizer.
pub fn rasterize_png(draw_list: &DrawList, scale: f32) -> Result<Vec<u8>, String> {
    let pixmap = crate::software::render_pixmap(draw_list, scale)?;
    pixmap.encode_png().map_err(|e| format!("PNG encode error: {}", e))
}
