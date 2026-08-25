/**
 * Procedural Vector Graphics (PVG) 0.1 - Pure Vanilla JavaScript Engine
 * Specification Conformant Lexer, Recursive Descent Parser, Evaluator & Render Pipeline
 * Includes standard <pvg-view> W3C Custom Element Web Component
 */

// ==========================================
// 0. INDENTATION NORMALIZER & UTILITIES
// ==========================================

function dedentCode(text) {
  if (!text) return '';
  const lines = text.split(/\r?\n/);
  while (lines.length > 0 && lines[0].trim().length === 0) {
    lines.shift();
  }
  while (lines.length > 0 && lines[lines.length - 1].trim().length === 0) {
    lines.pop();
  }
  if (lines.length === 0) return '';

  let minIndent = Infinity;
  for (const line of lines) {
    if (line.trim().length === 0) continue;
    const match = line.match(/^( +)/);
    const indent = match ? match[1].length : 0;
    if (indent < minIndent) {
      minIndent = indent;
    }
  }

  if (minIndent === Infinity || minIndent === 0) {
    return lines.join('\n');
  }

  return lines.map(line => {
    if (line.trim().length === 0) return '';
    return line.startsWith(' '.repeat(minIndent)) ? line.slice(minIndent) : line.trimStart();
  }).join('\n');
}

function detectLoopDuration(source) {
  if (!source) return 2.0;
  const match = source.match(/time\s*%\s*([0-9]+(?:\.[0-9]+)?)/);
  if (match && parseFloat(match[1]) > 0) {
    return parseFloat(match[1]);
  }
  return 2.0; // Standard 2.0s loop cycle
}

function escapeXml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

// ==========================================
// 1. AST & COLOR PRIMITIVES
// ==========================================

class PvgColor {
  constructor(r = 0, g = 0, b = 0, a = 255, isNone = false) {
    this.r = r;
    this.g = g;
    this.b = b;
    this.a = a;
    this.isNone = isNone;
  }

  static None() {
    return new PvgColor(0, 0, 0, 0, true);
  }

  static Black() { return new PvgColor(0, 0, 0, 255); }
  static White() { return new PvgColor(255, 255, 255, 255); }
  static Red() { return new PvgColor(255, 0, 0, 255); }
  static Green() { return new PvgColor(0, 255, 0, 255); }
  static Blue() { return new PvgColor(0, 0, 255, 255); }
  static Yellow() { return new PvgColor(255, 255, 0, 255); }
  static Cyan() { return new PvgColor(0, 255, 255, 255); }
  static Magenta() { return new PvgColor(255, 0, 255, 255); }
  static Transparent() { return new PvgColor(0, 0, 0, 0); }

  static fromHex(hex) {
    let s = hex.startsWith('#') ? hex.slice(1) : hex;
    if (s.length === 3) {
      const r = parseInt(s[0] + s[0], 16);
      const g = parseInt(s[1] + s[1], 16);
      const b = parseInt(s[2] + s[2], 16);
      if (isNaN(r) || isNaN(g) || isNaN(b)) return null;
      return new PvgColor(r, g, b, 255);
    }
    if (s.length === 6) {
      const r = parseInt(s.slice(0, 2), 16);
      const g = parseInt(s.slice(2, 4), 16);
      const b = parseInt(s.slice(4, 6), 16);
      if (isNaN(r) || isNaN(g) || isNaN(b)) return null;
      return new PvgColor(r, g, b, 255);
    }
    if (s.length === 8) {
      const r = parseInt(s.slice(0, 2), 16);
      const g = parseInt(s.slice(2, 4), 16);
      const b = parseInt(s.slice(4, 6), 16);
      const a = parseInt(s.slice(6, 8), 16);
      if (isNaN(r) || isNaN(g) || isNaN(b) || isNaN(a)) return null;
      return new PvgColor(r, g, b, a);
    }
    return null;
  }

  toRgbaString(opacityMultiplier = 1.0) {
    if (this.isNone) return 'transparent';
    const effectiveAlpha = Math.max(0, Math.min(1, (this.a / 255.0) * opacityMultiplier));
    return `rgba(${this.r}, ${this.g}, ${this.b}, ${effectiveAlpha})`;
  }

  toSvgString() {
    if (this.isNone) return 'none';
    if (this.a === 255) {
      const r = this.r.toString(16).padStart(2, '0');
      const g = this.g.toString(16).padStart(2, '0');
      const b = this.b.toString(16).padStart(2, '0');
      return `#${r}${g}${b}`;
    }
    return `rgba(${this.r}, ${this.g}, ${this.b}, ${(this.a / 255.0).toFixed(3)})`;
  }
}

// 2D Matrix Affine Transformations
class Transform2D {
  constructor(a = 1, b = 0, c = 0, d = 1, tx = 0, ty = 0) {
    this.a = a;
    this.b = b;
    this.c = c;
    this.d = d;
    this.tx = tx;
    this.ty = ty;
  }

  static identity() {
    return new Transform2D(1, 0, 0, 1, 0, 0);
  }

  mul(o) {
    return new Transform2D(
      this.a * o.a + this.c * o.b,
      this.b * o.a + this.d * o.b,
      this.a * o.c + this.c * o.d,
      this.b * o.c + this.d * o.d,
      this.a * o.tx + this.c * o.ty + this.tx,
      this.b * o.tx + this.d * o.ty + this.ty
    );
  }

  transformPoint(p) {
    return [
      this.a * p[0] + this.c * p[1] + this.tx,
      this.b * p[0] + this.d * p[1] + this.ty,
    ];
  }
}

class DrawStyle {
  constructor(fill = PvgColor.Black(), stroke = PvgColor.None(), width = 1.0, opacity = 1.0) {
    this.fill = fill;
    this.stroke = stroke;
    this.width = width;
    this.opacity = opacity;
  }

  clone() {
    return new DrawStyle(
      new PvgColor(this.fill.r, this.fill.g, this.fill.b, this.fill.a, this.fill.isNone),
      new PvgColor(this.stroke.r, this.stroke.g, this.stroke.b, this.stroke.a, this.stroke.isNone),
      this.width,
      this.opacity
    );
  }
}

// ==========================================
// 2. LEXICAL ANALYZER (TOKENIZER)
// ==========================================

const TokenKind = {
  Indent: 'Indent',
  Dedent: 'Dedent',
  Newline: 'Newline',
  Eof: 'Eof',
  Number: 'Number',
  String: 'String',
  Color: 'Color',
  Ident: 'Ident',

  // Keywords
  Pvg: 'Pvg',
  Canvas: 'Canvas',
  Background: 'Background',
  Set: 'Set',
  Def: 'Def',
  Return: 'Return',
  For: 'For',
  From: 'From',
  To: 'To',
  Step: 'Step',
  While: 'While',
  If: 'If',
  Else: 'Else',
  Seed: 'Seed',

  // Shape & Visual Primitives
  Circle: 'Circle',
  Ellipse: 'Ellipse',
  Rectangle: 'Rectangle',
  Line: 'Line',
  Polygon: 'Polygon',
  Path: 'Path',
  Text: 'Text',
  Group: 'Group',

  // Properties
  Center: 'Center',
  Radius: 'Radius',
  Pos: 'Pos',
  Size: 'Size',
  Points: 'Points',
  Content: 'Content',
  Font: 'Font',
  Align: 'Align',
  Fill: 'Fill',
  Stroke: 'Stroke',
  Width: 'Width',
  Opacity: 'Opacity',
  Rot: 'Rot',
  Scale: 'Scale',

  // Path Commands
  Start: 'Start',
  Quad: 'Quad',
  Curve: 'Curve',
  Arc: 'Arc',
  Close: 'Close',

  // Operators & Symbols
  LBracket: '[',
  RBracket: ']',
  LParen: '(',
  RParen: ')',
  Comma: ',',
  Question: '?',
  Colon: ':',
  Plus: '+',
  Minus: '-',
  Star: '*',
  Slash: '/',
  Percent: '%',
  Caret: '^',
  Equal: '=',
  EqualEqual: '==',
  NotEqual: '!=',
  Less: '<',
  LessEqual: '<=',
  Greater: '>',
  GreaterEqual: '>=',
  And: 'and',
  Or: 'or',
  Not: 'not',
};

class Token {
  constructor(kind, value, line, col) {
    this.kind = kind;
    this.value = value;
    this.line = line;
    this.col = col;
  }
}

class Lexer {
  constructor(source) {
    this.source = dedentCode(source);
    this.lines = this.source.split(/\r?\n/);
    this.currentLineIdx = 0;
    this.indentStack = [0];
  }

  tokenizeAll() {
    const tokens = [];

    while (this.currentLineIdx < this.lines.length) {
      const rawLine = this.lines[this.currentLineIdx];
      const lineNum = this.currentLineIdx + 1;
      this.currentLineIdx++;

      const trimmed = rawLine.trimStart();
      if (trimmed.length === 0 || trimmed.startsWith('#')) {
        continue;
      }

      if (rawLine.includes('\t')) {
        throw new Error(`Line ${lineNum}: Tabs are forbidden. Use 2 spaces for indentation.`);
      }

      let spaces = 0;
      while (spaces < rawLine.length && rawLine[spaces] === ' ') {
        spaces++;
      }

      const currentIndent = this.indentStack[this.indentStack.length - 1];
      if (spaces > currentIndent) {
        this.indentStack.push(spaces);
        tokens.push(new Token(TokenKind.Indent, null, lineNum, spaces + 1));
      } else if (spaces < currentIndent) {
        while (this.indentStack.length > 0 && spaces < this.indentStack[this.indentStack.length - 1]) {
          this.indentStack.pop();
          tokens.push(new Token(TokenKind.Dedent, null, lineNum, spaces + 1));
        }
        if (spaces !== this.indentStack[this.indentStack.length - 1]) {
          throw new Error(`Line ${lineNum}: Inconsistent indentation level.`);
        }
      }

      const content = rawLine.slice(spaces);
      const lineTokens = this.tokenizeLine(content, lineNum, spaces + 1);
      tokens.push(...lineTokens);
      tokens.push(new Token(TokenKind.Newline, null, lineNum, rawLine.length + 1));
    }

    while (this.indentStack.length > 1) {
      this.indentStack.pop();
      tokens.push(new Token(TokenKind.Dedent, null, this.lines.length || 1, 1));
    }

    tokens.push(new Token(TokenKind.Eof, null, this.lines.length || 1, 1));
    return tokens;
  }

  tokenizeLine(text, lineNum, colOffset) {
    const tokens = [];
    const len = text.length;
    let i = 0;

    while (i < len) {
      const c = text[i];
      if (c === ' ' || c === '\t' || c === '\r') {
        i++;
        continue;
      }

      const col = colOffset + i;

      // Single line comments or Hex color
      if (c === '#') {
        let hexEnd = i + 1;
        while (hexEnd < len && /[0-9a-fA-F]/.test(text[hexEnd])) {
          hexEnd++;
        }
        const hexLen = hexEnd - (i + 1);
        if (hexLen === 3 || hexLen === 6 || hexLen === 8) {
          const isDelim = hexEnd === len || /[\s\],):]/.test(text[hexEnd]);
          if (isDelim) {
            const hexStr = text.slice(i, hexEnd);
            const color = PvgColor.fromHex(hexStr);
            if (color) {
              tokens.push(new Token(TokenKind.Color, color, lineNum, col));
              i = hexEnd;
              continue;
            }
          }
        }
        break;
      }

      // Single character delimiters
      if (c === '[') { tokens.push(new Token(TokenKind.LBracket, '[', lineNum, col)); i++; continue; }
      if (c === ']') { tokens.push(new Token(TokenKind.RBracket, ']', lineNum, col)); i++; continue; }
      if (c === '(') { tokens.push(new Token(TokenKind.LParen, '(', lineNum, col)); i++; continue; }
      if (c === ')') { tokens.push(new Token(TokenKind.RParen, ')', lineNum, col)); i++; continue; }
      if (c === ',') { tokens.push(new Token(TokenKind.Comma, ',', lineNum, col)); i++; continue; }
      if (c === '?') { tokens.push(new Token(TokenKind.Question, '?', lineNum, col)); i++; continue; }
      if (c === ':') { tokens.push(new Token(TokenKind.Colon, ':', lineNum, col)); i++; continue; }
      if (c === '^') { tokens.push(new Token(TokenKind.Caret, '^', lineNum, col)); i++; continue; }

      // Relational and logical multi-char symbols
      if (c === '=') {
        if (i + 1 < len && text[i + 1] === '=') {
          tokens.push(new Token(TokenKind.EqualEqual, '==', lineNum, col));
          i += 2;
        } else {
          tokens.push(new Token(TokenKind.Equal, '=', lineNum, col));
          i++;
        }
        continue;
      }
      if (c === '!') {
        if (i + 1 < len && text[i + 1] === '=') {
          tokens.push(new Token(TokenKind.NotEqual, '!=', lineNum, col));
          i += 2;
        } else {
          tokens.push(new Token(TokenKind.Not, 'not', lineNum, col));
          i++;
        }
        continue;
      }
      if (c === '<') {
        if (i + 1 < len && text[i + 1] === '=') {
          tokens.push(new Token(TokenKind.LessEqual, '<=', lineNum, col));
          i += 2;
        } else {
          tokens.push(new Token(TokenKind.Less, '<', lineNum, col));
          i++;
        }
        continue;
      }
      if (c === '>') {
        if (i + 1 < len && text[i + 1] === '=') {
          tokens.push(new Token(TokenKind.GreaterEqual, '>=', lineNum, col));
          i += 2;
        } else {
          tokens.push(new Token(TokenKind.Greater, '>', lineNum, col));
          i++;
        }
        continue;
      }
      if (c === '&' && i + 1 < len && text[i + 1] === '&') {
        tokens.push(new Token(TokenKind.And, 'and', lineNum, col));
        i += 2;
        continue;
      }
      if (c === '|' && i + 1 < len && text[i + 1] === '|') {
        tokens.push(new Token(TokenKind.Or, 'or', lineNum, col));
        i += 2;
        continue;
      }

      if (c === '+') { tokens.push(new Token(TokenKind.Plus, '+', lineNum, col)); i++; continue; }
      if (c === '-') { tokens.push(new Token(TokenKind.Minus, '-', lineNum, col)); i++; continue; }
      if (c === '*') { tokens.push(new Token(TokenKind.Star, '*', lineNum, col)); i++; continue; }
      if (c === '/') { tokens.push(new Token(TokenKind.Slash, '/', lineNum, col)); i++; continue; }
      if (c === '%') { tokens.push(new Token(TokenKind.Percent, '%', lineNum, col)); i++; continue; }

      // UTF-8 Clean String Literals
      if (c === '"') {
        i++;
        let strVal = '';
        let closed = false;
        while (i < len) {
          if (text[i] === '\\' && i + 1 < len) {
            const next = text[i + 1];
            if (next === 'n') strVal += '\n';
            else if (next === 't') strVal += '\t';
            else if (next === 'r') strVal += '\r';
            else if (next === '"') strVal += '"';
            else if (next === '\\') strVal += '\\';
            else strVal += next;
            i += 2;
          } else if (text[i] === '"') {
            closed = true;
            i++;
            break;
          } else {
            strVal += text[i];
            i++;
          }
        }
        if (!closed) {
          throw new Error(`Line ${lineNum}: Unclosed string literal.`);
        }
        tokens.push(new Token(TokenKind.String, strVal, lineNum, col));
        continue;
      }

      // Numbers with optional deg/rad unit suffix
      if (/[0-9]/.test(c) || (c === '.' && i + 1 < len && /[0-9]/.test(text[i + 1]))) {
        const start = i;
        let hasDot = false;
        while (i < len && (/[0-9]/.test(text[i]) || (!hasDot && text[i] === '.'))) {
          if (text[i] === '.') hasDot = true;
          i++;
        }
        let numVal = parseFloat(text.slice(start, i));

        if (i + 3 <= len && text.slice(i, i + 3) === 'deg') {
          numVal = (numVal * Math.PI) / 180.0;
          i += 3;
        } else if (i + 3 <= len && text.slice(i, i + 3) === 'rad') {
          i += 3;
        }

        tokens.push(new Token(TokenKind.Number, numVal, lineNum, col));
        continue;
      }

      // Identifiers, Keywords, and Color Literals
      if (/[a-zA-Z_]/.test(c)) {
        const start = i;
        while (i < len && /[a-zA-Z0-9_-]/.test(text[i])) {
          i++;
        }
        const ident = text.slice(start, i);

        let kind = TokenKind.Ident;
        let value = ident;

        switch (ident) {
          case 'PVG':
          case 'CPSVG':
            kind = TokenKind.Pvg; break;
          case 'canvas': kind = TokenKind.Canvas; break;
          case 'background': kind = TokenKind.Background; break;
          case 'set': kind = TokenKind.Set; break;
          case 'def': kind = TokenKind.Def; break;
          case 'return': kind = TokenKind.Return; break;
          case 'for': kind = TokenKind.For; break;
          case 'from': kind = TokenKind.From; break;
          case 'to': kind = TokenKind.To; break;
          case 'step': kind = TokenKind.Step; break;
          case 'while': kind = TokenKind.While; break;
          case 'if': kind = TokenKind.If; break;
          case 'else': kind = TokenKind.Else; break;
          case 'seed': kind = TokenKind.Seed; break;
          case 'circle': kind = TokenKind.Circle; break;
          case 'ellipse': kind = TokenKind.Ellipse; break;
          case 'rectangle':
          case 'rect':
            kind = TokenKind.Rectangle; break;
          case 'line': kind = TokenKind.Line; break;
          case 'polygon': kind = TokenKind.Polygon; break;
          case 'path': kind = TokenKind.Path; break;
          case 'text': kind = TokenKind.Text; break;
          case 'group': kind = TokenKind.Group; break;
          case 'center': kind = TokenKind.Center; break;
          case 'radius': kind = TokenKind.Radius; break;
          case 'pos': kind = TokenKind.Pos; break;
          case 'size': kind = TokenKind.Size; break;
          case 'points': kind = TokenKind.Points; break;
          case 'content': kind = TokenKind.Content; break;
          case 'font': kind = TokenKind.Font; break;
          case 'align': kind = TokenKind.Align; break;
          case 'fill': kind = TokenKind.Fill; break;
          case 'stroke': kind = TokenKind.Stroke; break;
          case 'width': kind = TokenKind.Width; break;
          case 'opacity': kind = TokenKind.Opacity; break;
          case 'rot': kind = TokenKind.Rot; break;
          case 'scale': kind = TokenKind.Scale; break;
          case 'start': kind = TokenKind.Start; break;
          case 'quad': kind = TokenKind.Quad; break;
          case 'curve': kind = TokenKind.Curve; break;
          case 'arc': kind = TokenKind.Arc; break;
          case 'close': kind = TokenKind.Close; break;
          case 'and': kind = TokenKind.And; break;
          case 'or': kind = TokenKind.Or; break;
          case 'not': kind = TokenKind.Not; break;
          case 'black': kind = TokenKind.Color; value = PvgColor.Black(); break;
          case 'white': kind = TokenKind.Color; value = PvgColor.White(); break;
          case 'red': kind = TokenKind.Color; value = PvgColor.Red(); break;
          case 'green': kind = TokenKind.Color; value = PvgColor.Green(); break;
          case 'blue': kind = TokenKind.Color; value = PvgColor.Blue(); break;
          case 'yellow': kind = TokenKind.Color; value = PvgColor.Yellow(); break;
          case 'cyan': kind = TokenKind.Color; value = PvgColor.Cyan(); break;
          case 'magenta': kind = TokenKind.Color; value = PvgColor.Magenta(); break;
          case 'none':
          case 'transparent':
            kind = TokenKind.Color; value = PvgColor.None(); break;
        }

        tokens.push(new Token(kind, value, lineNum, col));
        continue;
      }

      throw new Error(`Line ${lineNum}, Col ${col}: Unexpected character '${c}'`);
    }

    return tokens;
  }
}

// ==========================================
// 3. PARSER (RECURSIVE DESCENT)
// ==========================================

class Parser {
  constructor(tokens) {
    this.tokens = tokens;
    this.pos = 0;
  }

  peek() {
    return this.tokens[Math.min(this.pos, this.tokens.length - 1)];
  }

  advance() {
    const tok = this.peek();
    if (this.pos < this.tokens.length) {
      this.pos++;
    }
    return tok;
  }

  match(kind) {
    if (this.peek().kind === kind) {
      this.advance();
      return true;
    }
    return false;
  }

  expect(kind) {
    const tok = this.peek();
    if (tok.kind === kind) {
      return this.advance();
    }
    throw new Error(`Line ${tok.line}, Col ${tok.col}: Expected ${kind}, found ${tok.kind}`);
  }

  skipNewlines() {
    while (this.peek().kind === TokenKind.Newline) {
      this.advance();
    }
  }

  parseDocument() {
    this.skipNewlines();

    // 1. Header: PVG 0.1
    this.expect(TokenKind.Pvg);
    const verTok = this.advance();
    if (verTok.kind !== TokenKind.Number) {
      throw new Error(`Line ${verTok.line}: Expected version number after PVG (e.g. 0.1)`);
    }
    const version = [Math.floor(verTok.value), Math.round((verTok.value % 1) * 10)];
    this.skipNewlines();

    // 2. Canvas declaration
    this.expect(TokenKind.Canvas);
    const wTok = this.advance();
    const hTok = this.advance();
    if (wTok.kind !== TokenKind.Number || hTok.kind !== TokenKind.Number) {
      throw new Error(`Line ${wTok.line}: Expected canvas width and height numbers`);
    }
    const width = wTok.value;
    const height = hTok.value;

    let background = null;
    if (this.match(TokenKind.Newline) && this.match(TokenKind.Indent)) {
      if (this.match(TokenKind.Background)) {
        const bgTok = this.advance();
        if (bgTok.kind === TokenKind.Color) {
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
    const statements = [];
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

  parseStatement() {
    const tok = this.peek();

    switch (tok.kind) {
      case TokenKind.Set: {
        this.advance();
        const nameTok = this.advance();
        if (nameTok.kind !== TokenKind.Ident) {
          throw new Error(`Line ${tok.line}: Expected identifier name after 'set'`);
        }
        this.expect(TokenKind.Equal);
        const expr = this.parseExpression();
        return { type: 'Set', name: nameTok.value, expr };
      }
      case TokenKind.Seed: {
        this.advance();
        const seedTok = this.advance();
        return { type: 'Seed', seed: seedTok.kind === TokenKind.Number ? Math.floor(seedTok.value) : 42 };
      }
      case TokenKind.Def: {
        this.advance();
        const nameTok = this.advance();
        if (nameTok.kind !== TokenKind.Ident) {
          throw new Error(`Line ${tok.line}: Expected function name`);
        }
        this.expect(TokenKind.LParen);
        const params = [];
        if (this.peek().kind !== TokenKind.RParen) {
          while (true) {
            const pTok = this.advance();
            if (pTok.kind === TokenKind.Ident) params.push(pTok.value);
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
        return { type: 'Def', name: nameTok.value, params, body };
      }
      case TokenKind.For: {
        this.advance();
        const varTok = this.advance();
        if (varTok.kind !== TokenKind.Ident) {
          throw new Error(`Line ${tok.line}: Expected loop variable`);
        }
        this.expect(TokenKind.From);
        const from = this.parseExpression();
        this.expect(TokenKind.To);
        const to = this.parseExpression();
        let step = null;
        if (this.match(TokenKind.Step)) {
          step = this.parseExpression();
        }
        this.skipNewlines();
        const body = this.parseBlock();
        return { type: 'For', var: varTok.value, from, to, step, body };
      }
      case TokenKind.While: {
        this.advance();
        const cond = this.parseExpression();
        this.skipNewlines();
        const body = this.parseBlock();
        return { type: 'While', cond, body };
      }
      case TokenKind.If: {
        this.advance();
        const cond = this.parseExpression();
        this.skipNewlines();
        const thenBody = this.parseBlock();
        let elseBody = [];
        this.skipNewlines();
        if (this.match(TokenKind.Else)) {
          if (this.peek().kind === TokenKind.If) {
            elseBody.push(this.parseStatement());
          } else {
            this.skipNewlines();
            elseBody = this.parseBlock();
          }
        }
        return { type: 'If', cond, thenBody, elseBody };
      }
      case TokenKind.Return: {
        this.advance();
        const expr = this.parseExpression();
        return { type: 'Return', expr };
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
        const name = tok.value;
        this.advance();
        if (this.match(TokenKind.LParen)) {
          const args = [];
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
          return { type: 'Call', name, args };
        }
        throw new Error(`Line ${tok.line}: Unexpected identifier in statement position '${name}'`);
      }
      default:
        throw new Error(`Line ${tok.line}: Unexpected statement token '${tok.kind}'`);
    }
  }

  parseBlock() {
    this.expect(TokenKind.Indent);
    const statements = [];
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

  parseCircle() {
    this.expect(TokenKind.Indent);
    let center = null, radius = null, fill = null, stroke = null, width = null, opacity = null;

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
    return { type: 'Circle', center, radius, fill, stroke, width, opacity };
  }

  parseEllipse() {
    this.expect(TokenKind.Indent);
    let center = null, radius = null, fill = null, stroke = null, width = null, opacity = null;

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
    return { type: 'Ellipse', center, radius, fill, stroke, width, opacity };
  }

  parseRectangle() {
    this.expect(TokenKind.Indent);
    let pos = null, size = null, radius = null, fill = null, stroke = null, width = null, opacity = null;

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
    return { type: 'Rectangle', pos, size, radius, fill, stroke, width, opacity };
  }

  parseLine() {
    this.expect(TokenKind.Indent);
    let from = null, to = null, stroke = null, width = null, opacity = null;

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
    return { type: 'Line', from, to, stroke, width, opacity };
  }

  parsePolygon() {
    this.expect(TokenKind.Indent);
    const points = [];
    let fill = null, stroke = null, width = null, opacity = null;

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
    return { type: 'Polygon', points, fill, stroke, width, opacity };
  }

  parsePath() {
    this.expect(TokenKind.Indent);
    let fill = null, stroke = null, width = null, opacity = null;
    const commands = [];

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Set: {
          this.advance();
          const nameTok = this.advance();
          this.expect(TokenKind.Equal);
          const expr = this.parseExpression();
          commands.push({ cmd: 'Set', name: nameTok.value, expr });
          break;
        }
        case TokenKind.Fill: this.advance(); fill = this.parseExpression(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parseExpression(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Start: this.advance(); commands.push({ cmd: 'Start', pt: this.parseExpression() }); break;
        case TokenKind.Line: this.advance(); commands.push({ cmd: 'Line', pt: this.parseExpression() }); break;
        case TokenKind.Quad: {
          this.advance();
          const cp = this.parseExpression();
          const ep = this.parseExpression();
          commands.push({ cmd: 'Quad', cp, ep });
          break;
        }
        case TokenKind.Curve: {
          this.advance();
          const c1 = this.parseExpression();
          const c2 = this.parseExpression();
          const ep = this.parseExpression();
          commands.push({ cmd: 'Curve', c1, c2, ep });
          break;
        }
        case TokenKind.Arc: {
          this.advance();
          const center = this.parseExpression();
          const radius = this.parseExpression();
          const startAngle = this.parseExpression();
          const endAngle = this.parseExpression();
          commands.push({ cmd: 'Arc', center, radius, startAngle, endAngle });
          break;
        }
        case TokenKind.Close: this.advance(); commands.push({ cmd: 'Close' }); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          throw new Error(`Line ${this.peek().line}: Invalid path property/command '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    return { type: 'Path', fill, stroke, width, opacity, commands };
  }

  parseText() {
    this.expect(TokenKind.Indent);
    let pos = null, content = null, size = null, font = null, align = null;
    let fill = null, stroke = null, width = null, opacity = null;

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
    return { type: 'Text', pos, content, size, font, align, fill, stroke, width, opacity };
  }

  parseGroup() {
    this.expect(TokenKind.Indent);
    let pos = null, rot = null, scale = null, opacity = null, fill = null, stroke = null;
    const body = [];

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
    return { type: 'Group', pos, rot, scale, opacity, fill, stroke, body };
  }

  parseExpression() {
    return this.parseTernary();
  }

  parseTernary() {
    let cond = this.parseLogicalOr();
    if (this.match(TokenKind.Question)) {
      const trueBranch = this.parseExpression();
      this.expect(TokenKind.Colon);
      const falseBranch = this.parseExpression();
      return { type: 'Ternary', cond, trueBranch, falseBranch };
    }
    return cond;
  }

  parseLogicalOr() {
    let left = this.parseLogicalAnd();
    while (this.match(TokenKind.Or)) {
      const right = this.parseLogicalAnd();
      left = { type: 'Binary', op: 'or', left, right };
    }
    return left;
  }

  parseLogicalAnd() {
    let left = this.parseEquality();
    while (this.match(TokenKind.And)) {
      const right = this.parseEquality();
      left = { type: 'Binary', op: 'and', left, right };
    }
    return left;
  }

  parseEquality() {
    let left = this.parseComparison();
    while (this.peek().kind === TokenKind.EqualEqual || this.peek().kind === TokenKind.NotEqual) {
      const op = this.advance().value;
      const right = this.parseComparison();
      left = { type: 'Binary', op, left, right };
    }
    return left;
  }

  parseComparison() {
    let left = this.parseAdditive();
    while (
      this.peek().kind === TokenKind.Less ||
      this.peek().kind === TokenKind.LessEqual ||
      this.peek().kind === TokenKind.Greater ||
      this.peek().kind === TokenKind.GreaterEqual
    ) {
      const op = this.advance().value;
      const right = this.parseAdditive();
      left = { type: 'Binary', op, left, right };
    }
    return left;
  }

  parseAdditive() {
    let left = this.parseMultiplicative();
    while (this.peek().kind === TokenKind.Plus || this.peek().kind === TokenKind.Minus) {
      const op = this.advance().value;
      const right = this.parseMultiplicative();
      left = { type: 'Binary', op, left, right };
    }
    return left;
  }

  parseMultiplicative() {
    let left = this.parsePower();
    while (
      this.peek().kind === TokenKind.Star ||
      this.peek().kind === TokenKind.Slash ||
      this.peek().kind === TokenKind.Percent
    ) {
      const op = this.advance().value;
      const right = this.parsePower();
      left = { type: 'Binary', op, left, right };
    }
    return left;
  }

  parsePower() {
    const left = this.parseUnary();
    if (this.match(TokenKind.Caret)) {
      const right = this.parsePower();
      return { type: 'Binary', op: '^', left, right };
    }
    return left;
  }

  parseUnary() {
    if (this.match(TokenKind.Minus)) {
      return { type: 'Unary', op: '-', inner: this.parseUnary() };
    }
    if (this.match(TokenKind.Not)) {
      return { type: 'Unary', op: 'not', inner: this.parseUnary() };
    }
    return this.parsePrimary();
  }

  parsePrimary() {
    const tok = this.advance();
    switch (tok.kind) {
      case TokenKind.Number:
        return { type: 'Number', value: tok.value };
      case TokenKind.String:
        return { type: 'String', value: tok.value };
      case TokenKind.Color:
        return { type: 'Color', value: tok.value };
      case TokenKind.LBracket: {
        const x = this.parseExpression();
        this.expect(TokenKind.Comma);
        const y = this.parseExpression();
        this.expect(TokenKind.RBracket);
        return { type: 'Vec2', x, y };
      }
      case TokenKind.LParen: {
        const expr = this.parseExpression();
        this.expect(TokenKind.RParen);
        return expr;
      }
      case TokenKind.Ident: {
        if (tok.value === 'true') return { type: 'Bool', value: true };
        if (tok.value === 'false') return { type: 'Bool', value: false };

        if (this.match(TokenKind.LParen)) {
          const args = [];
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
          return { type: 'Call', name: tok.value, args };
        }
        return { type: 'Ident', name: tok.value };
      }
      default:
        throw new Error(`Line ${tok.line}: Unexpected token in expression '${tok.kind}'`);
    }
  }
}

// ==========================================
// 4. EVALUATOR & PROCEDURAL RUNTIME
// ==========================================

class Evaluator {
  constructor(time = 0.0) {
    this.globals = new Map([
      ['PI', Math.PI],
      ['TAU', Math.PI * 2.0],
      ['time', time],
      ['t', time],
    ]);
    this.functions = new Map();
    this.rngState = 88172645463325252n;
    this.loopLimit = 100000;
    this.loopCount = 0;
    this.drawList = [];
    this.transformStack = [Transform2D.identity()];
    this.styleStack = [new DrawStyle()];
  }

  currentTransform() {
    return this.transformStack[this.transformStack.length - 1];
  }

  currentStyle() {
    return this.styleStack[this.styleStack.length - 1].clone();
  }

  nextRandom() {
    this.rngState ^= (this.rngState << 13n) & 0xffffffffffffffffn;
    this.rngState ^= (this.rngState >> 7n) & 0xffffffffffffffffn;
    this.rngState ^= (this.rngState << 17n) & 0xffffffffffffffffn;
    return Number(this.rngState & 0xffffffffffffffffn) / Number(0xffffffffffffffffn);
  }

  evaluateDocument(doc) {
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

  evalStmt(stmt, locals) {
    switch (stmt.type) {
      case 'Set': {
        const val = this.evalExpr(stmt.expr, locals);
        if (locals.has(stmt.name)) {
          locals.set(stmt.name, val);
        } else {
          this.globals.set(stmt.name, val);
        }
        return null;
      }
      case 'Seed': {
        const s = BigInt(stmt.seed || 42);
        this.rngState = s === 0n ? 88172645463325252n : s;
        return null;
      }
      case 'Def':
        this.functions.set(stmt.name, stmt);
        return null;
      case 'Return':
        return { isReturn: true, value: this.evalExpr(stmt.expr, locals) };
      case 'For': {
        const startVal = this.asNumber(this.evalExpr(stmt.from, locals));
        const endVal = this.asNumber(this.evalExpr(stmt.to, locals));
        let stepVal = stmt.step
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
      case 'While': {
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
      case 'If': {
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
      case 'Call': {
        const evalArgs = stmt.args.map((a) => this.evalExpr(a, locals));
        this.invokeFunction(stmt.name, evalArgs);
        return null;
      }
      case 'Circle': {
        const centerRaw = this.asVec2(this.evalExpr(stmt.center, locals));
        const radius = this.asNumber(this.evalExpr(stmt.radius, locals));
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const center = this.currentTransform().transformPoint(centerRaw);
        this.drawList.push({ type: 'Circle', center, radius, style });
        return null;
      }
      case 'Ellipse': {
        const centerRaw = this.asVec2(this.evalExpr(stmt.center, locals));
        const radiusRaw = this.asVec2(this.evalExpr(stmt.radius, locals));
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const center = this.currentTransform().transformPoint(centerRaw);
        this.drawList.push({ type: 'Ellipse', center, radius: radiusRaw, style });
        return null;
      }
      case 'Rectangle': {
        const posRaw = this.asVec2(this.evalExpr(stmt.pos, locals));
        const sizeRaw = this.asVec2(this.evalExpr(stmt.size, locals));
        const cornerRadius = stmt.radius ? this.asNumber(this.evalExpr(stmt.radius, locals)) : 0.0;
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const pos = this.currentTransform().transformPoint(posRaw);
        this.drawList.push({ type: 'Rectangle', pos, size: sizeRaw, cornerRadius, style });
        return null;
      }
      case 'Line': {
        const fromRaw = this.asVec2(this.evalExpr(stmt.from, locals));
        const toRaw = this.asVec2(this.evalExpr(stmt.to, locals));
        const style = this.currentStyle();
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const trans = this.currentTransform();
        this.drawList.push({
          type: 'Line',
          from: trans.transformPoint(fromRaw),
          to: trans.transformPoint(toRaw),
          style,
        });
        return null;
      }
      case 'Polygon': {
        const trans = this.currentTransform();
        const points = stmt.points.map((p) => trans.transformPoint(this.asVec2(this.evalExpr(p, locals))));
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        this.drawList.push({ type: 'Polygon', points, style });
        return null;
      }
      case 'Text': {
        const posRaw = this.asVec2(this.evalExpr(stmt.pos, locals));
        const content = this.asString(this.evalExpr(stmt.content, locals));
        const size = stmt.size ? this.asNumber(this.evalExpr(stmt.size, locals)) : 16.0;
        const fontFamily = stmt.font ? this.asString(this.evalExpr(stmt.font, locals)) : 'sans-serif';
        let align = 'left';
        if (stmt.align) {
          const a = this.asString(this.evalExpr(stmt.align, locals)).toLowerCase();
          if (a === 'center') align = 'center';
          else if (a === 'right') align = 'right';
          else align = 'left';
        }

        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const pos = this.currentTransform().transformPoint(posRaw);
        this.drawList.push({
          type: 'Text',
          pos,
          content,
          size,
          fontFamily,
          align,
          style,
        });
        return null;
      }
      case 'Path': {
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asColor(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asColor(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));

        const trans = this.currentTransform();
        const drawCommands = [];
        const pathLocals = new Map(locals);

        for (const cmd of stmt.commands) {
          switch (cmd.cmd) {
            case 'Set': {
              const val = this.evalExpr(cmd.expr, pathLocals);
              pathLocals.set(cmd.name, val);
              locals.set(cmd.name, val);
              break;
            }
            case 'Start': {
              const pt = trans.transformPoint(this.asVec2(this.evalExpr(cmd.pt, pathLocals)));
              drawCommands.push({ cmd: 'Start', pt });
              break;
            }
            case 'Line': {
              const pt = trans.transformPoint(this.asVec2(this.evalExpr(cmd.pt, pathLocals)));
              drawCommands.push({ cmd: 'Line', pt });
              break;
            }
            case 'Quad': {
              const cp = trans.transformPoint(this.asVec2(this.evalExpr(cmd.cp, pathLocals)));
              const ep = trans.transformPoint(this.asVec2(this.evalExpr(cmd.ep, pathLocals)));
              drawCommands.push({ cmd: 'Quad', cp, ep });
              break;
            }
            case 'Curve': {
              const c1 = trans.transformPoint(this.asVec2(this.evalExpr(cmd.c1, pathLocals)));
              const c2 = trans.transformPoint(this.asVec2(this.evalExpr(cmd.c2, pathLocals)));
              const ep = trans.transformPoint(this.asVec2(this.evalExpr(cmd.ep, pathLocals)));
              drawCommands.push({ cmd: 'Curve', c1, c2, ep });
              break;
            }
            case 'Arc': {
              const center = trans.transformPoint(this.asVec2(this.evalExpr(cmd.center, pathLocals)));
              const radius = this.asNumber(this.evalExpr(cmd.radius, pathLocals));
              const startAngle = this.asNumber(this.evalExpr(cmd.startAngle, pathLocals));
              const endAngle = this.asNumber(this.evalExpr(cmd.endAngle, pathLocals));
              drawCommands.push({ cmd: 'Arc', center, radius, startAngle, endAngle });
              break;
            }
            case 'Close':
              drawCommands.push({ cmd: 'Close' });
              break;
          }
        }

        this.drawList.push({ type: 'Path', commands: drawCommands, style });
        return null;
      }
      case 'Group': {
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

  invokeFunction(name, args) {
    const func = this.functions.get(name);
    if (!func) throw new Error(`Undefined function '${name}'`);
    if (func.params.length !== args.length) {
      throw new Error(`Function '${name}' expects ${func.params.length} arguments, got ${args.length}`);
    }

    const locals = new Map();
    for (let i = 0; i < func.params.length; i++) {
      locals.set(func.params[i], args[i]);
    }

    for (const stmt of func.body) {
      const ret = this.evalStmt(stmt, locals);
      if (ret && ret.isReturn) return ret.value;
    }
    return null;
  }

  evalExpr(expr, locals) {
    switch (expr.type) {
      case 'Number': return expr.value;
      case 'String': return expr.value;
      case 'Bool': return expr.value;
      case 'Color': return expr.value;
      case 'Vec2': {
        const x = this.asNumber(this.evalExpr(expr.x, locals));
        const y = this.asNumber(this.evalExpr(expr.y, locals));
        return [x, y];
      }
      case 'Ident': {
        if (locals.has(expr.name)) return locals.get(expr.name);
        if (this.globals.has(expr.name)) return this.globals.get(expr.name);
        throw new Error(`Undefined variable '${expr.name}'`);
      }
      case 'Unary': {
        const v = this.evalExpr(expr.inner, locals);
        if (expr.op === '-') return -this.asNumber(v);
        if (expr.op === 'not') return !this.isTruthy(v);
        throw new Error(`Unknown unary operator '${expr.op}'`);
      }
      case 'Binary': {
        const l = this.evalExpr(expr.left, locals);
        const r = this.evalExpr(expr.right, locals);
        switch (expr.op) {
          case '+': {
            if (typeof l === 'string' || typeof r === 'string') {
              return `${l}${r}`;
            }
            return this.asNumber(l) + this.asNumber(r);
          }
          case '-': return this.asNumber(l) - this.asNumber(r);
          case '*': return this.asNumber(l) * this.asNumber(r);
          case '/': {
            const denom = this.asNumber(r);
            return denom === 0.0 ? 0.0 : this.asNumber(l) / denom;
          }
          case '%': return this.asNumber(l) % this.asNumber(r);
          case '^': return Math.pow(this.asNumber(l), this.asNumber(r));
          case '==': return l === r;
          case '!=': return l !== r;
          case '<': return this.asNumber(l) < this.asNumber(r);
          case '<=': return this.asNumber(l) <= this.asNumber(r);
          case '>': return this.asNumber(l) > this.asNumber(r);
          case '>=': return this.asNumber(l) >= this.asNumber(r);
          case 'and': return this.isTruthy(l) && this.isTruthy(r);
          case 'or': return this.isTruthy(l) || this.isTruthy(r);
          default:
            throw new Error(`Unknown binary operator '${expr.op}'`);
        }
      }
      case 'Ternary':
        return this.isTruthy(this.evalExpr(expr.cond, locals))
          ? this.evalExpr(expr.trueBranch, locals)
          : this.evalExpr(expr.falseBranch, locals);
      case 'Call': {
        const args = expr.args.map((a) => this.evalExpr(a, locals));
        switch (expr.name) {
          case 'sin': return Math.sin(this.asNumber(args[0]));
          case 'cos': return Math.cos(this.asNumber(args[0]));
          case 'tan': return Math.tan(this.asNumber(args[0]));
          case 'sqrt': return Math.sqrt(this.asNumber(args[0]));
          case 'abs': return Math.abs(this.asNumber(args[0]));
          case 'floor': return Math.floor(this.asNumber(args[0]));
          case 'ceil': return Math.ceil(this.asNumber(args[0]));
          case 'round': return Math.round(this.asNumber(args[0]));
          case 'min': return Math.min(this.asNumber(args[0]), this.asNumber(args[1]));
          case 'max': return Math.max(this.asNumber(args[0]), this.asNumber(args[1]));
          case 'pow': return Math.pow(this.asNumber(args[0]), this.asNumber(args[1]));
          case 'random': {
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

  asNumber(val) {
    if (typeof val === 'number') return val;
    if (typeof val === 'boolean') return val ? 1.0 : 0.0;
    throw new Error(`Expected number, got ${JSON.stringify(val)}`);
  }

  asString(val) {
    if (typeof val === 'string') return val;
    if (typeof val === 'number') return val.toString();
    if (typeof val === 'boolean') return val.toString();
    throw new Error(`Expected string or displayable value, got ${JSON.stringify(val)}`);
  }

  asVec2(val) {
    if (Array.isArray(val) && val.length === 2 && typeof val[0] === 'number' && typeof val[1] === 'number') {
      return val;
    }
    throw new Error(`Expected [x, y] vector, got ${JSON.stringify(val)}`);
  }

  asColor(val) {
    if (val instanceof PvgColor) return val;
    throw new Error(`Expected color value, got ${JSON.stringify(val)}`);
  }

  isTruthy(val) {
    if (typeof val === 'boolean') return val;
    if (typeof val === 'number') return val !== 0.0;
    if (typeof val === 'string') return val.length > 0;
    return val != null;
  }
}

// ==========================================
// 5. CANVAS & SVG RENDER PIPELINE
// ==========================================

function compilePVG(source, time = 0.0) {
  const cleanSource = dedentCode(source);
  const lexer = new Lexer(cleanSource);
  const tokens = lexer.tokenizeAll();
  const parser = new Parser(tokens);
  const ast = parser.parseDocument();
  const evaluator = new Evaluator(time);
  return evaluator.evaluateDocument(ast);
}

function renderDrawListToCanvas(ctx, drawList, originX, originY, zoom) {
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
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';

    switch (cmd.type) {
      case 'Circle': {
        ctx.beginPath();
        ctx.arc(cmd.center[0], cmd.center[1], cmd.radius, 0, Math.PI * 2);
        if (hasFill) ctx.fill();
        if (hasStroke) ctx.stroke();
        break;
      }
      case 'Ellipse': {
        ctx.beginPath();
        ctx.ellipse(cmd.center[0], cmd.center[1], cmd.radius[0], cmd.radius[1], 0, 0, Math.PI * 2);
        if (hasFill) ctx.fill();
        if (hasStroke) ctx.stroke();
        break;
      }
      case 'Rectangle': {
        const [x, y] = cmd.pos;
        const [w, h] = cmd.size;
        const r = Math.max(0, Math.min(cmd.cornerRadius, w / 2, h / 2));

        ctx.beginPath();
        if (r > 0) {
          if (ctx.roundRect) {
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
      case 'Line': {
        ctx.beginPath();
        ctx.moveTo(cmd.from[0], cmd.from[1]);
        ctx.lineTo(cmd.to[0], cmd.to[1]);
        if (hasStroke) ctx.stroke();
        break;
      }
      case 'Polygon': {
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
      case 'Text': {
        const [x, y] = cmd.pos;
        const sizePx = cmd.size;
        let fontFam = cmd.fontFamily || 'sans-serif';
        const fLower = fontFam.toLowerCase();
        if (fLower === 'mono' || fLower === 'monospace' || fLower === 'code') {
          fontFam = '"Fira Code", "JetBrains Mono", Consolas, monospace';
        } else if (fLower === 'sans' || fLower === 'sans-serif') {
          fontFam = 'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
        } else if (fLower === 'serif') {
          fontFam = 'Georgia, "Times New Roman", serif';
        }

        ctx.font = `${sizePx}px ${fontFam}`;
        ctx.textAlign = cmd.align; // 'left', 'center', 'right'
        ctx.textBaseline = 'top';

        if (hasFill) {
          ctx.fillText(cmd.content, x, y);
        }
        if (hasStroke) {
          ctx.strokeText(cmd.content, x, y);
        }
        break;
      }
      case 'Path': {
        ctx.beginPath();
        for (const pCmd of cmd.commands) {
          switch (pCmd.cmd) {
            case 'Start': ctx.moveTo(pCmd.pt[0], pCmd.pt[1]); break;
            case 'Line': ctx.lineTo(pCmd.pt[0], pCmd.pt[1]); break;
            case 'Quad': ctx.quadraticCurveTo(pCmd.cp[0], pCmd.cp[1], pCmd.ep[0], pCmd.ep[1]); break;
            case 'Curve': ctx.bezierCurveTo(pCmd.c1[0], pCmd.c1[1], pCmd.c2[0], pCmd.c2[1], pCmd.ep[0], pCmd.ep[1]); break;
            case 'Arc': {
              const delta = pCmd.endAngle - pCmd.startAngle;
              const counterclockwise = delta < 0;
              ctx.arc(pCmd.center[0], pCmd.center[1], pCmd.radius, pCmd.startAngle, pCmd.endAngle, counterclockwise);
              break;
            }
            case 'Close': ctx.closePath(); break;
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

function formatSvgStyle(s) {
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

function emitSvgCommands(items, indent = '  ') {
  let out = '';
  for (const cmd of items) {
    switch (cmd.type) {
      case 'Circle':
        out += `${indent}<circle cx="${cmd.center[0].toFixed(2)}" cy="${cmd.center[1].toFixed(2)}" r="${cmd.radius.toFixed(2)}" ${formatSvgStyle(cmd.style)} />\n`;
        break;
      case 'Ellipse':
        out += `${indent}<ellipse cx="${cmd.center[0].toFixed(2)}" cy="${cmd.center[1].toFixed(2)}" rx="${cmd.radius[0].toFixed(2)}" ry="${cmd.radius[1].toFixed(2)}" ${formatSvgStyle(cmd.style)} />\n`;
        break;
      case 'Rectangle': {
        const rxAttr = cmd.cornerRadius > 0 ? ` rx="${cmd.cornerRadius.toFixed(2)}" ry="${cmd.cornerRadius.toFixed(2)}"` : '';
        out += `${indent}<rect x="${cmd.pos[0].toFixed(2)}" y="${cmd.pos[1].toFixed(2)}" width="${cmd.size[0].toFixed(2)}" height="${cmd.size[1].toFixed(2)}"${rxAttr} ${formatSvgStyle(cmd.style)} />\n`;
        break;
      }
      case 'Line':
        out += `${indent}<line x1="${cmd.from[0].toFixed(2)}" y1="${cmd.from[1].toFixed(2)}" x2="${cmd.to[0].toFixed(2)}" y2="${cmd.to[1].toFixed(2)}" ${formatSvgStyle(cmd.style)} />\n`;
        break;
      case 'Polygon': {
        const pts = cmd.points.map((p) => `${p[0].toFixed(2)},${p[1].toFixed(2)}`).join(' ');
        out += `${indent}<polygon points="${pts}" ${formatSvgStyle(cmd.style)} />\n`;
        break;
      }
      case 'Text': {
        let anchor = 'start';
        if (cmd.align === 'center') anchor = 'middle';
        else if (cmd.align === 'right') anchor = 'end';
        out += `${indent}<text x="${cmd.pos[0].toFixed(2)}" y="${cmd.pos[1].toFixed(2)}" font-size="${cmd.size.toFixed(2)}" font-family="${cmd.fontFamily}" text-anchor="${anchor}" dominant-baseline="hanging" ${formatSvgStyle(cmd.style)}>${escapeXml(cmd.content)}</text>\n`;
        break;
      }
      case 'Path': {
        const d = [];
        for (const pCmd of cmd.commands) {
          switch (pCmd.cmd) {
            case 'Start': d.push(`M ${pCmd.pt[0].toFixed(2)} ${pCmd.pt[1].toFixed(2)}`); break;
            case 'Line': d.push(`L ${pCmd.pt[0].toFixed(2)} ${pCmd.pt[1].toFixed(2)}`); break;
            case 'Quad': d.push(`Q ${pCmd.cp[0].toFixed(2)} ${pCmd.cp[1].toFixed(2)}, ${pCmd.ep[0].toFixed(2)} ${pCmd.ep[1].toFixed(2)}`); break;
            case 'Curve': d.push(`C ${pCmd.c1[0].toFixed(2)} ${pCmd.c1[1].toFixed(2)}, ${pCmd.c2[0].toFixed(2)} ${pCmd.c2[1].toFixed(2)}, ${pCmd.ep[0].toFixed(2)} ${pCmd.ep[0].toFixed(2)}`); break;
            case 'Arc': {
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
            case 'Close': d.push('Z'); break;
          }
        }
        out += `${indent}<path d="${d.join(' ')}" ${formatSvgStyle(cmd.style)} />\n`;
        break;
      }
    }
  }
  return out;
}

function exportToSvgString(drawList) {
  let svg = `<?xml version="1.0" encoding="UTF-8"?>\n`;
  svg += `<svg viewBox="0 0 ${drawList.canvasWidth} ${drawList.canvasHeight}" width="100%" height="100%" xmlns="http://www.w3.org/2000/svg">\n`;

  if (drawList.background && !drawList.background.isNone) {
    svg += `  <rect width="100%" height="100%" fill="${drawList.background.toSvgString()}" />\n`;
  }

  svg += emitSvgCommands(drawList.items, '  ');
  svg += `</svg>\n`;
  return svg;
}

function exportToAnimatedSvgString(sourceCode, duration = 2.0, fps = 30) {
  const totalFrames = Math.max(2, Math.round(duration * fps));
  const frames = [];

  for (let i = 0; i < totalFrames; i++) {
    const t = (i / totalFrames) * duration;
    const drawList = compilePVG(sourceCode, t);
    frames.push(drawList);
  }

  if (frames.length === 0) return '';

  const first = frames[0];
  let svg = `<?xml version="1.0" encoding="UTF-8"?>\n`;
  svg += `<svg viewBox="0 0 ${first.canvasWidth} ${first.canvasHeight}" width="100%" height="100%" xmlns="http://www.w3.org/2000/svg">\n`;

  if (first.background && !first.background.isNone) {
    svg += `  <rect width="100%" height="100%" fill="${first.background.toSvgString()}" />\n`;
  }

  const n = totalFrames;
  for (let i = 0; i < n; i++) {
    let valuesStr, keyTimesStr;
    if (i === 0) {
      const t1 = (1.0 / n).toFixed(4);
      valuesStr = 'visible;hidden';
      keyTimesStr = `0; ${t1}`;
    } else if (i === n - 1) {
      const t0 = ((n - 1.0) / n).toFixed(4);
      valuesStr = 'hidden;visible';
      keyTimesStr = `0; ${t0}`;
    } else {
      const t0 = (i / n).toFixed(4);
      const t1 = ((i + 1) / n).toFixed(4);
      valuesStr = 'hidden;visible;hidden';
      keyTimesStr = `0; ${t0}; ${t1}`;
    }

    svg += `  <g>\n`;
    svg += `    <animate attributeName="visibility" values="${valuesStr}" keyTimes="${keyTimesStr}" dur="${duration.toFixed(2)}s" repeatCount="indefinite" calcMode="discrete" />\n`;
    svg += emitSvgCommands(frames[i].items, '    ');
    svg += `  </g>\n`;
  }

  svg += `</svg>\n`;
  return svg;
}

// ==========================================
// 6. GLOBAL TICKER FOR <pvg-view> ELEMENTS
// ==========================================

class PvgTicker {
  constructor() {
    this.activeViews = new Set();
    this.rafId = null;
    this.lastTimestamp = performance.now();
    this.onFrame = this.onFrame.bind(this);
  }

  register(view) {
    this.activeViews.add(view);
    if (!this.rafId && this.activeViews.size > 0) {
      this.lastTimestamp = performance.now();
      this.rafId = requestAnimationFrame(this.onFrame);
    }
  }

  unregister(view) {
    this.activeViews.delete(view);
    if (this.activeViews.size === 0 && this.rafId) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
  }

  onFrame(timestamp) {
    for (const view of this.activeViews) {
      if (view.isConnected && view.isPlaying && view.isVisible) {
        view._handleTick(timestamp);
      }
    }
    if (this.activeViews.size > 0) {
      this.rafId = requestAnimationFrame(this.onFrame);
    } else {
      this.rafId = null;
    }
  }
}

const GLOBAL_PVG_TICKER = new PvgTicker();

// ==========================================
// 7. W3C CUSTOM ELEMENT: <pvg-view>
// ==========================================

class PvgView extends HTMLElement {
  static get observedAttributes() {
    return [
      'src',
      'code',
      'render',
      'autoplay',
      'loop',
      'fps',
      'time',
      't',
      'scale',
      'fit',
      'interactive',
      'lazy',
      'background',
    ];
  }

  constructor() {
    super();
    this.attachShadow({ mode: 'open' });

    this._sourceCode = '';
    this._currentDrawList = null;
    this._currentTime = 0.0;
    this._startTime = performance.now();
    this._lastFrameTime = 0;
    this._isPlaying = false;
    this._isVisible = true;
    this._isAnimatedDoc = false;
    this._manuallySetCode = false;

    // Pan & Zoom
    this._panX = 0;
    this._panY = 0;
    this._zoom = 1.0;
    this._isDragging = false;
    this._dragStartX = 0;
    this._dragStartY = 0;

    // Build Internal Shadow DOM
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: inline-block;
          position: relative;
          width: 100%;
          height: 100%;
          min-width: 60px;
          min-height: 60px;
          overflow: hidden;
          vertical-align: middle;
          contain: layout paint;
          box-sizing: border-box;
        }
        :host([hidden]) {
          display: none !important;
        }
        .pvg-viewport {
          width: 100%;
          height: 100%;
          display: flex;
          align-items: center;
          justify-content: center;
          position: relative;
          overflow: hidden;
        }
        canvas, svg {
          display: block;
          max-width: 100%;
          max-height: 100%;
          touch-action: none;
        }
        svg {
          width: 100%;
          height: 100%;
        }
        .overlay-error {
          position: absolute;
          inset: 0;
          background: rgba(18, 10, 14, 0.92);
          color: #ff4766;
          font-family: ui-monospace, SFMono-Regular, Consolas, "Courier New", monospace;
          font-size: 11px;
          padding: 10px;
          box-sizing: border-box;
          white-space: pre-wrap;
          overflow: auto;
          z-index: 10;
          display: none;
        }
        .overlay-loading {
          position: absolute;
          inset: 0;
          background: rgba(10, 12, 16, 0.5);
          color: #00d2ff;
          font-family: system-ui, -apple-system, sans-serif;
          font-size: 12px;
          display: none;
          align-items: center;
          justify-content: center;
          z-index: 5;
        }
      </style>
      <div class="pvg-viewport" part="viewport">
        <div class="overlay-loading" part="loading">Loading PVG...</div>
        <div class="overlay-error" part="error"></div>
      </div>
    `;

    this._viewport = this.shadowRoot.querySelector('.pvg-viewport');
    this._errorOverlay = this.shadowRoot.querySelector('.overlay-error');
    this._loadingOverlay = this.shadowRoot.querySelector('.overlay-loading');
    this._canvas = null;
    this._ctx = null;

    this._onMouseDown = this._onMouseDown.bind(this);
    this._onMouseMove = this._onMouseMove.bind(this);
    this._onMouseUp = this._onMouseUp.bind(this);
    this._onWheel = this._onWheel.bind(this);
    this._onDblClick = this._onDblClick.bind(this);
  }

  get isPlaying() {
    return this._isPlaying;
  }

  get isVisible() {
    return this._isVisible;
  }

  get isAnimated() {
    return this._isAnimatedDoc;
  }

  get time() {
    return this._currentTime;
  }

  set time(val) {
    this._currentTime = Number(val) || 0.0;
    this.renderAt(this._currentTime);
  }

  get code() {
    return this._sourceCode;
  }

  set code(val) {
    this._sourceCode = dedentCode(String(val || ''));
    this._manuallySetCode = true;
    this._isAnimatedDoc =
      this._sourceCode.includes('time') ||
      this._sourceCode.includes(' t ') ||
      this._sourceCode.includes('(t)') ||
      this._sourceCode.includes('* t');
    this._setupRenderSurface();
    this.renderAt(this._currentTime);
  }

  get src() {
    return this.getAttribute('src');
  }

  set src(val) {
    if (val) this.setAttribute('src', val);
    else this.removeAttribute('src');
  }

  get renderMode() {
    return (this.getAttribute('render') || 'canvas').toLowerCase();
  }

  set renderMode(val) {
    this.setAttribute('render', val);
  }

  get fit() {
    return this.getAttribute('fit') || 'contain';
  }

  get fpsCap() {
    const attr = this.getAttribute('fps');
    return attr ? parseInt(attr, 10) : 0;
  }

  connectedCallback() {
    // 1. Intersection Observer for Lazy Rendering
    if (window.IntersectionObserver && this.getAttribute('lazy') !== 'false') {
      this._intersectionObserver = new IntersectionObserver((entries) => {
        for (const entry of entries) {
          this._isVisible = entry.isIntersecting;
          if (this._isVisible && this._isPlaying) {
            this.renderAt(this._currentTime);
          }
        }
      });
      this._intersectionObserver.observe(this);
    }

    // 2. Resize Observer for Adaptive Scaling
    if (window.ResizeObserver) {
      this._resizeObserver = new ResizeObserver(() => {
        if (this.renderMode === 'canvas') {
          this._syncCanvasSize();
          this.renderAt(this._currentTime);
        }
      });
      this._resizeObserver.observe(this);
    }

    // 3. Child Mutation Observer (watches <script type="text/pvg">)
    this._mutationObserver = new MutationObserver(() => {
      if (!this.hasAttribute('src') && !this.hasAttribute('code') && !this._manuallySetCode) {
        this.extractAndCompile();
      }
    });
    this._mutationObserver.observe(this, { childList: true, characterData: true, subtree: true });

    // 4. Interactive Events
    this._viewport.addEventListener('mousedown', this._onMouseDown);
    window.addEventListener('mousemove', this._onMouseMove);
    window.addEventListener('mouseup', this._onMouseUp);
    this._viewport.addEventListener('wheel', this._onWheel, { passive: false });
    this._viewport.addEventListener('dblclick', this._onDblClick);

    this.extractAndCompile();

    if (this.hasAttribute('autoplay') || this.hasAttribute('play')) {
      this.play();
    }
  }

  disconnectedCallback() {
    GLOBAL_PVG_TICKER.unregister(this);

    if (this._intersectionObserver) this._intersectionObserver.disconnect();
    if (this._resizeObserver) this._resizeObserver.disconnect();
    if (this._mutationObserver) this._mutationObserver.disconnect();

    this._viewport.removeEventListener('mousedown', this._onMouseDown);
    window.removeEventListener('mousemove', this._onMouseMove);
    window.removeEventListener('mouseup', this._onMouseUp);
    this._viewport.removeEventListener('wheel', this._onWheel);
    this._viewport.removeEventListener('dblclick', this._onDblClick);
  }

  attributeChangedCallback(name, oldValue, newValue) {
    if (oldValue === newValue) return;

    if (name === 'src') {
      this._fetchSrc(newValue);
    } else if (name === 'code') {
      this._sourceCode = dedentCode(newValue || '');
      this._manuallySetCode = true;
      this.extractAndCompile();
    } else if (name === 'render') {
      this._setupRenderSurface();
      this.renderAt(this._currentTime);
    } else if (name === 'time' || name === 't') {
      this._currentTime = parseFloat(newValue) || 0.0;
      this.renderAt(this._currentTime);
    } else if (name === 'autoplay') {
      if (this.hasAttribute('autoplay')) this.play();
      else this.pause();
    } else {
      this.renderAt(this._currentTime);
    }
  }

  play() {
    this._isPlaying = true;
    this._startTime = performance.now() - this._currentTime * 1000.0;
    GLOBAL_PVG_TICKER.register(this);
    this.dispatchEvent(new CustomEvent('play', { detail: { time: this._currentTime } }));
  }

  pause() {
    this._isPlaying = false;
    GLOBAL_PVG_TICKER.unregister(this);
    this.dispatchEvent(new CustomEvent('pause', { detail: { time: this._currentTime } }));
  }

  togglePlay() {
    if (this._isPlaying) this.pause();
    else this.play();
  }

  reset() {
    this._startTime = performance.now();
    this._currentTime = 0.0;
    this._panX = 0;
    this._panY = 0;
    this._zoom = 1.0;
    this.renderAt(0.0);
    this.dispatchEvent(new CustomEvent('reset'));
  }

  seek(seconds) {
    this._currentTime = Math.max(0, seconds);
    this._startTime = performance.now() - this._currentTime * 1000.0;
    this.renderAt(this._currentTime);
    this.dispatchEvent(new CustomEvent('seek', { detail: { time: this._currentTime } }));
  }

  exportSvg(options = {}) {
    const isAnimated = options.animated !== undefined ? options.animated : this._isAnimatedDoc;
    if (isAnimated && this._sourceCode) {
      const duration = options.duration || detectLoopDuration(this._sourceCode);
      const fps = options.fps || 30;
      return exportToAnimatedSvgString(this._sourceCode, duration, fps);
    }
    if (!this._currentDrawList) return '';
    return exportToSvgString(this._currentDrawList);
  }

  async toPngBlob(scale = 2) {
    if (!this._currentDrawList) return null;
    const offscreen = document.createElement('canvas');
    offscreen.width = this._currentDrawList.canvasWidth * scale;
    offscreen.height = this._currentDrawList.canvasHeight * scale;
    const offCtx = offscreen.getContext('2d');
    renderDrawListToCanvas(offCtx, this._currentDrawList, 0, 0, scale);
    return new Promise((resolve) => offscreen.toBlob(resolve, 'image/png'));
  }

  getDrawList() {
    return this._currentDrawList;
  }

  async _fetchSrc(url) {
    if (!url) return;
    this._loadingOverlay.style.display = 'flex';
    try {
      const resp = await fetch(url);
      if (!resp.ok) throw new Error(`HTTP ${resp.status}: Failed to fetch '${url}'`);
      const text = await resp.text();
      this._sourceCode = dedentCode(text);
      this.extractAndCompile();
    } catch (err) {
      this._showError(err.message);
    } finally {
      this._loadingOverlay.style.display = 'none';
    }
  }

  extractAndCompile() {
    if (this.hasAttribute('src')) return;

    if (!this.hasAttribute('code') && !this._manuallySetCode) {
      const scriptTag = this.querySelector('script[type="text/pvg"], script[type="text/plain"]');
      if (scriptTag) {
        this._sourceCode = dedentCode(scriptTag.textContent);
      } else {
        const rawText = this.textContent;
        if (rawText && rawText.trim().length > 0) {
          this._sourceCode = dedentCode(rawText);
        }
      }
    }

    if (!this._sourceCode) return;

    this._isAnimatedDoc =
      this._sourceCode.includes('time') ||
      this._sourceCode.includes(' t ') ||
      this._sourceCode.includes('(t)') ||
      this._sourceCode.includes('* t');

    this._setupRenderSurface();
    this.renderAt(this._currentTime);
  }

  _setupRenderSurface() {
    const mode = this.renderMode;
    this._viewport.innerHTML = '';
    this._viewport.appendChild(this._loadingOverlay);
    this._viewport.appendChild(this._errorOverlay);

    if (mode === 'canvas') {
      this._canvas = document.createElement('canvas');
      this._ctx = this._canvas.getContext('2d');
      this._viewport.appendChild(this._canvas);
      this._syncCanvasSize();
    }
  }

  _syncCanvasSize() {
    if (!this._canvas || !this._ctx) return;
    const dpr = parseFloat(this.getAttribute('scale')) || window.devicePixelRatio || 1;
    const w = this._viewport.clientWidth || 300;
    const h = this._viewport.clientHeight || 300;
    this._canvas.width = Math.round(w * dpr);
    this._canvas.height = Math.round(h * dpr);
    this._canvas.style.width = `${w}px`;
    this._canvas.style.height = `${h}px`;
    this._ctx.setTransform(1, 0, 0, 1, 0, 0);
    this._ctx.scale(dpr, dpr);
  }

  renderAt(time) {
    if (!this._sourceCode) return;

    const t0 = performance.now();
    try {
      this._currentDrawList = compilePVG(this._sourceCode, time);
      this._hideError();

      if (this.renderMode === 'svg') {
        this._renderSvg(this._currentDrawList);
      } else {
        this._renderCanvas(this._currentDrawList);
      }

      const elapsed = performance.now() - t0;
      this.dispatchEvent(
        new CustomEvent('render', {
          detail: {
            drawList: this._currentDrawList,
            time,
            renderTimeMs: elapsed,
          },
        })
      );
    } catch (err) {
      this._showError(err.message);
      this.dispatchEvent(new CustomEvent('error', { detail: { error: err.message } }));
    }
  }

  _renderCanvas(drawList) {
    if (!this._ctx || !this._canvas) return;

    const w = this._viewport.clientWidth;
    const h = this._viewport.clientHeight;
    this._ctx.clearRect(0, 0, w, h);

    const fit = this.fit;
    let baseZoom = 1.0;
    if (fit === 'contain') {
      baseZoom = Math.min(w / drawList.canvasWidth, h / drawList.canvasHeight);
    } else if (fit === 'cover') {
      baseZoom = Math.max(w / drawList.canvasWidth, h / drawList.canvasHeight);
    }

    const effectiveZoom = baseZoom * this._zoom;
    const originX = (w - drawList.canvasWidth * effectiveZoom) / 2 + this._panX;
    const originY = (h - drawList.canvasHeight * effectiveZoom) / 2 + this._panY;

    renderDrawListToCanvas(this._ctx, drawList, originX, originY, effectiveZoom);
  }

  _renderSvg(drawList) {
    const existingSvg = this._viewport.querySelector('svg');
    const svgStr = exportToSvgString(drawList);
    if (existingSvg) {
      const parser = new DOMParser();
      const doc = parser.parseFromString(svgStr, 'image/svg+xml');
      const newSvg = doc.querySelector('svg');
      if (newSvg) {
        this._viewport.replaceChild(newSvg, existingSvg);
      }
    } else {
      const container = document.createElement('div');
      container.innerHTML = svgStr;
      const svgEl = container.firstElementChild;
      if (svgEl) {
        this._viewport.appendChild(svgEl);
      }
    }
  }

  _handleTick(timestamp) {
    const fps = this.fpsCap;
    if (fps > 0) {
      const frameDuration = 1000.0 / fps;
      if (timestamp - this._lastFrameTime < frameDuration) {
        return;
      }
    }
    this._lastFrameTime = timestamp;

    if (this._isAnimatedDoc) {
      this._currentTime = (timestamp - this._startTime) / 1000.0;
      this.renderAt(this._currentTime);
      this.dispatchEvent(new CustomEvent('timeupdate', { detail: { time: this._currentTime } }));
    }
  }

  _showError(msg) {
    this._errorOverlay.textContent = `⚡ PVG Execution Error:\n${msg}`;
    this._errorOverlay.style.display = 'block';
  }

  _hideError() {
    this._errorOverlay.style.display = 'none';
  }

  // Interactive Viewport Events
  _onMouseDown(e) {
    if (!this.hasAttribute('interactive')) return;
    this._isDragging = true;
    this._dragStartX = e.clientX - this._panX;
    this._dragStartY = e.clientY - this._panY;
    this._viewport.style.cursor = 'grabbing';
  }

  _onMouseMove(e) {
    if (!this._isDragging) return;
    this._panX = e.clientX - this._dragStartX;
    this._panY = e.clientY - this._dragStartY;
    this.renderAt(this._currentTime);
  }

  _onMouseUp() {
    if (this._isDragging) {
      this._isDragging = false;
      this._viewport.style.cursor = this.hasAttribute('interactive') ? 'grab' : 'default';
    }
  }

  _onWheel(e) {
    if (!this.hasAttribute('interactive')) return;
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.15 : 0.85;
    this._zoom = Math.max(0.05, Math.min(20.0, this._zoom * factor));
    this.renderAt(this._currentTime);
  }

  _onDblClick() {
    if (!this.hasAttribute('interactive')) return;
    this._panX = 0;
    this._panY = 0;
    this._zoom = 1.0;
    this.renderAt(this._currentTime);
  }
}

// Register Custom Element
if (typeof customElements !== 'undefined' && !customElements.get('pvg-view')) {
  customElements.define('pvg-view', PvgView);
}

// Global API
window.PVG = {
  compile: compilePVG,
  render: renderDrawListToCanvas,
  exportSvg: exportToSvgString,
  exportAnimatedSvg: exportToAnimatedSvgString,
  detectLoopDuration,
  dedent: dedentCode,
  PvgView,
  get presets() {
    return window.PVG_PRESETS || [];
  },
};