use pvg_lib::png_rasterizer::{rasterize_draw_list_to_png, rasterize_pvg_to_png};
use pvg_lib::compile_pvg_at_time;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

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

fn find_presets_dir(custom_path: Option<&str>) -> Option<PathBuf> {
    if let Some(cp) = custom_path {
        let p = PathBuf::from(cp);
        if p.is_dir() {
            return Some(p);
        }
    }

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
    let png_bytes = rasterize_pvg_to_png(&source, time, scale)?;
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

    let mut i = 1;
    let is_batch = args[1] == "--all";
    if is_batch {
        i = 2;
    }

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

    // Batch Transpilation Mode: --all
    if is_batch {
        let presets_dir = match find_presets_dir(None) {
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

    // Single File Mode
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