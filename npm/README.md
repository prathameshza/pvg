# ⚡ `pvg` NPM Package — Overview & Complete Usage Guide

The **`pvg`** npm package is the official, zero-dependency, isomorphic TypeScript & JavaScript engine for **Procedural Vector Graphics (PVG)**.

It brings the deterministic procedural graphics capabilities of the Rust engine directly to the JavaScript ecosystem—running seamlessly across **Node.js, Deno, Bun, React, Vue, Svelte, Next.js, Nuxt, and Vanilla HTML**.

---

## 🎯 What is the `pvg` Package?

Traditional vector graphics formats like SVG are strictly declarative documents. To animate them or generate repetitive structures (e.g., radar rings, clock ticks, HUD meters, charts), developers must either bloat the SVG with duplicate XML tags or attach heavy JavaScript libraries that trigger browser DOM reflows and CSS recalculations.

**`pvg` solves this with a procedural-native approach:**
- Write 2D vector graphics with native `for` loops, variables, trigonometry (`sin`, `cos`), and timeline clocks (`time`).
- Evaluates documents inside a **`< 50 KB` heap budget** with per-frame evaluation latencies of **`5–40 µs`**.
- Provides multiple output pipelines: **HTML5 `<canvas>` (60+ FPS)**, **W3C Static SVG**, **W3C SMIL-Animated SVG**, and the drop-in **`<pvg-view>` W3C Web Component**.

---

## 🏗️ Architecture & Execution Pipeline

```
[ PVG 0.1 Source Code ]
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
| **`parse`** | `(source: string) => Document` | Tokenizes and parses PVG source text into an Abstract Syntax Tree (`Document`). |
| **`evaluate`** | `(doc: Document, time?: number) => DrawList` | Evaluates a pre-parsed AST at a specific timestamp ($t$). Skips re-parsing for 60 FPS animation loops. |
| **`compile`** | `(source: string, time?: number) => DrawList` | Convenience single-pass function that parses and evaluates source text at timestamp ($t$). |
| **`toSvg`** | `(input: string \| DrawList, time?: number) => string` | Serializes a PVG string or evaluated `DrawList` into standard W3C SVG XML. |
| **`toAnimatedSvg`** | `(source: string, options?: AnimatedSvgOptions) => string` | Compiles an animated PVG document into a standalone W3C SVG with SMIL animation tags. |
| **`renderToCanvas`** | `(ctx: CanvasRenderingContext2D, dl: DrawList, opts?: RenderCanvasOptions) => void` | Rasterizes a `DrawList` directly onto an HTML5 2D Canvas context. |
| **`PvgView`** | `class PvgView extends HTMLElement` | The `<pvg-view>` W3C Custom Element class. |
| **`registerPvgView`** | `(tagName?: string) => void` | Manually registers the `<pvg-view>` custom element in the DOM (auto-registered in browsers). |
| **`PvgColor`** | `class PvgColor` | 32-bit RGBA color representation with hex parsers (`PvgColor.fromHex("#00ffcc")`). |
| **`Transform2D`** | `class Transform2D` | $2 \times 3$ Affine matrix transformation primitive. |

---

## 💻 Practical Usage Patterns

### 1. Node.js / Server-Side Rendering (SSR) & SVG Export

Generate static SVGs on your server or in build scripts without headless browsers:

```typescript
import { toSvg, compile } from "pvg";
import fs from "node:fs";

const pvgSource = `
PVG 0.1
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
import { parse, evaluate, renderToCanvas } from "pvg";

const canvas = document.querySelector<HTMLCanvasElement>("#viewport")!;
const ctx = canvas.getContext("2d")!;

const pvgSource = `
PVG 0.1
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
import { toAnimatedSvg } from "pvg";
import fs from "node:fs";

const pulsingRadar = `
PVG 0.1
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
  import "pvg";
</script>

<!-- 2. Use the Custom Element -->
<pvg-view autoplay interactive render="canvas" style="width: 400px; height: 400px;">
  <script type="text/pvg">
    PVG 0.1
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

Wrap `pvg` in a lightweight, reusable React component:

```tsx
import React, { useEffect, useRef } from "react";
import { parse, evaluate, renderToCanvas } from "pvg";

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

## 📦 Package Distribution Structure

When consumers install `pvg` from npm, the package exports:

```
node_modules/pvg/
├── dist/
│   ├── index.js          # ESM entry point (import { parse } from 'pvg')
│   ├── index.cjs         # CommonJS entry point (const { parse } = require('pvg'))
│   ├── index.d.ts        # TypeScript declarations
│   ├── index.global.js   # Browser bundle for CDNs (window.PVG)
│   ├── component.js      # Isolated <pvg-view> submodule
│   └── component.d.ts    # Web Component TypeScript declarations
├── package.json
└── README.md
```

This ensures compatibility across all major bundlers (**Vite, Webpack, Rollup, esbuild, Turbopack**) and runtime environments.