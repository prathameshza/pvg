import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { Lexer, TokenKind, PvgColor } from "../dist/index.js";

describe("Lexer & Tokenizer", () => {
  it("should tokenize basic PVG header and canvas declaration", () => {
    const src = `PVG 0.1\ncanvas 400 300\n`;
    const lexer = new Lexer(src);
    const tokens = lexer.tokenizeAll();

    assert.equal(tokens[0].kind, TokenKind.Pvg);
    assert.equal(tokens[1].kind, TokenKind.Number);
    assert.equal(tokens[1].value, 0.1);
    assert.equal(tokens[3].kind, TokenKind.Canvas);
    assert.equal(tokens[4].value, 400);
    assert.equal(tokens[5].value, 300);
  });

  it("should convert degree units (deg) to radians at parse time", () => {
    const src = `PVG 0.1\ncanvas 100 100\nset a = 180deg\nset b = 90deg\n`;
    const lexer = new Lexer(src);
    const tokens = lexer.tokenizeAll();

    const deg180Token = tokens.find((t) => typeof t.value === "number" && Math.abs(t.value - Math.PI) < 1e-6);
    const deg90Token = tokens.find((t) => typeof t.value === "number" && Math.abs(t.value - Math.PI / 2) < 1e-6);

    assert.ok(deg180Token, "180deg should convert to PI (~3.14159)");
    assert.ok(deg90Token, "90deg should convert to PI/2 (~1.57079)");
  });

  it("should parse 3-digit, 6-digit, and 8-digit hex colors", () => {
    const src = `PVG 0.1\ncanvas 100 100\nset c1 = #fff\nset c2 = #00ffcc\nset c3 = #ff005580\n`;
    const lexer = new Lexer(src);
    const tokens = lexer.tokenizeAll();

    const colorTokens = tokens.filter((t) => t.kind === TokenKind.Color);
    assert.equal(colorTokens.length, 3);

    const c1 = colorTokens[0].value;
    assert.deepEqual([c1.r, c1.g, c1.b, c1.a], [255, 255, 255, 255]);

    const c2 = colorTokens[1].value;
    assert.deepEqual([c2.r, c2.g, c2.b, c2.a], [0, 255, 204, 255]);

    const c3 = colorTokens[2].value;
    assert.deepEqual([c3.r, c3.g, c3.b, c3.a], [255, 0, 85, 128]);
  });

  it("should decode escaped strings properly", () => {
    const src = `PVG 0.1\ncanvas 100 100\nset label = "Line 1\\nLine 2\\tTabbed \\"Quotes\\""\n`;
    const lexer = new Lexer(src);
    const tokens = lexer.tokenizeAll();

    const strToken = tokens.find((t) => t.kind === TokenKind.String);
    assert.ok(strToken);
    assert.equal(strToken.value, 'Line 1\nLine 2\tTabbed "Quotes"');
  });

  it("should reject tab indentation with a descriptive error", () => {
    const src = "PVG 0.1\ncanvas 100 100\n\tcircle\n";
    const lexer = new Lexer(src);
    assert.throws(
      () => lexer.tokenizeAll(),
      /Tabs are forbidden/
    );
  });
});