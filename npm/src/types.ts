import type { PvgColor } from "./color.js";
import type { Transform2D } from "./transform.js";

export type Vec2 = [x: number, y: number];

export type TextAlign = "left" | "center" | "right";

export interface CanvasDecl {
  width: number;
  height: number;
  background: PvgColor | null;
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

export type Expr =
  | { type: "Number"; value: number }
  | { type: "String"; value: string }
  | { type: "Bool"; value: boolean }
  | { type: "Color"; value: PvgColor }
  | { type: "Vec2"; x: Expr; y: Expr }
  | { type: "Ident"; name: string }
  | { type: "Unary"; op: UnaryOp; inner: Expr }
  | { type: "Binary"; op: BinaryOp; left: Expr; right: Expr }
  | { type: "Ternary"; cond: Expr; trueBranch: Expr; falseBranch: Expr }
  | { type: "Call"; name: string; args: Expr[] };

export type PathCommandAst =
  | { cmd: "Set"; name: string; expr: Expr }
  | { cmd: "Start"; pt: Expr }
  | { cmd: "Line"; pt: Expr }
  | { cmd: "Quad"; cp: Expr; ep: Expr }
  | { cmd: "Curve"; c1: Expr; c2: Expr; ep: Expr }
  | { cmd: "Arc"; center: Expr; radius: Expr; startAngle: Expr; endAngle: Expr }
  | { cmd: "Close" };

export type Stmt =
  | { type: "Set"; name: string; expr: Expr }
  | { type: "Seed"; seed: number }
  | { type: "Def"; name: string; params: string[]; body: Stmt[] }
  | { type: "Return"; expr: Expr }
  | { type: "For"; var: string; from: Expr; to: Expr; step: Expr | null; body: Stmt[] }
  | { type: "While"; cond: Expr; body: Stmt[] }
  | { type: "If"; cond: Expr; thenBody: Stmt[]; elseBody: Stmt[] }
  | { type: "Call"; name: string; args: Expr[] }
  | {
      type: "Circle";
      center: Expr;
      radius: Expr;
      fill: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    }
  | {
      type: "Ellipse";
      center: Expr;
      radius: Expr;
      fill: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    }
  | {
      type: "Rectangle";
      pos: Expr;
      size: Expr;
      radius: Expr | null;
      fill: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    }
  | {
      type: "Line";
      from: Expr;
      to: Expr;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    }
  | {
      type: "Polygon";
      points: Expr[];
      fill: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
    }
  | {
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
    }
  | {
      type: "Path";
      fill: Expr | null;
      stroke: Expr | null;
      width: Expr | null;
      opacity: Expr | null;
      commands: PathCommandAst[];
    }
  | {
      type: "Group";
      pos: Expr | null;
      rot: Expr | null;
      scale: Expr | null;
      opacity: Expr | null;
      fill: Expr | null;
      stroke: Expr | null;
      body: Stmt[];
    };

export interface Document {
  version: [major: number, minor: number];
  canvas: CanvasDecl;
  statements: Stmt[];
}

export interface DrawStyle {
  fill: PvgColor;
  stroke: PvgColor;
  width: number;
  opacity: number;
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
  | { type: "Path"; commands: DrawPathCommand[]; style: DrawStyle };

export interface DrawList {
  canvasWidth: number;
  canvasHeight: number;
  background: PvgColor | null;
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