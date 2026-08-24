use pvg::ast::Color;
use pvg::compile_pvg_at_time;
use pvg::draw_list::{DrawCmd, DrawList, DrawPathCommand, DrawStyle};
use std::env;
use std::f64::consts::PI;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub fn color_to_svg(col: &Color) -> String {
    match col {
        Color::Rgba(r, g, b, 255) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        Color::Rgba(r, g, b, a) => {
            format!("rgba({}, {}, {}, {:.3})", r, g, b, *a as f64 / 255.0)
        }
        Color::None => "none".to_string(),
    }
}

pub fn format_svg_attributes(style: &DrawStyle) -> String {
    let mut attrs = Vec::new();
    attrs.push(format!("fill=\"{}\"", color_to_svg(&style.fill)));

    if style.stroke != Color::None {
        attrs.push(format!("stroke=\"{}\"", color_to_svg(&style.stroke)));
        attrs.push(format!("stroke-width=\"{:.2}\"", style.width));
    } else {
        attrs.push("stroke=\"none\"".to_string());
    }

    if (style.opacity - 1.0).abs() > 0.001 {
        attrs.push(format!("opacity=\"{:.3}\"", style.opacity));
    }

    attrs.join(" ")
}

pub fn emit_draw_commands(items: &[DrawCmd], indent: &str) -> String {
    let mut out = String::new();

    for cmd in items {
        match cmd {
            DrawCmd::Circle { center, radius, style } => {
                out.push_str(&format!(
                    "{}<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" {} />\n",
                    indent, center.0, center.1, radius, format_svg_attributes(style)
                ));
            }
            DrawCmd::Ellipse { center, radius, style } => {
                out.push_str(&format!(
                    "{}<ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" {} />\n",
                    indent, center.0, center.1, radius.0, radius.1, format_svg_attributes(style)
                ));
            }
            DrawCmd::Rectangle { pos, size, corner_radius, style } => {
                if *corner_radius > 0.0 {
                    out.push_str(&format!(
                        "{}<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" {} />\n",
                        indent, pos.0, pos.1, size.0, size.1, corner_radius, corner_radius, format_svg_attributes(style)
                    ));
                } else {
                    out.push_str(&format!(
                        "{}<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" {} />\n",
                        indent, pos.0, pos.1, size.0, size.1, format_svg_attributes(style)
                    ));
                }
            }
            DrawCmd::Line { from, to, style } => {
                out.push_str(&format!(
                    "{}<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" {} />\n",
                    indent, from.0, from.1, to.0, to.1, format_svg_attributes(style)
                ));
            }
            DrawCmd::Polygon { points, style } => {
                if points.is_empty() {
                    continue;
                }
                let pts_str: Vec<String> = points
                    .iter()
                    .map(|p| format!("{:.2},{:.2}", p.0, p.1))
                    .collect();
                out.push_str(&format!(
                    "{}<polygon points=\"{}\" {} />\n",
                    indent,
                    pts_str.join(" "),
                    format_svg_attributes(style)
                ));
            }
            DrawCmd::Path { commands, style } => {
                let mut d_tokens = Vec::new();

                for cmd in commands {
                    match cmd {
                        DrawPathCommand::Start(p) => {
                            d_tokens.push(format!("M {:.2} {:.2}", p.0, p.1));
                        }
                        DrawPathCommand::Line(p) => {
                            d_tokens.push(format!("L {:.2} {:.2}", p.0, p.1));
                        }
                        DrawPathCommand::Quad { cp, ep } => {
                            d_tokens.push(format!("Q {:.2} {:.2}, {:.2} {:.2}", cp.0, cp.1, ep.0, ep.1));
                        }
                        DrawPathCommand::Curve { c1, c2, ep } => {
                            d_tokens.push(format!(
                                "C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2}",
                                c1.0, c1.1, c2.0, c2.1, ep.0, ep.1
                            ));
                        }
                        DrawPathCommand::Arc { center, radius, start_angle, end_angle } => {
                            let r = *radius;
                            let delta = end_angle - start_angle;
                            let end_x = center.0 + r * end_angle.cos();
                            let end_y = center.1 + r * end_angle.sin();

                            if delta.abs() >= (2.0 * PI - 1e-4) {
                                let mid_angle = start_angle + delta / 2.0;
                                let mid_x = center.0 + r * mid_angle.cos();
                                let mid_y = center.1 + r * mid_angle.sin();
                                let sweep = if delta > 0.0 { 1 } else { 0 };
                                d_tokens.push(format!("A {:.2} {:.2} 0 0 {} {:.2} {:.2}", r, r, sweep, mid_x, mid_y));
                                d_tokens.push(format!("A {:.2} {:.2} 0 0 {} {:.2} {:.2}", r, r, sweep, end_x, end_y));
                            } else {
                                let large_arc = if delta.abs() > PI { 1 } else { 0 };
                                let sweep = if delta > 0.0 { 1 } else { 0 };
                                d_tokens.push(format!(
                                    "A {:.2} {:.2} 0 {} {} {:.2} {:.2}",
                                    r, r, large_arc, sweep, end_x, end_y
                                ));
                            }
                        }
                        DrawPathCommand::Close => {
                            d_tokens.push("Z".to_string());
                        }
                    }
                }

                out.push_str(&format!(
                    "{}<path d=\"{}\" {} />\n",
                    indent,
                    d_tokens.join(" "),
                    format_svg_attributes(style)
                ));
            }
        }
    }

    out
}

pub fn emit_svg(draw_list: &DrawList) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
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

    out.push_str(&emit_draw_commands(&draw_list.items, "  "));
    out.push_str("</svg>\n");
    out
}

pub fn emit_animated_svg(frames: &[DrawList], duration_sec: f64) -> String {
    let first = &frames[0];
    let frame_count = frames.len();
    let mut out = String::new();

    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<svg width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\">\n",
        first.canvas_width, first.canvas_height, first.canvas_width, first.canvas_height
    ));

    if let Some(ref bg) = first.background {
        out.push_str(&format!(
            "  <rect width=\"100%\" height=\"100%\" fill=\"{}\" />\n",
            color_to_svg(bg)
        ));
    }

    let n = frame_count as f64;
    for (i, frame) in frames.iter().enumerate() {
        let i_f = i as f64;

        let (values_str, keytimes_str) = if i == 0 {
            let t1 = 1.0 / n;
            ("visible;hidden".to_string(), format!("0; {:.4}", t1))
        } else if i == frame_count - 1 {
            let t0 = (n - 1.0) / n;
            ("hidden;visible".to_string(), format!("0; {:.4}", t0))
        } else {
            let t0 = i_f / n;
            let t1 = (i_f + 1.0) / n;
            ("hidden;visible;hidden".to_string(), format!("0; {:.4}; {:.4}", t0, t1))
        };

        out.push_str("  <g>\n");
        out.push_str(&format!(
            "    <animate attributeName=\"visibility\" values=\"{}\" keyTimes=\"{}\" dur=\"{:.1}s\" repeatCount=\"indefinite\" calcMode=\"discrete\" />\n",
            values_str, keytimes_str, duration_sec
        ));
        out.push_str(&emit_draw_commands(&frame.items, "    "));
        out.push_str("  </g>\n");
    }

    out.push_str("</svg>\n");
    out
}

fn print_usage() {
    println!("\n===============================================================================");
    println!("             PVG 0.1 TO SVG COMPILER & TRANSPILER CLI                          ");
    println!("===============================================================================");
    println!("Usage:");
    println!("  cargo run --bin pvg_to_svg -- <input.pvg> [output.svg] [--time <seconds>] [--static]");
    println!("  cargo run --bin pvg_to_svg -- --all");
    println!();
    println!("Options:");
    println!("  -t, --time <sec>    Capture static frame at specific timestamp (default: 0.0)");
    println!("  --static            Force static export even if file contains animation");
    println!("  -h, --help          Show this help message");
    println!();
    println!("Examples:");
    println!("  cargo run --bin pvg_to_svg -- presets/dial.pvg");
    println!("  cargo run --bin pvg_to_svg -- presets/radar.pvg radar.svg");
    println!("  cargo run --bin pvg_to_svg -- --all");
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

fn transpile_file(input_path: &Path, output_path: &Path, time: f64, force_static: bool) -> Result<(), String> {
    let source = fs::read_to_string(input_path)
        .map_err(|e| format!("Failed to read '{}': {}", input_path.display(), e))?;

    let start = Instant::now();
    let is_animated = !force_static && (source.contains("time") || source.contains(" t ") || source.contains("(t)") || source.contains("* t"));

    let svg_content = if is_animated {
        let total_frames = 30;
        let loop_dur = 3.0;
        let mut frames = Vec::with_capacity(total_frames);

        for f in 0..total_frames {
            let t = (f as f64 / total_frames as f64) * loop_dur;
            let dl = compile_pvg_at_time(&source, t)
                .map_err(|e| format!("Runtime Error at t={:.2}: {}", t, e))?;
            frames.push(dl);
        }

        emit_animated_svg(&frames, loop_dur)
    } else {
        let dl = compile_pvg_at_time(&source, time)
            .map_err(|e| format!("Runtime Error: {}", e))?;
        emit_svg(&dl)
    };

    fs::write(output_path, svg_content)
        .map_err(|e| format!("Failed to write '{}': {}", output_path.display(), e))?;

    let elapsed = start.elapsed().as_micros();
    let mode_str = if is_animated { "Animated (SMIL 30 FPS)" } else { "Static" };

    println!(
        "  ✓ [SUCCESS] {:<20} -> {:<20} ({:<24} | {:.3} ms)",
        input_path.file_name().unwrap().to_str().unwrap_or(""),
        output_path.file_name().unwrap().to_str().unwrap_or(""),
        mode_str,
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

    if args[1] == "--all" {
        let presets_dir = match find_presets_dir() {
            Some(d) => d,
            None => {
                eprintln!("\n❌ Could not locate the 'presets' directory.\n");
                return;
            }
        };

        println!("\n===============================================================================");
        println!("              PVG TO SVG BATCH TRANSPILER                                      ");
        println!("              Scanning: {}                                                     ", presets_dir.display());
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
                out_path.set_extension("svg");
                if let Err(e) = transpile_file(&path, &out_path, 0.0, false) {
                    eprintln!("  ✗ Failed '{}': {}", path.display(), e);
                } else {
                    count += 1;
                }
            }
        }

        println!("\n-------------------------------------------------------------------------------");
        println!(" ✨ Successfully transpiled {} file(s) into valid SVG in '{}'!", count, presets_dir.display());
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

    let mut output_path = None;
    let mut time = 0.0;
    let mut force_static = false;

    let mut i = 2;
    while i < args.len() {
        if args[i] == "-t" || args[i] == "--time" {
            if i + 1 < args.len() {
                time = args[i + 1].parse::<f64>().unwrap_or(0.0);
                i += 2;
                continue;
            }
        } else if args[i] == "--static" {
            force_static = true;
        } else if output_path.is_none() && !args[i].starts_with('-') {
            output_path = Some(PathBuf::from(&args[i]));
        }
        i += 1;
    }

    let final_output = output_path.unwrap_or_else(|| {
        let mut p = PathBuf::from(input_path.file_name().unwrap());
        p.set_extension("svg");
        p
    });

    println!("\n⚡ Transpiling PVG -> SVG...");
    match transpile_file(&input_path, &final_output, time, force_static) {
        Ok(_) => {
            println!("✨ Done! Created output at '{}'.\n", final_output.display());
        }
        Err(e) => {
            eprintln!("\n❌ Transpilation failed: {}\n", e);
            std::process::exit(1);
        }
    }
}
