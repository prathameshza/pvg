# ⚡ PVG — Procedural Vector Graphics Core Engine

[![Crates.io](https://img.shields.io/crates/v/pvg.svg)](https://crates.io/crates/pvg)
[![Documentation](https://docs.rs/pvg/badge.svg)](https://docs.rs/pvg)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Frame Latency](https://img.shields.io/badge/Frame_Eval-<0.04_ms-brightgreen.svg)](https://github.com/prathameshza/pvg)
[![Memory Footprint](https://img.shields.io/badge/Peak_Heap-<50_KB-blueviolet.svg)](https://github.com/prathameshza/pvg)

A deterministic, human-readable 2D vector graphics and procedural scene description engine.  
Combines the declarative clarity of vector graphics with native loops, typography, trigonometry, and microsecond CPU evaluation.

[Live Web Studio](https://prathameshza.github.io/pvg/pvg_web_gui/index.html) • [Interactive Showcase](https://prathameshza.github.io/pvg/) • [Language Specification](https://github.com/prathameshza/pvg/blob/main/PVG_SPECS.md) • [GitHub Repository](https://github.com/prathameshza/pvg)

---

## ✨ Features

- 🦀 **Zero GPU Dependency**: Pure deterministic CPU evaluation and geometric rasterization.
- ⏱️ **Microsecond CPU Evaluation**: Evaluates animated scenes in `< 40 µs` per frame on a single CPU thread.
- 🪶 **Sub-50 KB Memory Footprint**: Evaluates scenes directly into flat contiguous 2D draw lists with zero DOM overhead.
- 🔄 **Two-Phase AST Caching**: Parse source text once into an Abstract Syntax Tree (`Document`) and re-evaluate per frame across timeline ticks with zero re-parsing churn.
- 🌐 **Built-in Standalone SVG Emitter**: Direct, zero-dependency serialization into static and SMIL-animated W3C SVG XML strings.
- 📐 **Direct Trigonometric Arcs**: Replaces SVG's heavy endpoint-to-center elliptical arc matrix inversions with direct forward trigonometry.
- 🎲 **Deterministic Xorshift RNG**: Integrated 64-bit pseudo-random number generator for 100% reproducible generative art.

---

## 🚀 Quickstart

Add `pvg` to your `Cargo.toml`:

```toml
[dependencies]
pvg = "0.1.0"
```

### Basic Compilation & SVG Export

```rust
use pvg::{compile, compile_at_time};

let source = r#"
PVG 0.1
canvas 400 400
  background #080a0f

circle
  center [200, 200]
  radius 60 + 20 * sin(time * 3.0)
  fill #00ffcc

text
  pos [200, 290]
  content "PULSING CORE"
  size 14
  font "mono"
  align "center"
  fill #ffffff
"#;

// 1. Compile at timestamp t = 0.0s
let draw_list = compile(source).unwrap();
assert_eq!(draw_list.items.len(), 2);

// 2. Export directly to standalone W3C SVG XML
let svg_xml = draw_list.to_svg();
println!("{}", svg_xml);

// 3. Animate at timestamp t = 1.25s
let animated_list = compile_at_time(source, 1.25).unwrap();
```

---

## ⚡ AST Caching for 60 FPS Real-time Loops

For high-performance 60 FPS animation loops, parse the AST once and re-evaluate only the cached AST per frame tick in **`< 40 µs`**:

```rust
use pvg::{parse, Evaluator};

let source = "PVG 0.1\ncanvas 200 200\ncircle\n  center [100, 100]\n  radius 40\n";

// Phase 1: Parse string to AST once
let doc = parse(source).unwrap();

// Phase 2: 60 FPS Re-evaluation loop (Microseconds per frame)
for frame in 0..60 {
    let time = frame as f64 / 60.0;
    let evaluator = Evaluator::new_with_time(time);
    let draw_list = evaluator.evaluate_document(&doc).unwrap();
    assert_eq!(draw_list.items.len(), 1);
}
```

---

## 🛡️ Safety & Sandbox Guarantees

| Guarantee | Mechanism | Purpose |
| :--- | :--- | :--- |
| **No Remote Network Requests** | Air-gapped engine | Zero remote asset fetching, eliminating XSS and SSRF risks. |
| **Loop Execution Bounds** | Default $100,000$ iterations | Prevents infinite `while` loop hangs and DoS crashes. |
| **Call Stack Limits** | Max $64$ activation frames | Prevents stack-overflow recursion panics. |
| **Bounded Heap Arena** | Contiguous draw commands | Guaranteed scene evaluation inside $< 50\text{ KB}$ working memory. |
| **Input Consistency** | Mandatory 2-space indentation | Tabs are forbidden at parse time to eliminate cross-platform layout ambiguity. |

---

## 📄 License

Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at

[http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)

Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.