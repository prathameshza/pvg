import type { AnimatedSvgOptions, DrawCmd, DrawList, DrawStyle, RenderCanvasOptions } from "./types.js";
import { Lexer } from "./lexer.js";
import { Parser } from "./parser.js";
import { Evaluator } from "./evaluator.js";

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

  if (drawList.background && !drawList.background.isNone) {
    ctx.fillStyle = drawList.background.toRgbaString(1.0);
    ctx.fillRect(0, 0, drawList.canvasWidth, drawList.canvasHeight);
  }

  for (const cmd of drawList.items) {
    const { style } = cmd;
    const hasFill = !style.fill.isNone;
    const hasStroke = !style.stroke.isNone && style.width > 0;

    ctx.fillStyle = style.fill.toRgbaString(style.opacity);
    ctx.strokeStyle = style.stroke.toRgbaString(style.opacity);
    ctx.lineWidth = style.width;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    switch (cmd.type) {
      case "Circle": {
        ctx.beginPath();
        ctx.arc(cmd.center[0], cmd.center[1], cmd.radius, 0, Math.PI * 2);
        if (hasFill) ctx.fill();
        if (hasStroke) ctx.stroke();
        break;
      }
      case "Ellipse": {
        ctx.beginPath();
        ctx.ellipse(cmd.center[0], cmd.center[1], cmd.radius[0], cmd.radius[1], 0, 0, Math.PI * 2);
        if (hasFill) ctx.fill();
        if (hasStroke) ctx.stroke();
        break;
      }
      case "Rectangle": {
        const [x, y] = cmd.pos;
        const [w, h] = cmd.size;
        const r = Math.max(0, Math.min(cmd.cornerRadius, w / 2, h / 2));

        ctx.beginPath();
        if (r > 0) {
          if (typeof ctx.roundRect === "function") {
            ctx.roundRect(x, y, w, h, r);
          } else {
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
        } else {
          ctx.rect(x, y, w, h);
        }
        if (hasFill) ctx.fill();
        if (hasStroke) ctx.stroke();
        break;
      }
      case "Line": {
        ctx.beginPath();
        ctx.moveTo(cmd.from[0], cmd.from[1]);
        ctx.lineTo(cmd.to[0], cmd.to[1]);
        if (hasStroke) ctx.stroke();
        break;
      }
      case "Polygon": {
        if (cmd.points.length < 2) continue;
        ctx.beginPath();
        ctx.moveTo(cmd.points[0][0], cmd.points[0][1]);
        for (let i = 1; i < cmd.points.length; i++) {
          ctx.lineTo(cmd.points[i][0], cmd.points[i][1]);
        }
        ctx.closePath();
        if (hasFill) ctx.fill();
        if (hasStroke) ctx.stroke();
        break;
      }
      case "Text": {
        const [x, y] = cmd.pos;
        const sizePx = cmd.size;
        let fontFam = cmd.fontFamily || "sans-serif";
        const fLower = fontFam.toLowerCase();
        if (fLower === "mono" || fLower === "monospace" || fLower === "code") {
          fontFam = '"Fira Code", "JetBrains Mono", Consolas, monospace';
        } else if (fLower === "sans" || fLower === "sans-serif") {
          fontFam = 'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
        } else if (fLower === "serif") {
          fontFam = 'Georgia, "Times New Roman", serif';
        }

        ctx.font = `${sizePx}px ${fontFam}`;
        ctx.textAlign = cmd.align;
        ctx.textBaseline = "top";

        if (hasFill) ctx.fillText(cmd.content, x, y);
        if (hasStroke) ctx.strokeText(cmd.content, x, y);
        break;
      }
      case "Path": {
        ctx.beginPath();
        for (const pCmd of cmd.commands) {
          switch (pCmd.cmd) {
            case "Start": ctx.moveTo(pCmd.pt[0], pCmd.pt[1]); break;
            case "Line": ctx.lineTo(pCmd.pt[0], pCmd.pt[1]); break;
            case "Quad": ctx.quadraticCurveTo(pCmd.cp[0], pCmd.cp[1], pCmd.ep[0], pCmd.ep[1]); break;
            case "Curve": ctx.bezierCurveTo(pCmd.c1[0], pCmd.c1[1], pCmd.c2[0], pCmd.c2[1], pCmd.ep[0], pCmd.ep[1]); break;
            case "Arc": {
              const delta = pCmd.endAngle - pCmd.startAngle;
              const counterclockwise = delta < 0;
              ctx.arc(pCmd.center[0], pCmd.center[1], pCmd.radius, pCmd.startAngle, pCmd.endAngle, counterclockwise);
              break;
            }
            case "Close": ctx.closePath(); break;
          }
        }
        if (hasFill) ctx.fill();
        if (hasStroke) ctx.stroke();
        break;
      }
    }
  }

  ctx.restore();
}

function formatSvgStyle(s: DrawStyle): string {
  let attrs = `fill="${s.fill.toSvgString()}"`;
  if (!s.stroke.isNone && s.width > 0) {
    attrs += ` stroke="${s.stroke.toSvgString()}" stroke-width="${s.width.toFixed(2)}" stroke-linecap="round" stroke-linejoin="round"`;
  } else {
    attrs += ` stroke="none"`;
  }
  if (Math.abs(s.opacity - 1.0) > 0.001) {
    attrs += ` opacity="${s.opacity.toFixed(3)}"`;
  }
  return attrs;
}

export function emitSvgCommands(items: DrawCmd[], indent = "  "): string {
  let out = "";
  for (const cmd of items) {
    switch (cmd.type) {
      case "Circle":
        out += `${indent}<circle cx="${cmd.center[0].toFixed(2)}" cy="${cmd.center[1].toFixed(2)}" r="${cmd.radius.toFixed(2)}" ${formatSvgStyle(cmd.style)} />\n`;
        break;
      case "Ellipse":
        out += `${indent}<ellipse cx="${cmd.center[0].toFixed(2)}" cy="${cmd.center[1].toFixed(2)}" rx="${cmd.radius[0].toFixed(2)}" ry="${cmd.radius[1].toFixed(2)}" ${formatSvgStyle(cmd.style)} />\n`;
        break;
      case "Rectangle": {
        const rxAttr = cmd.cornerRadius > 0 ? ` rx="${cmd.cornerRadius.toFixed(2)}" ry="${cmd.cornerRadius.toFixed(2)}"` : "";
        out += `${indent}<rect x="${cmd.pos[0].toFixed(2)}" y="${cmd.pos[1].toFixed(2)}" width="${cmd.size[0].toFixed(2)}" height="${cmd.size[1].toFixed(2)}"${rxAttr} ${formatSvgStyle(cmd.style)} />\n`;
        break;
      }
      case "Line":
        out += `${indent}<line x1="${cmd.from[0].toFixed(2)}" y1="${cmd.from[1].toFixed(2)}" x2="${cmd.to[0].toFixed(2)}" y2="${cmd.to[1].toFixed(2)}" ${formatSvgStyle(cmd.style)} />\n`;
        break;
      case "Polygon": {
        const pts = cmd.points.map((p) => `${p[0].toFixed(2)},${p[1].toFixed(2)}`).join(" ");
        out += `${indent}<polygon points="${pts}" ${formatSvgStyle(cmd.style)} />\n`;
        break;
      }
      case "Text": {
        let anchor = "start";
        if (cmd.align === "center") anchor = "middle";
        else if (cmd.align === "right") anchor = "end";
        out += `${indent}<text x="${cmd.pos[0].toFixed(2)}" y="${cmd.pos[1].toFixed(2)}" font-size="${cmd.size.toFixed(2)}" font-family="${cmd.fontFamily}" text-anchor="${anchor}" dominant-baseline="hanging" ${formatSvgStyle(cmd.style)}>${escapeXml(cmd.content)}</text>\n`;
        break;
      }
      case "Path": {
        const d: string[] = [];
        for (const pCmd of cmd.commands) {
          switch (pCmd.cmd) {
            case "Start": d.push(`M ${pCmd.pt[0].toFixed(2)} ${pCmd.pt[1].toFixed(2)}`); break;
            case "Line": d.push(`L ${pCmd.pt[0].toFixed(2)} ${pCmd.pt[1].toFixed(2)}`); break;
            case "Quad": d.push(`Q ${pCmd.cp[0].toFixed(2)} ${pCmd.cp[1].toFixed(2)}, ${pCmd.ep[0].toFixed(2)} ${pCmd.ep[1].toFixed(2)}`); break;
            case "Curve": d.push(`C ${pCmd.c1[0].toFixed(2)} ${pCmd.c1[1].toFixed(2)}, ${pCmd.c2[0].toFixed(2)} ${pCmd.c2[1].toFixed(2)}, ${pCmd.ep[0].toFixed(2)} ${pCmd.ep[0].toFixed(2)}`); break;
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
        out += `${indent}<path d="${d.join(" ")}" ${formatSvgStyle(cmd.style)} />\n`;
        break;
      }
    }
  }
  return out;
}

export function exportToSvgString(drawList: DrawList): string {
  let svg = `<?xml version="1.0" encoding="UTF-8"?>\n`;
  svg += `<svg viewBox="0 0 ${drawList.canvasWidth} ${drawList.canvasHeight}" width="100%" height="100%" xmlns="http://www.w3.org/2000/svg">\n`;

  if (drawList.background && !drawList.background.isNone) {
    svg += `  <rect width="100%" height="100%" fill="${drawList.background.toSvgString()}" />\n`;
  }

  svg += emitSvgCommands(drawList.items, "  ");
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
    svg += emitSvgCommands(frames[i].items, "    ");
    svg += `  </g>\n`;
  }

  svg += `</svg>\n`;
  return svg;
}