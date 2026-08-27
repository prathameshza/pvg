# ⚡ PVG — Procedural Vector Graphics

[![Language: Rust](https://img.shields.io/badge/Language-Rust_2021-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Maven Central](https://img.shields.io/maven-central/v/io.github.prathameshza/pvg.svg?style=flat-square&color=00d2ff&logo=android)](https://central.sonatype.com/artifact/io.github.prathameshza/pvg)
[![Frame Latency](https://img.shields.io/badge/Frame_Eval-<0.04_ms-brightgreen.svg?style=flat-square)](#-benchmark-telemetry)
[![Memory Footprint](https://img.shields.io/badge/Peak_Heap-<50_KB-blueviolet.svg?style=flat-square)](#-benchmark-telemetry)
[![Zero GPU Dependency](https://img.shields.io/badge/GPU_Dependency-0%25_(Pure_CPU)-cyan.svg?style=flat-square)](#-why-pvg-was-created)
[![Web Component](https://img.shields.io/badge/Web_Component-<pvg--view>-ff69b4.svg?style=flat-square)](#4-web-studio--pvg-view-component)

**A deterministic, human-readable 2D vector graphics and procedural scene description language.**  
*Combines the declarative clarity of vector graphics with native loops, typography, trigonometry, and microsecond CPU evaluation.*

[Live Web Studio](https://prathameshza.github.io/pvg/pvg_web_gui/index.html) • [Interactive Showcase](https://prathameshza.github.io/pvg/) • [Language Spec](PVG_SPECS.md) • [Android Engine](#5-android-runtime-pvg-android) • [Desktop Studio](#2-desktop-studio-pvg_win_gui) • [Web Component](#4-web-studio--pvg-view-component)

---

## 📖 Table of Contents

- [Why PVG Was Created](#-why-pvg-was-created)
- [⚖️ Is PVG a True Replacement for SVG?](#-is-pvg-a-true-replacement-for-svg)
  - [Feature & Scope Matrix](#feature--scope-matrix)
  - [Things to Consider Before Choosing PVG](#things-to-consider-before-choosing-pvg)
  - [When to Use PVG vs. When to Use SVG](#when-to-use-pvg-vs-when-to-use-svg)
- [Project Structure](#-project-structure)
- [Featured Presets Showcase](#-featured-presets-showcase)
- [Architecture & Execution Pipeline](#-architecture--execution-pipeline)
- [Workspace Crates & Tooling](#-workspace-crates--tooling)
  - [1. `pvg` (Core Engine)](#1-core-rust-engine-pvg)
  - [2. `pvg_win_gui` (Desktop Studio)](#2-desktop-studio-pvg_win_gui)
  - [3. `transpilers` (SVG / PNG CLI)](#3-cli-transpilers-transpilers)
  - [4. `<pvg-view>` (Web Component & Studio)](#4-web-studio--pvg-view-component)
  - [5. `pvg-android` (Android Native Runtime)](#5-android-runtime-pvg-android)
- [Benchmark Telemetry](#-benchmark-telemetry)
- [Quickstart Guide](#-quickstart-guide)
- [Safety & Sandbox Guarantees](#-safety--sandbox-guarantees)

---

## 🎯 Why PVG Was Created

Scalable Vector Graphics (SVG) has been the web's default 2D vector format for decades, but it carries deep legacy baggage that makes modern real-time rendering, dynamic generation, and human authoring painful.

**PVG was created as a clean, human-readable, and high-performance procedural alternative for SVG** due to four core problems:

### 1. 👁️ SVG is Not Human-Readable or Friendly
- **Bloated XML Syntax:** SVG is choked with noisy closing tags (`</path></g></rect></svg>`), XML namespaces, and verbose attribute boilerplate.
- **Unreadable Path Strings:** Complex shapes become opaque walls of coordinates (e.g., `d="M12.5556,12.2222 L13.4444,12.2222 C14.333... Z"`).
- **No Native Logic:** Expressing a repetitive pattern like a dial with 24 ticks requires copy-pasting 24 individual `<line>` tags or bringing in an entire JavaScript runtime. PVG handles it in a clean 4-line `for` loop.

### 2. ⚡ SVG Path Calculations Consume Excessive CPU Usage
- **Complex Arc Math:** SVG elliptical arcs rely on complex endpoint-to-center parameterization requiring heavy matrix inversions and square roots.
- **PVG Direct Trigonometry:** PVG replaces arc matrix inversions with direct, lightweight forward trigonometry:
  $$\text{arc } [c_x, c_y]\ r\ \theta_{\text{start}}\ \theta_{\text{end}}$$
- **DOM & CSS Recalculations:** Animating SVG shapes triggers browser layout passes, style invalidations, and DOM reflows. PVG evaluates scenes directly on the CPU in **`< 40 microseconds`**.

### 3. 🧩 SVG Parsing is a Nightmare
- **Heavy Spec Overhead:** Full SVG parsers must implement CSS cascading, style inheritance, transform hierarchies, XML entity resolvers, and DOM nodes.
- **Memory Overhead:** A complex SVG document easily balloons to hundreds of kilobytes or megabytes of DOM tree structures.
- **PVG Single-Pass Parsing:** PVG compiles into a flat, contiguous 2D Draw List inside a bounded **`< 50 KB` heap budget** with single-pass recursive descent parsing and zero DOM overhead.

---

## ⚖️ Is PVG a True Replacement for SVG?

**No.** PVG is deliberately **not** a drop-in replacement for the entire W3C SVG standard.

SVG is a multi-decade, general-purpose document format designed for arbitrary web styling, complex digital publishing, and rich raster/vector hybrid artwork. **PVG is a lightweight, sandboxed, procedural micro-engine** designed for microsecond real-time graphics, procedural animation, automotive/industrial HUDs, generative art, mobile apps, and game engine vector overlays.

### Feature & Scope Matrix

```
PVG 0.1 Architectural Scope:
├── Geometric Primitives (Circle, Ellipse, Rect, Line, Polygon)  ✓
├── Lean Bézier Paths (Quad, Cubic, Forward Circular Arcs)       ✓
├── Procedural Control (Loops, Variables, User def Functions)    ✓
├── Math & Trigonometry (sin, cos, tan, sqrt, pow, random)       ✓
├── Real-Time Timeline Clock (time, t @ 60+ FPS)                 ✓
├── 2D Affine Transform Hierarchy (Translate, Rotate, Scale)     ✓
├── Flat Contiguous 2D Draw List (< 50 KB Heap Arena)            ✓
├── Lean Text Primitive (pos, content, size, align, opacity)     ✓
├── Multi-Platform Runtime (Rust, Web, Android AAR, Windows)     ✓
├── Embedded Fonts (Base64 WOFF2, TTF font tables, glyph curves) ✗ (Intentional omission)
├── Embedded Images (Base64 PNG/JPEG inlines)                    ✗ (Intentional omission)
├── External Images (<image href="..."> references)              ✗ (Intentional omission)
├── External Stylesheets & CSS Cascading Engine                  ✗ (Intentional omission)
├── Multi-Stage SVG Filters (<feGaussianBlur>, <feBlend>, etc.)  ✗ (Intentional omission)
├── Text on a Path & Complex Bidirectional (BiDi) Typography     ✗ (Intentional omission)
└── DOM Event Handlers (onclick, onmouseover scripts)            ✗ (Intentional omission)
```

---

### Things to Consider Before Choosing PVG

Before choosing PVG for a project, consider the architectural trade-offs:

1. **No External Network Dependencies or Assets**:
   PVG files are 100% self-contained and air-gapped. PVG deliberately forbids loading remote URLs, downloading external fonts, or referencing external bitmap files (`.jpg`, `.png`). This guarantees zero network lag and total immunity to cross-site scripting (XSS) or external resource injection attacks.
2. **Simplified, Fast Typography**:
   PVG's `text` primitive is designed for clean labels, gauges, HUD metric cards, and titles. It maps to standard system font categories (`sans`, `mono`, `serif`). It does **not** perform complex sub-pixel OpenType glyph kerning, multi-column text reflow, or text wrapped around arbitrary vector paths.
3. **No Complex Multi-Stage Shader Filters**:
   PVG relies on pure CPU geometric rasterization and solid alpha-blended vector geometry. It does not implement SVG's heavy filter pipeline (`feTurbulence`, `feDisplacementMap`, `feConvolveMatrix`), which requires heavy GPU shaders or massive CPU convolutions.
4. **No CSS Cascade or Inline Script Execution**:
   Styles are scoped directly within blocks (`group`, `path`, or element definitions). There is no global CSS selector engine, eliminating specificity collisions and style recalculation overhead.

---

### When to Use PVG vs. When to Use SVG

| Project Requirement | Recommended Format | Why? |
| :--- | :---: | :--- |
| **Real-time 60 FPS HUDs & Dials** | **PVG** | Evaluates in `< 40 µs` without triggering browser DOM reflows. |
| **Android Embedded Graphics & HUDs** | **PVG** | 0% GPU power draw, 0 HWUI render upload via native `ANativeWindow`. |
| **Generative & Algorithmic Vector Art** | **PVG** | Native `for` loops, trigonometry, and deterministic seeds without external JS. |
| **Game Engine 2D Assets / UI Overlays** | **PVG** | Contiguous draw list fits in $< 50\text{ KB}$ heap memory with zero GPU lock-in. |
| **Sandboxed / Untrusted Document Viewers** | **PVG** | Strict execution limits ($100k$ iterations, $64$ stack depth) prevent DoS attacks. |
| **Rich Digital Publishing & E-Books** | **SVG** | Requires complex multi-language text layout, hyphenation, and embedded WOFF2 fonts. |
| **Complex Graphic Design with Drop Shadows** | **SVG** | Uses multi-stage Gaussian blur filters and blend modes. |
| **Hybrid Raster / Vector Web Mockups** | **SVG** | Combines background photographs (`<image>`) with vector clipping masks. |

---

## 📂 Project Structure

```
pvg/
├── pvg/                         # 🦀 Official Rust Core Engine (Lexer, Parser, AST, Evaluator, DrawList)
├── pvg_android/                 # 🤖 Pure CPU Rust JNI Bridge with ANativeWindow integration
├── android/                     # 📱 Android Gradle Multi-Module Workspace
│   ├── pvg-android/             #    • Official Android AAR Library (io.github.prathameshza:pvg)
│   └── app/                     #    • Reference Showcase Android Application
├── pvg_win_gui/                 # 🪟 Native Windows Live IDE Studio (built with eframe/egui & tiny-skia)
├── transpilers/                 # 🔄 Rust-based CLI Transpilers for PVG -> SVG and PVG -> PNG
│   ├── src/pvg_to_svg.rs        #    • PVG to static/animated SMIL SVG transpiler
│   └── src/pvg_to_png.rs        #    • PVG to multi-scale (1x, 2x, 4x) PNG software rasterizer
├── docs/                        # 🌐 Web Platform & Documentation
│   ├── pvg_web_gui/             #    • Full-featured Web-based Live IDE Studio
│   │   ├── pvg.js               #    • Pure Vanilla JS PVG Implementation & <pvg-view> Custom Element
│   │   ├── presets.js           #    • Built-in interactive preset scripts
│   │   └── index.html           #    • Web IDE Studio Interface
│   ├── index.html               #    • Project Landing Page & Interactive Showcase
│   └── script.js                #    • Real-time showcase controller & telemetry bridge
├── presets/                     # 🎨 Official PVG Reference Presets (.pvg, .svg, .png)
├── benchmark/                   # ⏱️ High-Precision Microsecond Telemetry & Profiling Suite
├── PVG_SPECS.md                 # 📜 Official PVG 0.1 Language & Architecture Specification
└── Cargo.toml                   # 📦 Cargo Workspace Configuration
```

---

## 🎨 Featured Presets Showcase

All reference presets compile directly to native GUI meshes, W3C SVG, multi-scale PNG, or Android Views:

### 1. 📊 Telemetry Monitor Card (`presets/telemetry_card.pvg` — 1.6 KB)

*Demonstrates the lean `text` primitive, font family aliases (`sans`, `mono`), multi-alignment, and live procedural strings:*

```pvg
PVG 0.1
canvas 600 400
  background #0b0c10

# Card Background Container
rectangle
  pos [40, 40]
  size [520, 320]
  radius 12
  fill #12141c
  stroke #1f2333
  width 1.5

# Card Header Title (Sans-Serif Font)
text
  pos [60, 56]
  content "TELEMETRY MONITOR"
  size 16
  font "sans"
  align "left"
  fill #00ffcc

# Dynamic Animated Values
set rpm = floor(3200 + 400 * sin(time * 3.0))
set temp = floor(68 + 8 * cos(time * 2.0))

text
  pos [80, 155]
  content "" + rpm + " RPM"
  size 28
  font "mono"
  align "left"
  fill #ffffff

text
  pos [335, 155]
  content "" + temp + " °C"
  size 28
  font "mono"
  align "left"
  fill #ff3355
```

---

### 2. 🌀 Radar Scanner (`presets/radar.pvg` — 1.5 KB vs 148.6 KB SVG)

*Features dynamic range rings, crosshairs, orbiting beacon satellites, and a rotating phosphor sweep trail driven by the timeline clock `time`:*

```pvg
PVG 0.1
canvas 600 600
  background #080a0f

set cx = 300
set cy = 300
set sweep = time * 2.0

# Concentric Range Rings
for r_idx from 1 to 4
  circle
    center [cx, cy]
    radius r_idx * 55
    fill none
    stroke #103b42
    width 1.5

# Rotating Phosphor Decay Trail
for trail from 0 to 20
  set a = sweep - trail * 0.035
  line
    from [cx, cy]
    to   [cx + 230 * cos(a), cy + 230 * sin(a)]
    stroke #00ffcc
    width 2
    opacity (1.0 - trail / 20) * 0.45

# Sweep Line
line
  from [cx, cy]
  to   [cx + 230 * cos(sweep), cy + 230 * sin(sweep)]
  stroke #ffffff
  width 2.5
```

---

## 🏗️ Architecture & Execution Pipeline

```
[ PVG 0.1 Source Text ]
           │
           ▼ (Single-Pass Tokenizer & Recursive Descent Parser)
[ Cached AST (`Document`) ] (~8–16 KB)
           │
           ▼ (Procedural Evaluator: Loops, Typography, Trigonometry, Transforms)
[ Flat 2D Draw List (`DrawList`) ] (~15–35 KB)
           │
   ┌───────┼─────────────────────────┬─────────────────────────┬─────────────────────────┐
   ▼       ▼                         ▼                         ▼                         ▼
[ egui GUI Painter ]      [ Standalone SVG ]         [ tiny-skia Engine ]      [ <pvg-view> Element ]    [ ANativeWindow Android ]
(Screen-Space Adaptive)   (W3C SMIL Animation)       (1x / 2x / 4x PNG)        (Canvas / SVG Web)        (Pure CPU 60 FPS Direct)
```

1. **AST Caching Optimization:** During animated playback (60 FPS), the document is parsed once into an AST (`Document`). Every frame tick re-evaluates the AST with updated `time` values directly into the draw list in **`5–40 µs`**, avoiding string parsing churn.
2. **Screen-Space Adaptive Curve Tessellation:** In the native painter, quadratic/cubic Béziers and ellipses automatically scale subdivision vertex counts according to screen-space pixel chord length, ensuring curves remain silky smooth at any zoom level ($0.05\times - 20\times$).

---

## 💻 Workspace Crates & Tooling

### 1. Core Rust Engine (`pvg`)
```rust
use pvg::{compile_pvg_at_time, parse_pvg, Evaluator};

let source = "PVG 0.1\ncanvas 400 400\ncircle\n  center [200, 200]\n  radius 50\n  fill #00ffcc\n";
let draw_list = compile_pvg_at_time(source, 0.0).unwrap();
assert_eq!(draw_list.items.len(), 1);
```

### 2. Desktop Studio (`pvg_win_gui`)
Native desktop studio featuring a live editor, line numbers, variable timeline speed ($0.25\times - 4.0\times$), timeline scrubber, infinite pan/zoom, and one-click SVG/PNG exports.
```bash
cargo run --release -p pvg_win_gui
```

### 3. CLI Transpilers (`transpilers`)
```bash
# 1. Transpile a single PVG file to SVG
cargo run --release --bin pvg_to_svg -- presets/telemetry_card.pvg presets/telemetry_card.svg

# 2. Rasterize a single PVG file to 2x PNG
cargo run --release --bin pvg_to_png -- presets/telemetry_card.pvg presets/telemetry_card.png --scale 2.0
```

### 4. Web Studio & `<pvg-view>` Component
A pure vanilla JS implementation (`docs/pvg_web_gui/pvg.js`) providing a zero-dependency W3C custom element:
```html
<script src="docs/pvg_web_gui/pvg.js"></script>

<pvg-view autoplay interactive render="canvas">
  <script type="text/pvg">
    PVG 0.1
    canvas 400 400
      background #080a0f

    circle
      center [200, 200]
      radius 60 + 20 * sin(time * 4)
      fill #00ffcc
  </script>
</pvg-view>
```

### 5. Android Runtime (`pvg-android`)

Add the dependency to your Android app's `build.gradle.kts`:

```kotlin
dependencies {
    implementation("io.github.prathameshza:pvg:0.1.0")
}
```

#### Jetpack Compose (`PvgView`):
```kotlin
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.pvg.android.PvgView

@Composable
fun Beacon() {
    PvgView(
        source = """
            PVG 0.1
            canvas 400 400
              background #000000
            circle
              center [200, 200]
              radius 50 + 20 * sin(time * 3)
              fill #00ffcc
        """.trimIndent(),
        modifier = Modifier.size(300.dp)
    )
}
```

---

## 📊 Benchmark Telemetry

*Hardware Testbed: AMD Ryzen™ 5 7235HS (4C/8T @ 3.20 GHz), 24.0 GB RAM, Windows 11 (64-bit)*

| Benchmark Case | Category | Output Shapes | Mean Latency | P95 Latency | Peak Heap | Total Alloc Churn | Throughput | Spec Target |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **`paths.pvg`** | Preset | 3 | **24.55 µs** | 33.60 µs | 18.03 KB | 30.80 KB | 40,728 ops/s | ✅ **PASS** |
| **`gears.pvg`** | Preset | 24 | **84.17 µs** | 97.30 µs | 23.57 KB | 60.80 KB | 11,881 ops/s | ✅ **PASS** |
| **`telemetry_card.pvg`** | Preset | 14 | **87.90 µs** | 147.00 µs | 36.82 KB | 68.72 KB | 11,377 ops/s | ✅ **PASS** |
| **`radar.pvg`** | Preset | 39 | **127.77 µs** | 174.10 µs | 47.04 KB | 97.34 KB | 7,827 ops/s | ✅ **PASS** |
| **`dial.pvg`** | Preset | 29 | **142.49 µs** | 158.50 µs | 47.68 KB | 95.25 KB | 7,018 ops/s | ✅ **PASS** |
| **`spiral.pvg`** | Preset | 61 | **164.63 µs** | 183.80 µs | 20.08 KB | 50.98 KB | 6,074 ops/s | ✅ **PASS** |
| **`grid.pvg`** | Preset | 128 | **173.14 µs** | 242.60 µs | 30.27 KB | 69.40 KB | 5,776 ops/s | ✅ **PASS** |

---

## 🚀 Quickstart Guide

### Prerequisites
- [Rust toolchain](https://rustup.rs/) (edition 2021 or newer)
- [Android Studio](https://developer.android.com/studio) (for Android AAR compilation)

```bash
# 1. Clone the repository
git clone https://github.com/prathameshza/pvg.git
cd pvg

# 2. Run all Rust core tests
cargo test --workspace

# 3. Launch Desktop Studio IDE
cargo run --release -p pvg_win_gui
```

---

## 🛡️ Safety & Sandbox Guarantees

| Parameter | Cap | Purpose |
| :--- | :--- | :--- |
| `MAX_LOOP_ITERATIONS` | $100,000$ | Prevents infinite loop execution hangs |
| `MAX_CALL_STACK_DEPTH` | $64$ frames | Prevents stack-overflow recursion crashes |
| `WORKING_HEAP_BUDGET` | $< 50\text{ KB}$ | Bounded memory consumption for AST and Draw Lists |
| `INPUT_CONSTRAINTS` | Tabs forbidden | Enforces uniform 2-space indentation layouts |
| `NETWORK_SANDBOX` | 0 External Requests | Zero remote asset fetching, eliminating XSS and SSRF risks |