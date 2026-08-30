import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { parse } from "../dist/index.js";

describe("Parser & AST Generation", () => {
  it("should parse canvas background and document version", () => {
    const src = `PVG 0.1\ncanvas 600 400\n  background #0b0c10\n`;
    const ast = parse(src);

    assert.deepEqual(ast.version, [0, 1]);
    assert.equal(ast.canvas.width, 600);
    assert.equal(ast.canvas.height, 400);
    assert.ok(ast.canvas.background);
    assert.equal(ast.canvas.background.toSvgString(), "#0b0c10");
  });

  it("should parse geometric shapes (Circle, Rectangle, Line, Polygon)", () => {
    const src = `
PVG 0.1
canvas 500 500

circle
  center [100, 100]
  radius 50
  fill #00ffcc
  stroke #ffffff
  width 2

rectangle
  pos [200, 200]
  size [100, 80]
  radius 8
  fill #ff3355

line
  from [0, 0]
  to [500, 500]
  stroke #ffff00
  width 1.5

polygon
  points [10, 10] [50, 10] [30, 40]
  fill #00d2ff
`;
    const ast = parse(src);
    assert.equal(ast.statements.length, 4);

    assert.equal(ast.statements[0].type, "Circle");
    assert.equal(ast.statements[1].type, "Rectangle");
    assert.equal(ast.statements[2].type, "Line");
    assert.equal(ast.statements[3].type, "Polygon");
  });

  it("should parse paths and Bézier commands", () => {
    const src = `
PVG 0.1
canvas 500 500

path
  fill none
  stroke #00ffcc
  width 2
  start [50, 50]
  line [100, 50]
  quad [150, 100] [200, 50]
  curve [250, 20] [300, 80] [350, 50]
  arc [400, 50] 30 0deg 180deg
  close
`;
    const ast = parse(src);
    assert.equal(ast.statements.length, 1);

    const path = ast.statements[0];
    assert.equal(path.type, "Path");
    assert.equal(path.commands.length, 6);
    assert.equal(path.commands[0].cmd, "Start");
    assert.equal(path.commands[1].cmd, "Line");
    assert.equal(path.commands[2].cmd, "Quad");
    assert.equal(path.commands[3].cmd, "Curve");
    assert.equal(path.commands[4].cmd, "Arc");
    assert.equal(path.commands[5].cmd, "Close");
  });

  it("should parse control flow (for, while, if/else, def)", () => {
    const src = `
PVG 0.1
canvas 500 500

def draw_ring(cx, cy, r)
  circle
    center [cx, cy]
    radius r

for i from 0 to 10 step 2
  if i > 4
    draw_ring(i * 20, 200, 10)
  else
    draw_ring(i * 20, 100, 5)
`;
    const ast = parse(src);
    assert.equal(ast.statements.length, 2);
    assert.equal(ast.statements[0].type, "Def");
    assert.equal(ast.statements[1].type, "For");
  });
});