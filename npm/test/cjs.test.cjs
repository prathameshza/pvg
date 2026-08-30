const { describe, it } = require("node:test");
const assert = require("node:assert/strict");
const { parse, compile, evaluate, toSvg, toAnimatedSvg, PvgColor } = require("../dist/index.cjs");

describe("CommonJS Bundle Interoperability", () => {
  it("should allow require('pvg') and execute compile()", () => {
    const src = `
PVG 0.1
canvas 400 400
circle
  center [200, 200]
  radius 60
  fill #00ffcc
`;
    const dl = compile(src);
    assert.equal(dl.items.length, 1);
    assert.equal(dl.canvasWidth, 400);
  });

  it("should export PvgColor and toSvg in CJS", () => {
    const col = PvgColor.fromHex("#ff007f");
    assert.equal(col.toSvgString(), "#ff007f");

    const svg = toSvg("PVG 0.1\ncanvas 100 100\ncircle\n  center [50, 50]\n  radius 20\n");
    assert.ok(svg.includes("<circle cx=\"50.00\" cy=\"50.00\" r=\"20.00\""));
  });
});