# ⚡ PVG 0.1 Benchmark Suite

This standalone benchmark suite delivers high-precision profiling of the Procedural Vector Graphics core engine written in Rust.

## Features Profiled

1. **Sub-Byte Heap Memory Footprint:** Built-in zero-dependency `TrackingAllocator` measures exact peak heap allocations, allocation operations, deallocations, and memory churn rate against PVG's `< 50 KB` specification target.
2. **Microsecond Pipeline Latency:** Measures step-by-step latency for:
   - **Lexer:** Tokenization throughput
   - **Parser:** AST generation
   - **Evaluator:** Procedural resolution (loops, dynamic scopes, functions, math)
   - **SVG Emitter:** XML vector string generation
   - **Software Rasterizer:** Skia 1x pixel buffer rasterization
3. **60 FPS Real-time Timeline Jitter:** Measures simulated 60 FPS animation frame budgets across animated presets (e.g. `radar.pvg`).
4. **Stress & Scalability Tests:** Evaluates performance with 10,000 geometric primitives, deep 2D affine transform hierarchies, and heavy trigonometric loops.

---

## 🖥️ Benchmark Platform Testbed Environment

The reference benchmark results were executed on the following system configuration:

* **Machine:** Lenovo LOQ 15ARP9 (83JC)
* **Processor:** AMD Ryzen™ 5 7235HS (4 Cores, 8 Threads @ 3.20 GHz)
* **Installed Memory:** 24.0 GB Physical RAM
* **Operating System:** Microsoft Windows 11 Home Single Language (64-bit, Build 26200)

---

## Running Benchmarks

### 1. Run the Full Suite
```bash
cargo run --release --bin pvg_benchmark -- --detailed

# 1. Real-time 60 FPS animation timeline pacing and jitter analysis
cargo run --release --bin pvg_benchmark -- --timeline

# 2. Export GitHub-ready Markdown summary tables
cargo run --release --bin pvg_benchmark -- --markdown

# 3. Export raw JSON telemetry for CI/CD regression tracking
cargo run --release --bin pvg_benchmark -- --json