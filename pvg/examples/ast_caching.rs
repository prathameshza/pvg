use pvg::{parse, Evaluator};
use std::time::Instant;

fn main() {
    let source = r#"
PVG 0.1
canvas 600 600
  background #0b0c10

set cx = 300
set cy = 300

for i from 0 to 19
  set a = time * 2.0 + i * (TAU / 20)
  circle
    center [cx + 150 * cos(a), cy + 150 * sin(a)]
    radius 8
    fill #00ffcc
    opacity 0.8
"#;

    // Phase 1: Parse string to AST once
    let t0 = Instant::now();
    let doc = parse(source).expect("Parse error");
    let parse_dur = t0.elapsed();
    println!("Phase 1 (One-Time Parse to AST): {:?}", parse_dur);

    // Phase 2: 60 FPS Re-evaluation loop (Microseconds)
    let frames = 600; // 10 seconds of simulated 60 FPS
    let mut total_eval_time = std::time::Duration::ZERO;

    for f in 0..frames {
        let t = f as f64 / 60.0;
        let start = Instant::now();
        let evaluator = Evaluator::new_with_time(t);
        let _ = evaluator.evaluate_document(&doc).expect("Eval error");
        total_eval_time += start.elapsed();
    }

    let avg_us = (total_eval_time.as_secs_f64() / frames as f64) * 1_000_000.0;
    println!("Phase 2 (60 FPS AST Evaluation): {:.2} µs per frame across {} frames", avg_us, frames);
}