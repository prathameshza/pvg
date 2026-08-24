use pvg::ast::Color;
use pvg::compile_pvg_at_time;
use pvg::draw_list::{DrawCmd, DrawList, DrawPathCommand};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

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

    // 2. Shapes
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
                            let mut stroke = Stroke::default();
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

                if let Some(rect) = Rect::from_xywh(x, y, w, h) {
                    let mut pb = PathBuilder::new();
                    pb.push_oval(rect);
                    if let Some(path) = pb.finish() {
                        if let Some(fill_paint) = color_to_skia(&style.fill, style.opacity) {
                            pixmap.fill_path(&path, &fill_paint, FillRule::Winding, transform, None);
                        }
                        if let Some(stroke_paint) = color_to_skia(&style.stroke, style.opacity) {
                            if style.width > 0.0 {
                                let mut stroke = Stroke::default();
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
                    } else if let Some(rect) = Rect::from_xywh(x, y, w, h) {
                        let mut pb = PathBuilder::new();
                        pb.push_rect(rect);
                        pb.finish()
                    } else {
                        None
                    };

                    if let Some(path) = path {
                        if let Some(fill_paint) = color_to_skia(&style.fill, style.opacity) {
                            pixmap.fill_path(&path, &fill_paint, FillRule::Winding, transform, None);
                        }
                        if let Some(stroke_paint) = color_to_skia(&style.stroke, style.opacity) {
                            if style.width > 0.0 {
                                let mut stroke = Stroke::default();
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
                            let mut stroke = Stroke::default();
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
                    for pt in &points[1..] {
                        pb.line_to(pt.0 as f32, pt.1 as f32);
                    }
                    pb.close();

                    if let Some(path) = pb.finish() {
                        if let Some(fill_paint) = color_to_skia(&style.fill, style.opacity) {
                            pixmap.fill_path(&path, &fill_paint, FillRule::Winding, transform, None);
                        }
                        if let Some(stroke_paint) = color_to_skia(&style.stroke, style.opacity) {
                            if style.width > 0.0 {
                                let mut stroke = Stroke::default();
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
                    if let Some(fill_paint) = color_to_skia(&style.fill, style.opacity) {
                        pixmap.fill_path(&path, &fill_paint, FillRule::Winding, transform, None);
                    }
                    if let Some(stroke_paint) = color_to_skia(&style.stroke, style.opacity) {
                        if style.width > 0.0 {
                            let mut stroke = Stroke::default();
                            stroke.width = style.width as f32;
                            pixmap.stroke_path(&path, &stroke_paint, &stroke, transform, None);
                        }
                    }
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
    println!("             PVG 0.1 TO PNG RASTERIZER & EXPORTER CLI                          ");
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
