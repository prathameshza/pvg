use pvg_lib::compile_pvg_at_time;
use pvg_lib::svg_emitter::{emit_animated_svg, emit_svg};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

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

    // Batch Transpilation Mode: --all
    if args[1] == "--all" {
        let custom_dir = if args.len() >= 3 && !args[2].starts_with('-') {
            Some(args[2].as_str())
        } else {
            None
        };

        let presets_dir = match find_presets_dir(custom_dir) {
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

    // Single File Mode
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