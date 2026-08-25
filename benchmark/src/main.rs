mod allocator;
mod rasterizer;
mod stats;
mod suite;

use allocator::{MemoryDelta, TrackingAllocator};
use pvg::compile_pvg_at_time;
use pvg::eval::Evaluator;
use pvg::lexer::Lexer;
use pvg::parser::Parser;
use stats::LatencyStats;
use suite::{get_reference_presets, get_stress_benchmarks, TestCase};

use std::env;
use std::time::Instant;

#[global_allocator]
static GLOBAL: TrackingAllocator = TrackingAllocator;

struct StageProfile {
    name: &'static str,
    latency: LatencyStats,
    memory: MemoryDelta,
}

struct BenchmarkResult {
    test_case: TestCase,
    primitive_count: usize,
    ast_node_estimate: usize,
    overall_latency: LatencyStats,
    memory_delta: MemoryDelta,
    stages: Vec<StageProfile>,
    spec_compliant_memory: bool,
    spec_compliant_time: bool,
}

fn run_pipeline_benchmark(tc: &TestCase, iterations: usize) -> Result<BenchmarkResult, String> {
    // 1. Warmup Run to ensure caches are warm
    let initial_draw_list = compile_pvg_at_time(&tc.source, 0.0)
        .map_err(|e| format!("Benchmark parse/eval error in '{}': {}", tc.name, e))?;
    let primitive_count = initial_draw_list.items.len();

    // 2. Measure Memory Profile on fresh clean run
    let (_, full_mem) = TrackingAllocator::profile(|| {
        let _ = compile_pvg_at_time(&tc.source, 0.0);
    });

    // 3. Stage-by-Stage Latency & Memory Breakdown
    // Stage A: Lexer
    let mut lex_durations = Vec::with_capacity(iterations);
    let mut last_tokens = Vec::new();
    for _ in 0..iterations {
        let t0 = Instant::now();
        let mut lexer = Lexer::new(&tc.source);
        let tokens = lexer.tokenize_all().unwrap();
        lex_durations.push(t0.elapsed());
        last_tokens = tokens;
    }
    let (_, lex_mem) = TrackingAllocator::profile(|| {
        let mut lexer = Lexer::new(&tc.source);
        let _ = lexer.tokenize_all().unwrap();
    });

    // Stage B: Parser
    let mut parse_durations = Vec::with_capacity(iterations);
    let mut last_doc = None;
    for _ in 0..iterations {
        let t0 = Instant::now();
        let mut parser = Parser::new(last_tokens.clone());
        let doc = parser.parse_document().unwrap();
        parse_durations.push(t0.elapsed());
        last_doc = Some(doc);
    }
    let parsed_doc = last_doc.unwrap();
    let ast_node_estimate = parsed_doc.statements.len();
    let (_, parse_mem) = TrackingAllocator::profile(|| {
        let mut parser = Parser::new(last_tokens.clone());
        let _ = parser.parse_document().unwrap();
    });

    // Stage C: Evaluator
    let mut eval_durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let t0 = Instant::now();
        let evaluator = Evaluator::new_with_time(0.0);
        let _ = evaluator.evaluate_document(&parsed_doc).unwrap();
        eval_durations.push(t0.elapsed());
    }
    let (_, eval_mem) = TrackingAllocator::profile(|| {
        let evaluator = Evaluator::new_with_time(0.0);
        let _ = evaluator.evaluate_document(&parsed_doc).unwrap();
    });

    // Stage D: Full End-to-End Compile
    let mut full_durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let t0 = Instant::now();
        let _ = compile_pvg_at_time(&tc.source, 0.0).unwrap();
        full_durations.push(t0.elapsed());
    }

    // Stage E: SVG Emission
    let mut svg_durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let t0 = Instant::now();
        let _ = rasterizer::emit_svg(&initial_draw_list);
        svg_durations.push(t0.elapsed());
    }
    let (_, svg_mem) = TrackingAllocator::profile(|| {
        let _ = rasterizer::emit_svg(&initial_draw_list);
    });

    // Stage F: Tiny-Skia 1x Software Rasterization
    let mut skia_durations = Vec::with_capacity(iterations.min(100));
    for _ in 0..iterations.min(100) {
        let t0 = Instant::now();
        let _ = rasterizer::rasterize_skia(&initial_draw_list, 1.0);
        skia_durations.push(t0.elapsed());
    }
    let (_, skia_mem) = TrackingAllocator::profile(|| {
        let _ = rasterizer::rasterize_skia(&initial_draw_list, 1.0);
    });

    let overall_stats = LatencyStats::from_durations(full_durations, primitive_count);

    let stages = vec![
        StageProfile {
            name: "Lexer (Tokenize)",
            latency: LatencyStats::from_durations(lex_durations, primitive_count),
            memory: lex_mem,
        },
        StageProfile {
            name: "Parser (AST)",
            latency: LatencyStats::from_durations(parse_durations, primitive_count),
            memory: parse_mem,
        },
        StageProfile {
            name: "Evaluator (DrawList)",
            latency: LatencyStats::from_durations(eval_durations, primitive_count),
            memory: eval_mem,
        },
        StageProfile {
            name: "SVG Emitter",
            latency: LatencyStats::from_durations(svg_durations, primitive_count),
            memory: svg_mem,
        },
        StageProfile {
            name: "Skia 1x Rasterizer",
            latency: LatencyStats::from_durations(skia_durations, primitive_count),
            memory: skia_mem,
        },
    ];

    // Spec compliance checks: < 50 KB peak memory, < 0.200 ms latency
    let spec_compliant_memory = (full_mem.peak_bytes as f64 / 1024.0) < 50.0;
    let spec_compliant_time = overall_stats.mean_us < 200.0;

    Ok(BenchmarkResult {
        test_case: tc.clone(),
        primitive_count,
        ast_node_estimate,
        overall_latency: overall_stats,
        memory_delta: full_mem,
        stages,
        spec_compliant_memory,
        spec_compliant_time,
    })
}

fn run_timeline_fps_benchmark(tc: &TestCase, total_frames: usize) -> Result<(), String> {
    println!("\n┌─────────────────────────────────────────────────────────────────────────────────────────────┐");
    println!("│ ⏱  REAL-TIME 60 FPS ANIMATION TIMELINE BENCHMARK: {:<40} │", tc.name);
    println!("├─────────────────────────────────────────────────────────────────────────────────────────────┤");
    println!("│ Frame Count: {:<6} | Simulated Duration: {:.2}s                                          │", total_frames, total_frames as f64 / 60.0);
    println!("├───────┬──────────────┬──────────────┬──────────────┬──────────────────┬─────────────────────┤");
    println!("│ Frame │ Timestamp(s) │ Eval Time    │ Primitives   │ Heap Peak (KB)   │ 60 FPS Target (16ms)│");
    println!("├───────┼──────────────┼──────────────┼──────────────┼──────────────────┼─────────────────────┤");

    let mut frame_times = Vec::with_capacity(total_frames);
    let frame_budget_us = 16_666.0; // 16.66ms for 60 FPS

    for f in 0..total_frames {
        let t = f as f64 / 60.0;
        let t0 = Instant::now();
        let (dl, mem) = TrackingAllocator::profile(|| compile_pvg_at_time(&tc.source, t).unwrap());
        let elapsed = t0.elapsed();
        frame_times.push(elapsed);

        let us = elapsed.as_secs_f64() * 1_000_000.0;
        let budget_pct = (us / frame_budget_us) * 100.0;
        let peak_kb = mem.peak_bytes as f64 / 1024.0;

        if f % (total_frames / 10).max(1) == 0 || f == total_frames - 1 {
            println!(
                "│ {:>5} │ {:>10.3}s │ {:>9.2} µs │ {:>12} │ {:>14.2} KB │ {:>16.2}% │",
                f + 1,
                t,
                us,
                dl.items.len(),
                peak_kb,
                budget_pct
            );
        }
    }

    let stats = LatencyStats::from_durations(frame_times, 0);
    println!("├───────┴──────────────┴──────────────┴──────────────┴──────────────────┴─────────────────────┤");
    println!(
        "│ Summary: Mean: {:.2} µs ({:.3} ms) | Min: {:.2} µs | Max: {:.2} µs | Max FPS Capacity: {:.0} FPS │",
        stats.mean_us,
        stats.mean_us / 1000.0,
        stats.min_us,
        stats.max_us,
        stats.ops_per_sec
    );
    println!("└─────────────────────────────────────────────────────────────────────────────────────────────┘\n");
    Ok(())
}

fn print_header() {
    println!("\n╔═══════════════════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║                 PVG 0.1 NATIVE RUST HIGH-PRECISION BENCHMARK SUITE                                ║");
    println!("║    Deterministic Procedural Vector Graphics • Microsecond CPU Latency • < 50 KB Memory Footprint  ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════════════════════════════╝\n");
}

fn print_summary_table(results: &[BenchmarkResult]) {
    println!("┌─────────────────────────────────┬────────────┬────────────┬─────────────┬─────────────┬──────────────┬────────────┬─────────┐");
    println!("│ Benchmark Case                  │ Category   │ Shapes     │ Mean (µs)   │ P95 (µs)    │ Heap Peak    │ Heap Alloc │ Spec    │");
    println!("├─────────────────────────────────┼────────────┼────────────┼─────────────┼─────────────┼──────────────┼────────────┼─────────┤");

    for r in results {
        let mem_kb = r.memory_delta.peak_bytes as f64 / 1024.0;
        let alloc_kb = r.memory_delta.bytes_allocated as f64 / 1024.0;
        let spec_tag = if r.spec_compliant_memory && r.spec_compliant_time {
            "✓ PASS"
        } else if r.spec_compliant_memory {
            "~ TIME"
        } else {
            "✗ WARN"
        };

        println!(
            "│ {:<31} │ {:<10} │ {:>10} │ {:>9.2} µs │ {:>9.2} µs │ {:>9.2} KB │ {:>8.1} KB │ {:^7} │",
            r.test_case.name,
            r.test_case.category,
            r.primitive_count,
            r.overall_latency.mean_us,
            r.overall_latency.p95_us,
            mem_kb,
            alloc_kb,
            spec_tag
        );
    }
    println!("└─────────────────────────────────┴────────────┴────────────┴─────────────┴─────────────┴──────────────┴────────────┴─────────┘");
}

fn print_detailed_result(r: &BenchmarkResult) {
    println!("\n┌─────────────────────────────────────────────────────────────────────────────────────────────────────┐");
    println!(
        "│ Case: {:<33} Category: {:<10} Shapes: {:<6} AST Stmts: {:<5} │",
        r.test_case.name, r.test_case.category, r.primitive_count, r.ast_node_estimate
    );
    println!("│ Description: {:<86} │", r.test_case.description);
    println!("├─────────────────────────────────────────────────────────────────────────────────────────────────────┤");
    println!(
        "│ Total Latency : Min: {:>8.2} µs  |  Mean: {:>8.2} µs  |  Median: {:>8.2} µs  |  P95: {:>8.2} µs  |  P99: {:>8.2} µs │",
        r.overall_latency.min_us,
        r.overall_latency.mean_us,
        r.overall_latency.median_us,
        r.overall_latency.p95_us,
        r.overall_latency.p99_us
    );
    println!(
        "│ Throughput    : {:>10.0} ops/sec  |  {:>12.0} primitives/sec  |  StdDev: {:>6.2} µs                     │",
        r.overall_latency.ops_per_sec,
        r.overall_latency.primitives_per_sec,
        r.overall_latency.std_dev_us
    );
    println!(
        "│ Heap Footprint: Peak: {:>8.2} KB  |  Churn: {:>8.2} KB  |  Alloc Ops: {:>6}  |  Dealloc Ops: {:>6}       │",
        r.memory_delta.peak_bytes as f64 / 1024.0,
        r.memory_delta.bytes_allocated as f64 / 1024.0,
        r.memory_delta.alloc_ops,
        r.memory_delta.dealloc_ops
    );
    println!("├──────────────────────────┬──────────────┬──────────────┬──────────────┬──────────────┬──────────────┤");
    println!("│ Pipeline Stage           │ Mean (µs)    │ Median (µs)  │ P95 (µs)     │ Peak Heap    │ Churn Alloc  │");
    println!("├──────────────────────────┼──────────────┼──────────────┼──────────────┼──────────────┼──────────────┤");

    for st in &r.stages {
        println!(
            "│ {:<24} │ {:>10.2} µs │ {:>10.2} µs │ {:>10.2} µs │ {:>9.2} KB │ {:>9.2} KB │",
            st.name,
            st.latency.mean_us,
            st.latency.median_us,
            st.latency.p95_us,
            st.memory.peak_bytes as f64 / 1024.0,
            st.memory.bytes_allocated as f64 / 1024.0
        );
    }
    println!("└──────────────────────────┴──────────────┴──────────────┴──────────────┴──────────────┴──────────────┘");
}

fn print_markdown_table(results: &[BenchmarkResult]) {
    println!("\n### PVG 0.1 Benchmark Execution Results\n");
    println!("| Benchmark Case | Category | Shapes | Latency (Mean) | Latency (P95) | Peak Heap | Total Alloc Churn | Throughput (ops/s) | Spec (<50KB / <0.2ms) |");
    println!("| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |");
    for r in results {
        let mem_kb = r.memory_delta.peak_bytes as f64 / 1024.0;
        let alloc_kb = r.memory_delta.bytes_allocated as f64 / 1024.0;
        let spec_tag = if r.spec_compliant_memory && r.spec_compliant_time {
            "✅ PASS"
        } else if r.spec_compliant_memory {
            "⚠️ TIME"
        } else {
            "❌ LIMIT"
        };
        println!(
            "| **{}** | {} | {} | {:.2} µs | {:.2} µs | {:.2} KB | {:.2} KB | {:.0} | {} |",
            r.test_case.name,
            r.test_case.category,
            r.primitive_count,
            r.overall_latency.mean_us,
            r.overall_latency.p95_us,
            mem_kb,
            alloc_kb,
            r.overall_latency.ops_per_sec,
            spec_tag
        );
    }
    println!();
}

fn print_json(results: &[BenchmarkResult]) {
    println!("[");
    for (i, r) in results.iter().enumerate() {
        let comma = if i + 1 < results.len() { "," } else { "" };
        println!("  {{");
        println!("    \"name\": \"{}\",", r.test_case.name);
        println!("    \"category\": \"{}\",", r.test_case.category);
        println!("    \"primitives\": {},", r.primitive_count);
        println!("    \"latency_mean_us\": {:.3},", r.overall_latency.mean_us);
        println!("    \"latency_median_us\": {:.3},", r.overall_latency.median_us);
        println!("    \"latency_p95_us\": {:.3},", r.overall_latency.p95_us);
        println!("    \"latency_p99_us\": {:.3},", r.overall_latency.p99_us);
        println!("    \"ops_per_sec\": {:.1},", r.overall_latency.ops_per_sec);
        println!("    \"peak_heap_kb\": {:.3},", r.memory_delta.peak_bytes as f64 / 1024.0);
        println!("    \"allocated_bytes\": {},", r.memory_delta.bytes_allocated);
        println!("    \"spec_compliant\": {}", r.spec_compliant_memory && r.spec_compliant_time);
        println!("  }}{}", comma);
    }
    println!("]");
}

fn print_usage() {
    println!("\nPVG 0.1 Benchmark Suite CLI");
    println!("Usage:");
    println!("  cargo run --bin pvg_benchmark -- [options]\n");
    println!("Options:");
    println!("  --all              Run full benchmark suite (Presets + Stress Tests) [default]");
    println!("  --presets          Run only official reference presets (Radar, Dial, Grid, etc.)");
    println!("  --stress           Run high-load procedural stress benchmarks");
    println!("  --preset <name>    Benchmark a specific preset by name (e.g. radar, dial, grid)");
    println!("  --timeline         Run 60 FPS real-time animation jitter benchmark");
    println!("  --iterations <N>   Number of benchmark sample iterations (default: 500)");
    println!("  --detailed         Display detailed stage breakdowns for every test case");
    println!("  --markdown         Output GitHub-ready Markdown summary table");
    println!("  --json             Output raw JSON data for CI/CD automation");
    println!("  -h, --help         Show this help message\n");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_usage();
        return;
    }

    let mut iterations: usize = 500;
    let mut mode_presets = false;
    let mut mode_stress = false;
    let mut mode_timeline = false;
    let mut mode_detailed = false;
    let mut mode_markdown = false;
    let mut mode_json = false;
    let mut specific_preset: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--presets" => mode_presets = true,
            "--stress" => mode_stress = true,
            "--timeline" => mode_timeline = true,
            "--detailed" => mode_detailed = true,
            "--markdown" => mode_markdown = true,
            "--json" => mode_json = true,
            "--iterations" => {
                if i + 1 < args.len() {
                    iterations = args[i + 1].parse::<usize>().unwrap_or(500);
                    i += 1;
                }
            }
            "--preset" => {
                if i + 1 < args.len() {
                    specific_preset = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let mut test_cases = Vec::new();

    if let Some(name) = specific_preset {
        let presets = get_reference_presets();
        let stress = get_stress_benchmarks();
        if let Some(tc) = presets.iter().chain(stress.iter()).find(|c| c.name.eq_ignore_ascii_case(&name)) {
            test_cases.push(tc.clone());
        } else {
            eprintln!("❌ Benchmark case '{}' not found.", name);
            return;
        }
    } else if mode_presets {
        test_cases.extend(get_reference_presets());
    } else if mode_stress {
        test_cases.extend(get_stress_benchmarks());
    } else {
        test_cases.extend(get_reference_presets());
        test_cases.extend(get_stress_benchmarks());
    }

    if !mode_json && !mode_markdown {
        print_header();
        println!("🚀 Executing {} benchmark case(s) with {} sampling iterations each...\n", test_cases.len(), iterations);
    }

    let mut results = Vec::new();
    for tc in &test_cases {
        match run_pipeline_benchmark(tc, iterations) {
            Ok(res) => {
                if mode_detailed && !mode_json && !mode_markdown {
                    print_detailed_result(&res);
                }
                results.push(res);
            }
            Err(e) => {
                eprintln!("❌ Error benchmarking '{}': {}", tc.name, e);
            }
        }
    }

    if mode_json {
        print_json(&results);
    } else if mode_markdown {
        print_markdown_table(&results);
    } else {
        print_summary_table(&results);
    }

    if mode_timeline {
        for tc in test_cases.iter().filter(|c| c.is_animated) {
            let _ = run_timeline_fps_benchmark(tc, 120);
        }
    }
}