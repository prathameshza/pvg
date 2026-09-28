# ⚡ `pvgview` NPM Package (PVG 0.2) — Overview & Complete Usage Guide

The **`pvgview`** npm package (v0.2.0) is the official, zero-dependency, isomorphic TypeScript & JavaScript engine for **Procedural Vector Graphics (PVG 0.2)** — full parity with the Rust `pvg` 0.2.0 core.

> 📜 **Language specification:** everything below is defined normatively in [`PVG_SPECS.md`](https://github.com/prathameshza/pvg/blob/main/PVG_SPECS.md) — start there to learn the language (Section 2 grammar, Sections 6-13 shapes & rendering, Section 18 host uniforms/noise/sprites/patterns/splines).

It brings the deterministic procedural graphics capabilities of the Rust engine directly to the JavaScript ecosystem—running seamlessly across **Node.js, Deno, Bun, React, Vue, Svelte, Next.js, Nuxt, and Vanilla HTML**.

---

## 🎯 What is the `pvgview` Package?

Traditional vector graphics formats like SVG are strictly declarative documents. To animate them or generate repetitive structures (e.g., radar rings, clock ticks, HUD meters, charts), developers must either bloat the SVG with duplicate XML tags or attach heavy JavaScript libraries that trigger browser DOM reflows and CSS recalculations.

**`pvgview` solves this with a procedural-native approach:**
- Write 2D vector graphics with native `for` loops, variables, trigonometry (`sin`, `cos`), and timeline clocks (`time`).
- Evaluates documents inside a **`< 50 KB` heap budget** with per-frame evaluation latencies of **`5–40 µs`**.
- Provides multiple output pipelines: **HTML5 `<canvas>` (60+ FPS)**, **W3C Static SVG**, **W3C SMIL-Animated SVG**, and the drop-in **`<pvg-view>` W3C Web Component**.
- **PVG 0.2 parity:** stroke topology (`cap`/`join`/`miter`/`dash`/`align`), gradient paints (`linear`/`radial`/`angular` + `stop`), `blur`/`shadow`/`glow`, `clip` masks, `blend` modes, functional `rgb()`/`rgba()` colors — plus host uniforms (`param`/`PvgScene`), deterministic `noise2d`/`noise3d`, pixel `sprite`s (`snap`/`filter`), `pattern` tiles, `spline`s (`array`/`len`/`get`), and `for`/`while`/`if` inside `path` bodies. Accepts both `PVG 0.1` and `PVG 0.2` headers (0.1 docs evaluate identically). Safety caps enforced per evaluation: 100k loop iterations, 64 call-stack frames, 50k draw commands.

---

## 🏗️ Architecture & Execution Pipeline

```
[ PVG 0.2 Source Code ]
           │
           ▼ (Lexer & Parser)
[ Cached Document AST ] (~8–16 KB) ───► Stored in memory once
           │
           ▼ (Evaluator at time t @ 60 FPS) ───► Runs in 5–30 µs
[ Flat 2D Draw List (`DrawList`) ] (~15–35 KB)
           │
   ┌───────┼──────────────────────────┬──────────────────────────┐
   ▼                                  ▼                          ▼
[ HTML5 2D Canvas ]          [ Static W3C SVG ]         [ SMIL Animated SVG ]
(High-DPI Retina Scaled)     (`toSvg()`)                (`toAnimatedSvg()`)
```

---

## 📊 Complete API Reference

| Export | Signature | Description |
| :--- | :--- | :--- |
| **`parse`** | `(source: string) => Document` | Tokenizes and parses PVG 0.2 source text into an Abstract Syntax Tree (`Document`). Accepts `PVG 0.1` and `PVG 0.2` headers. |
| **`evaluate`** | `(doc: Document, time?: number) => DrawList` | Evaluates a pre-parsed AST at a specific timestamp ($t$). Skips re-parsing for 60 FPS animation loops. |
| **`compile`** | `(source: string, time?: number, params?: Record<string, Value>) => DrawList` | Convenience single-pass function that parses and evaluates source text at timestamp ($t$), with optional host `param` overrides. |
| **`compileWithParams`** | `(source: string, params: Record<string, Value>, time?: number) => DrawList` | Compiles with host uniforms (`param`, Section 18.1) overridden — mirrors `pvg::compile_with_params`. |
| **`PvgScene`** | `PvgScene.fromSource(src) / .fromDocument(doc)` | Host-driven scene handle: parse once, `setParam`/`clearParam`/`setTime` per frame, `evaluate()`. Mirrors `pvg::Scene`. |
| **`toSvg`** | `(input: string \| DrawList, time?: number) => string` | Serializes a PVG string or evaluated `DrawList` into standard W3C SVG XML (gradients, filters, clips, patterns, sprites, splines). |
| **`toAnimatedSvg`** | `(source: string, options?: AnimatedSvgOptions) => string` | Compiles an animated PVG document into a standalone W3C SVG with SMIL animation tags. |
| **`renderToCanvas`** | `(ctx: CanvasRenderingContext2D, dl: DrawList, opts?: RenderCanvasOptions) => void` | Rasterizes a `DrawList` directly onto an HTML5 2D Canvas context. |
| **`PvgView`** | `class PvgView extends HTMLElement` | The `<pvg-view>` W3C Custom Element class. |
| **`registerPvgView`** | `(tagName?: string) => void` | Manually registers the `<pvg-view>` custom element in the DOM (auto-registered in browsers). |
| **`PvgColor`** | `class PvgColor` | 32-bit RGBA color representation with hex parsers (`PvgColor.fromHex("#00ffcc")`). |
| **`Transform2D`** | `class Transform2D` | $2 \times 3$ Affine matrix transformation primitive. |

---

## 📜 Language Specification

The authoritative definition of the PVG language lives in **[`PVG_SPECS.md`](https://github.com/prathameshza/pvg/blob/main/PVG_SPECS.md)** (repo root). Use it as the reference for anything this guide summarizes:

- **Sections 2-4** — headers (`PVG 0.1` / `PVG 0.2`), indentation, strings, colors (`#rgb`, keywords, `rgb()`/`rgba()`), expression precedence, built-ins (`sin`, `random`, `noise2d`, `array`/`len`/`get`, …)
- **Sections 6-7** — shapes (`circle`, `ellipse`, `rectangle`, `line`, `polygon`, `text`, `path`, `spline`, `sprite`) and the lean path commands
- **Sections 8-12** — 0.2 rendering: stroke topology, gradients, `blur`/`shadow`/`glow`, `clip`, `blend`
- **Sections 13-14** — `group` transforms, `time`/`t` animation clock
- **Section 15** — safety caps enforced by this package (100k loop iterations, 64 call-stack frames, 50k draw commands)
- **Section 18** — host uniforms (`param`), patterns, pixel mode, arrays & splines, control flow inside `path`

> This package tracks the spec exactly: every valid 0.1 document evaluates identically under 0.2, and `PVG 0.1` headers remain accepted (see the back-compatibility test in `test/features_02.test.js`).

---

## 💻 Practical Usage Patterns

### 1. Node.js / Server-Side Rendering (SSR) & SVG Export

Generate static SVGs on your server or in build scripts without headless browsers:

```typescript
import { toSvg, compile } from "pvgview";
import fs from "node:fs";

const pvgSource = `
PVG 0.2
canvas 500 500
  background #0b0c10

# Concentric Range Rings
for r from 1 to 4
  circle
    center [250, 250]
    radius r * 50
    fill none
    stroke #103b42
    width 1.5

# Central Core
circle
  center [250, 250]
  radius 10
  fill #00ffcc
`;

// 1. Direct SVG generation
const svgXml = toSvg(pvgSource);
fs.writeFileSync("radar.svg", svgXml, "utf-8");

// 2. Inspect evaluated geometric primitives
const drawList = compile(pvgSource);
console.log(`Canvas: ${drawList.canvasWidth}x${drawList.canvasHeight}`);
console.log(`Rendered Shapes: ${drawList.items.length}`);
```

---

### 2. High-Performance 60 FPS Canvas (Two-Phase AST Caching)

In browser applications, avoid string re-parsing churn by compiling the AST once and evaluating only the AST on every frame tick:

```typescript
import { parse, evaluate, renderToCanvas } from "pvgview";

const canvas = document.querySelector<HTMLCanvasElement>("#viewport")!;
const ctx = canvas.getContext("2d")!;

const pvgSource = `
PVG 0.2
canvas 400 400
  background #080a0f

set cx = 200
set cy = 200
set pulse = 50 + 20 * sin(time * 4.0)

circle
  center [cx, cy]
  radius pulse
  fill #ff0055
  stroke #ffffff
  width 2.0

circle
  center [cx + 100 * cos(time * 2.0), cy + 100 * sin(time * 2.0)]
  radius 10
  fill #00ffcc
`;

// Phase 1: Parse string to AST once
const ast = parse(pvgSource);

// Phase 2: Microsecond evaluation loop (~15–30 µs per frame)
function renderFrame(timeMs: number) {
  const t = timeMs / 1000.0;
  
  // Evaluate AST at timestamp t
  const drawList = evaluate(ast, t);

  // High-DPI Retina Canvas Scaling
  const dpr = window.devicePixelRatio || 1;
  const targetW = canvas.clientWidth * dpr;
  const targetH = canvas.clientHeight * dpr;

  if (canvas.width !== targetW || canvas.height !== targetH) {
    canvas.width = targetW;
    canvas.height = targetH;
  }

  // Calculate proportional zoom & letterbox alignment
  const zoom = Math.min(targetW / drawList.canvasWidth, targetH / drawList.canvasHeight);
  const originX = (targetW - drawList.canvasWidth * zoom) / 2;
  const originY = (targetH - drawList.canvasHeight * zoom) / 2;

  // In-place Canvas render
  renderToCanvas(ctx, drawList, { originX, originY, zoom });

  requestAnimationFrame(renderFrame);
}

requestAnimationFrame(renderFrame);
```

---

### 3. Generating SMIL-Animated SVGs

Create self-contained animated vector files that loop inside standard `<img>` tags without JavaScript:

```typescript
import { toAnimatedSvg } from "pvgview";
import fs from "node:fs";

const pulsingRadar = `
PVG 0.2
canvas 400 400
  background #000000

set sweep = time * 3.14159

line
  from [200, 200]
  to   [200 + 150 * cos(sweep), 200 + 150 * sin(sweep)]
  stroke #00ffcc
  width 2.0
`;

// Exports SVG with W3C <animate> SMIL keyframe tags
const animatedSvg = toAnimatedSvg(pulsingRadar, {
  duration: 2.0, // 2-second loop
  fps: 30        // 30 frames per second
});

fs.writeFileSync("radar_animated.svg", animatedSvg, "utf-8");
```

---

### 4. `<pvg-view>` Drop-In Web Component

The `<pvg-view>` custom element allows zero-boilerplate vector rendering in plain HTML, Markdown, or web apps:

```html
<!-- 1. Import PVG -->
<script type="module">
  import "pvgview";
</script>

<!-- 2. Use the Custom Element -->
<pvg-view autoplay interactive render="canvas" style="width: 400px; height: 400px;">
  <script type="text/pvg">
    PVG 0.2
    canvas 400 400
      background #080a0f

    circle
      center [200, 200]
      radius 60 + 20 * sin(time * 3.0)
      fill #00ffcc
  </script>
</pvg-view>
```

#### `<pvg-view>` Attributes:
- `autoplay`: Automatically runs the 60 FPS timeline loop.
- `interactive`: Enables mouse dragging (pan) and scroll-wheel (zoom).
- `render="canvas"` / `render="svg"`: Selects GPU-accelerated Canvas or DOM SVG backend.
- `code="..."`: Dynamically binds a PVG code string.
- `src="path/to/file.pvg"`: Fetches and executes remote `.pvg` files.
- `fps="60"`: Caps animation frame rate.
- `time="1.25"`: Manually scrubs the timeline clock.

---

### 5. React Integration Pattern

Wrap `pvgview` in a lightweight, reusable React component:

```tsx
import React, { useEffect, useRef } from "react";
import { parse, evaluate, renderToCanvas } from "pvgview";

interface PvgCanvasProps {
  code: string;
  className?: string;
  style?: React.CSSProperties;
}

export const PvgCanvas: React.FC<PvgCanvasProps> = ({ code, className, style }) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let animId: number;
    let ast: ReturnType<typeof parse> | null = null;

    try {
      ast = parse(code);
    } catch (e) {
      console.error("PVG Syntax Error:", e);
      return;
    }

    const t0 = performance.now();

    const loop = (now: number) => {
      if (!ast) return;
      const t = (now - t0) / 1000.0;
      const drawList = evaluate(ast, t);

      const dpr = window.devicePixelRatio || 1;
      const w = canvas.clientWidth * dpr;
      const h = canvas.clientHeight * dpr;

      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }

      const zoom = Math.min(w / drawList.canvasWidth, h / drawList.canvasHeight);
      const originX = (w - drawList.canvasWidth * zoom) / 2;
      const originY = (h - drawList.canvasHeight * zoom) / 2;

      renderToCanvas(ctx, drawList, { originX, originY, zoom });
      animId = requestAnimationFrame(loop);
    };

    animId = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(animId);
  }, [code]);

  return <canvas ref={canvasRef} className={className} style={{ width: "100%", height: "100%", ...style }} />;
};
```

---

### 6. PVG 0.2 Features (gradients, clip, host params)

```typescript
import { compile, PvgScene } from "pvgview";

const src = `
PVG 0.2
canvas 512 512
  background #07090e

param hull_hp: 0.65

rectangle
  pos [106, 106]
  size [300, 300]
  radius 36
  fill linear [106, 106] [406, 406]
    stop 0.0 #2b3040
    stop 1.0 #0e1017
  shadow [0, 18] 24 #000000bb

clip
  circle
    center [256, 256]
    radius 105
  circle
    center [256, 256]
    radius 90
    fill radial [256, 256] 90
      stop 0.0 #00ffff
      stop 1.0 #07090e
`;

// One-shot with overrides:
const dl = compile(src, 1.0, { hull_hp: 0.2 });

// Or host-driven (parse once, evaluate per frame):
const scene = PvgScene.fromSource(src);
scene.setParam("hull_hp", 0.2);
scene.setTime(1 / 60);
const frame = scene.evaluate();
```

> Back-compat: `PVG 0.1` headers are still accepted and evaluate identically (new style props default to `butt`/`miter`/solid/`normal`, no filters, no clipping).

---

## 📦 Package Distribution Structure

When consumers install `pvgview` from npm (`npm install pvgview`), the package exports:

```
node_modules/pvgview/
├── dist/
│   ├── index.js          # ESM entry point (import { parse } from 'pvgview')
│   ├── index.cjs         # CommonJS entry point (const { parse } = require('pvgview'))
│   ├── index.d.ts        # TypeScript declarations
│   ├── index.global.js   # Browser bundle for CDNs (window.PVG)
│   ├── component.js      # Isolated <pvg-view> submodule
│   └── component.d.ts    # Web Component TypeScript declarations
├── package.json
└── README.md
```

This ensures compatibility across all major bundlers (**Vite, Webpack, Rollup, esbuild, Turbopack**) and runtime environments.