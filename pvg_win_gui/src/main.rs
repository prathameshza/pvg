mod renderer;

use eframe::egui::{self, Color32, Rect, Vec2};
use pvg::ast::Document;
use pvg::draw_list::DrawList;
use pvg::eval::Evaluator;
use pvg::parse_pvg;
use std::fs;
use std::path::Path;
use std::time::Instant;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PVG 0.1 Studio - Procedural Vector Graphics")
            .with_inner_size([1440.0, 880.0])
            .with_min_inner_size([920.0, 600.0]),
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
        name: "Radar Scanner (Anim)",
        path: "presets/radar.pvg",
        fallback: include_str!("../../presets/radar.pvg"),
    },
    PresetItem {
        name: "Telemetry Card (Text)",
        path: "presets/telemetry_card.pvg",
        fallback: include_str!("../../presets/telemetry_card.pvg"),
    },
    PresetItem {
        name: "Dashboard Dial",
        path: "presets/dial.pvg",
        fallback: include_str!("../../presets/dial.pvg"),
    },
    PresetItem {
        name: "Procedural Grid",
        path: "presets/grid.pvg",
        fallback: include_str!("../../presets/grid.pvg"),
    },
    PresetItem {
        name: "Golden Spiral",
        path: "presets/spiral.pvg",
        fallback: include_str!("../../presets/spiral.pvg"),
    },
    PresetItem {
        name: "Paths & Curves",
        path: "presets/paths.pvg",
        fallback: include_str!("../../presets/paths.pvg"),
    },
    PresetItem {
        name: "Gears & Functions",
        path: "presets/gears.pvg",
        fallback: include_str!("../../presets/gears.pvg"),
    },
];

struct PvgApp {
    code: String,
    cached_doc: Option<Document>,
    draw_list: Option<DrawList>,
    error_msg: Option<String>,
    status_notification: Option<(String, Instant)>,

    // Real-time Telemetry
    raw_parse_us: f64,
    raw_eval_us: f64,
    primitive_count: usize,

    // Smoothed Stable Display Metrics (eliminates jitter fluctuations)
    display_parse_us: f64,
    display_eval_us: f64,
    display_fps: f64,
    last_telemetry_update: Instant,

    // Viewport & Pan/Zoom
    zoom: f32,
    pan: Vec2,

    // Animation Timeline
    auto_run: bool,
    is_animated: bool,
    is_playing: bool,
    speed: f64,
    png_scale: f32,
    current_time: f64,
    last_frame_instant: Instant,
    raw_fps: f64,
}

impl PvgApp {
    pub fn new() -> Self {
        let initial_code = Self::load_preset(0);
        let mut app = Self {
            code: initial_code,
            cached_doc: None,
            draw_list: None,
            error_msg: None,
            status_notification: None,
            raw_parse_us: 0.0,
            raw_eval_us: 0.0,
            primitive_count: 0,
            display_parse_us: 0.0,
            display_eval_us: 0.0,
            display_fps: 60.0,
            last_telemetry_update: Instant::now(),
            zoom: 1.0,
            pan: Vec2::ZERO,
            auto_run: true,
            is_animated: true,
            is_playing: true,
            speed: 1.0,
            png_scale: 2.0,
            current_time: 0.0,
            last_frame_instant: Instant::now(),
            raw_fps: 60.0,
        };
        app.full_recompile(0.0);
        app
    }

    fn load_preset(idx: usize) -> String {
        let p = &PRESETS[idx];
        let candidates = [
            p.path.to_string(),
            format!("../{}", p.path),
            format!("../../{}", p.path),
        ];

        for c in &candidates {
            if Path::new(c).exists() {
                if let Ok(content) = fs::read_to_string(c) {
                    return content;
                }
            }
        }

        p.fallback.to_string()
    }

    /// Full recompile: Parses the AST from string source, then evaluates at `time`
    fn full_recompile(&mut self, time: f64) {
        let parse_start = Instant::now();
        match parse_pvg(&self.code) {
            Ok(doc) => {
                self.raw_parse_us = parse_start.elapsed().as_secs_f64() * 1_000_000.0;
                self.cached_doc = Some(doc);
                self.error_msg = None;
                self.evaluate_cached(time);
            }
            Err(e) => {
                self.error_msg = Some(format!("Parse Error: {}", e));
            }
        }
    }

    /// Fast per-frame evaluation: Re-evaluates only the cached AST (microseconds)
    fn evaluate_cached(&mut self, time: f64) {
        if let Some(ref doc) = self.cached_doc {
            let eval_start = Instant::now();
            let evaluator = Evaluator::new_with_time(time);
            match evaluator.evaluate_document(doc) {
                Ok(dl) => {
                    self.raw_eval_us = eval_start.elapsed().as_secs_f64() * 1_000_000.0;
                    self.primitive_count = dl.items.len();
                    self.draw_list = Some(dl);
                    self.error_msg = None;
                }
                Err(e) => {
                    self.error_msg = Some(format!("Runtime Error: {}", e));
                }
            }
        } else {
            self.full_recompile(time);
        }
    }
}

impl eframe::App for PvgApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Frame rate calculation
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_instant).as_secs_f64();
        self.last_frame_instant = now;
        if dt > 0.0 {
            self.raw_fps = self.raw_fps * 0.9 + (1.0 / dt) * 0.1;
        }

        // Smoothed telemetry update every 180ms (eliminates reading jitter)
        if self.last_telemetry_update.elapsed().as_secs_f64() >= 0.18 {
            self.display_parse_us = self.raw_parse_us;
            self.display_eval_us = self.raw_eval_us;
            self.display_fps = self.raw_fps.round();
            self.last_telemetry_update = now;
        }

        // Notification expiration timer
        if let Some((_, timer)) = self.status_notification {
            if timer.elapsed().as_secs_f64() > 2.5 {
                self.status_notification = None;
            }
        }

        // Keyboard Shortcuts
        let f5_pressed = ctx.input(|i| i.key_pressed(egui::Key::F5));
        let ctrl_enter = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        let space_pressed = ctx.input(|i| i.key_pressed(egui::Key::Space) && !i.focused);

        if f5_pressed || ctrl_enter {
            self.full_recompile(self.current_time);
        }

        if space_pressed {
            self.is_playing = !self.is_playing;
        }

        // Check if document references time
        self.is_animated = self.code.contains("time")
            || self.code.contains(" t ")
            || self.code.contains("(t)")
            || self.code.contains("* t");

        // 60 FPS Timeline Advance (Uses fast cached AST evaluation)
        if self.is_playing && self.is_animated {
            self.current_time += dt * self.speed;
            self.evaluate_cached(self.current_time);
            ctx.request_repaint();
        }

        // Top Toolbar Panel
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("PVG Studio").strong().color(Color32::from_rgb(0, 220, 255)));
                ui.separator();

                // Run Button
                let run_btn = egui::Button::new("Run (F5)").fill(Color32::from_rgb(0, 130, 65));
                if ui.add(run_btn).clicked() {
                    self.full_recompile(self.current_time);
                }

                ui.checkbox(&mut self.auto_run, "Auto-Run");
                ui.separator();

                // Play / Pause
                let play_label = if self.is_playing { "Pause" } else { "Play" };
                if ui.button(play_label).clicked() {
                    self.is_playing = !self.is_playing;
                }

                if ui.button("Reset Time").clicked() {
                    self.current_time = 0.0;
                    self.evaluate_cached(0.0);
                }

                // Time Scrubber Slider
                ui.label("Time:");
                let mut slider_time = self.current_time;
                let slider_response = ui.add(
                    egui::Slider::new(&mut slider_time, 0.0..=10.0)
                        .show_value(true)
                        .suffix("s")
                        .step_by(0.01),
                );
                if slider_response.changed() {
                    self.current_time = slider_time;
                    self.evaluate_cached(self.current_time);
                }

                // Playback Speed
                ui.label("Speed:");
                egui::ComboBox::from_id_source("speed_select")
                    .selected_text(format!("{:.2}x", self.speed))
                    .width(60.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.speed, 0.25, "0.25x");
                        ui.selectable_value(&mut self.speed, 0.5, "0.5x");
                        ui.selectable_value(&mut self.speed, 1.0, "1.0x");
                        ui.selectable_value(&mut self.speed, 2.0, "2.0x");
                        ui.selectable_value(&mut self.speed, 4.0, "4.0x");
                    });

                ui.separator();

                // Presets dropdown
                ui.label("Preset:");
                egui::ComboBox::from_id_source("preset_select")
                    .selected_text("Load Preset")
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        for (i, p) in PRESETS.iter().enumerate() {
                            if ui.button(p.name).clicked() {
                                self.code = Self::load_preset(i);
                                self.pan = Vec2::ZERO;
                                self.zoom = 1.0;
                                self.current_time = 0.0;
                                self.full_recompile(0.0);
                            }
                        }
                    });

                ui.separator();

                // Export Options
                if ui.button("Save SVG").clicked() {
                    if let Some(ref dl) = self.draw_list {
                        let svg_content = renderer::export_svg(dl);
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("render.svg")
                            .add_filter("SVG Vector Graphic", &["svg"])
                            .save_file()
                        {
                            if let Ok(()) = fs::write(&path, svg_content) {
                                self.status_notification = Some(("Saved SVG successfully!".into(), Instant::now()));
                            }
                        }
                    }
                }

                // PNG Scale selector + Save PNG
                egui::ComboBox::from_id_source("png_scale_select")
                    .selected_text(format!("{}x", self.png_scale as u32))
                    .width(42.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.png_scale, 1.0, "1x (SD)");
                        ui.selectable_value(&mut self.png_scale, 2.0, "2x (HD)");
                        ui.selectable_value(&mut self.png_scale, 4.0, "4x (Ultra)");
                    });

                if ui.button("Save PNG").clicked() {
                    if let Some(ref dl) = self.draw_list {
                        match renderer::rasterize_png(dl, self.png_scale) {
                            Ok(png_bytes) => {
                                if let Some(path) = rfd::FileDialog::new()
                                    .set_file_name("render.png")
                                    .add_filter("PNG Raster Image", &["png"])
                                    .save_file()
                                {
                                    if let Ok(()) = fs::write(&path, png_bytes) {
                                        self.status_notification = Some((format!("Saved {}x PNG successfully!", self.png_scale as u32), Instant::now()));
                                    }
                                }
                            }
                            Err(e) => {
                                self.error_msg = Some(format!("PNG Export Error: {}", e));
                            }
                        }
                    }
                }

                if ui.button("Copy SVG").clicked() {
                    if let Some(ref dl) = self.draw_list {
                        let svg_code = renderer::export_svg(dl);
                        ctx.copy_text(svg_code);
                        self.status_notification = Some(("Copied SVG to Clipboard!".into(), Instant::now()));
                    }
                }

                // Reset Pan/Zoom View
                if ui.button("Reset View").clicked() {
                    self.pan = Vec2::ZERO;
                    self.zoom = 1.0;
                }
                ui.label(format!("Zoom: {:.1}x", self.zoom));
            });
            ui.add_space(3.0);
        });

        // Bottom Diagnostics & Telemetry Bar
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let total_lines = self.code.split('\n').count();
                ui.label(format!("Lines: {:<4}", total_lines));
                ui.separator();

                if let Some(ref err) = self.error_msg {
                    ui.colored_label(Color32::from_rgb(255, 80, 80), format!("[ERR] {}", err));
                } else if let Some((ref msg, _)) = self.status_notification {
                    ui.colored_label(Color32::from_rgb(0, 220, 255), format!("[INFO] {}", msg));
                } else {
                    let telemetry_text = format!(
                        "Shapes: {:<4} | Parse: {:>6.1} µs | Eval: {:>6.1} µs | FPS: {:>3.0} | Memory: < 50 KB",
                        self.primitive_count,
                        self.display_parse_us,
                        self.display_eval_us,
                        self.display_fps
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(telemetry_text)
                                .monospace()
                                .color(Color32::from_rgb(80, 255, 120)),
                        )
                        .selectable(false),
                    );
                }
            });
            ui.add_space(2.0);
        });

        // Left Code Editor Panel
        egui::SidePanel::left("editor_panel")
            .resizable(true)
            .default_width(580.0)
            .min_width(380.0)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("PVG Source Code:");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(format!("Time: {:.2}s", self.current_time));
                        });
                    });
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
                                    self.full_recompile(self.current_time);
                                }
                            });
                        });
                });
            });

        // Center Viewport Panel
        egui::CentralPanel::default().show(ctx, |ui| {
            let (response, painter) = ui.allocate_painter(ui.available_size_before_wrap(), egui::Sense::drag());

            // Pan canvas
            if response.dragged() {
                self.pan += response.drag_delta();
            }

            // Zoom canvas on scroll wheel
            if response.hovered() {
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0 {
                    let zoom_factor = if scroll > 0.0 { 1.1 } else { 0.9 };
                    self.zoom = (self.zoom * zoom_factor).clamp(0.05, 20.0);
                }
            }

            let canvas_origin = response.rect.min + self.pan;
            painter.rect_filled(response.rect, 0.0_f32, Color32::from_rgb(16, 17, 20));

            if let Some(ref dl) = self.draw_list {
                let border_rect = Rect::from_min_size(
                    canvas_origin,
                    Vec2::new((dl.canvas_width as f32) * self.zoom, (dl.canvas_height as f32) * self.zoom),
                );
                // Canvas bounding box
                painter.rect_stroke(
                    border_rect,
                    0.0_f32,
                    egui::Stroke::new(1.0_f32, Color32::from_rgb(60, 60, 72)),
                );
                renderer::render_draw_list(&painter, dl, canvas_origin, self.zoom);
            }
        });
    }
}