import { Token, TokenKind } from "./lexer.js";
import type { Document, Expr, PathCommandAst, Stmt } from "./types.js";
import { PvgColor } from "./color.js";

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

    // 1. Header: PVG 0.1
    this.expect(TokenKind.Pvg);
    const verTok = this.advance();
    if (verTok.kind !== TokenKind.Number || typeof verTok.value !== "number") {
      throw new Error(`Line ${verTok.line}: Expected version number after PVG (e.g. 0.1)`);
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
    if (this.match(TokenKind.Newline) && this.match(TokenKind.Indent)) {
      if (this.match(TokenKind.Background)) {
        const bgTok = this.advance();
        if (bgTok.kind === TokenKind.Color && bgTok.value instanceof PvgColor) {
          background = bgTok.value;
        } else {
          throw new Error(`Line ${bgTok.line}: Expected color for canvas background`);
        }
      }
      this.skipNewlines();
      this.match(TokenKind.Dedent);
    }
    this.skipNewlines();

    // 3. Statements
    const statements: Stmt[] = [];
    while (this.peek().kind !== TokenKind.Eof) {
      if (this.peek().kind === TokenKind.Newline) {
        this.advance();
        continue;
      }
      statements.push(this.parseStatement());
      this.skipNewlines();
    }

    return {
      version,
      canvas: { width, height, background },
      statements,
    };
  }

  private parseStatement(): Stmt {
    const tok = this.peek();

    switch (tok.kind) {
      case TokenKind.Set: {
        this.advance();
        const nameTok = this.advance();
        if (nameTok.kind !== TokenKind.Ident || typeof nameTok.value !== "string") {
          throw new Error(`Line ${tok.line}: Expected identifier name after 'set'`);
        }
        this.expect(TokenKind.Equal);
        const expr = this.parseExpression();
        return { type: "Set", name: nameTok.value, expr };
      }
      case TokenKind.Seed: {
        this.advance();
        const seedTok = this.advance();
        return {
          type: "Seed",
          seed: seedTok.kind === TokenKind.Number && typeof seedTok.value === "number"
            ? Math.floor(seedTok.value)
            : 42,
        };
      }
      case TokenKind.Def: {
        this.advance();
        const nameTok = this.advance();
        if (nameTok.kind !== TokenKind.Ident || typeof nameTok.value !== "string") {
          throw new Error(`Line ${tok.line}: Expected function name`);
        }
        this.expect(TokenKind.LParen);
        const params: string[] = [];
        if (this.peek().kind !== TokenKind.RParen) {
          while (true) {
            const pTok = this.advance();
            if (pTok.kind === TokenKind.Ident && typeof pTok.value === "string") {
              params.push(pTok.value);
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
        return { type: "Def", name: nameTok.value, params, body };
      }
      case TokenKind.For: {
        this.advance();
        const varTok = this.advance();
        if (varTok.kind !== TokenKind.Ident || typeof varTok.value !== "string") {
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
        return { type: "For", var: varTok.value, from, to, step, body };
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
      case TokenKind.Ident: {
        const name = tok.value as string;
        this.advance();
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
        throw new Error(`Line ${tok.line}: Unexpected identifier in statement position '${name}'`);
      }
      default:
        throw new Error(`Line ${tok.line}: Unexpected statement token '${tok.kind}'`);
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

  private parseCircle(): Stmt {
    this.expect(TokenKind.Indent);
    let center: Expr | null = null,
      radius: Expr | null = null,
      fill: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Center: this.advance(); center = this.parseExpression(); break;
        case TokenKind.Radius: this.advance(); radius = this.parseExpression(); break;
        case TokenKind.Fill: this.advance(); fill = this.parseExpression(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parseExpression(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          throw new Error(`Line ${this.peek().line}: Invalid circle property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!center || !radius) throw new Error("Circle requires 'center [x, y]' and 'radius r'");
    return { type: "Circle", center, radius, fill, stroke, width, opacity };
  }

  private parseEllipse(): Stmt {
    this.expect(TokenKind.Indent);
    let center: Expr | null = null,
      radius: Expr | null = null,
      fill: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Center: this.advance(); center = this.parseExpression(); break;
        case TokenKind.Radius: this.advance(); radius = this.parseExpression(); break;
        case TokenKind.Fill: this.advance(); fill = this.parseExpression(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parseExpression(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          throw new Error(`Line ${this.peek().line}: Invalid ellipse property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!center || !radius) throw new Error("Ellipse requires 'center [x, y]' and 'radius [rx, ry]'");
    return { type: "Ellipse", center, radius, fill, stroke, width, opacity };
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

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Pos: this.advance(); pos = this.parseExpression(); break;
        case TokenKind.Size: this.advance(); size = this.parseExpression(); break;
        case TokenKind.Radius: this.advance(); radius = this.parseExpression(); break;
        case TokenKind.Fill: this.advance(); fill = this.parseExpression(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parseExpression(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          throw new Error(`Line ${this.peek().line}: Invalid rectangle property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!pos || !size) throw new Error("Rectangle requires 'pos [x, y]' and 'size [w, h]'");
    return { type: "Rectangle", pos, size, radius, fill, stroke, width, opacity };
  }

  private parseLine(): Stmt {
    this.expect(TokenKind.Indent);
    let from: Expr | null = null,
      to: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.From: this.advance(); from = this.parseExpression(); break;
        case TokenKind.To: this.advance(); to = this.parseExpression(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parseExpression(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          throw new Error(`Line ${this.peek().line}: Invalid line property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!from || !to) throw new Error("Line requires 'from [x, y]' and 'to [x, y]'");
    return { type: "Line", from, to, stroke, width, opacity };
  }

  private parsePolygon(): Stmt {
    this.expect(TokenKind.Indent);
    const points: Expr[] = [];
    let fill: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Points:
          this.advance();
          while (this.peek().kind === TokenKind.LBracket) {
            points.push(this.parseExpression());
          }
          break;
        case TokenKind.Fill: this.advance(); fill = this.parseExpression(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parseExpression(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          throw new Error(`Line ${this.peek().line}: Invalid polygon property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    return { type: "Polygon", points, fill, stroke, width, opacity };
  }

  private parsePath(): Stmt {
    this.expect(TokenKind.Indent);
    let fill: Expr | null = null,
      stroke: Expr | null = null,
      width: Expr | null = null,
      opacity: Expr | null = null;
    const commands: PathCommandAst[] = [];

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Set: {
          this.advance();
          const nameTok = this.advance();
          this.expect(TokenKind.Equal);
          const expr = this.parseExpression();
          commands.push({ cmd: "Set", name: nameTok.value as string, expr });
          break;
        }
        case TokenKind.Fill: this.advance(); fill = this.parseExpression(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parseExpression(); break;
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
          throw new Error(`Line ${this.peek().line}: Invalid path property/command '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    return { type: "Path", fill, stroke, width, opacity, commands };
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

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Pos: this.advance(); pos = this.parseExpression(); break;
        case TokenKind.Content:
        case TokenKind.Text: this.advance(); content = this.parseExpression(); break;
        case TokenKind.Size: this.advance(); size = this.parseExpression(); break;
        case TokenKind.Font: this.advance(); font = this.parseExpression(); break;
        case TokenKind.Align: this.advance(); align = this.parseExpression(); break;
        case TokenKind.Fill: this.advance(); fill = this.parseExpression(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parseExpression(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          throw new Error(`Line ${this.peek().line}: Invalid text property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!pos || !content) throw new Error("Text requires 'pos [x, y]' and 'content <expr>'");
    return { type: "Text", pos, content, size, font, align, fill, stroke, width, opacity };
  }

  private parseGroup(): Stmt {
    this.expect(TokenKind.Indent);
    let pos: Expr | null = null,
      rot: Expr | null = null,
      scale: Expr | null = null,
      opacity: Expr | null = null,
      fill: Expr | null = null,
      stroke: Expr | null = null;
    const body: Stmt[] = [];

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Pos: this.advance(); pos = this.parseExpression(); break;
        case TokenKind.Rot: this.advance(); rot = this.parseExpression(); break;
        case TokenKind.Scale: this.advance(); scale = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Fill: this.advance(); fill = this.parseExpression(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          body.push(this.parseStatement());
          break;
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    return { type: "Group", pos, rot, scale, opacity, fill, stroke, body };
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
        const x = this.parseExpression();
        this.expect(TokenKind.Comma);
        const y = this.parseExpression();
        this.expect(TokenKind.RBracket);
        return { type: "Vec2", x, y };
      }
      case TokenKind.LParen: {
        const expr = this.parseExpression();
        this.expect(TokenKind.RParen);
        return expr;
      }
      case TokenKind.Ident: {
        const name = tok.value as string;
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
      default:
        throw new Error(`Line ${tok.line}: Unexpected token in expression '${tok.kind}'`);
    }
  }
}