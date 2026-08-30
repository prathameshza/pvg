import { PvgColor } from "./color.js";
import { Transform2D } from "./transform.js";
import type {
  Document,
  DrawCmd,
  DrawList,
  DrawPathCommand,
  DrawStyle,
  Expr,
  Stmt,
  TextAlign,
  Vec2,
} from "./types.js";

type Value = number | string | boolean | PvgColor | Vec2 | null;

export class Evaluator {
  private globals: Map<string, Value>;
  private functions = new Map<string, { params: string[]; body: Stmt[] }>();
  private rngState = 88172645463325252n;
  private loopLimit: number;
  private loopCount = 0;
  private drawList: DrawCmd[] = [];
  private transformStack: Transform2D[] = [Transform2D.identity()];
  private styleStack: DrawStyle[] = [
    {
      fill: PvgColor.Black(),
      stroke: PvgColor.None(),
      width: 1.0,
      opacity: 1.0,
    },
  ];

  constructor(time = 0.0, loopLimit = 100_000, seed = 88172645463325252n) {
    this.globals = new Map<string, Value>([
      ["PI", Math.PI],
      ["TAU", Math.PI * 2.0],
      ["time", time],
      ["t", time],
    ]);
    this.loopLimit = loopLimit;
    this.rngState = seed === 0n ? 88172645463325252n : seed;
  }

  private currentTransform(): Transform2D {
    return this.transformStack[this.transformStack.length - 1];
  }

  private currentStyle(): DrawStyle {
    const s = this.styleStack[this.styleStack.length - 1];
    return {
      fill: new PvgColor(s.fill.r, s.fill.g, s.fill.b, s.fill.a, s.fill.isNone),
      stroke: new PvgColor(s.stroke.r, s.stroke.g, s.stroke.b, s.stroke.a, s.stroke.isNone),
      width: s.width,
      opacity: s.opacity,
    };
  }

  private nextRandom(): number {
    this.rngState ^= (this.rngState << 13n) & 0xffffffffffffffffn;
    this.rngState ^= (this.rngState >> 7n) & 0xffffffffffffffffn;
    this.rngState ^= (this.rngState << 17n) & 0xffffffffffffffffn;
    return Number(this.rngState & 0xffffffffffffffffn) / Number(0xffffffffffffffffn);
  }

  evaluateDocument(doc: Document): DrawList {
    for (const stmt of doc.statements) {
      this.evalStmt(stmt, new Map());
    }

    return {
      canvasWidth: doc.canvas.width,
      canvasHeight: doc.canvas.height,
      background: doc.canvas.background,
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
        const s = BigInt(stmt.seed || 42);
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
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const center = this.currentTransform().transformPoint(centerRaw);
        this.drawList.push({ type: "Circle", center, radius, style });
        return null;
      }
      case "Ellipse": {
        const centerRaw = this.asVec2(this.evalExpr(stmt.center, locals));
        const radiusRaw = this.asVec2(this.evalExpr(stmt.radius, locals));
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const center = this.currentTransform().transformPoint(centerRaw);
        this.drawList.push({ type: "Ellipse", center, radius: radiusRaw, style });
        return null;
      }
      case "Rectangle": {
        const posRaw = this.asVec2(this.evalExpr(stmt.pos, locals));
        const sizeRaw = this.asVec2(this.evalExpr(stmt.size, locals));
        const cornerRadius = stmt.radius ? this.asNumber(this.evalExpr(stmt.radius, locals)) : 0.0;
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const pos = this.currentTransform().transformPoint(posRaw);
        this.drawList.push({ type: "Rectangle", pos, size: sizeRaw, cornerRadius, style });
        return null;
      }
      case "Line": {
        const fromRaw = this.asVec2(this.evalExpr(stmt.from, locals));
        const toRaw = this.asVec2(this.evalExpr(stmt.to, locals));
        const style = this.currentStyle();
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const trans = this.currentTransform();
        this.drawList.push({
          type: "Line",
          from: trans.transformPoint(fromRaw),
          to: trans.transformPoint(toRaw),
          style,
        });
        return null;
      }
      case "Polygon": {
        const trans = this.currentTransform();
        const points = stmt.points.map((p) => trans.transformPoint(this.asVec2(this.evalExpr(p, locals))));
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        this.drawList.push({ type: "Polygon", points, style });
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
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const pos = this.currentTransform().transformPoint(posRaw);
        this.drawList.push({
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
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const trans = this.currentTransform();
        const drawCommands: DrawPathCommand[] = [];
        const pathLocals = new Map(locals);

        for (const cmd of stmt.commands) {
          switch (cmd.cmd) {
            case "Set": {
              const val = this.evalExpr(cmd.expr, pathLocals);
              pathLocals.set(cmd.name, val);
              locals.set(cmd.name, val);
              break;
            }
            case "Start": {
              const pt = trans.transformPoint(this.asVec2(this.evalExpr(cmd.pt, pathLocals)));
              drawCommands.push({ cmd: "Start", pt });
              break;
            }
            case "Line": {
              const pt = trans.transformPoint(this.asVec2(this.evalExpr(cmd.pt, pathLocals)));
              drawCommands.push({ cmd: "Line", pt });
              break;
            }
            case "Quad": {
              const cp = trans.transformPoint(this.asVec2(this.evalExpr(cmd.cp, pathLocals)));
              const ep = trans.transformPoint(this.asVec2(this.evalExpr(cmd.ep, pathLocals)));
              drawCommands.push({ cmd: "Quad", cp, ep });
              break;
            }
            case "Curve": {
              const c1 = trans.transformPoint(this.asVec2(this.evalExpr(cmd.c1, pathLocals)));
              const c2 = trans.transformPoint(this.asVec2(this.evalExpr(cmd.c2, pathLocals)));
              const ep = trans.transformPoint(this.asVec2(this.evalExpr(cmd.ep, pathLocals)));
              drawCommands.push({ cmd: "Curve", c1, c2, ep });
              break;
            }
            case "Arc": {
              const center = trans.transformPoint(this.asVec2(this.evalExpr(cmd.center, pathLocals)));
              const radius = this.asNumber(this.evalExpr(cmd.radius, pathLocals));
              const startAngle = this.asNumber(this.evalExpr(cmd.startAngle, pathLocals));
              const endAngle = this.asNumber(this.evalExpr(cmd.endAngle, pathLocals));
              drawCommands.push({ cmd: "Arc", center, radius, startAngle, endAngle });
              break;
            }
            case "Close":
              drawCommands.push({ cmd: "Close" });
              break;
          }
        }

        this.drawList.push({ type: "Path", commands: drawCommands, style });
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
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.styleStack.push(style);

        for (const bStmt of stmt.body) {
          this.evalStmt(bStmt, locals);
        }

        this.styleStack.pop();
        this.transformStack.pop();
        return null;
      }
    }
  }

  private invokeFunction(name: string, args: Value[]): Value {
    const func = this.functions.get(name);
    if (!func) throw new Error(`Undefined function '${name}'`);
    if (func.params.length !== args.length) {
      throw new Error(`Function '${name}' expects ${func.params.length} arguments, got ${args.length}`);
    }

    const locals = new Map<string, Value>();
    for (let i = 0; i < func.params.length; i++) {
      locals.set(func.params[i], args[i]);
    }

    for (const stmt of func.body) {
      const ret = this.evalStmt(stmt, locals);
      if (ret && ret.isReturn) return ret.value;
    }
    return null;
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
              return `${l}${r}`;
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
          case "==": return l === r;
          case "!=": return l !== r;
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
    }
  }

  private asNumber(val: Value): number {
    if (typeof val === "number") return val;
    if (typeof val === "boolean") return val ? 1.0 : 0.0;
    throw new Error(`Expected number, got ${JSON.stringify(val)}`);
  }

  private asString(val: Value): string {
    if (typeof val === "string") return val;
    if (typeof val === "number") return val.toString();
    if (typeof val === "boolean") return val.toString();
    throw new Error(`Expected string or displayable value, got ${JSON.stringify(val)}`);
  }

  private asVec2(val: Value): Vec2 {
    if (Array.isArray(val) && val.length === 2 && typeof val[0] === "number" && typeof val[1] === "number") {
      return val as Vec2;
    }
    throw new Error(`Expected [x, y] vector, got ${JSON.stringify(val)}`);
  }

  private asColor(val: Value): PvgColor {
    if (val instanceof PvgColor) return val;
    throw new Error(`Expected color value, got ${JSON.stringify(val)}`);
  }

  private isTruthy(val: Value): boolean {
    if (typeof val === "boolean") return val;
    if (typeof val === "number") return val !== 0.0;
    if (typeof val === "string") return val.length > 0;
    return val != null;
  }
}