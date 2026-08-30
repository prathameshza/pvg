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

  // Path Commands
  Start = "Start",
  Quad = "Quad",
  Curve = "Curve",
  Arc = "Arc",
  Close = "Close",

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