/**
 * Procedural Vector Graphics (PVG) 0.2 - Pure Vanilla JavaScript Engine
 * Specification Conformant Lexer, Recursive Descent Parser, Evaluator & Render Pipeline
 * Implements PVG 0.2 Sections 8-12: stroke topology, gradients, blur/shadow/glow, clip, blend;
 * post-0.2 Section 18: params, noise, sprites, patterns, splines, path control flow;
 * plus Section 2.6 rgb()/rgba() and the Section 15 safety caps (100k loops, 64 stack frames, 50k primitives).
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

// PVG 0.2 Paint: { kind: 'color', color } | { kind:'linear', start, end, stops }
// | { kind:'radial', center, radius, focal, stops } | { kind:'angular', center, startAngle, stops }
function solidPaint(color) {
  return { kind: 'color', color };
}

function clonePaint(p) {
  if (!p) return solidPaint(PvgColor.Black());
  if (p.kind === 'color') {
    const c = p.color;
    return { kind: 'color', color: new PvgColor(c.r, c.g, c.b, c.a, c.isNone) };
  }
  const stops = (p.stops || []).map((s) => ({
    offset: s.offset,
    color: new PvgColor(s.color.r, s.color.g, s.color.b, s.color.a, s.color.isNone),
  }));
  if (p.kind === 'linear') return { kind: 'linear', start: [...p.start], end: [...p.end], stops };
  if (p.kind === 'radial') {
    return {
      kind: 'radial', center: [...p.center], radius: p.radius,
      focal: p.focal ? [...p.focal] : null, stops,
    };
  }
  return { kind: 'angular', center: [...p.center], startAngle: p.startAngle, stops };
}

function paintIsNone(p) {
  return p && p.kind === 'color' && p.color.isNone;
}

class DrawStyle {
  constructor(fill = PvgColor.Black(), stroke = PvgColor.None(), width = 1.0, opacity = 1.0) {
    this.fill = fill instanceof PvgColor ? solidPaint(fill) : (fill || solidPaint(PvgColor.Black()));
    this.stroke = stroke instanceof PvgColor ? solidPaint(stroke) : (stroke || solidPaint(PvgColor.None()));
    this.width = width;
    this.opacity = opacity;
    // PVG 0.2 Section 8/10/12 defaults (match Rust DrawStyle::default)
    this.cap = 'butt';       // butt | round | square
    this.join = 'miter';     // miter | round | bevel
    this.miter = 4.0;
    this.dash = [];          // [] = solid
    this.strokeAlign = 'center'; // center | inside | outside
    this.blend = 'normal';   // normal | add | multiply | screen | overlay
    this.blur = 0.0;
    this.shadow = null;      // { offset:[dx,dy], radius, color }
    this.glow = null;        // { radius, color }
  }

  clone() {
    const s = new DrawStyle();
    s.fill = clonePaint(this.fill);
    s.stroke = clonePaint(this.stroke);
    s.width = this.width;
    s.opacity = this.opacity;
    s.cap = this.cap;
    s.join = this.join;
    s.miter = this.miter;
    s.dash = [...this.dash];
    s.strokeAlign = this.strokeAlign;
    s.blend = this.blend;
    s.blur = this.blur;
    s.shadow = this.shadow
      ? { offset: [...this.shadow.offset], radius: this.shadow.radius,
          color: new PvgColor(this.shadow.color.r, this.shadow.color.g, this.shadow.color.b, this.shadow.color.a, this.shadow.color.isNone) }
      : null;
    s.glow = this.glow
      ? { radius: this.glow.radius,
          color: new PvgColor(this.glow.color.r, this.glow.color.g, this.glow.color.b, this.glow.color.a, this.glow.color.isNone) }
      : null;
    return s;
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
  Clip: 'Clip',

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

  // PVG 0.2 Section 8-12 properties & paints
  Cap: 'Cap',
  Join: 'Join',
  Miter: 'Miter',
  Dash: 'Dash',
  Blend: 'Blend',
  Blur: 'Blur',
  Shadow: 'Shadow',
  Glow: 'Glow',
  Linear: 'Linear',
  Radial: 'Radial',
  Angular: 'Angular',
  Stop: 'Stop',

  // Post-0.2 Section 18 keywords (soft: still legal as identifiers)
  Snap: 'Snap',
  Filter: 'Filter',
  Param: 'Param',
  Pattern: 'Pattern',
  Sprite: 'Sprite',
  Spline: 'Spline',
  Palette: 'Palette',
  Data: 'Data',
  Row: 'Row',

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
      if (tripleOpenerBeforeComment(content)) {
        this.tokenizeLineWithTriple(content, lineNum, spaces + 1, spaces, tokens);
      } else {
        tokens.push(...this.tokenizeLine(content, lineNum, spaces + 1));
      }
      tokens.push(new Token(TokenKind.Newline, null, lineNum, rawLine.length + 1));
    }

    while (this.indentStack.length > 1) {
      this.indentStack.pop();
      tokens.push(new Token(TokenKind.Dedent, null, this.lines.length || 1, 1));
    }

    tokens.push(new Token(TokenKind.Eof, null, this.lines.length || 1, 1));
    return tokens;
  }

  /**
   * Lexes a line containing a `"""` opener. Emits the prefix tokens, one raw
   * multi-line String token, then consumes the continuation lines up to and
   * including the closing `"""` line (a trailing `# comment` is allowed on both).
   * Port of `tokenize_line_with_triple` in `pvg/src/lexer.rs`.
   */
  tokenizeLineWithTriple(content, lineNum, colOffset, baseSpaces, tokens) {
    const open = content.indexOf('"""');
    const prefix = content.slice(0, open);
    if (prefix.trim().length > 0) {
      tokens.push(...this.tokenizeLine(prefix, lineNum, colOffset));
    }
    const openCol = colOffset + open;
    const afterOpen = content.slice(open + 3);

    // Opener and closer on the same line: `data """..11.."""`.
    const close = afterOpen.indexOf('"""');
    if (close >= 0) {
      tokens.push(new Token(TokenKind.String, afterOpen.slice(0, close), lineNum, openCol));
      const tail = afterOpen.slice(close + 3);
      if (tail.trim().length > 0) {
        tokens.push(...this.tokenizeLine(tail, lineNum, openCol + 3 + close + 3));
      }
      return;
    }

    const openTail = afterOpen.trimStart();
    if (!(openTail.length === 0 || openTail.startsWith('#'))) {
      throw new Error(
        `Line ${lineNum}, Col ${openCol}: Multi-line """ strings must start at the end of the line (e.g. \`data """\`).`
      );
    }

    const body = [];
    for (;;) {
      const nextIdx = this.currentLineIdx;
      if (nextIdx >= this.lines.length) {
        throw new Error(`Line ${lineNum}, Col ${openCol}: Unclosed triple-quoted string.`);
      }
      const raw = this.lines[nextIdx];
      this.currentLineIdx++;

      let sp = 0;
      while (sp < raw.length && raw[sp] === ' ') sp++;

      if (raw.slice(sp).startsWith('"""')) {
        const closerTail = raw.slice(sp + 3).trimStart();
        if (!(closerTail.length === 0 || closerTail.startsWith('#'))) {
          throw new Error(`Line ${nextIdx + 1}, Col ${sp + 4}: Unexpected content after closing """.`);
        }
        break;
      }

      // Dedent continuation lines by the opener's block indent so sprite art
      // and text blocks read naturally in the source.
      let cut = 0;
      while (cut < baseSpaces && cut < raw.length && raw[cut] === ' ') cut++;
      body.push(raw.slice(cut));
    }

    tokens.push(new Token(TokenKind.String, body.join('\n'), lineNum, openCol));
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
          // PVG 0.2 reserved keywords (Section 2.7)
          case 'clip': kind = TokenKind.Clip; break;
          case 'cap': kind = TokenKind.Cap; break;
          case 'join': kind = TokenKind.Join; break;
          case 'miter': kind = TokenKind.Miter; break;
          case 'dash': kind = TokenKind.Dash; break;
          case 'blend': kind = TokenKind.Blend; break;
          case 'blur': kind = TokenKind.Blur; break;
          case 'shadow': kind = TokenKind.Shadow; break;
          case 'glow': kind = TokenKind.Glow; break;
          case 'linear': kind = TokenKind.Linear; break;
          case 'radial': kind = TokenKind.Radial; break;
          case 'angular':
          case 'conic': kind = TokenKind.Angular; break;
          case 'stop': kind = TokenKind.Stop; break;
          // Post-0.2 Section 18 keywords (soft keywords: accepted as names too)
          case 'snap': kind = TokenKind.Snap; break;
          case 'filter': kind = TokenKind.Filter; break;
          case 'param': kind = TokenKind.Param; break;
          case 'pattern': kind = TokenKind.Pattern; break;
          case 'sprite': kind = TokenKind.Sprite; break;
          case 'spline': kind = TokenKind.Spline; break;
          case 'palette': kind = TokenKind.Palette; break;
          case 'data': kind = TokenKind.Data; break;
          case 'row': kind = TokenKind.Row; break;
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

/**
 * Post-0.2 "soft" keyword names. These open new syntax in statement and
 * property position, but older documents use some as ordinary variable names
 * (`for row from 0 to 7`). Wherever an *identifier* is expected they are
 * accepted as names (mirrors `soft_ident` in `pvg/src/parser.rs`).
 */
function softIdentKind(kind, value) {
  switch (kind) {
    case TokenKind.Ident:
      return typeof value === 'string' ? value : null;
    case TokenKind.Row: return 'row';
    case TokenKind.Data: return 'data';
    case TokenKind.Filter: return 'filter';
    case TokenKind.Snap: return 'snap';
    case TokenKind.Palette: return 'palette';
    case TokenKind.Param: return 'param';
    case TokenKind.Pattern: return 'pattern';
    case TokenKind.Sprite: return 'sprite';
    case TokenKind.Spline: return 'spline';
    default:
      return null;
  }
}

/**
 * Reports whether a `"""` opener appears on this line *before* any real `#`
 * comment start (mirroring the hex-color rule so `#fff` colors don't count as
 * comments). Port of `triple_opener_before_comment` in `pvg/src/lexer.rs`.
 */
function tripleOpenerBeforeComment(content) {
  const open = content.indexOf('"""');
  if (open < 0) return false;
  let i = 0;
  while (i < open) {
    if (content[i] === '#') {
      let hexEnd = i + 1;
      while (hexEnd < content.length && /[0-9a-fA-F]/.test(content[hexEnd])) hexEnd++;
      const hexLen = hexEnd - (i + 1);
      if (hexLen === 3 || hexLen === 6 || hexLen === 8) {
        const isDelim =
          hexEnd === content.length ||
          /\s/.test(content[hexEnd]) ||
          [']', ')', ',', ':', '"'].includes(content[hexEnd]);
        if (isDelim) {
          i = hexEnd;
          continue;
        }
      }
      // A real comment starts here; the `"""` is inside it.
      return false;
    }
    i++;
  }
  return true;
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

    // 1. Header: PVG 0.1 / 0.2 (0.2 readers accept both; 0.1 docs evaluate identically)
    this.expect(TokenKind.Pvg);
    const verTok = this.advance();
    if (verTok.kind !== TokenKind.Number) {
      throw new Error(`Line ${verTok.line}: Expected version number after PVG (e.g. 0.2)`);
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
    let snap = 0;
    let pixelFilter = 'linear';
    if (this.match(TokenKind.Newline) && this.match(TokenKind.Indent)) {
      while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
        if (this.peek().kind === TokenKind.Newline) {
          this.advance();
          continue;
        }
        if (this.match(TokenKind.Background)) {
          const bgTok = this.advance();
          if (bgTok.kind === TokenKind.Color) {
            background = bgTok.value;
          } else {
            throw new Error(`Line ${bgTok.line}: Expected color for canvas background`);
          }
        } else if (this.match(TokenKind.Snap)) {
          const sTok = this.advance();
          if (sTok.kind !== TokenKind.Number) {
            throw new Error(`Line ${sTok.line}: Expected number for canvas snap grid`);
          }
          snap = Math.max(0, sTok.value);
        } else if (this.match(TokenKind.Filter)) {
          const fTok = this.advance();
          const name = typeof fTok.value === 'string' ? fTok.value.toLowerCase() : '';
          pixelFilter = name === 'nearest' ? 'nearest' : 'linear';
        } else {
          throw new Error(`Line ${this.peek().line}: Invalid canvas property '${this.peek().kind}'`);
        }
        this.skipNewlines();
      }
      this.match(TokenKind.Dedent);
    }
    this.skipNewlines();

    // 3. Top-level declarations (`param` uniforms, `pattern` tiles) then body.
    const params = [];
    const patterns = [];
    const statements = [];
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

  /** `param <name> : <expr>` or `param <name> = <expr>` (section 18.1). */
  parseParamDecl() {
    this.expect(TokenKind.Param);
    const nameTok = this.advance();
    const name = softIdentKind(nameTok.kind, nameTok.value);
    if (name === null) {
      throw new Error(`Line ${nameTok.line}: Expected param name, found '${nameTok.kind}'`);
    }
    if (!(this.match(TokenKind.Colon) || this.match(TokenKind.Equal))) {
      throw new Error(
        `Line ${this.peek().line}: Expected ':' or '=' after param name (e.g. \`param health: 0.75\`)`
      );
    }
    return { name, default: this.parseExpression() };
  }

  /** `pattern <name> <w> <h>` + indented body block (section 18.4). */
  parsePatternDef() {
    this.expect(TokenKind.Pattern);
    const nameTok = this.advance();
    const name = softIdentKind(nameTok.kind, nameTok.value);
    if (name === null) {
      throw new Error(`Line ${nameTok.line}: Expected pattern name, found '${nameTok.kind}'`);
    }
    const wTok = this.advance();
    const hTok = this.advance();
    if (wTok.kind !== TokenKind.Number || hTok.kind !== TokenKind.Number) {
      throw new Error(`Line ${wTok.line}: Expected pattern tile width and height numbers`);
    }
    this.skipNewlines();
    return { name, width: wTok.value, height: hTok.value, body: this.parseBlock() };
  }

  parseStatement() {
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
        return { type: 'Set', name, expr };
      }
      case TokenKind.Seed: {
        this.advance();
        const seedTok = this.advance();
        return { type: 'Seed', seed: seedTok.kind === TokenKind.Number ? Math.floor(seedTok.value) : 0 };
      }
      case TokenKind.Def: {
        this.advance();
        const nameTok = this.advance();
        const fnName = softIdentKind(nameTok.kind, nameTok.value);
        if (fnName === null) {
          throw new Error(`Line ${tok.line}: Expected function name`);
        }
        this.expect(TokenKind.LParen);
        const params = [];
        if (this.peek().kind !== TokenKind.RParen) {
          while (true) {
            const pTok = this.advance();
            const pName = softIdentKind(pTok.kind, pTok.value);
            if (pName !== null) params.push(pName);
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
        return { type: 'Def', name: fnName, params, body };
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
        let step = null;
        if (this.match(TokenKind.Step)) {
          step = this.parseExpression();
        }
        this.skipNewlines();
        const body = this.parseBlock();
        return { type: 'For', var: varName, from, to, step, body };
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
        // host may override; evaluates identically at runtime (Rust parity).
        // Top-level `param` is handled by parseDocument (host uniforms).
        const decl = this.parseParamDecl();
        return { type: 'Set', name: decl.name, expr: decl.default };
      }
      default: {
        // `pattern` tiles must be declared at top level (Rust parity).
        if (tok.kind === TokenKind.Pattern) {
          throw new Error(`Line ${tok.line}: Pattern blocks must be declared at top level.`);
        }
        // Plain identifiers and soft keywords alike: a function call.
        const softName = softIdentKind(tok.kind, tok.value);
        if (softName !== null && this.tokens[this.pos + 1] && this.tokens[this.pos + 1].kind === TokenKind.LParen) {
          this.advance();
          this.expect(TokenKind.LParen);
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
          return { type: 'Call', name: softName, args };
        }
        throw new Error(`Line ${tok.line}: Unexpected statement token '${tok.kind}'`);
      }
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

  // ---- PVG 0.2 paint & FX helpers (mirror Rust parse_paint_expr) ----
  parsePaintExpr() {
    if (this.peek().kind === TokenKind.Linear) {
      this.advance();
      const start = this.parseExpression();
      const end = this.parseExpression();
      const stops = this.tryParseGradientStops();
      return { type: 'Linear', start, end, stops };
    }
    if (this.peek().kind === TokenKind.Radial) {
      this.advance();
      const center = this.parseExpression();
      const radius = this.parseExpression();
      let focal = null;
      if (this.peek().kind === TokenKind.LBracket) {
        focal = this.parseExpression();
      }
      const stops = this.tryParseGradientStops();
      return { type: 'Radial', center, radius, focal, stops };
    }
    if (this.peek().kind === TokenKind.Angular) {
      this.advance();
      const center = this.parseExpression();
      const startAngle = this.parseExpression();
      const stops = this.tryParseGradientStops();
      return { type: 'Angular', center, startAngle, stops };
    }
    if (this.peek().kind === TokenKind.Pattern) {
      const after = this.tokens[this.pos + 1];
      const refName = after ? softIdentKind(after.kind, after.value) : null;
      if (refName !== null) {
        this.advance();
        this.advance();
        return { type: 'Pattern', name: refName };
      }
    }
    return this.parseExpression();
  }

  tryParseGradientStops() {
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
    const stops = [];
    for (;;) {
      this.skipNewlines();
      if (this.peek().kind === TokenKind.Dedent) { this.advance(); break; }
      if (this.peek().kind === TokenKind.Eof) break;
      this.expect(TokenKind.Stop);
      const offset = this.parseExpression();
      const color = this.parseExpression();
      stops.push({ type: 'GradientStop', offset, color });
      this.skipNewlines();
    }
    return stops;
  }

  parseDashArray() {
    this.expect(TokenKind.LBracket);
    const items = [];
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

  parseShadowExpr() {
    const offset = this.parseExpression();
    const radius = this.parseExpression();
    const color = this.parseExpression();
    return { type: 'ShadowExpr', offset, radius, color };
  }

  parseGlowExpr() {
    const radius = this.parseExpression();
    const color = this.parseExpression();
    return { type: 'GlowExpr', radius, color };
  }

  // Consumes one 0.2 style prop if the cursor is on it; returns true when handled.
  parseStylePropInto(out) {
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

  parseCircle() {
    this.expect(TokenKind.Indent);
    let center = null, radius = null, fill = null, stroke = null, width = null, opacity = null;
    const fx = {};

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
    return { type: 'Circle', center, radius, fill, stroke, width, opacity, ...fx };
  }

  parseEllipse() {
    this.expect(TokenKind.Indent);
    let center = null, radius = null, fill = null, stroke = null, width = null, opacity = null;
    const fx = {};

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
    return { type: 'Ellipse', center, radius, fill, stroke, width, opacity, ...fx };
  }

  parseRectangle() {
    this.expect(TokenKind.Indent);
    let pos = null, size = null, radius = null, fill = null, stroke = null, width = null, opacity = null;
    const fx = {};

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
    return { type: 'Rectangle', pos, size, radius, fill, stroke, width, opacity, ...fx };
  }

  parseLine() {
    this.expect(TokenKind.Indent);
    let from = null, to = null, stroke = null, width = null, opacity = null;
    const fx = {};

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
    return { type: 'Line', from, to, stroke, width, opacity, ...fx };
  }

  parsePolygon() {
    this.expect(TokenKind.Indent);
    const points = [];
    let fill = null, stroke = null, width = null, opacity = null;
    const fx = {};

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
    return { type: 'Polygon', points, fill, stroke, width, opacity, ...fx };
  }

  parsePath() {
    this.expect(TokenKind.Indent);
    let fill = null, stroke = null, width = null, opacity = null;
    const fx = {};
    const commands = [];

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
          commands.push({ cmd: 'Set', name, expr });
          break;
        }
        // Post-0.2 section 18.6: control flow inside a path body shares the
        // path's locals and emits path commands (style props stay outside).
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
          let step = null;
          if (this.match(TokenKind.Step)) {
            step = this.parseExpression();
          }
          this.skipNewlines();
          commands.push({ cmd: 'For', varName, from, to, step, body: this.parsePathBlock() });
          break;
        }
        case TokenKind.While: {
          this.advance();
          const cond = this.parseExpression();
          this.skipNewlines();
          commands.push({ cmd: 'While', cond, body: this.parsePathBlock() });
          break;
        }
        case TokenKind.If:
          commands.push(this.parsePathIf());
          break;
        case TokenKind.Fill: this.advance(); fill = this.parsePaintExpr(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parsePaintExpr(); break;
        case TokenKind.Width: this.advance(); width = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Cap:
        case TokenKind.Join:
        case TokenKind.Miter:
        case TokenKind.Dash:
        case TokenKind.Align:
        case TokenKind.Blur:
        case TokenKind.Shadow:
        case TokenKind.Glow:
        case TokenKind.Blend:
          this.parseStylePropInto(fx);
          break;
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
    return { type: 'Path', fill, stroke, width, opacity, commands, ...fx };
  }

  /**
   * Post-0.2 section 18.6: a `path` sub-block. Accepts only path commands,
   * `set` and nested control flow — style properties inside a control block
   * are rejected (they belong to the outer path body).
   */
  parsePathBlock() {
    this.expect(TokenKind.Indent);
    const commands = [];
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
  parsePathItem() {
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
        return { cmd: 'Set', name, expr: this.parseExpression() };
      }
      case TokenKind.Start: this.advance(); return { cmd: 'Start', pt: this.parseExpression() };
      case TokenKind.Line: this.advance(); return { cmd: 'Line', pt: this.parseExpression() };
      case TokenKind.Quad: {
        this.advance();
        const cp = this.parseExpression();
        const ep = this.parseExpression();
        return { cmd: 'Quad', cp, ep };
      }
      case TokenKind.Curve: {
        this.advance();
        const c1 = this.parseExpression();
        const c2 = this.parseExpression();
        const ep = this.parseExpression();
        return { cmd: 'Curve', c1, c2, ep };
      }
      case TokenKind.Arc: {
        this.advance();
        const center = this.parseExpression();
        const radius = this.parseExpression();
        const startAngle = this.parseExpression();
        const endAngle = this.parseExpression();
        return { cmd: 'Arc', center, radius, startAngle, endAngle };
      }
      case TokenKind.Close:
        this.advance();
        return { cmd: 'Close' };
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
        let step = null;
        if (this.match(TokenKind.Step)) {
          step = this.parseExpression();
        }
        this.skipNewlines();
        return { cmd: 'For', varName, from, to, step, body: this.parsePathBlock() };
      }
      case TokenKind.While: {
        this.advance();
        const cond = this.parseExpression();
        this.skipNewlines();
        return { cmd: 'While', cond, body: this.parsePathBlock() };
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
  parsePathIf() {
    this.expect(TokenKind.If);
    const cond = this.parseExpression();
    this.skipNewlines();
    const thenBody = this.parsePathBlock();
    let elseBody = [];
    this.skipNewlines();
    if (this.match(TokenKind.Else)) {
      if (this.peek().kind === TokenKind.If) {
        elseBody = [this.parsePathIf()];
      } else {
        this.skipNewlines();
        elseBody = this.parsePathBlock();
      }
    }
    return { cmd: 'If', cond, thenBody, elseBody };
  }

  parseText() {
    this.expect(TokenKind.Indent);
    let pos = null, content = null, size = null, font = null, align = null;
    let fill = null, stroke = null, width = null, opacity = null;
    const fx = {};

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
        // NOTE: text keeps 0.1 align (anchor); cap/join/miter/dash rejected per spec.
        case TokenKind.Blur: this.advance(); fx.blur = this.parseExpression(); break;
        case TokenKind.Shadow: this.advance(); fx.shadow = this.parseShadowExpr(); break;
        case TokenKind.Glow: this.advance(); fx.glow = this.parseGlowExpr(); break;
        case TokenKind.Blend: this.advance(); fx.blend = this.parseExpression(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          throw new Error(`Line ${this.peek().line}: Invalid text property '${this.peek().kind}'`);
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (!pos || !content) throw new Error("Text requires 'pos [x, y]' and 'content <expr>'");
    return { type: 'Text', pos, content, size, font, align, fill, stroke, width, opacity, ...fx };
  }

  parseGroup() {
    this.expect(TokenKind.Indent);
    let pos = null, rot = null, scale = null, opacity = null, fill = null, stroke = null;
    const fx = {};
    const body = [];

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Pos: this.advance(); pos = this.parseExpression(); break;
        case TokenKind.Rot: this.advance(); rot = this.parseExpression(); break;
        case TokenKind.Scale: this.advance(); scale = this.parseExpression(); break;
        case TokenKind.Opacity: this.advance(); opacity = this.parseExpression(); break;
        case TokenKind.Fill: this.advance(); fill = this.parsePaintExpr(); break;
        case TokenKind.Stroke: this.advance(); stroke = this.parsePaintExpr(); break;
        case TokenKind.Blend: this.advance(); fx.blend = this.parseExpression(); break;
        case TokenKind.Blur: this.advance(); fx.blur = this.parseExpression(); break;
        case TokenKind.Shadow: this.advance(); fx.shadow = this.parseShadowExpr(); break;
        case TokenKind.Glow: this.advance(); fx.glow = this.parseGlowExpr(); break;
        case TokenKind.Newline: this.advance(); break;
        default:
          body.push(this.parseStatement());
          break;
      }
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    return { type: 'Group', pos, rot, scale, opacity, fill, stroke, body, ...fx };
  }

  parseClip() {
    this.expect(TokenKind.Indent);
    const stmts = [];
    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      if (this.peek().kind === TokenKind.Newline) { this.advance(); continue; }
      stmts.push(this.parseStatement());
      this.skipNewlines();
    }
    this.expect(TokenKind.Dedent);
    if (stmts.length === 0) throw new Error('clip block requires a mask shape as first statement');
    const maskType = stmts[0].type;
    if (!['Circle', 'Ellipse', 'Rectangle', 'Line', 'Polygon', 'Path', 'Text'].includes(maskType)) {
      throw new Error(`clip mask must be a geometric shape, found '${maskType}'`);
    }
    return { type: 'Clip', mask: stmts[0], content: stmts.slice(1) };
  }

  /** `palette [c0, c1, ...]` (section 18.3). */
  parsePaletteArray() {
    this.expect(TokenKind.LBracket);
    const items = [];
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

  /** Section 18.3 pixel-art sprite: pos, palette, repeatable data/row strings. */
  parseSprite() {
    this.expect(TokenKind.Indent);
    let pos = null;
    let palette = [];
    const rows = [];
    let scale = null, opacity = null, blend = null;

    while (this.peek().kind !== TokenKind.Dedent && this.peek().kind !== TokenKind.Eof) {
      switch (this.peek().kind) {
        case TokenKind.Pos: this.advance(); pos = this.parseExpression(); break;
        case TokenKind.Palette: this.advance(); palette = this.parsePaletteArray(); break;
        case TokenKind.Data:
        case TokenKind.Row: {
          this.advance();
          const strTok = this.advance();
          if (strTok.kind !== TokenKind.String) {
            throw new Error(
              `Line ${strTok.line}: Expected quoted pixel row after \`data\` (e.g. data "..11.." or a """ block).`
            );
          }
          // One row per literal, or a whole triple-quoted block.
          let kept = 0;
          for (const rawRow of String(strTok.value).split('\n')) {
            const row = rawRow.endsWith('\r') ? rawRow.slice(0, -1) : rawRow;
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
    return { type: 'Sprite', pos, palette, rows, scale, opacity, blend };
  }

  /** Section 18.5 Catmull-Rom data spline (stroke-only; no fill, no align). */
  parseSpline() {
    this.expect(TokenKind.Indent);
    let points = null, pos = null, size = null, stroke = null, width = null, opacity = null;
    const fx = {};

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
      type: 'Spline',
      points, pos, size, stroke, width, opacity,
      cap: fx.cap ?? null,
      join: fx.join ?? null,
      miter: fx.miter ?? null,
      dash: fx.dash ?? null,
      blur: fx.blur ?? null,
      shadow: fx.shadow ?? null,
      glow: fx.glow ?? null,
      blend: fx.blend ?? null,
    };
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
        // Bracket list: exactly 2 scalar elements = Vec2 (back-compat for
        // positions); any other arity = Array literal. Nested compounds
        // (`[[10, 10], [90, 90]]`) are arrays. Trailing comma allowed (Section 18.5).
        const first = this.parseExpression();
        if (this.peek().kind !== TokenKind.Comma) {
          this.expect(TokenKind.RBracket);
          return { type: 'Array', items: [first] };
        }
        const items = [first];
        while (this.peek().kind === TokenKind.Comma) {
          this.advance();
          if (this.peek().kind === TokenKind.RBracket) break;
          items.push(this.parseExpression());
        }
        this.expect(TokenKind.RBracket);
        const isCompound = (e) => e.type === 'Vec2' || e.type === 'Array';
        if (items.length === 2 && !items.some(isCompound)) {
          return { type: 'Vec2', x: items[0], y: items[1] };
        }
        return { type: 'Array', items };
      }
      case TokenKind.LParen: {
        const expr = this.parseExpression();
        this.expect(TokenKind.RParen);
        return expr;
      }
      default: {
        // Plain identifiers and soft keywords (`row`, `data`, `snap`, ...)
        // alike: variables, `true`/`false`, and calls.
        const name = softIdentKind(tok.kind, tok.value);
        if (name === null) {
          throw new Error(`Line ${tok.line}: Unexpected token in expression '${tok.kind}'`);
        }
        if (name === 'true') return { type: 'Bool', value: true };
        if (name === 'false') return { type: 'Bool', value: false };

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
        return { type: 'Ident', name };
      }
    }
  }
}

// ==========================================
// 4. EVALUATOR & PROCEDURAL RUNTIME
// ==========================================

// Deterministic value noise (section 18.2) — bit-exact port of `pvg/src/eval.rs`.
// Rust uses u64 wrapping arithmetic, so BigInt is required for cross-engine parity.
const NOISE_MASK64 = 0xffffffffffffffffn;
const NOISE_U64_MAX = 18446744073709551615;

/** Maximum nested user-function call frames per evaluation (spec Section 15). */
const MAX_CALL_STACK_DEPTH = 64;
/** Maximum top-level draw commands per evaluated scene (spec Section 15). */
const MAX_SCENE_PRIMITIVES = 50000;

function noiseMulmod(a, b) {
  return (a * b) & NOISE_MASK64;
}

function hashLattice2d(ix, iy) {
  let h = noiseMulmod(BigInt.asUintN(64, BigInt(ix)), 0x9e3779b97f4a7c15n);
  h = (h ^ noiseMulmod(BigInt.asUintN(64, BigInt(iy)), 0xbf58476d1ce4e5b9n)) & NOISE_MASK64;
  h = (h ^ (h >> 30n)) & NOISE_MASK64;
  h = noiseMulmod(h, 0xbf58476d1ce4e5b9n);
  h = (h ^ (h >> 27n)) & NOISE_MASK64;
  h = noiseMulmod(h, 0x94d049bb133111ebn);
  h = (h ^ (h >> 31n)) & NOISE_MASK64;
  return Number(h) / NOISE_U64_MAX;
}

function hashLattice3d(ix, iy, iz) {
  let h = noiseMulmod(BigInt.asUintN(64, BigInt(ix)), 0x9e3779b97f4a7c15n);
  h = (h ^ noiseMulmod(BigInt.asUintN(64, BigInt(iy)), 0xbf58476d1ce4e5b9n)) & NOISE_MASK64;
  h = (h ^ noiseMulmod(BigInt.asUintN(64, BigInt(iz)), 0x94d049bb133111ebn)) & NOISE_MASK64;
  h = (h ^ (h >> 30n)) & NOISE_MASK64;
  h = noiseMulmod(h, 0xbf58476d1ce4e5b9n);
  h = (h ^ (h >> 27n)) & NOISE_MASK64;
  h = noiseMulmod(h, 0x94d049bb133111ebn);
  h = (h ^ (h >> 31n)) & NOISE_MASK64;
  return Number(h) / NOISE_U64_MAX;
}

function noiseSmooth(t) {
  return t * t * (3.0 - 2.0 * t);
}

/** Deterministic 2D value noise in [-1, 1]. */
function pvgNoise2(x, y) {
  if (!Number.isFinite(x) || !Number.isFinite(y)) return 0.0;
  const x0 = Math.floor(x);
  const y0 = Math.floor(y);
  const fx = x - x0;
  const fy = y - y0;
  const a = hashLattice2d(x0, y0);
  const b = hashLattice2d(x0 + 1, y0);
  const c = hashLattice2d(x0, y0 + 1);
  const d = hashLattice2d(x0 + 1, y0 + 1);
  const ux = noiseSmooth(fx);
  const uy = noiseSmooth(fy);
  const v = a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy;
  return v * 2.0 - 1.0;
}

/** Deterministic 3D value noise in [-1, 1]. */
function pvgNoise3(x, y, z) {
  if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z)) return 0.0;
  const x0 = Math.floor(x);
  const y0 = Math.floor(y);
  const z0 = Math.floor(z);
  const fx = x - x0;
  const fy = y - y0;
  const fz = z - z0;
  const ux = noiseSmooth(fx);
  const uy = noiseSmooth(fy);
  const uz = noiseSmooth(fz);
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

/** Catmull-Rom spline -> cubic Bezier segments [c1, c2, end]. */
function splineToBezier(points) {
  const n = points.length;
  if (n < 2) return [];
  if (n === 2) return [[points[0], points[0], points[1]]];
  const out = [];
  for (let i = 0; i < n - 1; i++) {
    const p0 = i === 0 ? points[0] : points[i - 1];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = i + 2 < n ? points[i + 2] : points[n - 1];
    out.push([
      [p1[0] + (p2[0] - p0[0]) / 6.0, p1[1] + (p2[1] - p0[1]) / 6.0],
      [p2[0] - (p3[0] - p1[0]) / 6.0, p2[1] - (p3[1] - p1[1]) / 6.0],
      p2,
    ]);
  }
  return out;
}

class Evaluator {
  constructor(time = 0.0) {
    this.globals = new Map([
      ['PI', Math.PI],
      ['TAU', Math.PI * 2.0],
      ['time', time],
      ['t', time],
      // Helpers so organic-shape examples read naturally: `deg * deg_to_rad`.
      ['deg_to_rad', Math.PI / 180.0],
      ['rad_to_deg', 180.0 / Math.PI],
    ]);
    this.functions = new Map();
    this.rngState = 88172645463325252n;
    this.loopLimit = 100000;
    this.loopCount = 0;
    /** Current nested user-function call depth (guarded by MAX_CALL_STACK_DEPTH). */
    this.callDepth = 0;
    this.drawList = [];
    this.transformStack = [Transform2D.identity()];
    this.styleStack = [new DrawStyle()];
    /** Pixel snap grid from `canvas snap` (0 = off). */
    this.snap = 0.0;
    /** Top-level pattern tile names valid for `fill pattern <name>`. */
    this.patternNames = [];
  }

  /** Host override for a declared `param` (section 18.1). */
  setParam(name, value) {
    this.globals.set(name, value);
  }

  /** Clears a host override so the document default applies again. */
  clearParam(name) {
    this.globals.delete(name);
  }

  currentTransform() {
    return this.transformStack[this.transformStack.length - 1];
  }

  currentStyle() {
    return this.styleStack[this.styleStack.length - 1].clone();
  }

  /** Pushes one top-level draw command, enforcing the scene primitive budget (Section 15). */
  pushDrawCmd(cmd) {
    if (this.drawList.length >= MAX_SCENE_PRIMITIVES) {
      throw new Error(`Exceeded scene primitive limit of ${MAX_SCENE_PRIMITIVES} draw commands`);
    }
    this.drawList.push(cmd);
  }

  nextRandom() {
    this.rngState ^= (this.rngState << 13n) & 0xffffffffffffffffn;
    this.rngState ^= (this.rngState >> 7n) & 0xffffffffffffffffn;
    this.rngState ^= (this.rngState << 17n) & 0xffffffffffffffffn;
    return Number(this.rngState & 0xffffffffffffffffn) / Number(0xffffffffffffffffn);
  }

  /** Rounds one coordinate to the active pixel grid. */
  snapVal(v) {
    if (this.snap > 0.0 && Number.isFinite(v)) {
      return Math.round(v / this.snap) * this.snap;
    }
    return v;
  }

  /** Rounds a point to the pixel grid after the world transform. */
  snapPoint(p) {
    return [this.snapVal(p[0]), this.snapVal(p[1])];
  }

  /**
   * Evaluates a document in the Rust order (sections 18.1/18.4): snap + pattern
   * names, host uniforms, pattern tiles, then the statement body.
   */
  evaluateDocument(doc) {
    this.snap = doc.canvas.snap;
    this.patternNames = (doc.patterns || []).map((p) => p.name);

    // 1. Host uniforms: declared defaults apply unless the host overrode them.
    for (const param of doc.params || []) {
      if (!this.globals.has(param.name)) {
        this.globals.set(param.name, this.evalExpr(param.default, new Map()));
      }
    }

    // 2. Pattern tiles evaluated in isolation (identity transform, default
    //    style, fresh locals) so `fill pattern name` has resolved content.
    const patterns = [];
    for (const pat of doc.patterns || []) {
      const savedDraw = this.drawList;
      const savedTrans = this.transformStack;
      const savedStyle = this.styleStack;
      this.drawList = [];
      this.transformStack = [Transform2D.identity()];
      this.styleStack = [new DrawStyle()];
      const tileLocals = new Map();
      for (const stmt of pat.body) {
        this.evalStmt(stmt, tileLocals);
      }
      const tiles = this.drawList;
      this.drawList = savedDraw;
      this.transformStack = savedTrans;
      this.styleStack = savedStyle;
      patterns.push({ name: pat.name, width: pat.width, height: pat.height, tiles });
    }

    // 3. Main scene body.
    const bodyLocals = new Map();
    for (const stmt of doc.statements) {
      this.evalStmt(stmt, bodyLocals);
    }

    return {
      canvasWidth: doc.canvas.width,
      canvasHeight: doc.canvas.height,
      background: doc.canvas.background,
      snap: doc.canvas.snap,
      pixelFilter: doc.canvas.pixelFilter,
      patterns,
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
        // `seed 0` (or a non-numeric seed, normalized to 0 by the parser)
        // selects the engine default, mirroring the Rust core.
        const s = BigInt(stmt.seed);
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
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        const center = this.snapPoint(this.currentTransform().transformPoint(centerRaw));
        this.pushDrawCmd({ type: 'Circle', center, radius, style });
        return null;
      }
      case 'Ellipse': {
        const centerRaw = this.asVec2(this.evalExpr(stmt.center, locals));
        const radiusRaw = this.asVec2(this.evalExpr(stmt.radius, locals));
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        const center = this.snapPoint(this.currentTransform().transformPoint(centerRaw));
        this.pushDrawCmd({ type: 'Ellipse', center, radius: radiusRaw, style });
        return null;
      }
      case 'Rectangle': {
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
        this.pushDrawCmd({ type: 'Rectangle', pos, size: sizeRaw, cornerRadius, style });
        return null;
      }
      case 'Line': {
        const fromRaw = this.asVec2(this.evalExpr(stmt.from, locals));
        const toRaw = this.asVec2(this.evalExpr(stmt.to, locals));
        const style = this.currentStyle();
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        const trans = this.currentTransform();
        this.pushDrawCmd({
          type: 'Line',
          from: this.snapPoint(trans.transformPoint(fromRaw)),
          to: this.snapPoint(trans.transformPoint(toRaw)),
          style,
        });
        return null;
      }
      case 'Polygon': {
        const trans = this.currentTransform();
        const points = stmt.points.map((p) => this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(p, locals)))));
        const style = this.currentStyle();
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        this.pushDrawCmd({ type: 'Polygon', points, style });
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
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals, true);

        const pos = this.snapPoint(this.currentTransform().transformPoint(posRaw));
        this.pushDrawCmd({
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
        if (stmt.fill) style.fill = this.asPaint(this.evalExpr(stmt.fill, locals));
        if (stmt.stroke) style.stroke = this.asPaint(this.evalExpr(stmt.stroke, locals));
        if (stmt.width) style.width = this.asNumber(this.evalExpr(stmt.width, locals));
        if (stmt.opacity) style.opacity *= this.asNumber(this.evalExpr(stmt.opacity, locals));
        this.applyFxProps(style, stmt, locals);

        const trans = this.currentTransform();
        const drawCommands = [];
        const pathLocals = new Map(locals);

        for (const cmd of stmt.commands) {
          this.evalPathCommand(cmd, pathLocals, trans, drawCommands, locals);
        }

        this.pushDrawCmd({ type: 'Path', commands: drawCommands, style });
        return null;
      }
      case 'Sprite': {
        // Section 18.3: palette-indexed pixel art; ignores inherited fill/stroke.
        const posRaw = this.asVec2(this.evalExpr(stmt.pos, locals));
        const palette = [];
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
        this.pushDrawCmd({ type: 'Sprite', pos, palette, rows: stmt.rows, scale, style });
        return null;
      }
      case 'Spline': {
        // Section 18.5: Catmull-Rom data spline (stroke-only).
        const raw = this.evalExpr(stmt.points, locals);
        const items = Array.isArray(raw) ? raw : [];
        const ctrl = [];
        const hasVec = items.some((v) => Array.isArray(v));
        if (hasVec) {
          for (const v of items) {
            if (Array.isArray(v)) {
              if (v.length === 2 && typeof v[0] === 'number' && typeof v[1] === 'number') {
                ctrl.push([v[0], v[1]]);
              } else {
                throw new Error('Spline points must be [x, y] vectors or numbers.');
              }
            } else if (typeof v === 'number') {
              ctrl.push([ctrl.length, v]);
            } else {
              throw new Error('Spline points must be [x, y] vectors or numbers.');
            }
          }
        } else {
          const n = items.length;
          if (n === 0) throw new Error('Spline requires at least one point.');
          const nums = items.map((v) => this.asNumber(v));
          const [ox, oy] = stmt.pos
            ? this.asVec2(this.evalExpr(stmt.pos, locals))
            : [0.0, 0.0];
          const [sw, sh] = stmt.size
            ? this.asVec2(this.evalExpr(stmt.size, locals))
            : [Math.max(2, n) - 1, 1];
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
        const points = ctrl.map((p) => this.snapPoint(trans.transformPoint(p)));
        this.pushDrawCmd({ type: 'Spline', points, style });
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
      case 'Clip': {
        // Evaluate mask in isolation (side-effect free), then content normally.
        const base = this.drawList.length;
        const scratch = new Map(locals);
        this.evalStmt(stmt.mask, scratch);
        if (this.drawList.length !== base + 1) {
          this.drawList.length = base;
          throw new Error('Clip mask must produce exactly one shape.');
        }
        const mask = this.drawList.pop();
        const contentBase = this.drawList.length;
        for (const bStmt of stmt.content) {
          this.evalStmt(bStmt, locals);
        }
        const content = this.drawList.splice(contentBase);
        this.pushDrawCmd({ type: 'Clip', mask, content });
        return null;
      }
    }
  }

  // ---- PVG 0.2 style helpers (mirror Rust eval_cap/join/align/blend/dash) ----
  /**
   * Post-0.2 section 18.6: evaluates `path` body items — geometry commands,
   * `set` (shared with the enclosing scope), and `for`/`while`/`if` control flow
   * that shares the path's locals.
   */
  evalPathCommand(cmd, pathLocals, trans, drawCommands, locals) {
    switch (cmd.cmd) {
      case 'Set': {
        const val = this.evalExpr(cmd.expr, pathLocals);
        pathLocals.set(cmd.name, val);
        locals.set(cmd.name, val);
        break;
      }
      case 'Start': {
        const pt = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.pt, pathLocals))));
        drawCommands.push({ cmd: 'Start', pt });
        break;
      }
      case 'Line': {
        const pt = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.pt, pathLocals))));
        drawCommands.push({ cmd: 'Line', pt });
        break;
      }
      case 'Quad': {
        const cp = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.cp, pathLocals))));
        const ep = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.ep, pathLocals))));
        drawCommands.push({ cmd: 'Quad', cp, ep });
        break;
      }
      case 'Curve': {
        const c1 = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.c1, pathLocals))));
        const c2 = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.c2, pathLocals))));
        const ep = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.ep, pathLocals))));
        drawCommands.push({ cmd: 'Curve', c1, c2, ep });
        break;
      }
      case 'Arc': {
        const center = this.snapPoint(trans.transformPoint(this.asVec2(this.evalExpr(cmd.center, pathLocals))));
        const radius = this.asNumber(this.evalExpr(cmd.radius, pathLocals));
        const startAngle = this.asNumber(this.evalExpr(cmd.startAngle, pathLocals));
        const endAngle = this.asNumber(this.evalExpr(cmd.endAngle, pathLocals));
        drawCommands.push({ cmd: 'Arc', center, radius, startAngle, endAngle });
        break;
      }
      case 'Close':
        drawCommands.push({ cmd: 'Close' });
        break;
      case 'For': {
        const startVal = this.asNumber(this.evalExpr(cmd.from, pathLocals));
        const endVal = this.asNumber(this.evalExpr(cmd.to, pathLocals));
        const stepVal = cmd.step
          ? this.asNumber(this.evalExpr(cmd.step, pathLocals))
          : endVal >= startVal ? 1.0 : -1.0;
        if (stepVal === 0.0) throw new Error('For loop step cannot be 0');
        let current = startVal;
        while ((stepVal > 0.0 && current <= endVal) || (stepVal < 0.0 && current >= endVal)) {
          this.loopCount++;
          if (this.loopCount > this.loopLimit) {
            throw new Error(`Exceeded safety loop limit of ${this.loopLimit} iterations`);
          }
          pathLocals.set(cmd.varName, current);
          for (const c of cmd.body) {
            this.evalPathCommand(c, pathLocals, trans, drawCommands, locals);
          }
          current += stepVal;
        }
        break;
      }
      case 'While': {
        while (this.isTruthy(this.evalExpr(cmd.cond, pathLocals))) {
          this.loopCount++;
          if (this.loopCount > this.loopLimit) {
            throw new Error(`Exceeded safety loop limit of ${this.loopLimit} iterations`);
          }
          for (const c of cmd.body) {
            this.evalPathCommand(c, pathLocals, trans, drawCommands, locals);
          }
        }
        break;
      }
      case 'If': {
        const body = this.isTruthy(this.evalExpr(cmd.cond, pathLocals)) ? cmd.thenBody : cmd.elseBody;
        for (const c of body) {
          this.evalPathCommand(c, pathLocals, trans, drawCommands, locals);
        }
        break;
      }
    }
  }

  evalCap(v) {
    const s = String(this.asString(v)).toLowerCase();
    if (s === 'round') return 'round';
    if (s === 'square') return 'square';
    return 'butt';
  }

  evalJoin(v) {
    const s = String(this.asString(v)).toLowerCase();
    if (s === 'round') return 'round';
    if (s === 'bevel') return 'bevel';
    return 'miter';
  }

  evalStrokeAlign(v) {
    const s = String(this.asString(v)).toLowerCase();
    if (s === 'inside') return 'inside';
    if (s === 'outside') return 'outside';
    return 'center';
  }

  evalBlend(v) {
    const s = String(this.asString(v)).toLowerCase();
    if (s === 'add') return 'add';
    if (s === 'multiply') return 'multiply';
    if (s === 'screen') return 'screen';
    if (s === 'overlay') return 'overlay';
    return 'normal';
  }

  evalDash(items, locals) {
    const out = [];
    for (const d of items || []) {
      const v = this.asNumber(this.evalExpr(d, locals));
      if (v > 0 && Number.isFinite(v)) out.push(v);
    }
    return out;
  }

  evalShadow(node, locals) {
    const offset = this.asVec2(this.evalExpr(node.offset, locals));
    const radius = Math.max(0, this.asNumber(this.evalExpr(node.radius, locals)));
    const color = this.asColor(this.evalExpr(node.color, locals));
    return { offset, radius, color };
  }

  evalGlow(node, locals) {
    const radius = Math.max(0, this.asNumber(this.evalExpr(node.radius, locals)));
    const color = this.asColor(this.evalExpr(node.color, locals));
    return { radius, color };
  }

  applyFxProps(style, stmt, locals, isText = false) {
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

  invokeFunction(name, args) {
    if (this.callDepth >= MAX_CALL_STACK_DEPTH) {
      throw new Error(`Exceeded call stack limit of ${MAX_CALL_STACK_DEPTH} frames`);
    }
    const func = this.functions.get(name);
    if (!func) throw new Error(`Undefined function '${name}'`);
    if (func.params.length !== args.length) {
      throw new Error(`Function '${name}' expects ${func.params.length} arguments, got ${args.length}`);
    }

    const locals = new Map();
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
      case 'Array': {
        const out = [];
        for (const item of expr.items) out.push(this.evalExpr(item, locals));
        return out;
      }
      case 'Pattern': {
        if (!this.patternNames.includes(expr.name)) {
          throw new Error(`Unknown pattern '${expr.name}'`);
        }
        return { kind: 'pattern', name: expr.name };
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
              return `${this.displayValue(l)}${this.displayValue(r)}`;
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
          // Equality and relational operators coerce via asNumber (numbers and
          // bools only, mirroring Rust `as_f64`); any other type is a runtime error.
          case '==': return this.asNumber(l) === this.asNumber(r);
          case '!=': return this.asNumber(l) !== this.asNumber(r);
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
          case 'radians': return (this.asNumber(args[0]) * Math.PI) / 180.0;
          case 'degrees': return (this.asNumber(args[0]) * 180.0) / Math.PI;
          case 'deg_to_rad': return (this.asNumber(args[0]) * Math.PI) / 180.0;
          case 'rgb': {
            if (args.length !== 3) throw new Error('rgb(r, g, b) needs 3 arguments');
            return new PvgColor(this.colorChannel(args[0]), this.colorChannel(args[1]), this.colorChannel(args[2]), 255);
          }
          case 'rgba': {
            if (args.length !== 4) throw new Error('rgba(r, g, b, a) needs 4 arguments');
            const av = this.asNumber(args[3]);
            const ab = Number.isNaN(av) ? 0 : Math.round(Math.max(0, Math.min(1, av)) * 255);
            return new PvgColor(this.colorChannel(args[0]), this.colorChannel(args[1]), this.colorChannel(args[2]), ab);
          }
          case 'noise2d': {
            if (args.length !== 2) throw new Error('noise2d(x, y) needs 2 arguments');
            return pvgNoise2(this.asNumber(args[0]), this.asNumber(args[1]));
          }
          case 'noise3d': {
            if (args.length !== 3) throw new Error('noise3d(x, y, z) needs 3 arguments');
            return pvgNoise3(this.asNumber(args[0]), this.asNumber(args[1]), this.asNumber(args[2]));
          }
          case 'array':
            return args;
          case 'len': {
            if (args.length === 0) throw new Error('len(arr) needs 1 argument');
            const v = args[0];
            if (Array.isArray(v)) return v.length;
            if (typeof v === 'string') return [...v].length;
            throw new Error('len() expects an array or string');
          }
          case 'get': {
            if (args.length !== 2) throw new Error('get(arr, i) needs 2 arguments');
            const idx = Math.trunc(this.asNumber(args[1]));
            const target = args[0];
            if (Array.isArray(target)) {
              const n = target.length;
              if (n === 0) throw new Error('get() from empty array');
              // Negative indices wrap (Python-style).
              return target[(((idx % n) + n) % n) | 0];
            }
            throw new Error('get() expects an array');
          }
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
      case 'Linear': {
        const s = this.asVec2(this.evalExpr(expr.start, locals));
        const e = this.asVec2(this.evalExpr(expr.end, locals));
        const trans = this.currentTransform();
        return {
          kind: 'linear',
          start: trans.transformPoint(s),
          end: trans.transformPoint(e),
          stops: this.evalStops(expr.stops, locals),
        };
      }
      case 'Radial': {
        const c = this.asVec2(this.evalExpr(expr.center, locals));
        const r = Math.max(0, this.asNumber(this.evalExpr(expr.radius, locals)));
        const trans = this.currentTransform();
        const f = expr.focal
          ? trans.transformPoint(this.asVec2(this.evalExpr(expr.focal, locals)))
          : null;
        return {
          kind: 'radial',
          center: trans.transformPoint(c),
          radius: r,
          focal: f,
          stops: this.evalStops(expr.stops, locals),
        };
      }
      case 'Angular': {
        const c = this.asVec2(this.evalExpr(expr.center, locals));
        const sa = this.asNumber(this.evalExpr(expr.startAngle, locals));
        const trans = this.currentTransform();
        return {
          kind: 'angular',
          center: trans.transformPoint(c),
          startAngle: sa,
          stops: this.evalStops(expr.stops, locals),
        };
      }
      default:
        throw new Error(`Unknown expression type '${expr.type}'`);
    }
  }

  evalStops(stops, locals) {
    // Offsets clamped to [0,1]; colors incl. transparent alpha preserved (Section 9.4).
    // Sorted by offset at evaluation (stable).
    const out = (stops || []).map((s) => ({
      offset: Math.max(0, Math.min(1, this.asNumber(this.evalExpr(s.offset, locals)))),
      color: this.asColor(this.evalExpr(s.color, locals)),
    }));
    out.sort((a, b) => a.offset - b.offset);
    return out;
  }

  asNumber(val) {
    if (typeof val === 'number') return val;
    if (typeof val === 'boolean') return val ? 1.0 : 0.0;
    throw new Error(`Expected number, got ${JSON.stringify(val)}`);
  }

  asString(val) {
    return this.displayValue(val);
  }

  /**
   * String display conversion for `+` concatenation and text content.
   * Mirrors Rust `Value::as_string`: strings pass through, numbers use integer
   * formatting when integral and |n| < 1e15, bools print as `true`/`false`,
   * arrays render recursively as `[a, b]`; colors, paints and `None` are
   * runtime errors (never silent `[object Object]` output).
   */
  displayValue(val) {
    if (typeof val === 'string') return val;
    if (typeof val === 'number') {
      if (Number.isNaN(val)) return 'NaN';
      if (val === Infinity) return 'inf';
      if (val === -Infinity) return '-inf';
      if (Object.is(val, -0)) return '-0';
      if (Number.isInteger(val) && Math.abs(val) < 1e15) return String(Math.trunc(val));
      return String(val);
    }
    if (typeof val === 'boolean') return val ? 'true' : 'false';
    if (Array.isArray(val)) {
      const parts = val.map((v) => {
        try {
          return this.displayValue(v);
        } catch (e) {
          return '?';
        }
      });
      return `[${parts.join(', ')}]`;
    }
    throw new Error('Expected string or displayable value');
  }

  /**
   * Rounds a color channel to the nearest integer and clamps it to [0, 255]
   * (Section 2.6 functional `rgb()` / `rgba()` form). Mirrors the Rust core's
   * `color_channel` (round-then-clamp; NaN maps to 0 via an explicit guard,
   * matching Rust's saturating float-to-int cast).
   */
  colorChannel(val) {
    const rounded = Math.round(this.asNumber(val));
    if (Number.isNaN(rounded)) return 0;
    return Math.max(0, Math.min(255, rounded));
  }

  asVec2(val) {
    if (Array.isArray(val) && val.length === 2 && typeof val[0] === 'number' && typeof val[1] === 'number') {
      return val;
    }
    throw new Error(`Expected [x, y] vector, got ${JSON.stringify(val)}`);
  }

  asColor(val) {
    if (val instanceof PvgColor) return val;
    if (val && val.kind === 'color' && val.color instanceof PvgColor) return val.color;
    throw new Error('Expected color value');
  }

  asPaint(val) {
    if (val instanceof PvgColor) return solidPaint(val);
    if (val && typeof val.kind === 'string' &&
        (val.kind === 'color' || val.kind === 'linear' || val.kind === 'radial' ||
         val.kind === 'angular' || val.kind === 'pattern')) {
      return val;
    }
    throw new Error('Expected paint (color, gradient or pattern) value');
  }

  isTruthy(val) {
    if (typeof val === 'boolean') return val;
    if (typeof val === 'number') return val !== 0.0;
    if (typeof val === 'string') return val.length > 0;
    // Empty data arrays are falsy (Rust parity); a 2-element vector array is
    // never empty so vectors stay truthy, matching Rust.
    if (Array.isArray(val)) return val.length > 0;
    return val != null;
  }
}

// ==========================================
// 5. CANVAS & SVG RENDER PIPELINE
// ==========================================

function compilePVG(source, time = 0.0, params) {
  const cleanSource = dedentCode(source);
  const lexer = new Lexer(cleanSource);
  const tokens = lexer.tokenizeAll();
  const parser = new Parser(tokens);
  const ast = parser.parseDocument();
  const evaluator = new Evaluator(time);
  // Host uniforms (section 18.1) win over the document's declared defaults.
  if (params) {
    for (const key of Object.keys(params)) {
      evaluator.setParam(key, params[key]);
    }
  }
  return evaluator.evaluateDocument(ast);
}

/** Compiles with host `param` overrides (mirrors `pvg::compile_with_params`). */
function compileWithParams(source, params, time = 0.0) {
  return compilePVG(source, time, params);
}

// ---- PVG 0.2 canvas helpers ----
/** Palette character -> index (digits, then a-z/A-Z for 10+). `.`/space = skip. */
function spriteCharIndex(ch) {
  if (ch === '.' || ch === ' ') return null;
  if (ch >= '0' && ch <= '9') return ch.charCodeAt(0) - 0x30;
  const lower = ch.toLowerCase();
  if (lower >= 'a' && lower <= 'z') return lower.charCodeAt(0) - 0x61 + 10;
  return null;
}

function paintToCanvas(ctx, paint, opacity, patterns) {
  if (!paint) return 'rgba(0,0,0,0)';
  if (paint.kind === 'color') return paint.color.toRgbaString(opacity);
  if (paint.kind === 'pattern') {
    const pat = patterns && patterns.get(paint.name);
    if (pat) return pat;
    // Missing/unknown tile resolves to neutral gray (matches native fallbacks).
    const a = Math.max(0, Math.min(1, opacity));
    return `rgba(136, 136, 136, ${a.toFixed(3)})`;
  }
  const stops = paint.stops || [];
  if (stops.length === 0) return 'rgba(0,0,0,0)';
  if (stops.length === 1) return stops[0].color.toRgbaString(opacity);
  let grad = null;
  if (paint.kind === 'linear') {
    grad = ctx.createLinearGradient(paint.start[0], paint.start[1], paint.end[0], paint.end[1]);
  } else if (paint.kind === 'radial') {
    const r = Math.max(0, paint.radius);
    const f = paint.focal || paint.center;
    try {
      grad = ctx.createRadialGradient(f[0], f[1], 0, paint.center[0], paint.center[1], Math.max(r, 0.001));
    } catch (e) { grad = null; }
  } else if (paint.kind === 'angular') {
    if (ctx.createConicGradient) {
      try {
        grad = ctx.createConicGradient(paint.startAngle, paint.center[0], paint.center[1]);
      } catch (e) { grad = null; }
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

function blendToComposite(blend) {
  switch (blend) {
    case 'add': return 'lighter';
    case 'multiply': return 'multiply';
    case 'screen': return 'screen';
    case 'overlay': return 'overlay';
    default: return 'source-over';
  }
}

function applyCanvasStyle(ctx, style, patterns) {
  ctx.fillStyle = paintToCanvas(ctx, style.fill, style.opacity, patterns);
  ctx.strokeStyle = paintToCanvas(ctx, style.stroke, style.opacity, patterns);
  ctx.lineWidth = style.width;
  ctx.lineCap = style.cap === 'round' ? 'round' : (style.cap === 'square' ? 'square' : 'butt');
  ctx.lineJoin = style.join === 'round' ? 'round' : (style.join === 'bevel' ? 'bevel' : 'miter');
  ctx.miterLimit = Math.max(1, style.miter || 4);
  try {
    ctx.setLineDash(style.dash && style.dash.length > 0 ? style.dash : []);
  } catch (e) { /* older canvas */ }
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
    ctx.shadowColor = 'rgba(0,0,0,0)';
  }
}

function traceCmdPath(ctx, cmd) {
  switch (cmd.type) {
    case 'Circle':
      ctx.beginPath();
      ctx.arc(cmd.center[0], cmd.center[1], Math.max(0, cmd.radius), 0, Math.PI * 2);
      break;
    case 'Ellipse':
      ctx.beginPath();
      ctx.ellipse(cmd.center[0], cmd.center[1], Math.abs(cmd.radius[0]), Math.abs(cmd.radius[1]), 0, 0, Math.PI * 2);
      break;
    case 'Rectangle': {
      const [x, y] = cmd.pos;
      const [w, h] = cmd.size;
      const r = Math.max(0, Math.min(cmd.cornerRadius, w / 2, h / 2));
      ctx.beginPath();
      if (r > 0) {
        if (ctx.roundRect) ctx.roundRect(x, y, w, h, r);
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
    case 'Line':
      ctx.beginPath();
      ctx.moveTo(cmd.from[0], cmd.from[1]);
      ctx.lineTo(cmd.to[0], cmd.to[1]);
      break;
    case 'Polygon':
      ctx.beginPath();
      if (cmd.points.length > 0) {
        ctx.moveTo(cmd.points[0][0], cmd.points[0][1]);
        for (let i = 1; i < cmd.points.length; i++) ctx.lineTo(cmd.points[i][0], cmd.points[i][1]);
        ctx.closePath();
      }
      break;
    case 'Path':
      ctx.beginPath();
      for (const pCmd of cmd.commands) {
        switch (pCmd.cmd) {
          case 'Start': ctx.moveTo(pCmd.pt[0], pCmd.pt[1]); break;
          case 'Line': ctx.lineTo(pCmd.pt[0], pCmd.pt[1]); break;
          case 'Quad': ctx.quadraticCurveTo(pCmd.cp[0], pCmd.cp[1], pCmd.ep[0], pCmd.ep[1]); break;
          case 'Curve': ctx.bezierCurveTo(pCmd.c1[0], pCmd.c1[1], pCmd.c2[0], pCmd.c2[1], pCmd.ep[0], pCmd.ep[1]); break;
          case 'Arc': {
            const delta = pCmd.endAngle - pCmd.startAngle;
            ctx.arc(pCmd.center[0], pCmd.center[1], Math.max(0, pCmd.radius), pCmd.startAngle, pCmd.endAngle, delta < 0);
            break;
          }
          case 'Close': ctx.closePath(); break;
        }
      }
      break;
    case 'Text':
      return false; // text uses fillText/strokeText
    case 'Sprite':
      return false; // sprites draw crisp rects (no path)
    case 'Spline': {
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

function paintCmdFillStroke(ctx, cmd, style) {
  const hasFill = !paintIsNone(style.fill);
  const hasStroke = !paintIsNone(style.stroke) && style.width > 0;
  if (cmd.type === 'Line') {
    if (hasStroke) ctx.stroke();
    return;
  }
  if (hasFill) {
    if (style.strokeAlign === 'inside' || style.strokeAlign === 'outside') {
      ctx.save();
      ctx.fill();
      ctx.restore();
    } else ctx.fill();
  }
  if (hasStroke) {
    if (style.strokeAlign === 'inside') {
      ctx.save();
      ctx.clip();
      ctx.lineWidth = style.width * 2;
      ctx.stroke();
      ctx.restore();
    } else if (style.strokeAlign === 'outside') {
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

function drawSingleCmd(ctx, cmd, patterns) {
  if (cmd.type === 'Clip') {
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
  // Glow: blurred silhouette additively underneath (Section 10.1 order shadow→glow→shape).
  if (glow && glow.radius > 0 && cmd.type !== 'Text' && cmd.type !== 'Clip' && cmd.type !== 'Sprite') {
    ctx.save();
    ctx.globalCompositeOperation = 'lighter';
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
    ctx.shadowColor = 'rgba(0,0,0,0)';
    if (style.shadow) {
      ctx.shadowOffsetX = style.shadow.offset[0];
      ctx.shadowOffsetY = style.shadow.offset[1];
      ctx.shadowBlur = Math.max(0, style.shadow.radius / 2);
      ctx.shadowColor = style.shadow.color.toRgbaString(1);
    }
  }
  const doDraw = () => {
    switch (cmd.type) {
      case 'Circle':
      case 'Ellipse':
      case 'Rectangle':
      case 'Line':
      case 'Polygon':
      case 'Path':
      case 'Spline':
        traceCmdPath(ctx, cmd);
        paintCmdFillStroke(ctx, cmd, style);
        break;
      case 'Sprite':
        drawSpriteCmd(ctx, cmd, style);
        break;
      case 'Text':
        drawTextCmd(ctx, cmd, style);
        break;
      case 'Clip': {
        ctx.save();
        // Mask path (never drawn itself)
        if (traceCmdPath(ctx, cmd.mask)) ctx.clip();
        // Content composited through intersection; nested clips recurse.
        for (const c of cmd.content) drawSingleCmd(ctx, c, patterns);
        ctx.restore();
        break;
      }
    }
  };
  if (blur > 0 && cmd.type !== 'Clip') {
    ctx.filter = `blur(${blur.toFixed(2)}px)`;
    doDraw();
    ctx.filter = 'none';
  } else {
    doDraw();
  }
  ctx.restore();
}

function drawTextCmd(ctx, cmd, style) {
  const [x, y] = cmd.pos;
  let fontFam = cmd.fontFamily || 'sans-serif';
  const fLower = String(fontFam).toLowerCase();
  if (fLower === 'mono' || fLower === 'monospace' || fLower === 'code') {
    fontFam = '"Fira Code", "JetBrains Mono", Consolas, monospace';
  } else if (fLower === 'sans' || fLower === 'sans-serif') {
    fontFam = 'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
  } else if (fLower === 'serif') {
    fontFam = 'Georgia, "Times New Roman", serif';
  }
  ctx.font = `${cmd.size}px ${fontFam}`;
  ctx.textAlign = cmd.align;
  ctx.textBaseline = 'top';
  ctx.fillStyle = paintToCanvas(ctx, style.fill, style.opacity);
  ctx.strokeStyle = paintToCanvas(ctx, style.stroke, style.opacity);
  const hasFill = !paintIsNone(style.fill);
  const hasStroke = !paintIsNone(style.stroke) && style.width > 0;
  if (hasFill) ctx.fillText(cmd.content, x, y);
  if (hasStroke) ctx.strokeText(cmd.content, x, y);
}

/** Section 18.3: palette-indexed pixel art drawn as crisp fillRect cells. */
function drawSpriteCmd(ctx, cmd, style) {
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
 * Section 18.4: pre-renders each pattern tile once into an offscreen canvas and
 * wraps it in a CanvasPattern (tiles render with an EMPTY pattern map so a tile
 * referencing itself falls back to gray and the cycle terminates).
 */
function buildPatternTiles(drawList, ctx) {
  const out = new Map();
  const tiles = drawList.patterns || [];
  if (tiles.length === 0) return out;
  if (typeof document === 'undefined' || typeof document.createElement !== 'function') return out;

  for (const pat of tiles) {
    const w = Math.max(1, Math.round(pat.width));
    const h = Math.max(1, Math.round(pat.height));
    const tileCanvas = document.createElement('canvas');
    tileCanvas.width = w;
    tileCanvas.height = h;
    const tctx = tileCanvas.getContext('2d');
    if (!tctx) continue;
    for (const c of pat.tiles) drawSingleCmd(tctx, c, new Map());
    const pattern = ctx.createPattern(tileCanvas, 'repeat');
    if (pattern) out.set(pat.name, pattern);
  }
  return out;
}

function renderDrawListToCanvas(ctx, drawList, originX, originY, zoom) {
  ctx.save();
  ctx.translate(originX, originY);
  ctx.scale(zoom, zoom);
  if (drawList.pixelFilter === 'nearest') {
    ctx.imageSmoothingEnabled = false;
  }

  if (drawList.background && !drawList.background.isNone) {
    ctx.save();
    ctx.globalCompositeOperation = 'source-over';
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

function colorNoAlpha(c) {
  if (!c || c.isNone) return 'none';
  const r = c.r.toString(16).padStart(2, '0');
  const g = c.g.toString(16).padStart(2, '0');
  const b = c.b.toString(16).padStart(2, '0');
  return `#${r}${g}${b}`;
}

function paintSvgRef(paint, ctx) {
  if (!paint) return 'none';
  if (paint.kind === 'color') return paint.color.toSvgString();
  if (paint.kind === 'pattern') return `url(#pvg-pat-${paint.name})`;
  const key = JSON.stringify(paint);
  if (ctx.gradIds.has(key)) return `url(#${ctx.gradIds.get(key)})`;
  const id = `pvg-g${ctx.gradCounter++}`;
  ctx.gradIds.set(key, id);
  ctx.gradOrder.push({ id, paint });
  return `url(#${id})`;
}

function filterSvgId(style, ctx) {
  const hasBlur = (style.blur || 0) > 1e-9;
  if (!hasBlur && !style.shadow && !style.glow) return null;
  const key = JSON.stringify({
    b: style.blur || 0,
    s: style.shadow ? { o: style.shadow.offset, r: style.shadow.radius, c: style.shadow.color } : null,
    g: style.glow ? { r: style.glow.radius, c: style.glow.color } : null,
  });
  if (ctx.filterIds.has(key)) return ctx.filterIds.get(key);
  const id = `pvg-f${ctx.filterCounter++}`;
  ctx.filterIds.set(key, id);
  ctx.filterOrder.push({ id, blur: style.blur || 0, shadow: style.shadow, glow: style.glow });
  return id;
}

function walkSvgCmd(cmd, ctx) {
  if (cmd.type === 'Clip') {
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

function emitGradientDef(id, paint) {
  const stops = (paint.stops || []).map((st) => {
    const c = st.color;
    if (!c || c.isNone) return `<stop offset="${st.offset.toFixed(3)}" stop-color="none" />`;
    if (c.a === 255) {
      const r = c.r.toString(16).padStart(2, '0');
      const g = c.g.toString(16).padStart(2, '0');
      const b = c.b.toString(16).padStart(2, '0');
      return `<stop offset="${st.offset.toFixed(3)}" stop-color="#${r}${g}${b}" />`;
    }
    const r = c.r.toString(16).padStart(2, '0');
    const g = c.g.toString(16).padStart(2, '0');
    const b = c.b.toString(16).padStart(2, '0');
    return `<stop offset="${st.offset.toFixed(3)}" stop-color="#${r}${g}${b}" stop-opacity="${(c.a / 255).toFixed(3)}" />`;
  }).join('');
  if (paint.kind === 'linear') {
    return `<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${paint.start[0].toFixed(2)}" y1="${paint.start[1].toFixed(2)}" x2="${paint.end[0].toFixed(2)}" y2="${paint.end[1].toFixed(2)}">${stops}</linearGradient>`;
  }
  if (paint.kind === 'radial') {
    const f = paint.focal || paint.center;
    return `<radialGradient id="${id}" gradientUnits="userSpaceOnUse" cx="${paint.center[0].toFixed(2)}" cy="${paint.center[1].toFixed(2)}" r="${paint.radius.toFixed(2)}" fx="${f[0].toFixed(2)}" fy="${f[1].toFixed(2)}">${stops}</radialGradient>`;
  }
  // Angular: no native SVG — linear fallback carrying same stops (matches Rust).
  return `<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${(paint.center[0] - 100).toFixed(2)}" y1="${paint.center[1].toFixed(2)}" x2="${(paint.center[0] + 100).toFixed(2)}" y2="${paint.center[1].toFixed(2)}">${stops}</linearGradient>`;
}

function emitFilterDef(entry) {
  const parts = [];
  if (entry.shadow) {
    parts.push(`<feDropShadow dx="${entry.shadow.offset[0].toFixed(2)}" dy="${entry.shadow.offset[1].toFixed(2)}" stdDeviation="${(entry.shadow.radius / 2).toFixed(2)}" flood-color="${colorNoAlpha(entry.shadow.color)}" />`);
  }
  if (entry.glow) {
    parts.push(`<feDropShadow dx="0" dy="0" stdDeviation="${(entry.glow.radius / 2).toFixed(2)}" flood-color="${colorNoAlpha(entry.glow.color)}" />`);
  }
  if ((entry.blur || 0) > 1e-9) {
    parts.push(`<feGaussianBlur stdDeviation="${(entry.blur / 2).toFixed(2)}" />`);
  }
  return `<filter id="${entry.id}" x="-60%" y="-60%" width="220%" height="220%">${parts.join('')}</filter>`;
}

function blendToSvg(blend) {
  switch (blend) {
    case 'add': return 'plus-lighter';
    case 'multiply': return 'multiply';
    case 'screen': return 'screen';
    case 'overlay': return 'overlay';
    default: return null;
  }
}

function formatSvgStyle(s, ctx) {
  let attrs = `fill="${paintSvgRef(s.fill, ctx)}"`;
  if (!paintIsNone(s.stroke) && s.width > 0) {
    attrs += ` stroke="${paintSvgRef(s.stroke, ctx)}" stroke-width="${s.width.toFixed(2)}"`;
  } else {
    attrs += ` stroke="none"`;
  }
  // Non-default topology only (keeps 0.1 output byte-stable)
  if (s.cap && s.cap !== 'butt') attrs += ` stroke-linecap="${s.cap}"`;
  if (s.join && s.join !== 'miter') attrs += ` stroke-linejoin="${s.join}"`;
  if (s.join === 'miter' && Math.abs((s.miter || 4) - 4.0) > 1e-6) {
    attrs += ` stroke-miterlimit="${s.miter.toFixed(2)}"`;
  }
  if (s.dash && s.dash.length > 0) {
    attrs += ` stroke-dasharray="${s.dash.map((v) => v.toFixed(2)).join(' ')}"`;
  }
  if (Math.abs(s.opacity - 1.0) > 0.001) {
    attrs += ` opacity="${s.opacity.toFixed(3)}"`;
  }
  const css = blendToSvg(s.blend);
  if (css) attrs += ` mix-blend-mode="${css}"`;
  const fid = ctx ? filterSvgId(s, ctx) : null;
  if (fid) attrs += ` filter="url(#${fid})"`;
  // Note: stroke-align inside/outside has no SVG equivalent (matches Rust: ignored).
  return attrs;
}

function clipMaskToSvg(mask, ctx) {
  // Raw mask geometry without paint/filter (matches Rust emit_clip_mask_shape).
  switch (mask.type) {
    case 'Circle':
      return `<circle cx="${mask.center[0].toFixed(2)}" cy="${mask.center[1].toFixed(2)}" r="${mask.radius.toFixed(2)}" />`;
    case 'Ellipse':
      return `<ellipse cx="${mask.center[0].toFixed(2)}" cy="${mask.center[1].toFixed(2)}" rx="${mask.radius[0].toFixed(2)}" ry="${mask.radius[1].toFixed(2)}" />`;
    case 'Rectangle': {
      const rx = mask.cornerRadius > 0 ? ` rx="${mask.cornerRadius.toFixed(2)}" ry="${mask.cornerRadius.toFixed(2)}"` : '';
      return `<rect x="${mask.pos[0].toFixed(2)}" y="${mask.pos[1].toFixed(2)}" width="${mask.size[0].toFixed(2)}" height="${mask.size[1].toFixed(2)}"${rx} />`;
    }
    case 'Line':
      return `<line x1="${mask.from[0].toFixed(2)}" y1="${mask.from[1].toFixed(2)}" x2="${mask.to[0].toFixed(2)}" y2="${mask.to[1].toFixed(2)}" stroke-width="${Math.max(1, mask.style.width).toFixed(2)}" />`;
    case 'Polygon': {
      const pts = mask.points.map((p) => `${p[0].toFixed(2)},${p[1].toFixed(2)}`).join(' ');
      return `<polygon points="${pts}" />`;
    }
    case 'Path':
      return `<path d="${pathToSvgD(mask.commands)}" />`;
    case 'Text': {
      let anchor = 'start';
      if (mask.align === 'center') anchor = 'middle';
      else if (mask.align === 'right') anchor = 'end';
      return `<text x="${mask.pos[0].toFixed(2)}" y="${mask.pos[1].toFixed(2)}" font-size="${mask.size.toFixed(2)}" font-family="${mask.fontFamily}" text-anchor="${anchor}" dominant-baseline="hanging">${escapeXml(mask.content)}</text>`;
    }
    case 'Clip':
      return clipMaskToSvg(mask.mask, ctx);
    default:
      return '';
  }
}

function pathToSvgD(commands) {
  const d = [];
  for (const pCmd of commands) {
    switch (pCmd.cmd) {
      case 'Start': d.push(`M ${pCmd.pt[0].toFixed(2)} ${pCmd.pt[1].toFixed(2)}`); break;
      case 'Line': d.push(`L ${pCmd.pt[0].toFixed(2)} ${pCmd.pt[1].toFixed(2)}`); break;
      case 'Quad': d.push(`Q ${pCmd.cp[0].toFixed(2)} ${pCmd.cp[1].toFixed(2)}, ${pCmd.ep[0].toFixed(2)} ${pCmd.ep[1].toFixed(2)}`); break;
      case 'Curve': d.push(`C ${pCmd.c1[0].toFixed(2)} ${pCmd.c1[1].toFixed(2)}, ${pCmd.c2[0].toFixed(2)} ${pCmd.c2[1].toFixed(2)}, ${pCmd.ep[0].toFixed(2)} ${pCmd.ep[1].toFixed(2)}`); break;
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
  return d.join(' ');
}

function emitSvgCommands(items, indent, ctx) {
  let out = '';
  for (const cmd of items) {
    switch (cmd.type) {
      case 'Circle':
        out += `${indent}<circle cx="${cmd.center[0].toFixed(2)}" cy="${cmd.center[1].toFixed(2)}" r="${cmd.radius.toFixed(2)}" ${formatSvgStyle(cmd.style, ctx)} />\n`;
        break;
      case 'Ellipse':
        out += `${indent}<ellipse cx="${cmd.center[0].toFixed(2)}" cy="${cmd.center[1].toFixed(2)}" rx="${cmd.radius[0].toFixed(2)}" ry="${cmd.radius[1].toFixed(2)}" ${formatSvgStyle(cmd.style, ctx)} />\n`;
        break;
      case 'Rectangle': {
        const rxAttr = cmd.cornerRadius > 0 ? ` rx="${cmd.cornerRadius.toFixed(2)}" ry="${cmd.cornerRadius.toFixed(2)}"` : '';
        out += `${indent}<rect x="${cmd.pos[0].toFixed(2)}" y="${cmd.pos[1].toFixed(2)}" width="${cmd.size[0].toFixed(2)}" height="${cmd.size[1].toFixed(2)}"${rxAttr} ${formatSvgStyle(cmd.style, ctx)} />\n`;
        break;
      }
      case 'Line':
        out += `${indent}<line x1="${cmd.from[0].toFixed(2)}" y1="${cmd.from[1].toFixed(2)}" x2="${cmd.to[0].toFixed(2)}" y2="${cmd.to[1].toFixed(2)}" ${formatSvgStyle(cmd.style, ctx)} />\n`;
        break;
      case 'Polygon': {
        const pts = cmd.points.map((p) => `${p[0].toFixed(2)},${p[1].toFixed(2)}`).join(' ');
        out += `${indent}<polygon points="${pts}" ${formatSvgStyle(cmd.style, ctx)} />\n`;
        break;
      }
      case 'Text': {
        let anchor = 'start';
        if (cmd.align === 'center') anchor = 'middle';
        else if (cmd.align === 'right') anchor = 'end';
        out += `${indent}<text x="${cmd.pos[0].toFixed(2)}" y="${cmd.pos[1].toFixed(2)}" font-size="${cmd.size.toFixed(2)}" font-family="${cmd.fontFamily}" text-anchor="${anchor}" dominant-baseline="hanging" ${formatSvgStyle(cmd.style, ctx)}>${escapeXml(cmd.content)}</text>\n`;
        break;
      }
      case 'Path': {
        out += `${indent}<path d="${pathToSvgD(cmd.commands)}" ${formatSvgStyle(cmd.style, ctx)} />\n`;
        break;
      }
      case 'Clip': {
        const id = `pvg-clip${ctx.clipCounter++}`;
        ctx.clipDefs.push({ id, mask: cmd.mask });
        out += `${indent}<g clip-path="url(#${id})">\n`;
        out += emitSvgCommands(cmd.content, indent + '  ', ctx);
        out += `${indent}</g>\n`;
        break;
      }
      case 'Spline': {
        if (cmd.points.length === 0) break;
        out += `${indent}<path d="${splinePathData(cmd.points)}" fill="none" ${formatSvgStyle(cmd.style, ctx)} />\n`;
        break;
      }
      case 'Sprite': {
        // Palette-indexed pixels as crisp rects (section 18.3).
        const opacityAttr =
          Math.abs(cmd.style.opacity - 1.0) > 0.001 ? ` opacity="${cmd.style.opacity.toFixed(3)}"` : '';
        const blendAttr = blendToSvg(cmd.style.blend);
        const blendPart = blendAttr ? ` mix-blend-mode="${blendAttr}"` : '';
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

/** Section 18.5: Catmull-Rom spline -> SVG path data (`M ... C ...`). */
function splinePathData(points) {
  if (points.length === 0) return '';
  if (points.length === 1) return `M ${points[0][0].toFixed(2)} ${points[0][1].toFixed(2)}`;
  const parts = [`M ${points[0][0].toFixed(2)} ${points[0][1].toFixed(2)} `];
  for (const [c1, c2, ep] of splineToBezier(points)) {
    parts.push(
      `C ${c1[0].toFixed(2)} ${c1[1].toFixed(2)}, ${c2[0].toFixed(2)} ${c2[1].toFixed(2)}, ${ep[0].toFixed(2)} ${ep[1].toFixed(2)} `
    );
  }
  return parts.join('').trimEnd();
}

function newSvgCtx() {
  return { gradIds: new Map(), gradOrder: [], gradCounter: 0, filterIds: new Map(), filterOrder: [], filterCounter: 0, clipCounter: 0, clipDefs: [] };
}

function exportToSvgString(drawList) {
  const ctx = newSvgCtx();
  const patterns = drawList.patterns || [];
  // Walk pattern tiles first so their gradient/filter ids land in <defs> too.
  for (const pat of patterns) {
    for (const tile of pat.tiles) walkSvgCmd(tile, ctx);
  }
  for (const cmd of drawList.items) walkSvgCmd(cmd, ctx);
  // Second pass populates ids in deterministic order
  let body = emitSvgCommands(drawList.items, '  ', ctx);

  // Section 18.4: repeatable tiles as real SVG <pattern> defs.
  let patternDefs = '';
  for (const pat of patterns) {
    let tileBody = '';
    for (const tile of pat.tiles) tileBody += emitSvgCommands([tile], '      ', ctx);
    patternDefs += `<pattern id="pvg-pat-${pat.name}" patternUnits="userSpaceOnUse" width="${pat.width.toFixed(2)}" height="${pat.height.toFixed(2)}">${tileBody}</pattern>`;
  }

  let defs = '';
  for (const g of ctx.gradOrder) defs += emitGradientDef(g.id, g.paint);
  for (const f of ctx.filterOrder) defs += emitFilterDef(f);
  defs += patternDefs;
  for (const c of ctx.clipDefs) defs += `<clipPath id="${c.id}">${clipMaskToSvg(c.mask, ctx)}</clipPath>`;
  if (defs) defs = `  <defs>${defs}</defs>\n`;

  const crispAttr = drawList.pixelFilter === 'nearest' ? ` shape-rendering="crispEdges"` : '';
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
    svg += emitSvgCommands(frames[i].items, '    ', newSvgCtx());
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
      'params',
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
    /** Host uniform overrides (`param`, section 18.1). */
    this._params = {};

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
          user-select: text;
          cursor: text;
        }
        .err-msg {
          user-select: text;
          cursor: text;
        }
        .err-copy-btn {
          position: sticky;
          top: 0;
          float: right;
          margin-left: 8px;
          background: rgba(255, 71, 102, 0.15);
          border: 1px solid #ff4766;
          color: #ff8ba0;
          border-radius: 4px;
          padding: 3px 8px;
          font-size: 11px;
          font-family: system-ui, -apple-system, sans-serif;
          cursor: pointer;
          z-index: 11;
        }
        .err-copy-btn:hover {
          background: rgba(255, 71, 102, 0.35);
          color: #fff;
        }
        .err-hint {
          color: #8a8fa3;
          font-family: system-ui, -apple-system, sans-serif;
          font-size: 10px;
          margin-top: 8px;
          user-select: none;
        }
        .err-toast {
          position: absolute;
          left: 50%;
          bottom: 16px;
          transform: translateX(-50%) translateY(8px);
          background: #0e7a4f;
          color: #fff;
          font-family: system-ui, -apple-system, sans-serif;
          font-size: 12px;
          padding: 6px 12px;
          border-radius: 6px;
          opacity: 0;
          pointer-events: none;
          transition: opacity 0.2s, transform 0.2s;
          z-index: 12;
          white-space: nowrap;
        }
        .err-toast.show {
          opacity: 1;
          transform: translateX(-50%) translateY(0);
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
        <div class="overlay-error" part="error" title="Click to copy error">
          <button class="err-copy-btn" part="error-copy-btn" title="Copy error to clipboard">📋 Copy</button>
          <div class="err-msg" part="error-msg"></div>
          <div class="err-hint">Click error text or Copy button to copy to clipboard</div>
          <div class="err-toast" part="error-toast">✓ Error copied to clipboard</div>
        </div>
      </div>
    `;

    this._viewport = this.shadowRoot.querySelector('.pvg-viewport');
    this._errorOverlay = this.shadowRoot.querySelector('.overlay-error');
    this._errorMsg = this.shadowRoot.querySelector('.err-msg');
    this._errorCopyBtn = this.shadowRoot.querySelector('.err-copy-btn');
    this._errorToast = this.shadowRoot.querySelector('.err-toast');
    this._lastError = '';
    this._errToastTimer = null;
    this._loadingOverlay = this.shadowRoot.querySelector('.overlay-loading');

    this._copyErrorToClipboard = this._copyErrorToClipboard.bind(this);
    this._errorOverlay.addEventListener('click', (e) => {
      // Copy on button click or on error-text click (but allow normal text selection via dbl-click/drag:
      // if user has an active text selection, don't hijack the click).
      const sel = this.shadowRoot.getSelection ? this.shadowRoot.getSelection() : window.getSelection();
      if (sel && String(sel).length > 0 && e.target !== this._errorCopyBtn) return;
      this._copyErrorToClipboard();
    });
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
    } else if (name === 'params') {
      this._parseParamsAttr(newValue);
      this.renderAt(this._currentTime);
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

  /**
   * Sets a host uniform (`param`, section 18.1) and re-renders. Overrides the
   * document's declared default.
   */
  setParam(name, value) {
    this._params[name] = value;
    this.renderAt(this._currentTime);
  }

  /** Clears a host uniform override and re-renders. */
  clearParam(name) {
    delete this._params[name];
    this.renderAt(this._currentTime);
  }

  /** Snapshot of the currently applied host uniform overrides. */
  get params() {
    return Object.assign({}, this._params);
  }

  /** Parses the `params` attribute (a JSON object). */
  _parseParamsAttr(json) {
    this._params = {};
    if (!json) return;
    try {
      const parsed = JSON.parse(json);
      for (const key of Object.keys(parsed)) {
        const v = parsed[key];
        if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') {
          this._params[key] = v;
        }
      }
    } catch (err) {
      this.dispatchEvent(
        new CustomEvent('error', { detail: { error: `Invalid params attribute: ${err.message}` } })
      );
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
      this._currentDrawList = compilePVG(this._sourceCode, time, this._params);
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
    this._lastError = `⚡ PVG Execution Error:\n${msg}`;
    // Keep textContent in sync for backwards-compat / query purposes.
    this._errorOverlay.setAttribute('data-error', this._lastError);
    if (this._errorMsg) {
      this._errorMsg.textContent = this._lastError;
    } else {
      this._errorOverlay.textContent = this._lastError;
    }
    this._errorOverlay.style.display = 'block';
  }

  _hideError() {
    this._errorOverlay.style.display = 'none';
  }

  _flashErrorToast(message) {
    if (!this._errorToast) return;
    this._errorToast.textContent = message;
    this._errorToast.classList.add('show');
    if (this._errToastTimer) clearTimeout(this._errToastTimer);
    this._errToastTimer = setTimeout(() => {
      if (this._errorToast) this._errorToast.classList.remove('show');
    }, 2000);
  }

  async _copyErrorToClipboard() {
    const text = this._lastError || (this._errorMsg ? this._errorMsg.textContent : this._errorOverlay.textContent) || '';
    if (!text) return;
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(text);
      } else {
        // Fallback for file:// or non-secure contexts.
        const ta = document.createElement('textarea');
        ta.value = text;
        ta.style.position = 'fixed';
        ta.style.opacity = '0';
        document.body.appendChild(ta);
        ta.select();
        document.execCommand('copy');
        ta.remove();
      }
      this._flashErrorToast('✓ Error copied to clipboard');
      if (this._errorCopyBtn) {
        const orig = this._errorCopyBtn.textContent;
        this._errorCopyBtn.textContent = '✓ Copied!';
        setTimeout(() => { if (this._errorCopyBtn) this._errorCopyBtn.textContent = orig; }, 2000);
      }
    } catch (err) {
      this._flashErrorToast('✗ Copy failed — select & press Ctrl+C');
    }
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
  MAX_CALL_STACK_DEPTH,
  MAX_SCENE_PRIMITIVES,
  get presets() {
    return window.PVG_PRESETS || [];
  },
};
