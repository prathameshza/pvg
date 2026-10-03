import type { PvgColor } from "./color.js";
import type { Transform2D } from "./transform.js";

export type Vec2 = [x: number, y: number];

export type TextAlign = "left" | "center" | "right";

/** PVG 0.2 Section 8: stroke line-cap topology. */
export type LineCap = "butt" | "round" | "square";

/** PVG 0.2 Section 8: stroke line-join topology. */
export type LineJoin = "miter" | "round" | "bevel";

/** PVG 0.2 Section 8: stroke alignment. */
export type StrokeAlign = "center" | "inside" | "outside";

/** PVG 0.2 Section 12: blend/composite mode. */
export type BlendMode = "normal" | "add" | "multiply" | "screen" | "overlay";

/** Post-0.2 Section 18: pixel sampling for `canvas filter` (sprite / retro mode). */
export type PixelFilter = "linear" | "nearest";

export interface CanvasDecl {
  width: number;
  height: number;
  background: PvgColor | null;
  /** Post-0.2 Section 18: pixel snap grid in px (0 = off). */
  snap: number;
  /** Post-0.2 Section 18: image smoothing for raster backends. */
  pixelFilter: PixelFilter;
}

export type UnaryOp = "neg" | "not";

export type BinaryOp =
  | "+"
  | "-"
  | "*"
  | "/"
  | "%"
  | "^"
  | "=="
  | "!="
  | "<"
  | "<="
  | ">"
  | ">="
  | "and"
  | "or";

/** PVG 0.2 Section 9: single gradient stop expression (`stop <offset> <color>`). */
export interface GradientStopExpr {
  type: "GradientStop";
  offset: Expr;
  color: Expr;
}

/** PVG 0.2 Section 10: drop-shadow expression (`shadow [dx, dy] <radius> <color>`). */
export interface ShadowExpr {
  type: "ShadowExpr";
  offset: Expr;
  radius: Expr;
  color: Expr;
}

/** PVG 0.2 Section 10: outer-glow expression (`glow <radius> <color>`). */
export interface GlowExpr {
  type: "GlowExpr";
  radius: Expr;
  color: Expr;
}

export type Expr =
  | { type: "Number"; value: number }
  | { type: "String"; value: string }
  | { type: "Bool"; value: boolean }
  | { type: "Color"; value: PvgColor }
  | { type: "Vec2"; x: Expr; y: Expr }
  /** Post-0.2 Section 18.5: 1D data array (bracket literal with arity != 2, or nested). */
  | { type: "Array"; items: Expr[] }
  /** Post-0.2 Section 18.4: reference to a top-level `pattern` tile (`fill pattern name`). */
  | { type: "Pattern"; name: string }
  | { type: "Ident"; name: string }
  | { type: "Unary"; op: UnaryOp; inner: Expr }
  | { type: "Binary"; op: BinaryOp; left: Expr; right: Expr }
  | { type: "Ternary"; cond: Expr; trueBranch: Expr; falseBranch: Expr }
  | { type: "Call"; name: string; args: Expr[] }
  | { type: "Linear"; start: Expr; end: Expr; stops: GradientStopExpr[] }
  | { type: "Radial"; center: Expr; radius: Expr; focal: Expr | null; stops: GradientStopExpr[] }
  | { type: "Angular"; center: Expr; startAngle: Expr; stops: GradientStopExpr[] }
  | GradientStopExpr
  | ShadowExpr
  | GlowExpr;

export type PathCommandAst =
  | { cmd: "Set"; name: string; expr: Expr }
  | { cmd: "Start"; pt: Expr }
  | { cmd: "Line"; pt: Expr }
  | { cmd: "Quad"; cp: Expr; ep: Expr }
  | { cmd: "Curve"; c1: Expr; c2: Expr; ep: Expr }
  | { cmd: "Arc"; center: Expr; radius: Expr; startAngle: Expr; endAngle: Expr }
  | { cmd: "Close" }
  /** Post-0.2 Section 18.6: control flow inside `path` bodies (shares the path locals). */
  | { cmd: "For"; varName: string; from: Expr; to: Expr; step: Expr | null; body: PathCommandAst[] }
  | { cmd: "While"; cond: Expr; body: PathCommandAst[] }
  | { cmd: "If"; cond: Expr; thenBody: PathCommandAst[]; elseBody: PathCommandAst[] };

/**
 * PVG 0.2 Sections 8/10/12 per-shape style properties.
 * `null` (or empty `dash`) means inherit the current style / engine default.
 */
export interface ShapeFx {
  cap: Expr | null;
  join: Expr | null;
  miter: Expr | null;
  dash: Expr[] | null;
  align: Expr | null;
  blur: Expr | null;
  shadow: ShadowExpr | null;
  glow: GlowExpr | null;
  blend: Expr | null;
}

/** FX subset valid on `group` blocks (blend/blur/shadow/glow only). */
export type GroupFx = Pick<ShapeFx, "blend" | "blur" | "shadow" | "glow">;

/** FX subset valid on `text` blocks (blend/blur/shadow/glow only; `align` stays the text anchor). */
export type TextFx = Pick<ShapeFx, "blend" | "blur" | "shadow" | "glow">;

export type Stmt =
  | { type: "Set"; name: string; expr: Expr }
  | { type: "Seed"; seed: number }
  | { type: "Def"; name: string; params: string[]; body: Stmt[] }
  | { type: "Return"; expr: Expr }
  | { type: "For"; var: string; from: Expr; to: Expr; step: Expr | null; body: Stmt[] }
  | { type: "While"; cond: Expr; body: Stmt[] }
  | { type: "If"; cond: Expr; thenBody: Stmt[]; elseBody: Stmt[] }
  | { type: "Call"; name: string; args: Expr[] }
  | ({
      type: "Circle";
      center: Expr;
      radius: Expr;
      fill: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    } & ShapeFx)
  | ({
      type: "Ellipse";
      center: Expr;
      radius: Expr;
      fill: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    } & ShapeFx)
  | ({
      type: "Rectangle";
      pos: Expr;
      size: Expr;
      radius: Expr | null;
      fill: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    } & ShapeFx)
  | ({
      type: "Line";
      from: Expr;
      to: Expr;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    } & ShapeFx)
  | ({
      type: "Polygon";
      points: Expr[];
      fill: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    } & ShapeFx)
  | ({
      type: "Text";
      pos: Expr;
      content: Expr;
      size: Expr | null;
      font: Expr | null;
      align: Expr | null;
      fill: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    } & TextFx)
  | ({
      type: "Path";
      fill: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
      commands: PathCommandAst[];
    } & ShapeFx)
  | ({
      type: "Group";
      pos: Expr | null;
      rot: Expr | null;
      scale: Expr | null;
      opacity: Expr | null;
      fill: Expr | null;
      stroke: Expr | null;
      body: Stmt[];
    } & GroupFx)
  | { type: "Clip"; mask: Stmt; content: Stmt[] }
  /** Post-0.2 Section 18.3: pixel-art sprite (palette-indexed rows). */
  | {
      type: "Sprite";
      pos: Expr;
      palette: Expr[];
      rows: string[];
      scale: Expr | null;
      opacity: Expr | null;
      blend: Expr | null;
    }
  /** Post-0.2 Section 18.5: smooth Catmull-Rom spline through an array of points. */
  | (SplineFx & {
      type: "Spline";
      points: Expr;
      pos: Expr | null;
      size: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    });

/** Spline style surface (stroke topology + Section 10 FX; no fill, no align). */
export type SplineFx = Pick<ShapeFx, "cap" | "join" | "miter" | "dash" | "blur" | "shadow" | "glow" | "blend">;

/** Post-0.2 Section 18.1: host-overridable uniform (`param name: default`). */
export interface ParamDecl {
  name: string;
  default: Expr;
}

/** Post-0.2 Section 18.4: repeatable tile (`pattern name w h` + body block). */
export interface PatternDef {
  name: string;
  width: number;
  height: number;
  body: Stmt[];
}

export interface Document {
  version: [major: number, minor: number];
  canvas: CanvasDecl;
  params: ParamDecl[];
  patterns: PatternDef[];
  statements: Stmt[];
}

/** PVG 0.2 Section 9: evaluated gradient stop (offset clamped to [0, 1]). */
export interface GradientStop {
  offset: number;
  color: PvgColor;
}

/** PVG 0.2 Section 9: evaluated paint (solid color, world-space gradient, or pattern tile). */
export type Paint =
  | { kind: "color"; color: PvgColor }
  | { kind: "linear"; start: Vec2; end: Vec2; stops: GradientStop[] }
  | { kind: "radial"; center: Vec2; radius: number; focal: Vec2 | null; stops: GradientStop[] }
  | { kind: "angular"; center: Vec2; startAngle: number; stops: GradientStop[] }
  /** Post-0.2 Section 18.4: resolved via `DrawList.patterns`. */
  | { kind: "pattern"; name: string };

/** PVG 0.2 Section 10: evaluated drop shadow. */
export interface Shadow {
  offset: Vec2;
  radius: number;
  color: PvgColor;
}

/** PVG 0.2 Section 10: evaluated outer glow. */
export interface Glow {
  radius: number;
  color: PvgColor;
}

export interface DrawStyle {
  fill: Paint;
  stroke: Paint;
  width: number;
  opacity: number;
  cap: LineCap;
  join: LineJoin;
  miter: number;
  dash: number[];
  strokeAlign: StrokeAlign;
  blend: BlendMode;
  blur: number;
  shadow: Shadow | null;
  glow: Glow | null;
}

export type DrawPathCommand =
  | { cmd: "Start"; pt: Vec2 }
  | { cmd: "Line"; pt: Vec2 }
  | { cmd: "Quad"; cp: Vec2; ep: Vec2 }
  | { cmd: "Curve"; c1: Vec2; c2: Vec2; ep: Vec2 }
  | { cmd: "Arc"; center: Vec2; radius: number; startAngle: number; endAngle: number }
  | { cmd: "Close" };

export type DrawCmd =
  | { type: "Circle"; center: Vec2; radius: number; style: DrawStyle }
  | { type: "Ellipse"; center: Vec2; radius: Vec2; style: DrawStyle }
  | { type: "Rectangle"; pos: Vec2; size: Vec2; cornerRadius: number; style: DrawStyle }
  | { type: "Line"; from: Vec2; to: Vec2; style: DrawStyle }
  | { type: "Polygon"; points: Vec2[]; style: DrawStyle }
  | {
      type: "Text";
      pos: Vec2;
      content: string;
      size: number;
      fontFamily: string;
      align: TextAlign;
      style: DrawStyle;
    }
  | { type: "Path"; commands: DrawPathCommand[]; style: DrawStyle }
  | { type: "Clip"; mask: DrawCmd; content: DrawCmd[] }
  /** Post-0.2 Section 18.3: pixel-art sprite rasterized as crisp palette rects. */
  | {
      type: "Sprite";
      pos: Vec2;
      palette: PvgColor[];
      rows: string[];
      scale: number;
      style: DrawStyle;
    }
  /** Post-0.2 Section 18.5: smooth Catmull-Rom spline (stroke-only). */
  | { type: "Spline"; points: Vec2[]; style: DrawStyle };

/** Post-0.2 Section 18.4: evaluated repeatable pattern tile. */
export interface DrawPattern {
  name: string;
  width: number;
  height: number;
  tiles: DrawCmd[];
}

export interface DrawList {
  canvasWidth: number;
  canvasHeight: number;
  background: PvgColor | null;
  /** Post-0.2 Section 18.3: pixel snap grid in px (0 = off). */
  snap: number;
  /** Post-0.2 Section 18.3: pixel sampling hint for raster backends. */
  pixelFilter: PixelFilter;
  /** Post-0.2 Section 18.4: evaluated pattern tiles referenced by pattern paints. */
  patterns: DrawPattern[];
  items: DrawCmd[];
}

export interface RenderCanvasOptions {
  originX?: number;
  originY?: number;
  zoom?: number;
  clear?: boolean;
}

export interface AnimatedSvgOptions {
  duration?: number;
  fps?: number;
}
