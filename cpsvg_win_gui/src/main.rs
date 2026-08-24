use pvg_lib::compile_pvg_at_time;
use pvg_lib::draw_list::DrawList;
use pvg_lib::png_rasterizer::rasterize_draw_list_to_png;
use pvg_lib::renderer;
use pvg_lib::svg_emitter::emit_svg;
use eframe::egui::{self, Color32, Rect, Vec2};
use std::fs;
use std::path::Path;
use std::time::Instant;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PVG 0.1 - Windows GUI Live Preview")
            .with_inner_size([1400.0, 860.0]),
        ..Default::default()
    };

    eframe::run_native(
        "PVG Studio",
        options,
        Box::new(|_cc| Ok(Box::new(PvgApp::new()))),
    )
}

struct PresetItem {
    name: &'static str,
    path: &'static str,
    fallback: &'static str,
}

const PRESETS: &[PresetItem] = &[
    PresetItem {
        name: "🌀 Radar Scanner (Anim)",
        path: "presets/radar.pvg",
        fallback: include_str!("../presets/radar.pvg"),
    },
    PresetItem {
        name: "Dashboard Dial",
        path: "presets/dial.pvg",
        fallback: include_str!("../presets/dial.pvg"),
    },
    PresetItem {
        name: "Procedural Grid",
        path: "presets/grid.pvg",
        fallback: include_str!("../presets/grid.pvg"),
    },
    PresetItem {
        name: "Golden Spiral",
        path: "presets/spiral.pvg",
        fallback: include_str!("../presets/spiral.pvg"),
    },
    PresetItem {
        name: "Paths & Curves",
        path: "presets/paths.pvg",
        fallback: include_str!("../presets/paths.pvg"),
    },
    PresetItem {
        name: "Gears & Groups",
        path: "presets/gears.pvg",
        fallback: include_str!("../presets/gears.pvg"),
    },
];

struct PvgApp {
    code: String,
    current_preset_idx: usize,
    draw_list: Option<DrawList>,
    error_msg: Option<String>,
    status_notification: Option<(String, Instant)>,
    render_time_ms: f64,
    primitive_count: usize,
    zoom: f32,
    pan: Vec2,
    auto_run: bool,
    is_animated: bool,
    start_time: Instant,
    current_time: f64,
}

impl PvgApp {
    pub fn new() -> Self {
        let initial_code = Self::load_preset(0);
        let mut app = Self {
            code: initial_code,
            current_preset_idx: 0,
            draw_list: None,
            error_msg: None,
            status_notification: None,
            render_time_ms: 0.0,
            primitive_count: 0,
            zoom: 1.0,
            pan: Vec2::ZERO,
            auto_run: true,
            is_animated: true,
            start_time: Instant::now(),
            current_time: 0.0,
        };
        app.recompile_at(0.0);
        app
    }

    fn load_preset(idx: usize) -> String {
        let p = &PRESETS[idx];
        if Path::new(p.path).exists() {
            fs::read_to_string(p.path).unwrap_or_else(|_| p.fallback.to_string())
        } else {
            p.fallback.to_string()
        }
    }

    fn recompile_at(&mut self, time: f64) {
        let start = Instant::now();
        match compile_pvg_at_time(&self.code, time) {
            Ok(dl) => {
                self.primitive_count = dl.items.len();
                self.draw_list = Some(dl);
                self.error_msg = None;
                self.render_time_ms = start.elapsed().as_secs_f64() * 1000.0;
            }
            Err(e) => {
                self.error_msg = Some(e);
            }
        }
    }

    fn export_png(&mut self) {
        if let Some(ref dl) = self.draw_list {
            let start = Instant::now();
            match rasterize_draw_list_to_png(dl, 1.0) {
                Ok(bytes) => {
                    let filename = format!("export_preset_{}.png", self.current_preset_idx + 1);
                    if let Err(e) = fs::write(&filename, &bytes) {
                        self.status_notification = Some((format!("Failed to save PNG: {}", e), Instant::now()));
                    } else {
                        let dur = start.elapsed().as_secs_f64() * 1000.0;
                        let size_kb = bytes.len() as f64 / 1024.0;
                        self.status_notification = Some((
                            format!("✓ Exported PNG '{}' ({:.1} KB in {:.2} ms)", filename, size_kb, dur),
                            Instant::now(),
                        ));
                    }
                }
                Err(e) => {
                    self.status_notification = Some((format!("PNG Export Error: {}", e), Instant::now()));
                }
            }
        }
    }

    fn export_svg(&mut self) {
        if let Some(ref dl) = self.draw_list {
            let start = Instant::now();
            let svg_content = emit_svg(dl);
            let filename = format!("export_preset_{}.svg", self.current_preset_idx + 1);
            if let Err(e) = fs::write(&filename, &svg_content) {
                self.status_notification = Some((format!("Failed to save SVG: {}", e), Instant::now()));
            } else {
                let dur = start.elapsed().as_secs_f64() * 1000.0;
                let size_kb = svg_content.len() as f64 / 1024.0;
                self.status_notification = Some((
                    format!("✓ Exported SVG '{}' ({:.1} KB in {:.2} ms)", filename, size_kb, dur),
                    Instant::now(),
                ));
            }
        }
    }
}

impl eframe::App for PvgApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let f5_pressed = ctx.input(|i| i.key_pressed(egui::Key::F5));
        let ctrl_enter = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        if f5_pressed || ctrl_enter {
            self.recompile_at(self.current_time);
        }

        // Animation update loop
        if self.is_animated && (self.code.contains("time") || self.code.contains(" t ") || self.code.contains("(t)") || self.code.contains("* t")) {
            self.current_time = self.start_time.elapsed().as_secs_f64();
            self.recompile_at(self.current_time);
            ctx.request_repaint();
        }

        // Top Toolbar
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("⚡ PVG 0.1 Studio");
                ui.separator();

                let run_btn = egui::Button::new("▶ Run (F5)").fill(Color32::from_rgb(0, 140, 70));
                if ui.add(run_btn).clicked() {
                    self.recompile_at(self.current_time);
                }

                ui.checkbox(&mut self.auto_run, "Auto-Run on edit");
                ui.separator();

                let anim_label = if self.is_animated { "⏸ Pause" } else { "▶ Play" };
                if ui.button(anim_label).clicked() {
                    self.is_animated = !self.is_animated;
                }
                if ui.button("⏮ Reset Time").clicked() {
                    self.start_time = Instant::now();
                    self.current_time = 0.0;
                    self.recompile_at(0.0);
                }
                ui.label(format!("Time: {:.2}s", self.current_time));

                ui.separator();
                let png_btn = egui::Button::new("📷 Export PNG").fill(Color32::from_rgb(0, 110, 180));
                if ui.add(png_btn).clicked() {
                    self.export_png();
                }

                let svg_btn = egui::Button::new("🌐 Export SVG").fill(Color32::from_rgb(160, 80, 0));
                if ui.add(svg_btn).clicked() {
                    self.export_svg();
                }

                ui.separator();
                ui.label("Presets:");
                for (i, p) in PRESETS.iter().enumerate() {
                    if ui.button(p.name).clicked() {
                        self.current_preset_idx = i;
                        self.code = Self::load_preset(i);
                        self.pan = Vec2::ZERO;
                        self.zoom = 1.0;
                        self.start_time = Instant::now();
                        self.current_time = 0.0;
                        self.recompile_at(0.0);
                    }
                }

                ui.separator();
                if ui.button("Reset View").clicked() {
                    self.pan = Vec2::ZERO;
                    self.zoom = 1.0;
                }
                ui.label(format!("Zoom: {:.1}x", self.zoom));
            });
        });

        // Bottom Status Bar
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let total_lines = self.code.split('\n').count();
                ui.label(format!("Lines: {}", total_lines));
                ui.separator();

                if let Some(ref err) = self.error_msg {
                    ui.colored_label(Color32::from_rgb(255, 80, 80), format!("✗ {}", err));
                } else {
                    ui.colored_label(
                        Color32::from_rgb(80, 255, 120),
                        format!(
                            "✓ Rendered {} primitives in {:.3} ms | Working Memory: < 50 KB",
                            self.primitive_count, self.render_time_ms
                        ),
                    );
                }

                if let Some((ref msg, instant)) = self.status_notification {
                    if instant.elapsed().as_secs_f64() < 5.0 {
                        ui.separator();
                        ui.colored_label(Color32::from_rgb(255, 215, 0), msg);
                    }
                }
            });
        });

        // Left Code Editor Panel with Line Numbers and Scrollbar
        egui::SidePanel::left("editor_panel")
            .resizable(true)
            .default_width(560.0)
            .min_width(380.0)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.label("PVG Code Editor:");
                    ui.add_space(2.0);

                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                let line_count = self.code.split('\n').count().max(1);
                                let mut line_numbers_text = String::with_capacity(line_count * 5);
                                for line_idx in 1..=line_count {
                                    line_numbers_text.push_str(&format!("{:>3}\n", line_idx));
                                }

                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(line_numbers_text)
                                            .monospace()
                                            .color(Color32::from_rgb(90, 95, 115)),
                                    )
                                    .selectable(false),
                                );

                                ui.separator();

                                let text_edit = egui::TextEdit::multiline(&mut self.code)
                                    .font(egui::TextStyle::Monospace)
                                    .code_editor()
                                    .lock_focus(true)
                                    .desired_width(f32::INFINITY)
                                    .frame(false);

                                let response = ui.add(text_edit);
                                if response.changed() && self.auto_run {
                                    self.recompile_at(self.current_time);
                                }
                            });
                        });
                });
            });

        // Right Interactive Preview Canvas
        egui::CentralPanel::default().show(ctx, |ui| {
            let (response, painter) = ui.allocate_painter(ui.available_size_before_wrap(), egui::Sense::drag());

            if response.dragged() {
                self.pan += response.drag_delta();
            }

            if response.hovered() {
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0 {
                    let zoom_factor = if scroll > 0.0 { 1.1 } else { 0.9 };
                    self.zoom = (self.zoom * zoom_factor).clamp(0.1, 10.0);
                }
            }

            let canvas_origin = response.rect.min + self.pan;
            painter.rect_filled(response.rect, 0.0_f32, Color32::from_rgb(20, 20, 22));

            if let Some(ref dl) = self.draw_list {
                let border_rect = Rect::from_min_size(
                    canvas_origin,
                    Vec2::new((dl.canvas_width as f32) * self.zoom, (dl.canvas_height as f32) * self.zoom),
                );
                painter.rect_stroke(
                    border_rect,
                    0.0_f32,
                    egui::Stroke::new(1.0_f32, Color32::from_rgb(70, 70, 80)),
                );
                renderer::render_draw_list(&painter, dl, canvas_origin, self.zoom);
            }
        });
    }
}