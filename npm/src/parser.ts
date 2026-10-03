import { Token, TokenKind, softIdentKind } from "./lexer.js";
import type {
  Document,
  Expr,
  GlowExpr,
  GradientStopExpr,
  ParamDecl,
  PathCommandAst,
  PatternDef,
  PixelFilter,
  ShadowExpr,
  ShapeFx,
  SplineFx,
  Stmt,
} from "./types.js";
import { PvgColor } from "./color.js";

function emptyFx(): ShapeFx {
  return {
    cap: null,
    join: null,
    miter: null,
    dash: null,
    align: null,
    blur: null,
    shadow: null,
    glow: null,
    blend: null,
  };
}

/** Spline FX surface: the `ShapeFx` subset a `spline` accepts (no `align`). */
function emptySplineFx(): SplineFx {
  return { cap: null, join: null, miter: null, dash: null, blur: null, shadow: null, glow: null, blend: null };
}

export class Parser {
  private tokens: Token[];
  private pos = 0;

  constructor(tokens: Token[]) {
    this.tokens = tokens;
  }

  private peek(): Token {
    return this.tokens[Math.min(this.pos, this.tokens.length - 1)];
  }

  private advance(): Token {
    const tok = this.peek();
    if (this.pos < this.tokens.length) {
      this.pos++;
    }
    return tok;
  }

  private match(kind: TokenKind): boolean {
    if (this.peek().kind === kind) {
      this.advance();
      return true;
    }
    return false;
  }

  private expect(kind: TokenKind): Token {
    const tok = this.peek();
    if (tok.kind === kind) {
      return this.advance();
    }
    throw new Error(`Line ${tok.line}, Col ${tok.col}: Expected ${kind}, found ${tok.kind}`);
  }

  private skipNewlines(): void {
    while (this.peek().kind === TokenKind.Newline) {
      this.advance();
    }
  }

  parseDocument(): Document {
    this.skipNewlines();

    // 1. Header: PVG 0.1 / 0.2 (0.2 readers accept both; 0.1 docs evaluate identically)
    this.expect(TokenKind.Pvg);
    const verTok = this.advance();
    if (verTok.kind !== TokenKind.Number || typeof verTok.value !== "number") {
      throw new Error(`Line ${verTok.line}: Expected version number after PVG (e.g. 0.2)`);
    }
    const version: [number, number] = [
      Math.floor(verTok.value),
      Math.round((verTok.value % 1) * 10),
    ];
    this.skipNewlines();

    // 2. Canvas declaration
    this.expect(TokenKind.Canvas);
    const wTok = this.advance();
    const hTok = this.advance();
    if (
      wTok.kind !== TokenKind.Number ||
      hTok.kind !== TokenKind.Number ||
      typeof wTok.value !== "number" ||
      typeof hTok.value !== "number"
    ) {
      throw new Error(`Line ${wTok.line}: Expected canvas width and height numbers`);
    }
    const width = wTok.value;
    const height = hTok.value;

    let background: PvgColor | null = null;
    let snap = 0;
    let pixelFilter: PixelFilter = "linear";
    if (this.match(TokenKind.Newline) && this.match(TokenKind.Indent)) {
      while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
        if (this.peek().kind === TokenKind.Newline) {
          this.advance();
          continue;
        }
        if (this.match(TokenKind.Background)) {
          const bgTok = this.advance();
          if (bgTok.kind === TokenKind.Color && bgTok.value instanceof PvgColor) {
            background = bgTok.value;
          } else {
            throw new Error(`Line ${bgTok.line}: Expected color for canvas background`);
          }
        } else if (this.match(TokenKind.Snap)) {
          const sTok = this.advance();
          if (sTok.kind !== TokenKind.Number || typeof sTok.value !== "number") {
            throw new Error(`Line ${sTok.line}: Expected number for canvas snap grid`);
          }
          snap = Math.max(0, sTok.value);
        } else if (this.match(TokenKind.Filter)) {
          const fTok = this.advance();
          const name = typeof fTok.value === "string" ? fTok.value.toLowerCase() : "";
          pixelFilter = name === "nearest" ? "nearest" : "linear";
        } else {
          throw new Error(`Line ${this.peek().line}: Invalid canvas property '${this.peek().kind}'`);
        }
        this.skipNewlines();
      }
      this.match(TokenKind.Dedent);
    }
    this.skipNewlines();

    // 3. Top-level declarations: host uniforms (`param`) and tiles (`pattern`),
    //    then the statement body (order matters for host-param display).
    const params: ParamDecl[] = [];
    const patterns: PatternDef[] = [];
    const statements: Stmt[] = [];
    while (this.peek().kind !== TokenKind.Eof) {
      if (this.peek().kind === TokenKind.Newline) {
        this.advance();
        continue;
      }
      if (this.peek().kind === TokenKind.Param) {
        params.push(this.parseParamDecl());
        this.skipNewlines();
        continue;
      }
      if (this.peek().kind === TokenKind.Pattern) {
        patterns.push(this.parsePatternDef());
        this.skipNewlines();
        continue;
      }
      statements.push(this.parseStatement());
      this.skipNewlines();
    }

    return {
      version,
      canvas: { width, height, background, snap, pixelFilter },
      params,
      patterns,
      statements,
    };
  }

  /** `param <name> : <expr>` or `param <name> = <expr>` (Section 18.1). */
  private parseParamDecl(): ParamDecl {
    this.expect(TokenKind.Param);
    const nameTok = this.advance();
    const name = softIdentKind(nameTok.kind, nameTok.value);
    if (name === null) {
      throw new Error(`Line ${nameTok.line}: Expected param name, found '${nameTok.kind}'`);
    }
    if (!(this.match(TokenKind.Colon) || this.match(TokenKind.Equal))) {
      throw new Error(`Line ${this.peek().line}: Expected ':' or '=' after param name (e.g. \`param health: 0.75\`)`);
    }
    const def = this.parseExpression();
    return { name, default: def };
  }

  /** `pattern <name> <w> <h>` + indented body block (Section 18.4). */
  private parsePatternDef(): PatternDef {
    this.expect(TokenKind.Pattern);
    const nameTok = this.advance();
    const name = softIdentKind(nameTok.kind, nameTok.value);
    if (name === null) {
      throw new Error(`Line ${nameTok.line}: Expected pattern name, found '${nameTok.kind}'`);
    }
    const wTok = this.advance();
    const hTok = this.advance();
    if (
      wTok.kind !== TokenKind.Number ||
      hTok.kind !== TokenKind.Number ||
      typeof wTok.value !== "number" ||
      typeof hTok.value !== "number"
    ) {
      throw new Error(`Line ${wTok.line}: Expected pattern tile width and height numbers`);
    }
    this.skipNewlines();
    const body = this.parseBlock();
    return { name, width: wTok.value, height: hTok.value, body };
  }

  private parseStatement(): Stmt {
    const tok = this.peek();

    switch (tok.kind) {
      case TokenKind.Set: {
        this.advance();
        const nameTok = this.advance();
        const name = softIdentKind(nameTok.kind, nameTok.value);
        if (name === null) {
          throw new Error(`Line ${tok.line}: Expected identifier name after 'set'`);
        }
        this.expect(TokenKind.Equal);
        const expr = this.parseExpression();
        return { type: "Set", name, expr };
      }
      case TokenKind.Seed: {
        this.advance();
        const seedTok = this.advance();
        return {
          type: "Seed",
          // Non-numeric seeds fall back to 0 (= engine default seed), mirroring
          // the Rust core (`Number(n) => n as u64, _ => 0`).
          seed: seedTok.kind === TokenKind.Number && typeof seedTok.value === "number"
            ? Math.floor(seedTok.value)
            : 0,
        };
      }
      case TokenKind.Def: {
        this.advance();
        const nameTok = this.advance();
        const fnName = softIdentKind(nameTok.kind, nameTok.value);
        if (fnName === null) {
          throw new Error(`Line ${tok.line}: Expected function name`);
        }
        this.expect(TokenKind.LParen);
        const params: string[] = [];
        if (this.peek().kind !== TokenKind.RParen) {
          while (true) {
            const pTok = this.advance();
            const pName = softIdentKind(pTok.kind, pTok.value);
            if (pName !== null) {
              params.push(pName);
            }
            if (this.peek().kind === TokenKind.Comma) {
              this.advance();
            } else {
              break;
            }
          }
        }
        this.expect(TokenKind.RParen);
        this.skipNewlines();
        const body = this.parseBlock();
        return { type: "Def", name: fnName, params, body };
      }
      case TokenKind.For: {
        this.advance();
        const varTok = this.advance();
        const varName = softIdentKind(varTok.kind, varTok.value);
        if (varName === null) {
          throw new Error(`Line ${tok.line}: Expected loop variable`);
        }
        this.expect(TokenKind.From);
        const from = this.parseExpression();
        this.expect(TokenKind.To);
        const to = this.parseExpression();
        let step: Expr | null = null;
        if (this.match(TokenKind.Step)) {
          step = this.parseExpression();
        }
        this.skipNewlines();
        const body = this.parseBlock();
        return { type: "For", var: varName, from, to, step, body };
      }
      case TokenKind.While: {
        this.advance();
        const cond = this.parseExpression();
        this.skipNewlines();
        const body = this.parseBlock();
        return { type: "While", cond, body };
      }
      case TokenKind.If: {
        this.advance();
        const cond = this.parseExpression();
        this.skipNewlines();
        const thenBody = this.parseBlock();
        let elseBody: Stmt[] = [];
        this.skipNewlines();
        if (this.match(TokenKind.Else)) {
          if (this.peek().kind === TokenKind.If) {
            elseBody.push(this.parseStatement());
          } else {
            this.skipNewlines();
            elseBody = this.parseBlock();
          }
        }
        return { type: "If", cond, thenBody, elseBody };
      }
      case TokenKind.Return: {
        this.advance();
        const expr = this.parseExpression();
        return { type: "Return", expr };
      }
      case TokenKind.Circle:
        this.advance();
        this.skipNewlines();
        return this.parseCircle();
      case TokenKind.Ellipse:
        this.advance();
        this.skipNewlines();
        return this.parseEllipse();
      case TokenKind.Rectangle:
        this.advance();
        this.skipNewlines();
        return this.parseRectangle();
      case TokenKind.Line:
        this.advance();
        this.skipNewlines();
        return this.parseLine();
      case TokenKind.Polygon:
        this.advance();
        this.skipNewlines();
        return this.parsePolygon();
      case TokenKind.Path:
        this.advance();
        this.skipNewlines();
        return this.parsePath();
      case TokenKind.Text:
        this.advance();
        this.skipNewlines();
        return this.parseText();
      case TokenKind.Group:
        this.advance();
        this.skipNewlines();
        return this.parseGroup();
      case TokenKind.Clip:
        this.advance();
        this.skipNewlines();
        return this.parseClip();
      case TokenKind.Sprite:
        this.advance();
        this.skipNewlines();
        return this.parseSprite();
      case TokenKind.Spline:
        this.advance();
        this.skipNewlines();
        return this.parseSpline();
      case TokenKind.Param: {
        // `param` inside a block behaves like `set` with a default that the
        // host may override; evaluates identically at runtime (Rust parity:
        // `Stmt::Set(decl.name, decl.default)`). Top-level `param` is handled
        // by parseDocument (host uniforms) and never reaches here.
        const decl = this.parseParamDecl();
        return { type: "Set", name: decl.name, expr: decl.default };
      }
      default: {
        // `pattern` tiles must be declared at top level (Rust parity).
        if (tok.kind === TokenKind.Pattern) {
          throw new Error(`Line ${tok.line}: Pattern blocks must be declared at top level.`);
        }
        const softName = softIdentKind(tok.kind, tok.value);
        if (softName !== null && this.tokens[this.pos + 1]?.kind === TokenKind.LParen) {
          this.advance();
          this.expect(TokenKind.LParen);
          const args: Expr[] = [];
          if (this.peek().kind !== TokenKind.RParen) {
            while (true) {
              args.push(this.parseExpression());
              if (this.peek().kind === TokenKind.Comma) {
                this.advance();
              } else {
                break;
              }
            }
          }
          this.expect(TokenKind.RParen);
          return { type: "Call", name: softName, args };
        }
        throw new Error(`Line ${tok.line}: Unexpected statement token '${tok.kind}'`);
      }
    }
  }

  private parseBlock(): Stmt[] {
    this.expect(TokenKind.Indent);
    const statements: Stmt[] = [];
    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      if (this.peek().kind === TokenKind.Newline) {
        this.advance();
        continue;
      }
      statements.push(this.parseStatement());
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    return statements;
  }

  // ---- PVG 0.2 paint & FX helpers (mirror Rust parse_paint_expr) ----
  private parsePaintExpr(): Expr {
    if (this.peek().kind === TokenKind.Linear) {
      this.advance();
      const start = this.parseExpression();
      const end = this.parseExpression();
      const stops = this.tryParseGradientStops();
      return { type: "Linear", start, end, stops };
    }
    if (this.peek().kind === TokenKind.Radial) {
      this.advance();
      const center = this.parseExpression();
      const radius = this.parseExpression();
      let focal: Expr | null = null;
      if (this.peek().kind === TokenKind.LBracket) {
        focal = this.parseExpression();
      }
      const stops = this.tryParseGradientStops();
      return { type: "Radial", center, radius, focal, stops };
    }
    if (this.peek().kind === TokenKind.Angular) {
      this.advance();
      const center = this.parseExpression();
      const startAngle = this.parseExpression();
      const stops = this.tryParseGradientStops();
      return { type: "Angular", center, startAngle, stops };
    }
    if (this.peek().kind === TokenKind.Pattern) {
      const after = this.tokens[this.pos + 1];
      const refName = after ? softIdentKind(after.kind, after.value) : null;
      if (refName !== null) {
        this.advance();
        this.advance();
        return { type: "Pattern", name: refName };
      }
    }
    return this.parseExpression();
  }
  private tryParseGradientStops(): GradientStopExpr[] {
    if (this.peek().kind !== TokenKind.Newline) return [];
    // Lookahead: Newline(s) Indent Newline(s) Stop
    let j = this.pos + 1;
    while (j < this.tokens.length && this.tokens[j].kind === TokenKind.Newline) j++;
    if (j >= this.tokens.length || this.tokens[j].kind !== TokenKind.Indent) return [];
    j++;
    while (j < this.tokens.length && this.tokens[j].kind === TokenKind.Newline) j++;
    if (j >= this.tokens.length || this.tokens[j].kind !== TokenKind.Stop) return [];
    // Commit
    while (this.peek().kind === TokenKind.Newline) this.advance();
    this.expect(TokenKind.Indent);
    const stops: GradientStopExpr[] = [];
    for (;;) {
      this.skipNewlines();
      if (this.peek().kind === TokenKind.Dedent) { this.advance(); break; }
      if (this.peek().kind === TokenKind.Eof) break;
      this.expect(TokenKind.Stop);
      const offset = this.parseExpression();
      const color = this.parseExpression();
      stops.push({ type: "GradientStop", offset, color });
      this.skipNewlines();
    }
    return stops;
  }

  private parseDashArray(): Expr[] {
    this.expect(TokenKind.LBracket);
    const items: Expr[] = [];
    this.skipNewlines();
    if (this.peek().kind !== TokenKind.RBracket) {
      for (;;) {
        items.push(this.parseExpression());
        if (this.peek().kind === TokenKind.Comma) {
          this.advance();
          this.skipNewlines();
        } else break;
      }
    }
    this.expect(TokenKind.RBracket);
    return items;
  }

  private parseShadowExpr(): ShadowExpr {
    const offset = this.parseExpression();
    const radius = this.parseExpression();
    const color = this.parseExpression();
    return { type: "ShadowExpr", offset, radius, color };
  }

  private parseGlowExpr(): GlowExpr {
    const radius = this.parseExpression();
    const color = this.parseExpression();
    return { type: "GlowExpr", radius, color };
  }

  // Consumes one 0.2 style prop if the cursor is on it; returns true when handled.
  private parseStylePropInto(out: ShapeFx): boolean {
    switch (this.peek().kind) {
      case TokenKind.Cap: this.advance(); out.cap = this.parseExpression(); return true;
      case TokenKind.Join: this.advance(); out.join = this.parseExpression(); return true;
      case TokenKind.Miter: this.advance(); out.miter = this.parseExpression(); return true;
      case TokenKind.Dash: this.advance(); out.dash = this.parseDashArray(); return true;
      case TokenKind.Align: this.advance(); out.align = this.parseExpression(); return true;
      case TokenKind.Blur: this.advance(); out.blur = this.parseExpression(); return true;
      case TokenKind.Shadow: this.advance(); out.shadow = this.parseShadowExpr(); return true;
      case TokenKind.Glow: this.advance(); out.glow = this.parseGlowExpr(); return true;
      case TokenKind.Blend: this.advance(); out.blend = this.parseExpression(); return true;
      default: return false;
    }
  }

  private parseCircle(): Stmt {
    this.expect(TokenKind.Indent);
    let center: Expr | null = null,
      radius: Expr | null = null,
      fill: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;
    const fx = emptyFx();

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Center: this.advance(); center = this.parseExpression(); break;
        case TokenKind.Radius: this.advance(); radius = this.parseExpression(); break;
        case TokenKind.Fill: this.advance(); fill = this.parsePaintExpr(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parsePaintExpr(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          if (this.parseStylePropInto(fx)) break;
          throw new Error(`Line ${this.peek().line}: Invalid circle property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!center || !radius) throw new Error("Circle requires 'center [x, y]' and 'radius r'");
    return { type: "Circle", center, radius, fill, stroke, width, opacity, ...fx };
  }

  private parseEllipse(): Stmt {
    this.expect(TokenKind.Indent);
    let center: Expr | null = null,
      radius: Expr | null = null,
      fill: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;
    const fx = emptyFx();

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Center: this.advance(); center = this.parseExpression(); break;
        case TokenKind.Radius: this.advance(); radius = this.parseExpression(); break;
        case TokenKind.Fill: this.advance(); fill = this.parsePaintExpr(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parsePaintExpr(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          if (this.parseStylePropInto(fx)) break;
          throw new Error(`Line ${this.peek().line}: Invalid ellipse property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!center || !radius) throw new Error("Ellipse requires 'center [x, y]' and 'radius [rx, ry]'");
    return { type: "Ellipse", center, radius, fill, stroke, width, opacity, ...fx };
  }

  private parseRectangle(): Stmt {
    this.expect(TokenKind.Indent);
    let pos: Expr | null = null,
      size: Expr | null = null,
      radius: Expr | null = null,
      fill: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;
    const fx = emptyFx();

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Pos: this.advance(); pos = this.parseExpression(); break;
        case TokenKind.Size: this.advance(); size = this.parseExpression(); break;
        case TokenKind.Radius: this.advance(); radius = this.parseExpression(); break;
        case TokenKind.Fill: this.advance(); fill = this.parsePaintExpr(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parsePaintExpr(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          if (this.parseStylePropInto(fx)) break;
          throw new Error(`Line ${this.peek().line}: Invalid rectangle property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!pos || !size) throw new Error("Rectangle requires 'pos [x, y]' and 'size [w, h]'");
    return { type: "Rectangle", pos, size, radius, fill, stroke, width, opacity, ...fx };
  }

  private parseLine(): Stmt {
    this.expect(TokenKind.Indent);
    let from: Expr | null = null,
      to: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;
    const fx = emptyFx();

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.From: this.advance(); from = this.parseExpression(); break;
        case TokenKind.To: this.advance(); to = this.parseExpression(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parsePaintExpr(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          if (this.parseStylePropInto(fx)) break;
          throw new Error(`Line ${this.peek().line}: Invalid line property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!from || !to) throw new Error("Line requires 'from [x, y]' and 'to [x, y]'");
    return { type: "Line", from, to, stroke, width, opacity, ...fx };
  }

  private parsePolygon(): Stmt {
    this.expect(TokenKind.Indent);
    const points: Expr[] = [];
    let fill: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;
    const fx = emptyFx();

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Points:
          this.advance();
          while (this.peek().kind === TokenKind.LBracket) {
            points.push(this.parseExpression());
          }
          break;
        case TokenKind.Fill: this.advance(); fill = this.parsePaintExpr(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parsePaintExpr(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          if (this.parseStylePropInto(fx)) break;
          throw new Error(`Line ${this.peek().line}: Invalid polygon property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    return { type: "Polygon", points, fill, stroke, width, opacity, ...fx };
  }

  private parsePath(): Stmt {
    this.expect(TokenKind.Indent);
    let fill: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;
    const fx = emptyFx();
    const commands: PathCommandAst[] = [];

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Set: {
          this.advance();
          const nameTok = this.advance();
          const name = softIdentKind(nameTok.kind, nameTok.value);
          if (name === null) {
            throw new Error(`Line ${nameTok.line}: Expected identifier name after 'set'`);
          }
          this.expect(TokenKind.Equal);
          const expr = this.parseExpression();
          commands.push({ cmd: "Set", name, expr });
          break;
        }
        // Post-0.2 Section 18.6: control flow inside a path body shares the path locals
        // and emits path commands (style props stay in the outer body).
        case TokenKind.For: {
          this.advance();
          const varTok = this.advance();
          const varName = softIdentKind(varTok.kind, varTok.value);
          if (varName === null) {
            throw new Error(`Line ${varTok.line}: Expected loop variable`);
          }
          this.expect(TokenKind.From);
          const from = this.parseExpression();
          this.expect(TokenKind.To);
          const to = this.parseExpression();
          let step: Expr | null = null;
          if (this.match(TokenKind.Step)) {
            step = this.parseExpression();
          }
          this.skipNewlines();
          const body = this.parsePathBlock();
          commands.push({ cmd: "For", varName, from, to, step, body });
          break;
        }
        case TokenKind.While: {
          this.advance();
          const cond = this.parseExpression();
          this.skipNewlines();
          const body = this.parsePathBlock();
          commands.push({ cmd: "While", cond, body });
          break;
        }
        case TokenKind.If: {
          commands.push(this.parsePathIf());
          break;
        }
        case TokenKind.Fill: this.advance(); fill = this.parsePaintExpr(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parsePaintExpr(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Start: this.advance(); commands.push({ cmd: "Start", pt: this.parseExpression() }); break;
        case TokenKind.Line: this.advance(); commands.push({ cmd: "Line", pt: this.parseExpression() }); break;
        case TokenKind.Quad: {
          this.advance();
          const cp = this.parseExpression();
          const ep = this.parseExpression();
          commands.push({ cmd: "Quad", cp, ep });
          break;
        }
        case TokenKind.Curve: {
          this.advance();
          const c1 = this.parseExpression();
          const c2 = this.parseExpression();
          const ep = this.parseExpression();
          commands.push({ cmd: "Curve", c1, c2, ep });
          break;
        }
        case TokenKind.Arc: {
          this.advance();
          const center = this.parseExpression();
          const radius = this.parseExpression();
          const startAngle = this.parseExpression();
          const endAngle = this.parseExpression();
          commands.push({ cmd: "Arc", center, radius, startAngle, endAngle });
          break;
        }
        case TokenKind.Close: this.advance(); commands.push({ cmd: "Close" }); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          if (this.parseStylePropInto(fx)) break;
          throw new Error(`Line ${this.peek().line}: Invalid path property/command '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    return { type: "Path", fill, stroke, width, opacity, commands, ...fx };
  }

  /**
   * Post-0.2 Section 18.6: a `path` sub-block. Accepts only path commands, `set` and
   * nested control flow — style properties are rejected inside a control block
   * (they belong to the outer path body), mirroring the Rust parser.
   */
  private parsePathBlock(): PathCommandAst[] {
    this.expect(TokenKind.Indent);
    const commands: PathCommandAst[] = [];
    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      if (this.peek().kind === TokenKind.Newline) {
        this.advance();
        continue;
      }
      commands.push(this.parsePathItem());
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    return commands;
  }

  /** Parses one path-body item (command, `set`, or control flow). */
  private parsePathItem(): PathCommandAst {
    const tok = this.peek();
    switch (tok.kind) {
      case TokenKind.Set: {
        this.advance();
        const nameTok = this.advance();
        const name = softIdentKind(nameTok.kind, nameTok.value);
        if (name === null) {
          throw new Error(`Line ${nameTok.line}: Expected identifier name after 'set'`);
        }
        this.expect(TokenKind.Equal);
        return { cmd: "Set", name, expr: this.parseExpression() };
      }
      case TokenKind.Start: this.advance(); return { cmd: "Start", pt: this.parseExpression() };
      case TokenKind.Line: this.advance(); return { cmd: "Line", pt: this.parseExpression() };
      case TokenKind.Quad: {
        this.advance();
        const cp = this.parseExpression();
        const ep = this.parseExpression();
        return { cmd: "Quad", cp, ep };
      }
      case TokenKind.Curve: {
        this.advance();
        const c1 = this.parseExpression();
        const c2 = this.parseExpression();
        const ep = this.parseExpression();
        return { cmd: "Curve", c1, c2, ep };
      }
      case TokenKind.Arc: {
        this.advance();
        const center = this.parseExpression();
        const radius = this.parseExpression();
        const startAngle = this.parseExpression();
        const endAngle = this.parseExpression();
        return { cmd: "Arc", center, radius, startAngle, endAngle };
      }
      case TokenKind.Close:
        this.advance();
        return { cmd: "Close" };
      case TokenKind.For: {
        this.advance();
        const varTok = this.advance();
        const varName = softIdentKind(varTok.kind, varTok.value);
        if (varName === null) {
          throw new Error(`Line ${varTok.line}: Expected loop variable`);
        }
        this.expect(TokenKind.From);
        const from = this.parseExpression();
        this.expect(TokenKind.To);
        const to = this.parseExpression();
        let step: Expr | null = null;
        if (this.match(TokenKind.Step)) {
          step = this.parseExpression();
        }
        this.skipNewlines();
        return { cmd: "For", varName, from, to, step, body: this.parsePathBlock() };
      }
      case TokenKind.While: {
        this.advance();
        const cond = this.parseExpression();
        this.skipNewlines();
        return { cmd: "While", cond, body: this.parsePathBlock() };
      }
      case TokenKind.If:
        return this.parsePathIf();
      default:
        throw new Error(
          `Line ${tok.line}: Invalid path command '${tok.kind}' (style properties belong in the outer path body)`
        );
    }
  }

  /** `if <cond>` / `else` (including `else if`) inside a `path` body. */
  private parsePathIf(): PathCommandAst {
    this.expect(TokenKind.If);
    const cond = this.parseExpression();
    this.skipNewlines();
    const thenBody = this.parsePathBlock();
    let elseBody: PathCommandAst[] = [];
    this.skipNewlines();
    if (this.match(TokenKind.Else)) {
      if (this.peek().kind === TokenKind.If) {
        elseBody = [this.parsePathIf()];
      } else {
        this.skipNewlines();
        elseBody = this.parsePathBlock();
      }
    }
    return { cmd: "If", cond, thenBody, elseBody };
  }

  /** `palette [c0, c1, ...]` (Section 18.3). */
  private parsePaletteArray(): Expr[] {
    this.expect(TokenKind.LBracket);
    const items: Expr[] = [];
    this.skipNewlines();
    if (this.peek().kind !== TokenKind.RBracket) {
      for (;;) {
        items.push(this.parseExpression());
        if (this.peek().kind === TokenKind.Comma) {
          this.advance();
          this.skipNewlines();
        } else break;
      }
    }
    this.expect(TokenKind.RBracket);
    return items;
  }

  /** Section 18.3 pixel-art sprite: `pos`, `palette`, repeatable `data`/`row` strings. */
  private parseSprite(): Stmt {
    this.expect(TokenKind.Indent);
    let pos: Expr | null = null;
    let palette: Expr[] = [];
    const rows: string[] = [];
    let scale: Expr | null = null;
    let opacity: Expr | null = null;
    let blend: Expr | null = null;

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Pos: this.advance(); pos = this.parseExpression(); break;
        case TokenKind.Palette: this.advance(); palette = this.parsePaletteArray(); break;
        case TokenKind.Data:
        case TokenKind.Row: {
          this.advance();
          const strTok = this.advance();
          if (strTok.kind !== TokenKind.String || typeof strTok.value !== "string") {
            throw new Error(
              `Line ${strTok.line}: Expected quoted pixel row after \`data\` (e.g. data "..11.." or a """ block).`
            );
          }
          // One row per literal, or a whole triple-quoted block: split on
          // newlines and keep non-blank rows.
          let kept = 0;
          for (const rawRow of strTok.value.split("\n")) {
            const row = rawRow.endsWith("\r") ? rawRow.slice(0, -1) : rawRow;
            if (row.trim().length === 0) continue;
            rows.push(row);
            kept++;
          }
          if (kept === 0) {
            throw new Error(`Line ${strTok.line}: Sprite \`data\` block has no pixel rows.`);
          }
          break;
        }
        case TokenKind.Scale: this.advance(); scale = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Blend: this.advance(); blend = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          throw new Error(`Line ${this.peek().line}: Invalid sprite property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!pos) throw new Error("Sprite requires 'pos [x, y]'");
    if (palette.length === 0) throw new Error("Sprite requires 'palette [c0, c1, ...]'.");
    if (rows.length === 0) throw new Error('Sprite requires at least one `data "..."` row.');
    return { type: "Sprite", pos, palette, rows, scale, opacity, blend };
  }

  /** Section 18.5 Catmull-Rom data spline (stroke-only; no `fill`, no `align`). */
  private parseSpline(): Stmt {
    this.expect(TokenKind.Indent);
    let points: Expr | null = null;
    let pos: Expr | null = null;
    let size: Expr | null = null;
    let stroke: Expr | null = null;
    let width: Expr | null = null;
    let opacity: Expr | null = null;
    const fx = emptyFx();

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Points: this.advance(); points = this.parseExpression(); break;
        case TokenKind.Pos: this.advance(); pos = this.parseExpression(); break;
        case TokenKind.Size: this.advance(); size = this.parseExpression(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parsePaintExpr(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          // `align` is a text anchor in PVG, so it is not a spline property.
          if (this.peek().kind !== TokenKind.Align && this.parseStylePropInto(fx)) break;
          throw new Error(`Line ${this.peek().line}: Invalid spline property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!points) throw new Error("Spline requires 'points <array>'");
    return {
      type: "Spline",
      points,
      pos,
      size,
      stroke,
      width,
      opacity,
      cap: fx.cap,
      join: fx.join,
      miter: fx.miter,
      dash: fx.dash,
      blur: fx.blur,
      shadow: fx.shadow,
      glow: fx.glow,
      blend: fx.blend,
    };
  }

  private parseText(): Stmt {
    this.expect(TokenKind.Indent);
    let pos: Expr | null = null,
      content: Expr | null = null,
      size: Expr | null = null,
      font: Expr | null = null,
      align: Expr | null = null,
      fill: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;
    let blur: Expr | null = null,
      shadow: ShadowExpr | null = null,
      glow: GlowExpr | null = null,
      blend: Expr | null = null;

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Pos: this.advance(); pos = this.parseExpression(); break;
        case TokenKind.Content:
        case TokenKind.Text: this.advance(); content = this.parseExpression(); break;
        case TokenKind.Size: this.advance(); size = this.parseExpression(); break;
        case TokenKind.Font: this.advance(); font = this.parseExpression(); break;
        case TokenKind.Align: this.advance(); align = this.parseExpression(); break;
        case TokenKind.Fill: this.advance(); fill = this.parsePaintExpr(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parsePaintExpr(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        // NOTE: text keeps 0.1-compatible align (anchor); cap/join/miter/dash rejected per spec.
        case TokenKind.Blur: this.advance(); blur = this.parseExpression(); break;
        case TokenKind.Shadow: this.advance(); shadow = this.parseShadowExpr(); break;
        case TokenKind.Glow: this.advance(); glow = this.parseGlowExpr(); break;
        case TokenKind.Blend: this.advance(); blend = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          throw new Error(`Line ${this.peek().line}: Invalid text property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!pos || !content) throw new Error("Text requires 'pos [x, y]' and 'content <expr>'");
    return { type: "Text", pos, content, size, font, align, fill, stroke, width, opacity, blur, shadow, glow, blend };
  }

  private parseGroup(): Stmt {
    this.expect(TokenKind.Indent);
    let pos: Expr | null = null,
      rot: Expr | null = null,
      scale: Expr | null = null,
      opacity: Expr | null = null,
      fill: Expr | null = null,
      stroke: Expr | null = null;
    let blend: Expr | null = null,
      blur: Expr | null = null,
      shadow: ShadowExpr | null = null,
      glow: GlowExpr | null = null;
    const body: Stmt[] = [];

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Pos: this.advance(); pos = this.parseExpression(); break;
        case TokenKind.Rot: this.advance(); rot = this.parseExpression(); break;
        case TokenKind.Scale: this.advance(); scale = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Fill: this.advance(); fill = this.parsePaintExpr(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parsePaintExpr(); break;
        case TokenKind.Blend: this.advance(); blend = this.parseExpression(); break;
        case TokenKind.Blur: this.advance(); blur = this.parseExpression(); break;
        case TokenKind.Shadow: this.advance(); shadow = this.parseShadowExpr(); break;
        case TokenKind.Glow: this.advance(); glow = this.parseGlowExpr(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          body.push(this.parseStatement());
          break;
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    return { type: "Group", pos, rot, scale, opacity, fill, stroke, body, blend, blur, shadow, glow };
  }

  private parseClip(): Stmt {
    this.expect(TokenKind.Indent);
    const stmts: Stmt[] = [];
    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      if (this.peek().kind === TokenKind.Newline) { this.advance(); continue; }
      stmts.push(this.parseStatement());
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (stmts.length === 0) throw new Error("clip block requires a mask shape as first statement");
    const maskType = stmts[0].type;
    if (!["Circle", "Ellipse", "Rectangle", "Line", "Polygon", "Path", "Text"].includes(maskType)) {
      throw new Error(`clip mask must be a geometric shape, found '${maskType}'`);
    }
    return { type: "Clip", mask: stmts[0], content: stmts.slice(1) };
  }

  private parseExpression(): Expr {
    return this.parseTernary();
  }

  private parseTernary(): Expr {
    const cond = this.parseLogicalOr();
    if (this.match(TokenKind.Question)) {
      const trueBranch = this.parseExpression();
      this.expect(TokenKind.Colon);
      const falseBranch = this.parseExpression();
      return { type: "Ternary", cond, trueBranch, falseBranch };
    }
    return cond;
  }

  private parseLogicalOr(): Expr {
    let left = this.parseLogicalAnd();
    while (this.match(TokenKind.Or)) {
      const right = this.parseLogicalAnd();
      left = { type: "Binary", op: "or", left, right };
    }
    return left;
  }

  private parseLogicalAnd(): Expr {
    let left = this.parseEquality();
    while (this.match(TokenKind.And)) {
      const right = this.parseEquality();
      left = { type: "Binary", op: "and", left, right };
    }
    return left;
  }

  private parseEquality(): Expr {
    let left = this.parseComparison();
    while (this.peek().kind === TokenKind.EqualEqual || this.peek().kind === TokenKind.NotEqual) {
      const op = this.advance().value as "==" | "!=";
      const right = this.parseComparison();
      left = { type: "Binary", op, left, right };
    }
    return left;
  }

  private parseComparison(): Expr {
    let left = this.parseAdditive();
    while (
      this.peek().kind === TokenKind.Less ||
      this.peek().kind === TokenKind.LessEqual ||
      this.peek().kind === TokenKind.Greater ||
      this.peek().kind === TokenKind.GreaterEqual
    ) {
      const op = this.advance().value as "<" | "<=" | ">" | ">=";
      const right = this.parseAdditive();
      left = { type: "Binary", op, left, right };
    }
    return left;
  }

  private parseAdditive(): Expr {
    let left = this.parseMultiplicative();
    while (this.peek().kind === TokenKind.Plus || this.peek().kind === TokenKind.Minus) {
      const op = this.advance().value as "+" | "-";
      const right = this.parseMultiplicative();
      left = { type: "Binary", op, left, right };
    }
    return left;
  }

  private parseMultiplicative(): Expr {
    let left = this.parsePower();
    while (
      this.peek().kind === TokenKind.Star ||
      this.peek().kind === TokenKind.Slash ||
      this.peek().kind === TokenKind.Percent
    ) {
      const op = this.advance().value as "*" | "/" | "%";
      const right = this.parsePower();
      left = { type: "Binary", op, left, right };
    }
    return left;
  }

  private parsePower(): Expr {
    const left = this.parseUnary();
    if (this.match(TokenKind.Caret)) {
      const right = this.parsePower();
      return { type: "Binary", op: "^", left, right };
    }
    return left;
  }

  private parseUnary(): Expr {
    if (this.match(TokenKind.Minus)) {
      return { type: "Unary", op: "neg", inner: this.parseUnary() };
    }
    if (this.match(TokenKind.Not)) {
      return { type: "Unary", op: "not", inner: this.parseUnary() };
    }
    return this.parsePrimary();
  }

  private parsePrimary(): Expr {
    const tok = this.advance();
    switch (tok.kind) {
      case TokenKind.Number:
        return { type: "Number", value: tok.value as number };
      case TokenKind.String:
        return { type: "String", value: tok.value as string };
      case TokenKind.Color:
        return { type: "Color", value: tok.value as PvgColor };
      case TokenKind.LBracket: {
        // Bracket list: exactly 2 scalar elements = Vec2 (back-compat for
        // positions); any other arity = Array literal. Nested compounds
        // (`[[10, 10], [90, 90]]`) are arrays. Use `array(a, b)` for an
        // explicit 2-element data array. Trailing comma allowed (Section 18.5).
        const first = this.parseExpression();
        if (this.peek().kind !== TokenKind.Comma) {
          this.expect(TokenKind.RBracket);
          return { type: "Array", items: [first] };
        }
        const items: Expr[] = [first];
        while (this.peek().kind === TokenKind.Comma) {
          this.advance();
          if (this.peek().kind === TokenKind.RBracket) break;
          items.push(this.parseExpression());
        }
        this.expect(TokenKind.RBracket);
        const isCompound = (e: Expr): boolean => e.type === "Vec2" || e.type === "Array";
        if (items.length === 2 && !items.some(isCompound)) {
          const [x, y] = items as [Expr, Expr];
          return { type: "Vec2", x, y };
        }
        return { type: "Array", items };
      }
      case TokenKind.LParen: {
        const expr = this.parseExpression();
        this.expect(TokenKind.RParen);
        return expr;
      }
      default: {
        // Plain identifiers and soft keywords (`row`, `data`, `snap`, ...)
        // alike: variables, `true`/`false`, and calls (mirrors the Rust
        // `parse_primary` soft-keyword fallback).
        const name = softIdentKind(tok.kind, tok.value);
        if (name === null) {
          throw new Error(`Line ${tok.line}: Unexpected token in expression '${tok.kind}'`);
        }
        if (name === "true") return { type: "Bool", value: true };
        if (name === "false") return { type: "Bool", value: false };

        if (this.match(TokenKind.LParen)) {
          const args: Expr[] = [];
          if (this.peek().kind !== TokenKind.RParen) {
            while (true) {
              args.push(this.parseExpression());
              if (this.peek().kind === TokenKind.Comma) {
                this.advance();
              } else {
                break;
              }
            }
          }
          this.expect(TokenKind.RParen);
          return { type: "Call", name, args };
        }
        return { type: "Ident", name };
      }
    }
  }
}
