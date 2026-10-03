import { PvgColor } from "./color.js";

export const enum TokenKind {
  Indent = "Indent",
  Dedent = "Dedent",
  Newline = "Newline",
  Eof = "Eof",
  Number = "Number",
  String = "String",
  Color = "Color",
  Ident = "Ident",

  // Keywords
  Pvg = "Pvg",
  Canvas = "Canvas",
  Background = "Background",
  Set = "Set",
  Def = "Def",
  Return = "Return",
  For = "For",
  From = "From",
  To = "To",
  Step = "Step",
  While = "While",
  If = "If",
  Else = "Else",
  Seed = "Seed",

  // Shapes
  Circle = "Circle",
  Ellipse = "Ellipse",
  Rectangle = "Rectangle",
  Line = "Line",
  Polygon = "Polygon",
  Path = "Path",
  Text = "Text",
  Group = "Group",
  Clip = "Clip",

  // Properties
  Center = "Center",
  Radius = "Radius",
  Pos = "Pos",
  Size = "Size",
  Points = "Points",
  Content = "Content",
  Font = "Font",
  Align = "Align",
  Fill = "Fill",
  Stroke = "Stroke",
  Width = "Width",
  Opacity = "Opacity",
  Rot = "Rot",
  Scale = "Scale",

  // PVG 0.2 Sections 8-12 properties & paints
  Cap = "Cap",
  Join = "Join",
  Miter = "Miter",
  Dash = "Dash",
  Blend = "Blend",
  Blur = "Blur",
  Shadow = "Shadow",
  Glow = "Glow",
  Linear = "Linear",
  Radial = "Radial",
  Angular = "Angular",
  Stop = "Stop",

  // Path Commands
  Start = "Start",
  Quad = "Quad",
  Curve = "Curve",
  Arc = "Arc",
  Close = "Close",

  // Post-0.2 Section 18 keywords (soft keywords: also legal as identifiers)
  Snap = "Snap",
  Filter = "Filter",
  Param = "Param",
  Pattern = "Pattern",
  Sprite = "Sprite",
  Spline = "Spline",
  Palette = "Palette",
  Data = "Data",
  Row = "Row",

  // Symbols
  LBracket = "[",
  RBracket = "]",
  LParen = "(",
  RParen = ")",
  Comma = ",",
  Question = "?",
  Colon = ":",
  Plus = "+",
  Minus = "-",
  Star = "*",
  Slash = "/",
  Percent = "%",
  Caret = "^",
  Equal = "=",
  EqualEqual = "==",
  NotEqual = "!=",
  Less = "<",
  LessEqual = "<=",
  Greater = ">",
  GreaterEqual = ">=",
  And = "and",
  Or = "or",
  Not = "not",
}

export class Token {
  constructor(
    public kind: TokenKind,
    public value: unknown,
    public line: number,
    public col: number
  ) {}
}

export function dedentCode(text: string): string {
  if (!text) return "";
  const lines = text.split(/\r?\n/);
  while (lines.length > 0 && lines[0].trim().length === 0) {
    lines.shift();
  }
  while (lines.length > 0 && lines[lines.length - 1].trim().length === 0) {
    lines.pop();
  }
  if (lines.length === 0) return "";

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
    return lines.join("\n");
  }

  return lines
    .map((line) => {
      if (line.trim().length === 0) return "";
      return line.startsWith(" ".repeat(minIndent)) ? line.slice(minIndent) : line.trimStart();
    })
    .join("\n");
}

export class Lexer {
  private source: string;
  private lines: string[];
  private currentLineIdx = 0;
  private indentStack = [0];

  constructor(source: string) {
    this.source = dedentCode(source);
    this.lines = this.source.split(/\r?\n/);
  }

  tokenizeAll(): Token[] {
    const tokens: Token[] = [];

    while (this.currentLineIdx < this.lines.length) {
      const rawLine = this.lines[this.currentLineIdx];
      const lineNum = this.currentLineIdx + 1;
      this.currentLineIdx++;

      const trimmed = rawLine.trimStart();
      if (trimmed.length === 0 || trimmed.startsWith("#")) {
        continue;
      }

      if (rawLine.includes("\t")) {
        throw new Error(`Line ${lineNum}: Tabs are forbidden. Use 2 spaces for indentation.`);
      }

      let spaces = 0;
      while (spaces < rawLine.length && rawLine[spaces] === " ") {
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
   * multi-line `String` token, then consumes the continuation lines up to and
   * including the closing `"""` line (a trailing `# comment` is allowed on both
   * the opener and the closer line).
   */
  private tokenizeLineWithTriple(
    content: string,
    lineNum: number,
    colOffset: number,
    baseSpaces: number,
    tokens: Token[]
  ): void {
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

    // Multi-line form: the opener must end the line (trailing comment allowed).
    const openTail = afterOpen.trimStart();
    if (!(openTail.length === 0 || openTail.startsWith("#"))) {
      throw new Error(
        `Line ${lineNum}, Col ${openCol}: Multi-line """ strings must start at the end of the line (e.g. \`data """\`).`
      );
    }

    const body: string[] = [];
    for (;;) {
      const nextIdx = this.currentLineIdx;
      if (nextIdx >= this.lines.length) {
        throw new Error(`Line ${lineNum}, Col ${openCol}: Unclosed triple-quoted string.`);
      }
      const raw = this.lines[nextIdx];
      this.currentLineIdx++;

      let sp = 0;
      while (sp < raw.length && raw[sp] === " ") sp++;

      if (raw.slice(sp).startsWith('"""')) {
        const closerTail = raw.slice(sp + 3).trimStart();
        if (!(closerTail.length === 0 || closerTail.startsWith("#"))) {
          throw new Error(
            `Line ${nextIdx + 1}, Col ${sp + 4}: Unexpected content after closing """.`
          );
        }
        break;
      }

      // Dedent continuation lines by the opener's block indent so sprite art
      // and text blocks read naturally in the source.
      let cut = 0;
      while (cut < baseSpaces && cut < raw.length && raw[cut] === " ") cut++;
      body.push(raw.slice(cut));
    }

    tokens.push(new Token(TokenKind.String, body.join("\n"), lineNum, openCol));
  }

  private tokenizeLine(text: string, lineNum: number, colOffset: number): Token[] {
    const tokens: Token[] = [];
    const len = text.length;
    let i = 0;

    while (i < len) {
      const c = text[i];
      if (c === " " || c === "\t" || c === "\r") {
        i++;
        continue;
      }

      const col = colOffset + i;

      if (c === "#") {
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

      if (c === "[") { tokens.push(new Token(TokenKind.LBracket, "[", lineNum, col)); i++; continue; }
      if (c === "]") { tokens.push(new Token(TokenKind.RBracket, "]", lineNum, col)); i++; continue; }
      if (c === "(") { tokens.push(new Token(TokenKind.LParen, "(", lineNum, col)); i++; continue; }
      if (c === ")") { tokens.push(new Token(TokenKind.RParen, ")", lineNum, col)); i++; continue; }
      if (c === ",") { tokens.push(new Token(TokenKind.Comma, ",", lineNum, col)); i++; continue; }
      if (c === "?") { tokens.push(new Token(TokenKind.Question, "?", lineNum, col)); i++; continue; }
      if (c === ":") { tokens.push(new Token(TokenKind.Colon, ":", lineNum, col)); i++; continue; }
      if (c === "^") { tokens.push(new Token(TokenKind.Caret, "^", lineNum, col)); i++; continue; }

      if (c === "=") {
        if (i + 1 < len && text[i + 1] === "=") {
          tokens.push(new Token(TokenKind.EqualEqual, "==", lineNum, col));
          i += 2;
        } else {
          tokens.push(new Token(TokenKind.Equal, "=", lineNum, col));
          i++;
        }
        continue;
      }

      if (c === "!") {
        if (i + 1 < len && text[i + 1] === "=") {
          tokens.push(new Token(TokenKind.NotEqual, "!=", lineNum, col));
          i += 2;
        } else {
          tokens.push(new Token(TokenKind.Not, "not", lineNum, col));
          i++;
        }
        continue;
      }

      if (c === "<") {
        if (i + 1 < len && text[i + 1] === "=") {
          tokens.push(new Token(TokenKind.LessEqual, "<=", lineNum, col));
          i += 2;
        } else {
          tokens.push(new Token(TokenKind.Less, "<", lineNum, col));
          i++;
        }
        continue;
      }

      if (c === ">") {
        if (i + 1 < len && text[i + 1] === "=") {
          tokens.push(new Token(TokenKind.GreaterEqual, ">=", lineNum, col));
          i += 2;
        } else {
          tokens.push(new Token(TokenKind.Greater, ">", lineNum, col));
          i++;
        }
        continue;
      }

      if (c === "&" && i + 1 < len && text[i + 1] === "&") {
        tokens.push(new Token(TokenKind.And, "and", lineNum, col));
        i += 2;
        continue;
      }

      if (c === "|" && i + 1 < len && text[i + 1] === "|") {
        tokens.push(new Token(TokenKind.Or, "or", lineNum, col));
        i += 2;
        continue;
      }

      if (c === "+") { tokens.push(new Token(TokenKind.Plus, "+", lineNum, col)); i++; continue; }
      if (c === "-") { tokens.push(new Token(TokenKind.Minus, "-", lineNum, col)); i++; continue; }
      if (c === "*") { tokens.push(new Token(TokenKind.Star, "*", lineNum, col)); i++; continue; }
      if (c === "/") { tokens.push(new Token(TokenKind.Slash, "/", lineNum, col)); i++; continue; }
      if (c === "%") { tokens.push(new Token(TokenKind.Percent, "%", lineNum, col)); i++; continue; }

      if (c === '"') {
        i++;
        let strVal = "";
        let closed = false;
        while (i < len) {
          if (text[i] === "\\" && i + 1 < len) {
            const next = text[i + 1];
            if (next === "n") strVal += "\n";
            else if (next === "t") strVal += "\t";
            else if (next === "r") strVal += "\r";
            else if (next === '"') strVal += '"';
            else if (next === "\\") strVal += "\\";
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

      if (/[0-9]/.test(c) || (c === "." && i + 1 < len && /[0-9]/.test(text[i + 1]))) {
        const start = i;
        let hasDot = false;
        while (i < len && (/[0-9]/.test(text[i]) || (!hasDot && text[i] === "."))) {
          if (text[i] === ".") hasDot = true;
          i++;
        }
        let numVal = parseFloat(text.slice(start, i));

        if (i + 3 <= len && text.slice(i, i + 3) === "deg") {
          numVal = (numVal * Math.PI) / 180.0;
          i += 3;
        } else if (i + 3 <= len && text.slice(i, i + 3) === "rad") {
          i += 3;
        }

        tokens.push(new Token(TokenKind.Number, numVal, lineNum, col));
        continue;
      }

      if (/[a-zA-Z_]/.test(c)) {
        const start = i;
        while (i < len && /[a-zA-Z0-9_-]/.test(text[i])) {
          i++;
        }
        const ident = text.slice(start, i);

        let kind = TokenKind.Ident;
        let value: unknown = ident;

        switch (ident) {
          case "PVG":
          case "CPSVG":
            kind = TokenKind.Pvg; break;
          case "canvas": kind = TokenKind.Canvas; break;
          case "background": kind = TokenKind.Background; break;
          case "set": kind = TokenKind.Set; break;
          case "def": kind = TokenKind.Def; break;
          case "return": kind = TokenKind.Return; break;
          case "for": kind = TokenKind.For; break;
          case "from": kind = TokenKind.From; break;
          case "to": kind = TokenKind.To; break;
          case "step": kind = TokenKind.Step; break;
          case "while": kind = TokenKind.While; break;
          case "if": kind = TokenKind.If; break;
          case "else": kind = TokenKind.Else; break;
          case "seed": kind = TokenKind.Seed; break;
          case "circle": kind = TokenKind.Circle; break;
          case "ellipse": kind = TokenKind.Ellipse; break;
          case "rectangle":
          case "rect":
            kind = TokenKind.Rectangle; break;
          case "line": kind = TokenKind.Line; break;
          case "polygon": kind = TokenKind.Polygon; break;
          case "path": kind = TokenKind.Path; break;
          case "text": kind = TokenKind.Text; break;
          case "group": kind = TokenKind.Group; break;
          case "clip": kind = TokenKind.Clip; break;
          case "center": kind = TokenKind.Center; break;
          case "radius": kind = TokenKind.Radius; break;
          case "pos": kind = TokenKind.Pos; break;
          case "size": kind = TokenKind.Size; break;
          case "points": kind = TokenKind.Points; break;
          case "content": kind = TokenKind.Content; break;
          case "font": kind = TokenKind.Font; break;
          case "align": kind = TokenKind.Align; break;
          case "fill": kind = TokenKind.Fill; break;
          case "stroke": kind = TokenKind.Stroke; break;
          case "width": kind = TokenKind.Width; break;
          case "opacity": kind = TokenKind.Opacity; break;
          case "rot": kind = TokenKind.Rot; break;
          case "scale": kind = TokenKind.Scale; break;
          case "start": kind = TokenKind.Start; break;
          case "quad": kind = TokenKind.Quad; break;
          case "curve": kind = TokenKind.Curve; break;
          case "arc": kind = TokenKind.Arc; break;
          case "close": kind = TokenKind.Close; break;
          // PVG 0.2 reserved keywords (Section 2.7)
          case "cap": kind = TokenKind.Cap; break;
          case "join": kind = TokenKind.Join; break;
          case "miter": kind = TokenKind.Miter; break;
          case "dash": kind = TokenKind.Dash; break;
          case "blend": kind = TokenKind.Blend; break;
          case "blur": kind = TokenKind.Blur; break;
          case "shadow": kind = TokenKind.Shadow; break;
          case "glow": kind = TokenKind.Glow; break;
          case "linear": kind = TokenKind.Linear; break;
          case "radial": kind = TokenKind.Radial; break;
          case "angular":
          case "conic": kind = TokenKind.Angular; break;
          case "stop": kind = TokenKind.Stop; break;
          // Post-0.2 Section 18 keywords (soft: legal as identifiers where a name is expected)
          case "snap": kind = TokenKind.Snap; break;
          case "filter": kind = TokenKind.Filter; break;
          case "param": kind = TokenKind.Param; break;
          case "pattern": kind = TokenKind.Pattern; break;
          case "sprite": kind = TokenKind.Sprite; break;
          case "spline": kind = TokenKind.Spline; break;
          case "palette": kind = TokenKind.Palette; break;
          case "data": kind = TokenKind.Data; break;
          case "row": kind = TokenKind.Row; break;
          case "and": kind = TokenKind.And; break;
          case "or": kind = TokenKind.Or; break;
          case "not": kind = TokenKind.Not; break;
          case "black": kind = TokenKind.Color; value = PvgColor.Black(); break;
          case "white": kind = TokenKind.Color; value = PvgColor.White(); break;
          case "red": kind = TokenKind.Color; value = PvgColor.Red(); break;
          case "green": kind = TokenKind.Color; value = PvgColor.Green(); break;
          case "blue": kind = TokenKind.Color; value = PvgColor.Blue(); break;
          case "yellow": kind = TokenKind.Color; value = PvgColor.Yellow(); break;
          case "cyan": kind = TokenKind.Color; value = PvgColor.Cyan(); break;
          case "magenta": kind = TokenKind.Color; value = PvgColor.Magenta(); break;
          case "none":
          case "transparent":
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
 * Post-0.2 "soft" keyword names. These words open new syntax in statement and
 * property position, but older documents use some of them as ordinary variable
 * names (`for row from 0 to 7`). Wherever an *identifier* is expected they are
 * accepted as names (mirrors `soft_ident` in `pvg/src/parser.rs`).
 */
export function softIdentKind(kind: TokenKind, value: unknown): string | null {
  switch (kind) {
    case TokenKind.Ident:
      return typeof value === "string" ? value : null;
    case TokenKind.Row:
      return "row";
    case TokenKind.Data:
      return "data";
    case TokenKind.Filter:
      return "filter";
    case TokenKind.Snap:
      return "snap";
    case TokenKind.Palette:
      return "palette";
    case TokenKind.Param:
      return "param";
    case TokenKind.Pattern:
      return "pattern";
    case TokenKind.Sprite:
      return "sprite";
    case TokenKind.Spline:
      return "spline";
    default:
      return null;
  }
}

/**
 * Reports whether a `"""` opener appears on this line *before* any real `#`
 * comment start (mirroring the hex-color rule so `#fff` colors don't count as
 * comments). Port of `triple_opener_before_comment` in `pvg/src/lexer.rs`.
 */
export function tripleOpenerBeforeComment(content: string): boolean {
  const open = content.indexOf('"""');
  if (open < 0) return false;
  let i = 0;
  while (i < open) {
    if (content[i] === "#") {
      let hexEnd = i + 1;
      while (hexEnd < content.length && /[0-9a-fA-F]/.test(content[hexEnd])) hexEnd++;
      const hexLen = hexEnd - (i + 1);
      if (hexLen === 3 || hexLen === 6 || hexLen === 8) {
        const isDelim =
          hexEnd === content.length ||
          /\s/.test(content[hexEnd]) ||
          ["]", ")", ",", ":", '"'].includes(content[hexEnd]);
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
