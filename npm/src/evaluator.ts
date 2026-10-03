import { PvgColor } from "./color.js";
import { Transform2D } from "./transform.js";
import type {
  BlendMode,
  Document,
  DrawCmd,
  DrawList,
  DrawPathCommand,
  DrawPattern,
  DrawStyle,
  Expr,
  Glow,
  GlowExpr,
  GradientStop,
  GradientStopExpr,
  LineCap,
  LineJoin,
  Paint,
  PathCommandAst,
  PixelFilter,
  Shadow,
  ShadowExpr,
  ShapeFx,
  Stmt,
  StrokeAlign,
  TextAlign,
  Vec2,
} from "./types.js";

/** Runtime value (dynamic typing, Section 5.1): number | string | bool | color | vec2 | array | paint | none. */
export type Value = number | string | boolean | PvgColor | Vec2 | Paint | null | Value[];

/** Maximum nested user-function call frames per evaluation (Section 15). */
export const MAX_CALL_STACK_DEPTH = 64;
/** Maximum top-level draw commands per evaluated scene (Section 15). */
export const MAX_SCENE_PRIMITIVES = 50_000;

function solidPaint(color: PvgColor): Paint {
  return { kind: "color", color };
}

// ---------------------------------------------------------------------------
// Deterministic value noise (Section 18.2) — bit-exact port of `pvg/src/eval.rs`.
// Rust uses u64 wrapping arithmetic, so BigInt is required for parity.
// ---------------------------------------------------------------------------

const MASK64 = 0xffffffffffffffffn;
const U64_MAX_F64 = 18446744073709551615;

function mulmod(a: bigint, b: bigint): bigint {
  return (a * b) & MASK64;
}

function hashLattice2d(ix: number, iy: number): number {
  let h = mulmod(BigInt.asUintN(64, BigInt(ix)), 0x9e3779b97f4a7c15n);
  h = (h ^ mulmod(BigInt.asUintN(64, BigInt(iy)), 0xbf58476d1ce4e5b9n)) & MASK64;
  h = (h ^ (h >> 30n)) & MASK64;
  h = mulmod(h, 0xbf58476d1ce4e5b9n);
  h = (h ^ (h >> 27n)) & MASK64;
  h = mulmod(h, 0x94d049bb133111ebn);
  h = (h ^ (h >> 31n)) & MASK64;
  return Number(h) / U64_MAX_F64;
}

function hashLattice3d(ix: number, iy: number, iz: number): number {
  let h = mulmod(BigInt.asUintN(64, BigInt(ix)), 0x9e3779b97f4a7c15n);
  h = (h ^ mulmod(BigInt.asUintN(64, BigInt(iy)), 0xbf58476d1ce4e5b9n)) & MASK64;
  h = (h ^ mulmod(BigInt.asUintN(64, BigInt(iz)), 0x94d049bb133111ebn)) & MASK64;
  h = (h ^ (h >> 30n)) & MASK64;
  h = mulmod(h, 0xbf58476d1ce4e5b9n);
  h = (h ^ (h >> 27n)) & MASK64;
  h = mulmod(h, 0x94d049bb133111ebn);
  h = (h ^ (h >> 31n)) & MASK64;
  return Number(h) / U64_MAX_F64;
}

function smooth(t: number): number {
  return t * t * (3.0 - 2.0 * t);
}

/** Deterministic 2D value noise in [-1, 1]. Pure function of its coordinates. */
export function pvgNoise2(x: number, y: number): number {
  if (!Number.isFinite(x) || !Number.isFinite(y)) return 0.0;
  const x0 = Math.floor(x);
  const y0 = Math.floor(y);
  const fx = x - x0;
  const fy = y - y0;
  const a = hashLattice2d(x0, y0);
  const b = hashLattice2d(x0 + 1, y0);
  const c = hashLattice2d(x0, y0 + 1);
  const d = hashLattice2d(x0 + 1, y0 + 1);
  const ux = smooth(fx);
  const uy = smooth(fy);
  const v = a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy;
  return v * 2.0 - 1.0;
}

/** Deterministic 3D value noise in [-1, 1]. */
export function pvgNoise3(x: number, y: number, z: number): number {
  if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z)) return 0.0;
  const x0 = Math.floor(x);
  const y0 = Math.floor(y);
  const z0 = Math.floor(z);
  const fx = x - x0;
  const fy = y - y0;
  const fz = z - z0;
  const ux = smooth(fx);
  const uy = smooth(fy);
  const uz = smooth(fz);
  const c000 = hashLattice3d(x0, y0, z0);
  const c100 = hashLattice3d(x0 + 1, y0, z0);
  const c010 = hashLattice3d(x0, y0 + 1, z0);
  const c110 = hashLattice3d(x0 + 1, y0 + 1, z0);
  const c001 = hashLattice3d(x0, y0, z0 + 1);
  const c101 = hashLattice3d(x0 + 1, y0, z0 + 1);
  const c011 = hashLattice3d(x0, y0 + 1, z0 + 1);
  const c111 = hashLattice3d(x0 + 1, y0 + 1, z0 + 1);
  const x00 = c000 + (c100 - c000) * ux;
  const x10 = c010 + (c110 - c010) * ux;
  const x01 = c001 + (c101 - c001) * ux;
  const x11 = c011 + (c111 - c011) * ux;
  const y0v = x00 + (x10 - x00) * uy;
  const y1v = x01 + (x11 - x01) * uy;
  const v = y0v + (y1v - y0v) * uz;
  return v * 2.0 - 1.0;
}

/**
 * Smooth Catmull-Rom spline through `points`, converted to cubic Bezier
 * segments `(c1, c2, endpoint)`. Port of `spline_to_bezier` in `pvg/src/eval.rs`.
 */
export function splineToBezier(points: Vec2[]): [Vec2, Vec2, Vec2][] {
  const n = points.length;
  if (n < 2) return [];
  if (n === 2) {
    // Straight line as a degenerate cubic (c1 == c2 == p0).
    return [[points[0], points[0], points[1]]];
  }
  const out: [Vec2, Vec2, Vec2][] = [];
  for (let i = 0; i < n - 1; i++) {
    const p0 = i === 0 ? points[0] : points[i - 1];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = i + 2 < n ? points[i + 2] : points[n - 1];
    const c1: Vec2 = [p1[0] + (p2[0] - p0[0]) / 6.0, p1[1] + (p2[1] - p0[1]) / 6.0];
    const c2: Vec2 = [p2[0] - (p3[0] - p1[0]) / 6.0, p2[1] - (p3[1] - p1[1]) / 6.0];
    out.push([c1, c2, p2]);
  }
  return out;
}

/** True when `v` is a nested bracket list (array-of-values, not a Vec2). */
function isArrayValue(v: Value): v is Value[] {
  return Array.isArray(v);
}

function clonePaint(p: Paint): Paint {
  if (p.kind === "color") {
    const c = p.color;
    return { kind: "color", color: new PvgColor(c.r, c.g, c.b, c.a, c.isNone) };
  }
  if (p.kind === "pattern") {
    return { kind: "pattern", name: p.name };
  }
  const stops: GradientStop[] = p.stops.map((s) => ({
    offset: s.offset,
    color: new PvgColor(s.color.r, s.color.g, s.color.b, s.color.a, s.color.isNone),
  }));
  if (p.kind === "linear") {
    return {
      kind: "linear",
      start: [p.start[0], p.start[1]],
      end: [p.end[0], p.end[1]],
      stops,
    };
  }
  if (p.kind === "radial") {
    return {
      kind: "radial",
      center: [p.center[0], p.center[1]],
      radius: p.radius,
      focal: p.focal ? [p.focal[0], p.focal[1]] : null,
      stops,
    };
  }
  return {
    kind: "angular",
    center: [p.center[0], p.center[1]],
    startAngle: p.startAngle,
    stops,
  };
}

function defaultStyle(): DrawStyle {
  return {
    fill: solidPaint(PvgColor.Black()),
    stroke: solidPaint(PvgColor.None()),
    width: 1.0,
    opacity: 1.0,
    // PVG 0.2 Section 8/10/12 defaults
    cap: "butt",
    join: "miter",
    miter: 4.0,
    dash: [],
    strokeAlign: "center",
    blend: "normal",
    blur: 0.0,
    shadow: null,
    glow: null,
  };
}

function cloneStyle(s: DrawStyle): DrawStyle {
  return {
    fill: clonePaint(s.fill),
    stroke: clonePaint(s.stroke),
    width: s.width,
    opacity: s.opacity,
    cap: s.cap,
    join: s.join,
    miter: s.miter,
    dash: [...s.dash],
    strokeAlign: s.strokeAlign,
    blend: s.blend,
    blur: s.blur,
    shadow: s.shadow
      ? {
          offset: [s.shadow.offset[0], s.shadow.offset[1]],
          radius: s.shadow.radius,
          color: new PvgColor(
            s.shadow.color.r,
            s.shadow.color.g,
            s.shadow.color.b,
            s.shadow.color.a,
            s.shadow.color.isNone
          ),
        }
      : null,
    glow: s.glow
      ? {
          radius: s.glow.radius,
          color: new PvgColor(
            s.glow.color.r,
            s.glow.color.g,
            s.glow.color.b,
            s.glow.color.a,
            s.glow.color.isNone
          ),
        }
      : null,
  };
}

export class Evaluator {
  private globals: Map<string, Value>;
  private functions = new Map<string, { params: string[]; body: Stmt[] }>();
  private rngState = 88172645463325252n;
  private loopLimit: number;
  private loopCount = 0;
  /** Current nested user-function call depth (guarded by MAX_CALL_STACK_DEPTH). */
  private callDepth = 0;
  private drawList: DrawCmd[] = [];
  private transformStack: Transform2D[] = [Transform2D.identity()];
  private styleStack: DrawStyle[] = [defaultStyle()];
  /** Pixel snap grid from `canvas snap` (0 = off), applied after the transform. */
  private snap = 0.0;
  /** Top-level pattern tile names valid for `fill pattern <name>` (Section 18.4). */
  private patternNames: string[] = [];

  constructor(time = 0.0, loopLimit = 100_000, seed = 88172645463325252n) {
    this.globals = new Map<string, Value>([
      ["PI", Math.PI],
      ["TAU", Math.PI * 2],
      ["time", time],
      ["t", time],
      // Helpers so organic-shape examples read naturally: `deg * deg_to_rad`.
      ["deg_to_rad", Math.PI / 180.0],
      ["rad_to_deg", 180.0 / Math.PI],
    ]);
    this.loopLimit = loopLimit;
    this.rngState = seed === 0n ? 88172645463325252n : seed;
  }

  /**
   * Host override for a declared `param` (Section 18.1). Values set here win over the
   * document's declared defaults for every subsequent evaluation.
   */
  setParam(name: string, value: Value): void {
    this.globals.set(name, value);
  }

  /** Clears a host override so the document default applies again. */
  clearParam(name: string): void {
    this.globals.delete(name);
  }

  /** Pushes one top-level draw command, enforcing the scene primitive budget (Section 15). */
  private pushDrawCmd(cmd: DrawCmd): void {
    if (this.drawList.length >= MAX_SCENE_PRIMITIVES) {
      throw new Error(`Exceeded scene primitive limit of ${MAX_SCENE_PRIMITIVES} draw commands`);
    }
    this.drawList.push(cmd);
  }

  private currentTransform(): Transform2D {
    return this.transformStack[this.transformStack.length - 1];
  }

  private currentStyle(): DrawStyle {
    return cloneStyle(this.styleStack[this.styleStack.length - 1]);
  }

  private nextRandom(): number {
    this.rngState ^= (this.rngState << 13n) & 0xffffffffffffffffn;
    this.rngState ^= (this.rngState >> 7n) & 0xffffffffffffffffn;
    this.rngState ^= (this.rngState << 17n) & 0xffffffffffffffffn;
    return Number(this.rngState & 0xffffffffffffffffn) / Number(0xffffffffffffffffn);
  }

  /** Rounds one coordinate to the active pixel grid (`canvas snap`, 0 = off). */
  private snapVal(v: number): number {
    if (this.snap > 0.0 && Number.isFinite(v)) {
      return Math.round(v / this.snap) * this.snap;
    }
    return v;
  }

  /** Rounds a point to the pixel grid after the world transform. */
  private snapPoint(p: Vec2): Vec2 {
    return [this.snapVal(p[0]), this.snapVal(p[1])];
  }

  /**
   * Evaluates a parsed document in the order the Rust engine does (Section 18.1/Section 18.4):
   * (1) snap + pattern names, (2) host uniforms, (3) pattern tiles, (4) body.
   */
  evaluateDocument(doc: Document): DrawList {
    this.snap = doc.canvas.snap;
    this.patternNames = doc.patterns.map((p) => p.name);

    // 1. Host uniforms: declared defaults apply unless the host already
    //    overrode them via `setParam` / `compileWithParams`.
    for (const param of doc.params) {
      if (!this.globals.has(param.name)) {
        this.globals.set(param.name, this.evalExpr(param.default, new Map()));
      }
    }

    // 2. Pattern tiles evaluated in isolation (identity transform, default
    //    style, fresh locals) so `fill pattern name` has resolved content.
    const evaluatedPatterns: DrawPattern[] = [];
    for (const pat of doc.patterns) {
      const savedDraw = this.drawList;
      const savedTrans = [...this.transformStack];
      const savedStyle = [...this.styleStack];
      this.drawList = [];
      this.transformStack = [Transform2D.identity()];
      this.styleStack = [defaultStyle()];
      const tileLocals = new Map<string, Value>();
      for (const stmt of pat.body) {
        this.evalStmt(stmt, tileLocals);
      }
      const tiles = this.drawList;
      this.drawList = savedDraw;
      this.transformStack = savedTrans;
      this.styleStack = savedStyle;
      evaluatedPatterns.push({ name: pat.name, width: pat.width, height: pat.height, tiles });
    }

    // 3. Main scene body.
    const bodyLocals = new Map<string, Value>();
    for (const stmt of doc.statements) {
      this.evalStmt(stmt, bodyLocals);
    }

    const pixelFilter: PixelFilter = doc.canvas.pixelFilter;
    return {
      canvasWidth: doc.canvas.width,
      canvasHeight: doc.canvas.height,
      background: doc.canvas.background,
      snap: doc.canvas.snap,
      pixelFilter,
      patterns: evaluatedPatterns,
      items: this.drawList,
    };
  }

  private evalStmt(stmt: Stmt, locals: Map<string, Value>): { isReturn: boolean; value: Value } | null {
    switch (stmt.type) {
      case "Set": {
        const val = this.evalExpr(stmt.expr, locals);
        if (locals.has(stmt.name)) {
          locals.set(stmt.name, val);
        } else {
          this.globals.set(stmt.name, val);
        }
        return null;
      }
      case "Seed": {
        // `seed 0` (or a non-numeric seed, normalized to 0 by the parser)
        // selects the engine default, mirroring the Rust core.
        const s = BigInt(stmt.seed);
        this.rngState = s === 0n ? 88172645463325252n : s;
        return null;
      }
      case "Def":
        this.functions.set(stmt.name, { params: stmt.params, body: stmt.body });
        return null;
      case "Return":
        return { isReturn: true, value: this.evalExpr(stmt.expr, locals) };
      case "For": {
        const startVal = this.asNumber(this.evalExpr(stmt.from, locals));
        const endVal = this.asNumber(this.evalExpr(stmt.to, locals));
        const stepVal = stmt.step
          ? this.asNumber(this.evalExpr(stmt.step, locals))
          : endVal >= startVal ? 1.0 : -1.0;

        if (stepVal === 0.0) throw new Error("For loop step cannot be 0");

        let current = startVal;
        while ((stepVal > 0.0 && current <= endVal) || (stepVal < 0.0 && current >= endVal)) {
          this.loopCount++;
          if (this.loopCount > this.loopLimit) {
            throw new Error(`Exceeded safety loop limit of ${this.loopLimit} iterations`);
          }
          locals.set(stmt.var, current);
          for (const bStmt of stmt.body) {
            const ret = this.evalStmt(bStmt, locals);
            if (ret && ret.isReturn) return ret;
          }
          current += stepVal;
        }
        return null;
      }
      case "While": {
        while (this.isTruthy(this.evalExpr(stmt.cond, locals))) {
          this.loopCount++;
          if (this.loopCount > this.loopLimit) {
            throw new Error(`Exceeded safety loop limit of ${this.loopLimit} iterations`);
          }
          for (const bStmt of stmt.body) {
            const ret = this.evalStmt(bStmt, locals);
            if (ret && ret.isReturn) return ret;
          }
        }
        return null;
      }
      case "If": {
        if (this.isTruthy(this.evalExpr(stmt.cond, locals))) {
          for (const bStmt of stmt.thenBody) {
            const ret = this.evalStmt(bStmt, locals);
            if (ret && ret.isReturn) return ret;
          }
        } else {
          for (const bStmt of stmt.elseBody) {
            const ret = this.evalStmt(bStmt, locals);
            if (ret && ret.isReturn) return ret;
          }
        }
        return null;
      }
      case "Call": {
        const evalArgs = stmt.args.map((a) => this.evalExpr(a, locals));
        this.invokeFunction(stmt.name, evalArgs);
        return null;
      }
      case "Circle": {
        const centerRaw = this.asVec2(this.evalExpr(stmt.center, locals));
        const radius = this.asNumber(this.evalExpr(stmt.radius, locals));
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        const center = this.snapPoint(this.currentTransform().transformPoint(centerRaw));
        this.pushDrawCmd({ type: "Circle", center, radius, style });
        return null;
      }
      case "Ellipse": {
        const centerRaw = this.asVec2(this.evalExpr(stmt.center, locals));
        const radiusRaw = this.asVec2(this.evalExpr(stmt.radius, locals));
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        const center = this.snapPoint(this.currentTransform().transformPoint(centerRaw));
        this.pushDrawCmd({ type: "Ellipse", center, radius: radiusRaw, style });
        return null;
      }
      case "Rectangle": {
        const posRaw = this.asVec2(this.evalExpr(stmt.pos, locals));
        const sizeRaw = this.asVec2(this.evalExpr(stmt.size, locals));
        const cornerRadius = stmt.radius ? this.asNumber(this.evalExpr(stmt.radius, locals)) : 0.0;
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        const pos = this.snapPoint(this.currentTransform().transformPoint(posRaw));
        this.pushDrawCmd({ type: "Rectangle", pos, size: sizeRaw, cornerRadius, style });
        return null;
      }
      case "Line": {
        const fromRaw = this.asVec2(this.evalExpr(stmt.from, locals));
        const toRaw = this.asVec2(this.evalExpr(stmt.to, locals));
        const style = this.currentStyle();
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        const trans = this.currentTransform();
        this.pushDrawCmd({
          type: "Line",
          from: this.snapPoint(trans.transformPoint(fromRaw)),
          to: this.snapPoint(trans.transformPoint(toRaw)),
          style,
        });
        return null;
      }
      case "Polygon": {
        const trans = this.currentTransform();
        const points = stmt.points.map((p) => this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(p, locals)))));
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        this.pushDrawCmd({ type: "Polygon", points, style });
        return null;
      }
      case "Text": {
        const posRaw = this.asVec2(this.evalExpr(stmt.pos, locals));
        const content = this.asString(this.evalExpr(stmt.content, locals));
        const size = stmt.size ? this.asNumber(this.evalExpr(stmt.size, locals)) : 16.0;
        const fontFamily = stmt.font ? this.asString(this.evalExpr(stmt.font, locals)) : "sans-serif";
        let align: TextAlign = "left";
        if (stmt.align) {
          const a = this.asString(this.evalExpr(stmt.align, locals)).toLowerCase();
          if (a === "center") align = "center";
          else if (a === "right") align = "right";
          else align = "left";
        }

        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals, true);

        const pos = this.snapPoint(this.currentTransform().transformPoint(posRaw));
        this.pushDrawCmd({
          type: "Text",
          pos,
          content,
          size,
          fontFamily,
          align,
          style,
        });
        return null;
      }
      case "Path": {
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        const trans = this.currentTransform();
        const drawCommands: DrawPathCommand[] = [];
        const pathLocals = new Map(locals);

        this.evalPathCommands(stmt.commands, pathLocals, trans, drawCommands, locals);

        this.pushDrawCmd({ type: "Path", commands: drawCommands, style });
        return null;
      }
      case "Group": {
        let localTrans = Transform2D.identity();
        if (stmt.pos) {
          const [tx, ty] = this.asVec2(this.evalExpr(stmt.pos, locals));
          localTrans.tx = tx;
          localTrans.ty = ty;
        }
        if (stmt.rot) {
          const angle = this.asNumber(this.evalExpr(stmt.rot, locals));
          const cos = Math.cos(angle);
          const sin = Math.sin(angle);
          localTrans = localTrans.mul(new Transform2D(cos, sin, -sin, cos, 0, 0));
        }
        if (stmt.scale) {
          const [sx, sy] = this.asVec2(this.evalExpr(stmt.scale, locals));
          localTrans = localTrans.mul(new Transform2D(sx, 0, 0, sy, 0, 0));
        }

        const newTrans = this.currentTransform().mul(localTrans);
        this.transformStack.push(newTrans);

        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        if (stmt.blend) style.blend = this.evalBlend(this.evalExpr(stmt.blend, locals));
        if (stmt.blur) style.blur = Math.max(0, this.asNumber(this.evalExpr(stmt.blur, locals)));
        if (stmt.shadow) style.shadow = this.evalShadow(stmt.shadow, locals);
        if (stmt.glow) style.glow = this.evalGlow(stmt.glow, locals);
        this.styleStack.push(style);

        for (const bStmt of stmt.body) {
          this.evalStmt(bStmt, locals);
        }

        this.styleStack.pop();
        this.transformStack.pop();
        return null;
      }
      case "Sprite": {
        // Section 18.3: palette-indexed pixel art; ignores the inherited fill/stroke.
        const posRaw = this.asVec2(this.evalExpr(stmt.pos, locals));
        const palette: PvgColor[] = [];
        for (const entry of stmt.palette) {
          palette.push(this.asColor(this.evalExpr(entry, locals)));
        }
        const scale = stmt.scale ? Math.max(0.01, this.asNumber(this.evalExpr(stmt.scale, locals))) : 1.0;
        const style = this.currentStyle();
        style.fill = solidPaint(PvgColor.White());
        style.stroke = solidPaint(PvgColor.None());
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        if (stmt.blend) style.blend = this.evalBlend(this.evalExpr(stmt.blend, locals));

        const pos = this.snapPoint(this.currentTransform().transformPoint(posRaw));
        this.pushDrawCmd({ type: "Sprite", pos, palette, rows: stmt.rows, scale, style });
        return null;
      }
      case "Spline": {
        // Section 18.5: Catmull-Rom data spline (stroke-only).
        const raw = this.evalExpr(stmt.points, locals);
        const items: Value[] = isArrayValue(raw) ? raw : [];
        const ctrl: Vec2[] = [];
        const hasVec = items.some((v) => Array.isArray(v));
        if (hasVec) {
          for (const v of items) {
            if (Array.isArray(v)) {
              if (v.length === 2 && typeof v[0] === "number" && typeof v[1] === "number") {
                ctrl.push([v[0], v[1]]);
              } else {
                throw new Error("Spline points must be [x, y] vectors or numbers.");
              }
            } else if (typeof v === "number") {
              ctrl.push([ctrl.length, v]);
            } else {
              throw new Error("Spline points must be [x, y] vectors or numbers.");
            }
          }
        } else {
          const n = items.length;
          if (n === 0) throw new Error("Spline requires at least one point.");
          const nums = items.map((v) => this.asNumber(v));
          const [ox, oy] = stmt.pos
            ? this.asVec2(this.evalExpr(stmt.pos, locals))
            : ([0.0, 0.0] as Vec2);
          const [sw, sh] = stmt.size
            ? this.asVec2(this.evalExpr(stmt.size, locals))
            : ([Math.max(2, n) - 1, 1] as Vec2);
          let lo = Infinity;
          let hi = -Infinity;
          for (const v of nums) {
            lo = Math.min(lo, v);
            hi = Math.max(hi, v);
          }
          if (!(hi > lo)) hi = lo + 1.0;
          for (let i = 0; i < n; i++) {
            // y is down-positive: the MAX value sits at the top.
            const t = n > 1 ? i / (n - 1) : 0.0;
            const y = oy + (1.0 - (nums[i] - lo) / (hi - lo)) * sh;
            ctrl.push([ox + t * sw, y]);
          }
        }
        if (ctrl.length === 1) ctrl.push([ctrl[0][0], ctrl[0][1]]);

        const style = this.currentStyle();
        style.stroke = stmt.stroke
          ? this.asPaint(this.evalExpr(stmt.stroke, locals))
          : solidPaint(PvgColor.White());
        style.fill = solidPaint(PvgColor.None());
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        const trans = this.currentTransform();
        const points: Vec2[] = ctrl.map((p) => this.snapPoint(trans.transformPoint(p)));
        this.pushDrawCmd({ type: "Spline", points, style });
        return null;
      }
      case "Clip": {
        // Evaluate mask in isolation (side-effect free), then content normally.
        const base = this.drawList.length;
        const scratch = new Map(locals);
        this.evalStmt(stmt.mask, scratch);
        if (this.drawList.length !== base + 1) {
          this.drawList.length = base;
          throw new Error("Clip mask must produce exactly one shape.");
        }
        const mask = this.drawList.pop()!;
        const contentBase = this.drawList.length;
        for (const bStmt of stmt.content) {
          this.evalStmt(bStmt, locals);
        }
        const content = this.drawList.splice(contentBase);
        this.pushDrawCmd({ type: "Clip", mask, content });
        return null;
      }
    }
  }

  /**
   * Post-0.2 Section 18.6: evaluates `path` body items — geometry commands, `set`
   * (shared with the enclosing scope), and `for`/`while`/`if` control flow that
   * shares the path's locals. Mirrors `eval_path_command` in the Rust core.
   */
  private evalPathCommands(
    commands: PathCommandAst[],
    pathLocals: Map<string, Value>,
    trans: Transform2D,
    drawCommands: DrawPathCommand[],
    locals: Map<string, Value>
  ): void {
    for (const cmd of commands) {
      switch (cmd.cmd) {
        case "Set": {
          const val = this.evalExpr(cmd.expr, pathLocals);
          pathLocals.set(cmd.name, val);
          locals.set(cmd.name, val);
          break;
        }
        case "Start": {
          const pt = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.pt, pathLocals))));
          drawCommands.push({ cmd: "Start", pt });
          break;
        }
        case "Line": {
          const pt = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.pt, pathLocals))));
          drawCommands.push({ cmd: "Line", pt });
          break;
        }
        case "Quad": {
          const cp = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.cp, pathLocals))));
          const ep = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.ep, pathLocals))));
          drawCommands.push({ cmd: "Quad", cp, ep });
          break;
        }
        case "Curve": {
          const c1 = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.c1, pathLocals))));
          const c2 = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.c2, pathLocals))));
          const ep = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.ep, pathLocals))));
          drawCommands.push({ cmd: "Curve", c1, c2, ep });
          break;
        }
        case "Arc": {
          const center = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.center, pathLocals))));
          const radius = this.asNumber(this.evalExpr(cmd.radius, pathLocals));
          const startAngle = this.asNumber(this.evalExpr(cmd.startAngle, pathLocals));
          const endAngle = this.asNumber(this.evalExpr(cmd.endAngle, pathLocals));
          drawCommands.push({ cmd: "Arc", center, radius, startAngle, endAngle });
          break;
        }
        case "Close":
          drawCommands.push({ cmd: "Close" });
          break;
        case "For": {
          const startVal = this.asNumber(this.evalExpr(cmd.from, pathLocals));
          const endVal = this.asNumber(this.evalExpr(cmd.to, pathLocals));
          const stepVal = cmd.step
            ? this.asNumber(this.evalExpr(cmd.step, pathLocals))
            : endVal >= startVal
              ? 1.0
              : -1.0;
          if (stepVal === 0.0) throw new Error("For loop step cannot be 0");
          let current = startVal;
          while ((stepVal > 0.0 && current <= endVal) || (stepVal < 0.0 && current >= endVal)) {
            this.loopCount++;
            if (this.loopCount > this.loopLimit) {
              throw new Error(`Exceeded safety loop limit of ${this.loopLimit} iterations`);
            }
            pathLocals.set(cmd.varName, current);
            this.evalPathCommands(cmd.body, pathLocals, trans, drawCommands, locals);
            current += stepVal;
          }
          break;
        }
        case "While": {
          while (this.isTruthy(this.evalExpr(cmd.cond, pathLocals))) {
            this.loopCount++;
            if (this.loopCount > this.loopLimit) {
              throw new Error(`Exceeded safety loop limit of ${this.loopLimit} iterations`);
            }
            this.evalPathCommands(cmd.body, pathLocals, trans, drawCommands, locals);
          }
          break;
        }
        case "If": {
          const body = this.isTruthy(this.evalExpr(cmd.cond, pathLocals)) ? cmd.thenBody : cmd.elseBody;
          this.evalPathCommands(body, pathLocals, trans, drawCommands, locals);
          break;
        }
      }
    }
  }

  // ---- PVG 0.2 style helpers ----
  private evalCap(v: Value): LineCap {
    const s = String(this.asString(v)).toLowerCase();
    if (s === "round") return "round";
    if (s === "square") return "square";
    return "butt";
  }

  private evalJoin(v: Value): LineJoin {
    const s = String(this.asString(v)).toLowerCase();
    if (s === "round") return "round";
    if (s === "bevel") return "bevel";
    return "miter";
  }

  private evalStrokeAlign(v: Value): StrokeAlign {
    const s = String(this.asString(v)).toLowerCase();
    if (s === "inside") return "inside";
    if (s === "outside") return "outside";
    return "center";
  }

  private evalBlend(v: Value): BlendMode {
    const s = String(this.asString(v)).toLowerCase();
    if (s === "add") return "add";
    if (s === "multiply") return "multiply";
    if (s === "screen") return "screen";
    if (s === "overlay") return "overlay";
    return "normal";
  }

  private evalDash(items: Expr[] | null, locals: Map<string, Value>): number[] {
    const out: number[] = [];
    for (const d of items || []) {
      const v = this.asNumber(this.evalExpr(d, locals));
      if (v > 0 && Number.isFinite(v)) out.push(v);
    }
    return out;
  }

  private evalShadow(node: ShadowExpr, locals: Map<string, Value>): Shadow {
    const offset = this.asVec2(this.evalExpr(node.offset, locals));
    const radius = Math.max(0, this.asNumber(this.evalExpr(node.radius, locals)));
    const color = this.asColor(this.evalExpr(node.color, locals));
    return { offset, radius, color };
  }

  private evalGlow(node: GlowExpr, locals: Map<string, Value>): Glow {
    const radius = Math.max(0, this.asNumber(this.evalExpr(node.radius, locals)));
    const color = this.asColor(this.evalExpr(node.color, locals));
    return { radius, color };
  }

  private applyFxProps(
    style: DrawStyle,
    stmt: Partial<ShapeFx>,
    locals: Map<string, Value>,
    isText = false
  ): void {
    if (stmt.cap && !isText) style.cap = this.evalCap(this.evalExpr(stmt.cap, locals));
    if (stmt.join && !isText) style.join = this.evalJoin(this.evalExpr(stmt.join, locals));
    if (stmt.miter && !isText) style.miter = Math.max(1, this.asNumber(this.evalExpr(stmt.miter, locals)));
    if (stmt.dash && !isText) style.dash = this.evalDash(stmt.dash, locals);
    if (stmt.align && !isText) style.strokeAlign = this.evalStrokeAlign(this.evalExpr(stmt.align, locals));
    if (stmt.blur) style.blur = Math.max(0, this.asNumber(this.evalExpr(stmt.blur, locals)));
    if (stmt.shadow) style.shadow = this.evalShadow(stmt.shadow, locals);
    if (stmt.glow) style.glow = this.evalGlow(stmt.glow, locals);
    if (stmt.blend) style.blend = this.evalBlend(this.evalExpr(stmt.blend, locals));
  }

  private invokeFunction(name: string, args: Value[]): Value {
    if (this.callDepth >= MAX_CALL_STACK_DEPTH) {
      throw new Error(`Exceeded call stack limit of ${MAX_CALL_STACK_DEPTH} frames`);
    }
    const func = this.functions.get(name);
    if (!func) throw new Error(`Undefined function '${name}'`);
    if (func.params.length !== args.length) {
      throw new Error(`Function '${name}' expects ${func.params.length} arguments, got ${args.length}`);
    }

    const locals = new Map<string, Value>();
    for (let i = 0; i < func.params.length; i++) {
      locals.set(func.params[i], args[i]);
    }

    // Depth is released on every exit path (return value or error).
    this.callDepth++;
    try {
      for (const stmt of func.body) {
        const ret = this.evalStmt(stmt, locals);
        if (ret && ret.isReturn) return ret.value;
      }
      return null;
    } finally {
      this.callDepth--;
    }
  }

  private evalExpr(expr: Expr, locals: Map<string, Value>): Value {
    switch (expr.type) {
      case "Number": return expr.value;
      case "String": return expr.value;
      case "Bool": return expr.value;
      case "Color": return expr.value;
      case "Vec2": {
        const x = this.asNumber(this.evalExpr(expr.x, locals));
        const y = this.asNumber(this.evalExpr(expr.y, locals));
        return [x, y];
      }
      case "Array": {
        const out: Value[] = [];
        for (const item of expr.items) out.push(this.evalExpr(item, locals));
        return out;
      }
      case "Pattern": {
        if (!this.patternNames.includes(expr.name)) {
          throw new Error(`Unknown pattern '${expr.name}'`);
        }
        return { kind: "pattern", name: expr.name } satisfies Paint;
      }
      case "Ident": {
        if (locals.has(expr.name)) return locals.get(expr.name)!;
        if (this.globals.has(expr.name)) return this.globals.get(expr.name)!;
        throw new Error(`Undefined variable '${expr.name}'`);
      }
      case "Unary": {
        const op = expr.op;
        const v = this.evalExpr(expr.inner, locals);
        if (op === "neg") return -this.asNumber(v);
        if (op === "not") return !this.isTruthy(v);
        throw new Error(`Unknown unary operator '${String(op)}'`);
      }
      case "Binary": {
        const op = expr.op;
        const l = this.evalExpr(expr.left, locals);
        const r = this.evalExpr(expr.right, locals);
        switch (op) {
          case "+": {
            if (typeof l === "string" || typeof r === "string") {
              return `${this.displayValue(l)}${this.displayValue(r)}`;
            }
            return this.asNumber(l) + this.asNumber(r);
          }
          case "-": return this.asNumber(l) - this.asNumber(r);
          case "*": return this.asNumber(l) * this.asNumber(r);
          case "/": {
            const denom = this.asNumber(r);
            return denom === 0.0 ? 0.0 : this.asNumber(l) / denom;
          }
          case "%": return this.asNumber(l) % this.asNumber(r);
          case "^": return Math.pow(this.asNumber(l), this.asNumber(r));
          // Equality and relational operators coerce via asNumber (numbers and
          // bools only, mirroring Rust `as_f64`); any other type is a runtime
          // error — e.g. `"a" == "a"` fails in both engines.
          case "==": return this.asNumber(l) === this.asNumber(r);
          case "!=": return this.asNumber(l) !== this.asNumber(r);
          case "<": return this.asNumber(l) < this.asNumber(r);
          case "<=": return this.asNumber(l) <= this.asNumber(r);
          case ">": return this.asNumber(l) > this.asNumber(r);
          case ">=": return this.asNumber(l) >= this.asNumber(r);
          case "and": return this.isTruthy(l) && this.isTruthy(r);
          case "or": return this.isTruthy(l) || this.isTruthy(r);
          default:
            throw new Error(`Unknown binary operator '${String(op)}'`);
        }
      }
      case "Ternary":
        return this.isTruthy(this.evalExpr(expr.cond, locals))
          ? this.evalExpr(expr.trueBranch, locals)
          : this.evalExpr(expr.falseBranch, locals);
      case "Call": {
        const args = expr.args.map((a) => this.evalExpr(a, locals));
        switch (expr.name) {
          case "sin": return Math.sin(this.asNumber(args[0]));
          case "cos": return Math.cos(this.asNumber(args[0]));
          case "tan": return Math.tan(this.asNumber(args[0]));
          case "sqrt": return Math.sqrt(this.asNumber(args[0]));
          case "abs": return Math.abs(this.asNumber(args[0]));
          case "floor": return Math.floor(this.asNumber(args[0]));
          case "ceil": return Math.ceil(this.asNumber(args[0]));
          case "round": return Math.round(this.asNumber(args[0]));
          case "min": return Math.min(this.asNumber(args[0]), this.asNumber(args[1]));
          case "max": return Math.max(this.asNumber(args[0]), this.asNumber(args[1]));
          case "pow": return Math.pow(this.asNumber(args[0]), this.asNumber(args[1]));
          case "radians": return (this.asNumber(args[0]) * Math.PI) / 180.0;
          case "degrees": return (this.asNumber(args[0]) * 180.0) / Math.PI;
          case "deg_to_rad": return (this.asNumber(args[0]) * Math.PI) / 180.0;
          case "rgb": {
            if (args.length !== 3) throw new Error("rgb(r, g, b) needs 3 arguments");
            return new PvgColor(this.colorChannel(args[0]), this.colorChannel(args[1]), this.colorChannel(args[2]), 255);
          }
          case "rgba": {
            if (args.length !== 4) throw new Error("rgba(r, g, b, a) needs 4 arguments");
            const av = this.asNumber(args[3]);
            const ab = Number.isNaN(av) ? 0 : Math.round(Math.max(0, Math.min(1, av)) * 255);
            return new PvgColor(this.colorChannel(args[0]), this.colorChannel(args[1]), this.colorChannel(args[2]), ab);
          }
          case "noise2d": {
            if (args.length !== 2) throw new Error("noise2d(x, y) needs 2 arguments");
            return pvgNoise2(this.asNumber(args[0]), this.asNumber(args[1]));
          }
          case "noise3d": {
            if (args.length !== 3) throw new Error("noise3d(x, y, z) needs 3 arguments");
            return pvgNoise3(this.asNumber(args[0]), this.asNumber(args[1]), this.asNumber(args[2]));
          }
          case "array":
            return args;
          case "len": {
            if (args.length === 0) throw new Error("len(arr) needs 1 argument");
            const v = args[0];
            if (isArrayValue(v)) return v.length;
            if (typeof v === "string") return [...v].length;
            throw new Error("len() expects an array or string");
          }
          case "get": {
            if (args.length !== 2) throw new Error("get(arr, i) needs 2 arguments");
            const idx = Math.trunc(this.asNumber(args[1]));
            const target = args[0];
            if (isArrayValue(target)) {
              const n = target.length;
              if (n === 0) throw new Error("get() from empty array");
              // Negative indices wrap (Python-style).
              return target[(((idx % n) + n) % n) | 0];
            }
            throw new Error("get() expects an array");
          }
          case "random": {
            const min = this.asNumber(args[0]);
            const max = this.asNumber(args[1]);
            const r = this.nextRandom();
            return min + r * (max - min);
          }
          default:
            return this.invokeFunction(expr.name, args);
        }
      }
      case "Linear": {
        const s = this.asVec2(this.evalExpr(expr.start, locals));
        const e = this.asVec2(this.evalExpr(expr.end, locals));
        const trans = this.currentTransform();
        const paint: Paint = {
          kind: "linear",
          start: trans.transformPoint(s),
          end: trans.transformPoint(e),
          stops: this.evalStops(expr.stops, locals),
        };
        return paint;
      }
      case "Radial": {
        const c = this.asVec2(this.evalExpr(expr.center, locals));
        const r = Math.max(0, this.asNumber(this.evalExpr(expr.radius, locals)));
        const trans = this.currentTransform();
        const f = expr.focal
          ? trans.transformPoint(this.asVec2(this.evalExpr(expr.focal, locals)))
          : null;
        const paint: Paint = {
          kind: "radial",
          center: trans.transformPoint(c),
          radius: r,
          focal: f,
          stops: this.evalStops(expr.stops, locals),
        };
        return paint;
      }
      case "Angular": {
        const c = this.asVec2(this.evalExpr(expr.center, locals));
        const sa = this.asNumber(this.evalExpr(expr.startAngle, locals));
        const trans = this.currentTransform();
        const paint: Paint = {
          kind: "angular",
          center: trans.transformPoint(c),
          startAngle: sa,
          stops: this.evalStops(expr.stops, locals),
        };
        return paint;
      }
      default:
        throw new Error(`Unknown expression type '${(expr as Expr).type}'`);
    }
  }

  private evalStops(stops: GradientStopExpr[], locals: Map<string, Value>): GradientStop[] {
    // Offsets clamped to [0,1]; colors incl. transparent alpha preserved (Section 9.4).
    // Sorted by offset at evaluation (stable).
    const out = (stops || []).map((s) => ({
      offset: Math.max(0, Math.min(1, this.asNumber(this.evalExpr(s.offset, locals)))),
      color: this.asColor(this.evalExpr(s.color, locals)),
    }));
    out.sort((a, b) => a.offset - b.offset);
    return out;
  }

  private asNumber(val: Value): number {
    if (typeof val === "number") return val;
    if (typeof val === "boolean") return val ? 1.0 : 0.0;
    throw new Error(`Expected number, got ${JSON.stringify(val)}`);
  }

  /**
   * Rounds a color channel to the nearest integer and clamps it to [0, 255]
   * (Section 2.6 functional `rgb()` / `rgba()` form). Mirrors the Rust core's
   * `color_channel` (round-then-clamp; NaN maps to 0 via an explicit guard,
   * matching Rust's saturating float-to-int cast).
   */
  private colorChannel(val: Value): number {
    const rounded = Math.round(this.asNumber(val));
    if (Number.isNaN(rounded)) return 0;
    return Math.max(0, Math.min(255, rounded));
  }

  private asString(val: Value): string {
    return this.displayValue(val);
  }

  /**
   * String display conversion for `+` concatenation and text content.
   * Mirrors Rust `Value::as_string`: strings pass through, numbers use integer
   * formatting when integral and |n| < 1e15, bools print as `true`/`false`,
   * arrays render recursively as `[a, b]`; colors, paints, vectors-as-values
   * and `None` are runtime errors (never silent `[object Object]` output).
   * (Note: plain `[x, y]` vectors are JS arrays here and format as arrays —
   * the only accepted deviation from Rust, which rejects vectors.)
   */
  private displayValue(val: Value): string {
    if (typeof val === "string") return val;
    if (typeof val === "number") {
      if (Number.isNaN(val)) return "NaN";
      if (val === Infinity) return "inf";
      if (val === -Infinity) return "-inf";
      if (Object.is(val, -0)) return "-0";
      if (Number.isInteger(val) && Math.abs(val) < 1e15) return String(Math.trunc(val));
      return String(val);
    }
    if (typeof val === "boolean") return val ? "true" : "false";
    if (Array.isArray(val)) {
      const parts = (val as Value[]).map((v) => {
        try {
          return this.displayValue(v);
        } catch {
          return "?";
        }
      });
      return `[${parts.join(", ")}]`;
    }
    throw new Error("Expected string or displayable value");
  }

  private asVec2(val: Value): Vec2 {
    if (Array.isArray(val) && val.length === 2 && typeof val[0] === "number" && typeof val[1] === "number") {
      return val as Vec2;
    }
    throw new Error(`Expected [x, y] vector, got ${JSON.stringify(val)}`);
  }

  private asColor(val: Value): PvgColor {
    if (val instanceof PvgColor) return val;
    if (val !== null && typeof val === "object" && "kind" in val) {
      const paint = val as Paint;
      if (paint.kind === "color") return paint.color;
    }
    throw new Error("Expected color value");
  }

  private asPaint(val: Value): Paint {
    if (val instanceof PvgColor) return solidPaint(val);
    if (val !== null && typeof val === "object" && "kind" in val) {
      const paint = val as Paint;
      if (
        paint.kind === "color" ||
        paint.kind === "linear" ||
        paint.kind === "radial" ||
        paint.kind === "angular" ||
        paint.kind === "pattern"
      ) {
        return paint;
      }
    }
    throw new Error("Expected paint (color, gradient or pattern) value");
  }

  private isTruthy(val: Value): boolean {
    if (typeof val === "boolean") return val;
    if (typeof val === "number") return val !== 0.0;
    if (typeof val === "string") return val.length > 0;
    // Empty data arrays are falsy (Rust parity); a 2-element vector array is
    // never empty so vectors stay truthy, matching Rust.
    if (Array.isArray(val)) return val.length > 0;
    return val != null;
  }
}
