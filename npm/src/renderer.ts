import type {
  AnimatedSvgOptions,
  DrawCmd,
  DrawList,
  DrawPathCommand,
  DrawStyle,
  Glow,
  Paint,
  RenderCanvasOptions,
  Shadow,
  Vec2,
} from "./types.js";
import { Lexer } from "./lexer.js";
import { Parser } from "./parser.js";
import { Evaluator, splineToBezier } from "./evaluator.js";

export function escapeXml(s: string): string {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

export function detectLoopDuration(source: string): number {
  if (!source) return 2.0;
  const match = source.match(/time\s*%\s*([0-9]+(?:\.[0-9]+)?)/);
  if (match && parseFloat(match[1]) > 0) {
    return parseFloat(match[1]);
  }
  return 2.0;
}

/** Accumulator for SVG `<defs>` (gradients, filters, clip paths) collected during emission. */
export interface SvgRenderCtx {
  gradIds: Map<string, string>;
  gradOrder: { id: string; paint: Paint }[];
  gradCounter: number;
  filterIds: Map<string, string>;
  filterOrder: { id: string; blur: number; shadow: Shadow | null; glow: Glow | null }[];
  filterCounter: number;
  clipCounter: number;
  clipDefs: { id: string; mask: DrawCmd }[];
}

export function newSvgCtx(): SvgRenderCtx {
  return {
    gradIds: new Map(),
    gradOrder: [],
    gradCounter: 0,
    filterIds: new Map(),
    filterOrder: [],
    filterCounter: 0,
    clipCounter: 0,
    clipDefs: [],
  };
}

function paintIsNone(paint: Paint): boolean {
  return paint.kind === "color" && paint.color.isNone;
}

// ---- PVG 0.2 canvas helpers ----

/** Palette character -> index (digits, then a-z/A-Z for 10+). `.`/space = skip. */
function spriteCharIndex(ch: string): number | null {
  if (ch === "." || ch === " ") return null;
  if (ch >= "0" && ch <= "9") return ch.charCodeAt(0) - 0x30;
  const lower = ch.toLowerCase();
  if (lower >= "a" && lower <= "z") return lower.charCodeAt(0) - 0x61 + 10;
  return null;
}

function paintToCanvas(
  ctx: CanvasRenderingContext2D,
  paint: Paint,
  opacity: number,
  patterns?: Map<string, CanvasPattern>
): string | CanvasGradient | CanvasPattern {
  if (paint.kind === "color") return paint.color.toRgbaString(opacity);
  if (paint.kind === "pattern") {
    const pat = patterns?.get(paint.name);
    if (pat) return pat;
    // Missing/unknown tile resolves to neutral gray (matches native fallbacks).
    const a = Math.max(0, Math.min(1, opacity));
    return `rgba(136, 136, 136, ${a.toFixed(3)})`;
  }
  const stops = paint.stops || [];
  if (stops.length === 0) return "rgba(0,0,0,0)";
  if (stops.length === 1) return stops[0].color.toRgbaString(opacity);
  const ctxAny = ctx as unknown as {
    createConicGradient?: (startAngle: number, x: number, y: number) => CanvasGradient;
  };
  let grad: CanvasGradient | null = null;
  if (paint.kind === "linear") {
    grad = ctx.createLinearGradient(paint.start[0], paint.start[1], paint.end[0], paint.end[1]);
  } else if (paint.kind === "radial") {
    const r = Math.max(0, paint.radius);
    const f = paint.focal || paint.center;
    try {
      grad = ctx.createRadialGradient(f[0], f[1], 0, paint.center[0], paint.center[1], Math.max(r, 0.001));
    } catch {
      grad = null;
    }
  } else if (paint.kind === "angular") {
    if (typeof ctxAny.createConicGradient === "function") {
      try {
        grad = ctxAny.createConicGradient(paint.startAngle, paint.center[0], paint.center[1]);
      } catch {
        grad = null;
      }
    }
  }
  if (!grad) {
    // Fallback: mid stop solid (matches native raster fallback intent)
    return stops[Math.floor(stops.length / 2)].color.toRgbaString(opacity);
  }
  for (const s of stops) {
    grad.addColorStop(Math.max(0, Math.min(1, s.offset)), s.color.toRgbaString(opacity));
  }
  return grad;
}

function blendToComposite(blend: DrawStyle["blend"]): GlobalCompositeOperation {
  switch (blend) {
    case "add": return "lighter";
    case "multiply": return "multiply";
    case "screen": return "screen";
    case "overlay": return "overlay";
    default: return "source-over";
  }
}

function applyCanvasStyle(
  ctx: CanvasRenderingContext2D,
  style: DrawStyle,
  patterns?: Map<string, CanvasPattern>
): void {
  ctx.fillStyle = paintToCanvas(ctx, style.fill, style.opacity, patterns);
  ctx.strokeStyle = paintToCanvas(ctx, style.stroke, style.opacity, patterns);
  ctx.lineWidth = style.width;
  ctx.lineCap = style.cap === "round" ? "round" : style.cap === "square" ? "square" : "butt";
  ctx.lineJoin = style.join === "round" ? "round" : style.join === "bevel" ? "bevel" : "miter";
  ctx.miterLimit = Math.max(1, style.miter || 4);
  try {
    ctx.setLineDash(style.dash && style.dash.length > 0 ? style.dash : []);
  } catch {
    /* older canvas */
  }
  ctx.globalCompositeOperation = blendToComposite(style.blend);
  // FX: shadow under shape (SourceOver semantics); glow additive under shape.
  if (style.shadow) {
    ctx.shadowOffsetX = style.shadow.offset[0];
    ctx.shadowOffsetY = style.shadow.offset[1];
    ctx.shadowBlur = Math.max(0, style.shadow.radius / 2);
    ctx.shadowColor = style.shadow.color.toRgbaString(1);
  } else {
    ctx.shadowOffsetX = 0;
    ctx.shadowOffsetY = 0;
    ctx.shadowBlur = 0;
    ctx.shadowColor = "rgba(0,0,0,0)";
  }
}

function traceCmdPath(ctx: CanvasRenderingContext2D, cmd: DrawCmd): boolean {
  switch (cmd.type) {
    case "Circle":
      ctx.beginPath();
      ctx.arc(cmd.center[0], cmd.center[1], Math.max(0, cmd.radius), 0, Math.PI * 2);
      break;
    case "Ellipse":
      ctx.beginPath();
      ctx.ellipse(cmd.center[0], cmd.center[1], Math.abs(cmd.radius[0]), Math.abs(cmd.radius[1]), 0, 0, Math.PI * 2);
      break;
    case "Rectangle": {
      const [x, y] = cmd.pos;
      const [w, h] = cmd.size;
      const r = Math.max(0, Math.min(cmd.cornerRadius, w / 2, h / 2));
      ctx.beginPath();
      if (r > 0) {
        const ctxAny = ctx as unknown as {
          roundRect?: (x: number, y: number, w: number, h: number, r: number) => void;
        };
        if (typeof ctxAny.roundRect === "function") ctxAny.roundRect(x, y, w, h, r);
        else {
          ctx.moveTo(x + r, y);
          ctx.lineTo(x + w - r, y);
          ctx.quadraticCurveTo(x + w, y, x + w, y + r);
          ctx.lineTo(x + w, y + h - r);
          ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
          ctx.lineTo(x + r, y + h);
          ctx.quadraticCurveTo(x, y + h, x, y + h - r);
          ctx.lineTo(x, y + r);
          ctx.quadraticCurveTo(x, y, x + r, y);
          ctx.closePath();
        }
      } else ctx.rect(x, y, w, h);
      break;
    }
    case "Line":
      ctx.beginPath();
      ctx.moveTo(cmd.from[0], cmd.from[1]);
      ctx.lineTo(cmd.to[0], cmd.to[1]);
      break;
    case "Polygon":
      ctx.beginPath();
      if (cmd.points.length > 0) {
        ctx.moveTo(cmd.points[0][0], cmd.points[0][1]);
        for (let i = 1; i < cmd.points.length; i++) ctx.lineTo(cmd.points[i][0], cmd.points[i][1]);
        ctx.closePath();
      }
      break;
    case "Path":
      ctx.beginPath();
      for (const pCmd of cmd.commands) {
        switch (pCmd.cmd) {
          case "Start": ctx.moveTo(pCmd.pt[0], pCmd.pt[1]); break;
          case "Line": ctx.lineTo(pCmd.pt[0], pCmd.pt[1]); break;
          case "Quad": ctx.quadraticCurveTo(pCmd.cp[0], pCmd.cp[1], pCmd.ep[0], pCmd.ep[1]); break;
          case "Curve": ctx.bezierCurveTo(pCmd.c1[0], pCmd.c1[1], pCmd.c2[0], pCmd.c2[1], pCmd.ep[0], pCmd.ep[1]); break;
          case "Arc": {
            const delta = pCmd.endAngle - pCmd.startAngle;
            ctx.arc(pCmd.center[0], pCmd.center[1], Math.max(0, pCmd.radius), pCmd.startAngle, pCmd.endAngle, delta < 0);
            break;
          }
          case "Close": ctx.closePath(); break;
        }
      }
      break;
    case "Text":
      return false; // text uses fillText/strokeText
    case "Sprite":
      return false; // sprites draw crisp rects (no path)
    case "Spline": {
      if (cmd.points.length === 0) return false;
      ctx.beginPath();
      const pts = cmd.points;
      ctx.moveTo(pts[0][0], pts[0][1]);
      if (pts.length === 2) {
        ctx.lineTo(pts[1][0], pts[1][1]);
      } else {
        for (const [c1, c2, ep] of splineToBezier(pts)) {
          ctx.bezierCurveTo(c1[0], c1[1], c2[0], c2[1], ep[0], ep[1]);
        }
      }
      break;
    }
    default:
      return false;
  }
  return true;
}

function paintCmdFillStroke(ctx: CanvasRenderingContext2D, cmd: DrawCmd, style: DrawStyle): void {
  const hasFill = !paintIsNone(style.fill);
  const hasStroke = !paintIsNone(style.stroke) && style.width > 0;
  if (cmd.type === "Line") {
    if (hasStroke) ctx.stroke();
    return;
  }
  if (hasFill) {
    if (style.strokeAlign === "inside" || style.strokeAlign === "outside") {
      ctx.save();
      ctx.fill();
      ctx.restore();
    } else ctx.fill();
  }
  if (hasStroke) {
    if (style.strokeAlign === "inside") {
      ctx.save();
      ctx.clip();
      ctx.lineWidth = style.width * 2;
      ctx.stroke();
      ctx.restore();
    } else if (style.strokeAlign === "outside") {
      ctx.save();
      // Outside: stroke full then punch interior via destination-out on copy is
      // expensive; approximate by double-width stroke clipped to outside using
      // evenodd trick is non-trivial — fall back to center with double width
      // note: visually wider; full fidelity needs layer compositing.
      ctx.stroke();
      ctx.restore();
    } else ctx.stroke();
  }
}

function drawSingleCmd(
  ctx: CanvasRenderingContext2D,
  cmd: DrawCmd,
  patterns?: Map<string, CanvasPattern>
): void {
  if (cmd.type === "Clip") {
    // Mask path is never drawn itself; content is composited through the intersection.
    ctx.save();
    if (traceCmdPath(ctx, cmd.mask)) ctx.clip();
    for (const c of cmd.content) drawSingleCmd(ctx, c, patterns);
    ctx.restore();
    return;
  }
  const style = cmd.style;
  ctx.save();
  applyCanvasStyle(ctx, style, patterns);
  const glow = style.glow;
  const blur = style.blur || 0;
  // Glow: blurred silhouette additively underneath (§10.1 order shadow→glow→shape).
  if (glow && glow.radius > 0 && cmd.type !== "Text" && cmd.type !== "Sprite") {
    ctx.save();
    ctx.globalCompositeOperation = "lighter";
    ctx.shadowColor = glow.color.toRgbaString(1);
    ctx.shadowBlur = Math.max(0, glow.radius);
    ctx.shadowOffsetX = 0;
    ctx.shadowOffsetY = 0;
    if (traceCmdPath(ctx, cmd)) {
      if (!paintIsNone(style.fill)) ctx.fill();
      if (!paintIsNone(style.stroke) && style.width > 0) ctx.stroke();
    }
    ctx.restore();
    // Clear shadow so the crisp shape on top isn't double-shadowed
    ctx.shadowBlur = 0;
    ctx.shadowColor = "rgba(0,0,0,0)";
    if (style.shadow) {
      ctx.shadowOffsetX = style.shadow.offset[0];
      ctx.shadowOffsetY = style.shadow.offset[1];
      ctx.shadowBlur = Math.max(0, style.shadow.radius / 2);
      ctx.shadowColor = style.shadow.color.toRgbaString(1);
    }
  }
  const doDraw = (): void => {
    switch (cmd.type) {
      case "Circle":
      case "Ellipse":
      case "Rectangle":
      case "Line":
      case "Polygon":
      case "Path":
      case "Spline":
        traceCmdPath(ctx, cmd);
        paintCmdFillStroke(ctx, cmd, style);
        break;
      case "Sprite":
        drawSpriteCmd(ctx, cmd, style);
        break;
      case "Text":
        drawTextCmd(ctx, cmd, style);
        break;
    }
  };
  if (blur > 0) {
    try {
      ctx.filter = `blur(${blur.toFixed(2)}px)`;
    } catch {
      /* older canvas */
    }
    doDraw();
    try {
      ctx.filter = "none";
    } catch {
      /* older canvas */
    }
  } else {
    doDraw();
  }
  ctx.restore();
}

function drawTextCmd(
  ctx: CanvasRenderingContext2D,
  cmd: Extract<DrawCmd, { type: "Text" }>,
  style: DrawStyle
): void {
  const [x, y] = cmd.pos;
  let fontFam = cmd.fontFamily || "sans-serif";
  const fLower = String(fontFam).toLowerCase();
  if (fLower === "mono" || fLower === "monospace" || fLower === "code") {
    fontFam = '"Fira Code", "JetBrains Mono", Consolas, monospace';
  } else if (fLower === "sans" || fLower === "sans-serif") {
    fontFam = 'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
  } else if (fLower === "serif") {
    fontFam = 'Georgia, "Times New Roman", serif';
  }
  ctx.font = `${cmd.size}px ${fontFam}`;
  ctx.textAlign = cmd.align;
  ctx.textBaseline = "top";
  ctx.fillStyle = paintToCanvas(ctx, style.fill, style.opacity);
  ctx.strokeStyle = paintToCanvas(ctx, style.stroke, style.opacity);
  const hasFill = !paintIsNone(style.fill);
  const hasStroke = !paintIsNone(style.stroke) && style.width > 0;
  if (hasFill) ctx.fillText(cmd.content, x, y);
  if (hasStroke) ctx.strokeText(cmd.content, x, y);
}

/** §18.3: palette-indexed pixel art drawn as crisp `fillRect` cells. */
function drawSpriteCmd(
  ctx: CanvasRenderingContext2D,
  cmd: Extract<DrawCmd, { type: "Sprite" }>,
  style: DrawStyle
): void {
  for (let ry = 0; ry < cmd.rows.length; ry++) {
    const row = cmd.rows[ry];
    for (let rx = 0; rx < row.length; rx++) {
      const idx = spriteCharIndex(row[rx]);
      if (idx === null) continue;
      const color = cmd.palette[idx];
      if (!color || color.isNone || color.a === 0) continue;
      ctx.fillStyle = color.toRgbaString(style.opacity);
      ctx.fillRect(
        cmd.pos[0] + rx * cmd.scale,
        cmd.pos[1] + ry * cmd.scale,
        cmd.scale,
        cmd.scale
      );
    }
  }
}

/**
 * §18.4: pre-renders each pattern tile once into an offscreen canvas and wraps
 * it in a `CanvasPattern` (tiles render with an EMPTY pattern map, so a tile
 * referencing itself falls back to gray and the cycle terminates).
 */
function buildPatternTiles(
  drawList: DrawList,
  ctx: CanvasRenderingContext2D
): Map<string, CanvasPattern> {
  const out = new Map<string, CanvasPattern>();
  const tiles = drawList.patterns;
  if (!tiles || tiles.length === 0) return out;
  if (typeof document === "undefined" || typeof document.createElement !== "function") return out;

  for (const pat of tiles) {
    const w = Math.max(1, Math.round(pat.width));
    const h = Math.max(1, Math.round(pat.height));
    const tileCanvas = document.createElement("canvas");
    tileCanvas.width = w;
    tileCanvas.height = h;
    const tctx = tileCanvas.getContext("2d");
    if (!tctx) continue;
    for (const c of pat.tiles) drawSingleCmd(tctx, c, new Map());
    const pattern = ctx.createPattern(tileCanvas, "repeat");
    if (pattern) out.set(pat.name, pattern);
  }
  return out;
}

export function renderDrawListToCanvas(
  ctx: CanvasRenderingContext2D,
  drawList: DrawList,
  options: RenderCanvasOptions = {}
): void {
  const originX = options.originX ?? 0;
  const originY = options.originY ?? 0;
  const zoom = options.zoom ?? 1.0;

  ctx.save();
  ctx.translate(originX, originY);
  ctx.scale(zoom, zoom);
  if (drawList.pixelFilter === "nearest") {
    ctx.imageSmoothingEnabled = false;
  }

  if (drawList.background && !drawList.background.isNone) {
    ctx.save();
    ctx.globalCompositeOperation = "source-over";
    ctx.fillStyle = drawList.background.toRgbaString(1.0);
    ctx.fillRect(0, 0, drawList.canvasWidth, drawList.canvasHeight);
    ctx.restore();
  }

  // Pattern tiles are defined in the current user space, so they scale with the
  // zoom transform exactly like the geometry that references them.
  const patterns = buildPatternTiles(drawList, ctx);

  for (const cmd of drawList.items) {
    drawSingleCmd(ctx, cmd, patterns);
  }

  ctx.restore();
}

function colorNoAlpha(c: { r: number; g: number; b: number; isNone: boolean } | null): string {
  if (!c || c.isNone) return "none";
  const r = c.r.toString(16).padStart(2, "0");
  const g = c.g.toString(16).padStart(2, "0");
  const b = c.b.toString(16).padStart(2, "0");
  return `#${r}${g}${b}`;
}

function paintSvgRef(paint: Paint, ctx: SvgRenderCtx): string {
  if (paint.kind === "color") return paint.color.toSvgString();
  if (paint.kind === "pattern") return `url(#pvg-pat-${paint.name})`;
  const key = JSON.stringify(paint);
  const existing = ctx.gradIds.get(key);
  if (existing) return `url(#${existing})`;
  const id = `pvg-g${ctx.gradCounter++}`;
  ctx.gradIds.set(key, id);
  ctx.gradOrder.push({ id, paint });
  return `url(#${id})`;
}

function filterSvgId(style: DrawStyle, ctx: SvgRenderCtx): string | null {
  const hasBlur = (style.blur || 0) > 1e-9;
  if (!hasBlur && !style.shadow && !style.glow) return null;
  const key = JSON.stringify({
    b: style.blur || 0,
    s: style.shadow ? { o: style.shadow.offset, r: style.shadow.radius, c: style.shadow.color } : null,
    g: style.glow ? { r: style.glow.radius, c: style.glow.color } : null,
  });
  const existing = ctx.filterIds.get(key);
  if (existing) return existing;
  const id = `pvg-f${ctx.filterCounter++}`;
  ctx.filterIds.set(key, id);
  ctx.filterOrder.push({ id, blur: style.blur || 0, shadow: style.shadow, glow: style.glow });
  return id;
}

function walkSvgCmd(cmd: DrawCmd, ctx: SvgRenderCtx): void {
  if (cmd.type === "Clip") {
    walkSvgCmd(cmd.mask, ctx);
    for (const c of cmd.content) walkSvgCmd(c, ctx);
    return;
  }
  const s = cmd.style;
  if (!s) return;
  paintSvgRef(s.fill, ctx);
  paintSvgRef(s.stroke, ctx);
  filterSvgId(s, ctx);
}

function emitGradientDef(id: string, paint: Paint): string {
  if (paint.kind === "color" || paint.kind === "pattern") return "";
  const stops = paint.stops
    .map((st) => {
      const c = st.color;
      if (!c || c.isNone) return `<stop offset="${st.offset.toFixed(3)}" stop-color="none" />`;
      const r = c.r.toString(16).padStart(2, "0");
      const g = c.g.toString(16).padStart(2, "0");
      const b = c.b.toString(16).padStart(2, "0");
      if (c.a === 255) {
        return `<stop offset="${st.offset.toFixed(3)}" stop-color="#${r}${g}${b}" />`;
      }
      return `<stop offset="${st.offset.toFixed(3)}" stop-color="#${r}${g}${b}" stop-opacity="${(c.a / 255).toFixed(3)}" />`;
    })
    .join("");
  if (paint.kind === "linear") {
    return `<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${paint.start[0].toFixed(2)}" y1="${paint.start[1].toFixed(2)}" x2="${paint.end[0].toFixed(2)}" y2="${paint.end[1].toFixed(2)}">${stops}</linearGradient>`;
  }
  if (paint.kind === "radial") {
    const f = paint.focal || paint.center;
    return `<radialGradient id="${id}" gradientUnits="userSpaceOnUse" cx="${paint.center[0].toFixed(2)}" cy="${paint.center[1].toFixed(2)}" r="${paint.radius.toFixed(2)}" fx="${f[0].toFixed(2)}" fy="${f[1].toFixed(2)}">${stops}</radialGradient>`;
  }
  if (paint.kind === "angular") {
    // Angular: no native SVG — linear fallback carrying same stops (matches Rust).
    return `<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${(paint.center[0] - 100).toFixed(2)}" y1="${paint.center[1].toFixed(2)}" x2="${(paint.center[0] + 100).toFixed(2)}" y2="${paint.center[1].toFixed(2)}">${stops}</linearGradient>`;
  }
  return "";
}

function emitFilterDef(entry: SvgRenderCtx["filterOrder"][number]): string {
  const parts: string[] = [];
  if (entry.shadow) {
    parts.push(
      `<feDropShadow dx="${entry.shadow.offset[0].toFixed(2)}" dy="${entry.shadow.offset[1].toFixed(2)}" stdDeviation="${(entry.shadow.radius / 2).toFixed(2)}" flood-color="${colorNoAlpha(entry.shadow.color)}" />`
    );
  }
  if (entry.glow) {
    parts.push(
      `<feDropShadow dx="0" dy="0" stdDeviation="${(entry.glow.radius / 2).toFixed(2)}" flood-color="${colorNoAlpha(entry.glow.color)}" />`
    );
  }
  if ((entry.blur || 0) > 1e-9) {
    parts.push(`<feGaussianBlur stdDeviation="${(entry.blur / 2).toFixed(2)}" />`);
  }
  return `<filter id="${entry.id}" x="-60%" y="-60%" width="220%" height="220%">${parts.join("")}</filter>`;
}

function blendToSvg(blend: DrawStyle["blend"]): string | null {
  switch (blend) {
    case "add": return "plus-lighter";
    case "multiply": return "multiply";
    case "screen": return "screen";
    case "overlay": return "overlay";
    default: return null;
  }
}

function formatSvgStyle(s: DrawStyle, ctx?: SvgRenderCtx): string {
  const c = ctx ?? newSvgCtx();
  let attrs = `fill="${paintSvgRef(s.fill, c)}"`;
  if (!paintIsNone(s.stroke) && s.width > 0) {
    attrs += ` stroke="${paintSvgRef(s.stroke, c)}" stroke-width="${s.width.toFixed(2)}"`;
  } else {
    attrs += ` stroke="none"`;
  }
  // Non-default topology only (keeps 0.1 output byte-stable)
  if (s.cap && s.cap !== "butt") attrs += ` stroke-linecap="${s.cap}"`;
  if (s.join && s.join !== "miter") attrs += ` stroke-linejoin="${s.join}"`;
  if (s.join === "miter" && Math.abs((s.miter || 4) - 4.0) > 1e-6) {
    attrs += ` stroke-miterlimit="${s.miter.toFixed(2)}"`;
  }
  if (s.dash && s.dash.length > 0) {
    attrs += ` stroke-dasharray="${s.dash.map((v) => v.toFixed(2)).join(" ")}"`;
  }
  if (Math.abs(s.opacity - 1.0) > 0.001) {
    attrs += ` opacity="${s.opacity.toFixed(3)}"`;
  }
  const css = blendToSvg(s.blend);
  if (css) attrs += ` mix-blend-mode="${css}"`;
  const fid = ctx ? filterSvgId(s, ctx) : filterSvgId(s, c);
  if (fid) attrs += ` filter="url(#${fid})"`;
  // Note: stroke-align inside/outside has no SVG equivalent (matches Rust: ignored).
  return attrs;
}

function clipMaskToSvg(mask: DrawCmd, ctx: SvgRenderCtx): string {
  // Raw mask geometry without paint/filter (matches Rust emit_clip_mask_shape).
  switch (mask.type) {
    case "Circle":
      return `<circle cx="${mask.center[0].toFixed(2)}" cy="${mask.center[1].toFixed(2)}" r="${mask.radius.toFixed(2)}" />`;
    case "Ellipse":
      return `<ellipse cx="${mask.center[0].toFixed(2)}" cy="${mask.center[1].toFixed(2)}" rx="${mask.radius[0].toFixed(2)}" ry="${mask.radius[1].toFixed(2)}" />`;
    case "Rectangle": {
      const rx = mask.cornerRadius > 0 ? ` rx="${mask.cornerRadius.toFixed(2)}" ry="${mask.cornerRadius.toFixed(2)}"` : "";
      return `<rect x="${mask.pos[0].toFixed(2)}" y="${mask.pos[1].toFixed(2)}" width="${mask.size[0].toFixed(2)}" height="${mask.size[1].toFixed(2)}"${rx} />`;
    }
    case "Line":
      return `<line x1="${mask.from[0].toFixed(2)}" y1="${mask.from[1].toFixed(2)}" x2="${mask.to[0].toFixed(2)}" y2="${mask.to[1].toFixed(2)}" stroke-width="${Math.max(1, mask.style.width).toFixed(2)}" />`;
    case "Polygon": {
      const pts = mask.points.map((p) => `${p[0].toFixed(2)},${p[1].toFixed(2)}`).join(" ");
      return `<polygon points="${pts}" />`;
    }
    case "Path":
      return `<path d="${pathToSvgD(mask.commands)}" />`;
    case "Text": {
      let anchor = "start";
      if (mask.align === "center") anchor = "middle";
      else if (mask.align === "right") anchor = "end";
      return `<text x="${mask.pos[0].toFixed(2)}" y="${mask.pos[1].toFixed(2)}" font-size="${mask.size.toFixed(2)}" font-family="${mask.fontFamily}" text-anchor="${anchor}" dominant-baseline="hanging">${escapeXml(mask.content)}</text>`;
    }
    case "Clip":
      return clipMaskToSvg(mask.mask, ctx);
    default:
      return "";
  }
}

/** §18.5: Catmull-Rom spline -> SVG path data (`M ... C ...`), 2-decimal coords. */
function splinePathData(points: Vec2[]): string {
  if (points.length === 0) return "";
  if (points.length === 1) return `M ${points[0][0].toFixed(2)} ${points[0][1].toFixed(2)}`;
  const parts: string[] = [`M ${points[0][0].toFixed(2)} ${points[0][1].toFixed(2)} `];
  for (const [c1, c2, ep] of splineToBezier(points)) {
    parts.push(
      `C ${c1[0].toFixed(2)} ${c1[1].toFixed(2)}, ${c2[0].toFixed(2)} ${c2[1].toFixed(2)}, ${ep[0].toFixed(2)} ${ep[1].toFixed(2)} `
    );
  }
  return parts.join("").trimEnd();
}

function pathToSvgD(commands: DrawPathCommand[]): string {
  const d: string[] = [];
  for (const pCmd of commands) {
    switch (pCmd.cmd) {
      case "Start": d.push(`M ${pCmd.pt[0].toFixed(2)} ${pCmd.pt[1].toFixed(2)}`); break;
      case "Line": d.push(`L ${pCmd.pt[0].toFixed(2)} ${pCmd.pt[1].toFixed(2)}`); break;
      case "Quad": d.push(`Q ${pCmd.cp[0].toFixed(2)} ${pCmd.cp[1].toFixed(2)}, ${pCmd.ep[0].toFixed(2)} ${pCmd.ep[1].toFixed(2)}`); break;
      case "Curve": d.push(`C ${pCmd.c1[0].toFixed(2)} ${pCmd.c1[1].toFixed(2)}, ${pCmd.c2[0].toFixed(2)} ${pCmd.c2[1].toFixed(2)}, ${pCmd.ep[0].toFixed(2)} ${pCmd.ep[1].toFixed(2)}`); break;
      case "Arc": {
        const r = pCmd.radius;
        const delta = pCmd.endAngle - pCmd.startAngle;
        const endX = pCmd.center[0] + r * Math.cos(pCmd.endAngle);
        const endY = pCmd.center[1] + r * Math.sin(pCmd.endAngle);
        if (Math.abs(delta) >= Math.PI * 2 - 1e-4) {
          const midAngle = pCmd.startAngle + delta / 2.0;
          const midX = pCmd.center[0] + r * Math.cos(midAngle);
          const midY = pCmd.center[1] + r * Math.sin(midAngle);
          const sweep = delta > 0 ? 1 : 0;
          d.push(`A ${r.toFixed(2)} ${r.toFixed(2)} 0 0 ${sweep} ${midX.toFixed(2)} ${midY.toFixed(2)}`);
          d.push(`A ${r.toFixed(2)} ${r.toFixed(2)} 0 0 ${sweep} ${endX.toFixed(2)} ${endY.toFixed(2)}`);
        } else {
          const largeArc = Math.abs(delta) > Math.PI ? 1 : 0;
          const sweep = delta > 0 ? 1 : 0;
          d.push(`A ${r.toFixed(2)} ${r.toFixed(2)} 0 ${largeArc} ${sweep} ${endX.toFixed(2)} ${endY.toFixed(2)}`);
        }
        break;
      }
      case "Close": d.push("Z"); break;
    }
  }
  return d.join(" ");
}

export function emitSvgCommands(items: DrawCmd[], indent = "  ", ctx?: SvgRenderCtx): string {
  const c = ctx ?? newSvgCtx();
  let out = "";
  for (const cmd of items) {
    switch (cmd.type) {
      case "Circle":
        out += `${indent}<circle cx="${cmd.center[0].toFixed(2)}" cy="${cmd.center[1].toFixed(2)}" r="${cmd.radius.toFixed(2)}" ${formatSvgStyle(cmd.style, c)} />\n`;
        break;
      case "Ellipse":
        out += `${indent}<ellipse cx="${cmd.center[0].toFixed(2)}" cy="${cmd.center[1].toFixed(2)}" rx="${cmd.radius[0].toFixed(2)}" ry="${cmd.radius[1].toFixed(2)}" ${formatSvgStyle(cmd.style, c)} />\n`;
        break;
      case "Rectangle": {
        const rxAttr = cmd.cornerRadius > 0 ? ` rx="${cmd.cornerRadius.toFixed(2)}" ry="${cmd.cornerRadius.toFixed(2)}"` : "";
        out += `${indent}<rect x="${cmd.pos[0].toFixed(2)}" y="${cmd.pos[1].toFixed(2)}" width="${cmd.size[0].toFixed(2)}" height="${cmd.size[1].toFixed(2)}"${rxAttr} ${formatSvgStyle(cmd.style, c)} />\n`;
        break;
      }
      case "Line":
        out += `${indent}<line x1="${cmd.from[0].toFixed(2)}" y1="${cmd.from[1].toFixed(2)}" x2="${cmd.to[0].toFixed(2)}" y2="${cmd.to[1].toFixed(2)}" ${formatSvgStyle(cmd.style, c)} />\n`;
        break;
      case "Polygon": {
        const pts = cmd.points.map((p) => `${p[0].toFixed(2)},${p[1].toFixed(2)}`).join(" ");
        out += `${indent}<polygon points="${pts}" ${formatSvgStyle(cmd.style, c)} />\n`;
        break;
      }
      case "Text": {
        let anchor = "start";
        if (cmd.align === "center") anchor = "middle";
        else if (cmd.align === "right") anchor = "end";
        out += `${indent}<text x="${cmd.pos[0].toFixed(2)}" y="${cmd.pos[1].toFixed(2)}" font-size="${cmd.size.toFixed(2)}" font-family="${cmd.fontFamily}" text-anchor="${anchor}" dominant-baseline="hanging" ${formatSvgStyle(cmd.style, c)}>${escapeXml(cmd.content)}</text>\n`;
        break;
      }
      case "Path": {
        out += `${indent}<path d="${pathToSvgD(cmd.commands)}" ${formatSvgStyle(cmd.style, c)} />\n`;
        break;
      }
      case "Clip": {
        const id = `pvg-clip${c.clipCounter++}`;
        c.clipDefs.push({ id, mask: cmd.mask });
        out += `${indent}<g clip-path="url(#${id})">\n`;
        out += emitSvgCommands(cmd.content, indent + "  ", c);
        out += `${indent}</g>\n`;
        break;
      }
      case "Spline": {
        if (cmd.points.length === 0) break;
        out += `${indent}<path d="${splinePathData(cmd.points)}" fill="none" ${formatSvgStyle(cmd.style, c)} />\n`;
        break;
      }
      case "Sprite": {
        // Palette-indexed pixels as crisp rects (§18.3); `.`/space = transparent.
        const opacityAttr =
          Math.abs(cmd.style.opacity - 1.0) > 0.001 ? ` opacity="${cmd.style.opacity.toFixed(3)}"` : "";
        const blendAttr = blendToSvg(cmd.style.blend);
        const blendPart = blendAttr ? ` mix-blend-mode="${blendAttr}"` : "";
        out += `${indent}<g shape-rendering="crispEdges"${opacityAttr}${blendPart}>\n`;
        const inner = `${indent}  `;
        for (let ry = 0; ry < cmd.rows.length; ry++) {
          const row = cmd.rows[ry];
          for (let rx = 0; rx < row.length; rx++) {
            const idx = spriteCharIndex(row[rx]);
            if (idx === null) continue;
            const color = cmd.palette[idx];
            if (!color || color.isNone || color.a === 0) continue;
            const x = cmd.pos[0] + rx * cmd.scale;
            const y = cmd.pos[1] + ry * cmd.scale;
            out += `${inner}<rect x="${x.toFixed(2)}" y="${y.toFixed(2)}" width="${cmd.scale.toFixed(2)}" height="${cmd.scale.toFixed(2)}" fill="${color.toSvgString()}" />\n`;
          }
        }
        out += `${indent}</g>\n`;
        break;
      }
    }
  }
  return out;
}

export function exportToSvgString(drawList: DrawList): string {
  const ctx = newSvgCtx();
  const patterns = drawList.patterns ?? [];
  // Walk pattern tiles first, then the body, so every gradient/filter id that
  // a tile or a clipped piece registers lands in <defs> in deterministic order.
  for (const pat of patterns) {
    for (const tile of pat.tiles) walkSvgCmd(tile, ctx);
  }
  for (const cmd of drawList.items) walkSvgCmd(cmd, ctx);
  // Second pass populates ids in deterministic order
  const body = emitSvgCommands(drawList.items, "  ", ctx);

  // §18.4: repeatable tiles as real SVG <pattern> defs.
  let patternDefs = "";
  for (const pat of patterns) {
    let tileBody = "";
    for (const tile of pat.tiles) tileBody += emitSvgCommands([tile], "      ", ctx);
    patternDefs += `<pattern id="pvg-pat-${pat.name}" patternUnits="userSpaceOnUse" width="${pat.width.toFixed(2)}" height="${pat.height.toFixed(2)}">${tileBody}</pattern>`;
  }

  let defs = "";
  for (const g of ctx.gradOrder) defs += emitGradientDef(g.id, g.paint);
  for (const f of ctx.filterOrder) defs += emitFilterDef(f);
  defs += patternDefs;
  for (const c of ctx.clipDefs) defs += `<clipPath id="${c.id}">${clipMaskToSvg(c.mask, ctx)}</clipPath>`;
  if (defs) defs = `  <defs>${defs}</defs>\n`;

  const crispAttr = drawList.pixelFilter === "nearest" ? ` shape-rendering="crispEdges"` : "";
  let svg = `<?xml version="1.0" encoding="UTF-8"?>\n`;
  svg += `<svg viewBox="0 0 ${drawList.canvasWidth} ${drawList.canvasHeight}" width="100%" height="100%"${crispAttr} xmlns="http://www.w3.org/2000/svg">\n`;

  // Background first, then <defs>, then the body (same element order as the
  // Rust emitter so downstream diffs stay clean).
  if (drawList.background && !drawList.background.isNone) {
    svg += `  <rect width="100%" height="100%" fill="${drawList.background.toSvgString()}" />\n`;
  }

  svg += defs;
  svg += body;
  svg += `</svg>\n`;
  return svg;
}

export function exportToAnimatedSvgString(
  sourceCode: string,
  options: AnimatedSvgOptions = {}
): string {
  const duration = options.duration ?? detectLoopDuration(sourceCode);
  const fps = options.fps ?? 30;
  const totalFrames = Math.max(2, Math.round(duration * fps));
  const frames: DrawList[] = [];

  const lexer = new Lexer(sourceCode);
  const tokens = lexer.tokenizeAll();
  const parser = new Parser(tokens);
  const ast = parser.parseDocument();

  for (let i = 0; i < totalFrames; i++) {
    const t = (i / totalFrames) * duration;
    const evaluator = new Evaluator(t);
    frames.push(evaluator.evaluateDocument(ast));
  }

  if (frames.length === 0) return "";

  const first = frames[0];
  let svg = `<?xml version="1.0" encoding="UTF-8"?>\n`;
  svg += `<svg viewBox="0 0 ${first.canvasWidth} ${first.canvasHeight}" width="100%" height="100%" xmlns="http://www.w3.org/2000/svg">\n`;

  if (first.background && !first.background.isNone) {
    svg += `  <rect width="100%" height="100%" fill="${first.background.toSvgString()}" />\n`;
  }

  const n = totalFrames;
  for (let i = 0; i < n; i++) {
    let valuesStr: string;
    let keyTimesStr: string;
    if (i === 0) {
      const t1 = (1.0 / n).toFixed(4);
      valuesStr = "visible;hidden";
      keyTimesStr = `0; ${t1}`;
    } else if (i === n - 1) {
      const t0 = ((n - 1.0) / n).toFixed(4);
      valuesStr = "hidden;visible";
      keyTimesStr = `0; ${t0}`;
    } else {
      const t0 = (i / n).toFixed(4);
      const t1 = ((i + 1) / n).toFixed(4);
      valuesStr = "hidden;visible;hidden";
      keyTimesStr = `0; ${t0}; ${t1}`;
    }

    svg += `  <g>\n`;
    svg += `    <animate attributeName="visibility" values="${valuesStr}" keyTimes="${keyTimesStr}" dur="${duration.toFixed(2)}s" repeatCount="indefinite" calcMode="discrete" />\n`;
    svg += emitSvgCommands(frames[i].items, "    ", newSvgCtx());
    svg += `  </g>\n`;
  }

  svg += `</svg>\n`;
  return svg;
}
