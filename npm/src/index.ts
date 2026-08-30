import { dedentCode, Lexer, Token, TokenKind } from "./lexer.js";
import { Parser } from "./parser.js";
import { Evaluator } from "./evaluator.js";
import { PvgColor } from "./color.js";
import { Transform2D } from "./transform.js";
import {
  detectLoopDuration,
  emitSvgCommands,
  escapeXml,
  exportToAnimatedSvgString,
  exportToSvgString,
  renderDrawListToCanvas,
} from "./renderer.js";
import { PvgView, registerPvgView } from "./component.js";
import type {
  AnimatedSvgOptions,
  CanvasDecl,
  Document,
  DrawCmd,
  DrawList,
  DrawPathCommand,
  DrawStyle,
  Expr,
  RenderCanvasOptions,
  Stmt,
  TextAlign,
  Vec2,
} from "./types.js";

/**
 * Parses a PVG source string into an Abstract Syntax Tree (`Document`).
 *
 * @param source PVG source code string
 * @returns Parsed AST Document
 *
 * @example
 * ```ts
 * import { parse } from 'pvg';
 *
 * const ast = parse(`
 * PVG 0.1
 * canvas 400 400
 * circle
 *   center [200, 200]
 *   radius 50
 *   fill #00ffcc
 * `);
 * ```
 */
export function parse(source: string): Document {
  const clean = dedentCode(source);
  const lexer = new Lexer(clean);
  const tokens = lexer.tokenizeAll();
  const parser = new Parser(tokens);
  return parser.parseDocument();
}

/**
 * Evaluates a pre-parsed AST Document at a specific timeline timestamp.
 * Ideal for 60 FPS real-time rendering loops without string re-parsing churn.
 *
 * @param doc Parsed Document AST
 * @param time Elapsed time in seconds (default: 0.0)
 * @returns Evaluated flat 2D DrawList
 */
export function evaluate(doc: Document, time = 0.0): DrawList {
  const evaluator = new Evaluator(time);
  return evaluator.evaluateDocument(doc);
}

/**
 * Compiles a PVG source text string directly into a flat 2D `DrawList`.
 *
 * @param source PVG source code string
 * @param time Elapsed time in seconds (default: 0.0)
 * @returns Evaluated 2D DrawList
 */
export function compile(source: string, time = 0.0): DrawList {
  const doc = parse(source);
  return evaluate(doc, time);
}

/**
 * Transpiles a PVG document or evaluated `DrawList` into a static W3C SVG string.
 *
 * @param input PVG source code string OR evaluated DrawList
 * @param time Timestamp in seconds if input is a string (default: 0.0)
 * @returns Standards-compliant W3C SVG XML string
 *
 * @example
 * ```ts
 * import { toSvg } from 'pvg';
 *
 * const svgXml = toSvg(`
 * PVG 0.1
 * canvas 200 200
 * circle
 *   center [100, 100]
 *   radius 40
 *   fill #ff0055
 * `);
 * ```
 */
export function toSvg(input: string | DrawList, time = 0.0): string {
  if (typeof input === "string") {
    const drawList = compile(input, time);
    return exportToSvgString(drawList);
  }
  return exportToSvgString(input);
}

/**
 * Transpiles an animated PVG document into a standalone SMIL-animated W3C SVG string.
 *
 * @param source PVG source code containing `time` or `t` variables
 * @param options Animation duration and frame-rate options
 * @returns Standalone animated SVG XML string
 */
export function toAnimatedSvg(
  source: string,
  options?: AnimatedSvgOptions
): string {
  return exportToAnimatedSvgString(source, options);
}

/**
 * Renders a `DrawList` directly onto an HTML5 `<canvas>` 2D rendering context.
 *
 * @param ctx Target CanvasRenderingContext2D
 * @param drawList Flat 2D DrawList from `compile()` or `evaluate()`
 * @param options Offset, zoom, and clear parameters
 */
export function renderToCanvas(
  ctx: CanvasRenderingContext2D,
  drawList: DrawList,
  options?: RenderCanvasOptions
): void {
  renderDrawListToCanvas(ctx, drawList, options);
}

// Re-export classes, utilities, and types
export {
  PvgColor,
  Transform2D,
  Lexer,
  Parser,
  Evaluator,
  Token,
  TokenKind,
  PvgView,
  registerPvgView,
  dedentCode,
  detectLoopDuration,
  escapeXml,
  exportToSvgString,
  exportToAnimatedSvgString,
  emitSvgCommands,
};

export type {
  Vec2,
  TextAlign,
  CanvasDecl,
  Expr,
  Stmt,
  Document,
  DrawStyle,
  DrawCmd,
  DrawPathCommand,
  DrawList,
  RenderCanvasOptions,
  AnimatedSvgOptions,
};

// Default export
export default {
  parse,
  compile,
  evaluate,
  toSvg,
  toAnimatedSvg,
  renderToCanvas,
  PvgColor,
  Transform2D,
  PvgView,
  registerPvgView,
};