import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { compile, parse, evaluate } from "../dist/index.js";

describe("Evaluator & Procedural Runtime", () => {
  it("should evaluate operator precedence and math functions correctly", () => {
    const src = `
PVG 0.1
canvas 400 400

set a = 10 + 5 * 2
set b = (2 ^ 3) + sqrt(16)
set c = min(a, b) + max(1, 2)
set d = abs(-50) + floor(4.9) + ceil(1.1)

line
  from [0, 0]
  to [a + b, c + d]
  stroke #ffffff
  width 1
`;
    const dl = compile(src);
    assert.equal(dl.items.length, 1);

    const line = dl.items[0];
    assert.equal(line.type, "Line");
    assert.deepEqual(line.from, [0, 0]);

    // a = 20, b = 12 -> x = 32
    // c = 12 + 2 = 14, d = 50 + 4 + 2 = 56 -> y = 70
    assert.deepEqual(line.to, [32, 70]);
  });

  it("should evaluate loops and variable scoping", () => {
    const src = `
PVG 0.1
canvas 600 600

for i from 0 to 4
  circle
    center [100 + i * 50, 200]
    radius 10
    fill #00ffcc
`;
    const dl = compile(src);
    assert.equal(dl.items.length, 5);

    assert.deepEqual(dl.items[0].center, [100, 200]);
    assert.deepEqual(dl.items[4].center, [300, 200]);
  });

  it("should compose 2D hierarchical affine transforms in groups", () => {
    const src = `
PVG 0.1
canvas 600 600

group
  pos [100, 100]
  rot 90deg
  scale [2.0, 2.0]
  circle
    center [10, 0]
    radius 5
    fill #ffffff
`;
    const dl = compile(src);
    assert.equal(dl.items.length, 1);

    const circle = dl.items[0];
    // [10, 0] scaled 2x -> [20, 0], rotated 90deg -> [0, 20], translated by [100, 100] -> [100, 120]
    assert.ok(Math.abs(circle.center[0] - 100.0) < 1e-4);
    assert.ok(Math.abs(circle.center[1] - 120.0) < 1e-4);
  });

  it("should execute deterministic 64-bit Xorshift random sequences", () => {
    const src = `
PVG 0.1
canvas 400 400
seed 998877

for i from 0 to 2
  set r = random(5, 50)
  circle
    center [r, r]
    radius 5
`;
    const dl1 = compile(src);
    const dl2 = compile(src);

    assert.deepEqual(dl1.items, dl2.items, "Same seed must yield identical shapes");
  });

  it("should prevent infinite loops with safety bounds", () => {
    const src = `
PVG 0.1
canvas 400 400
while true
  circle
    center [0, 0]
    radius 1
`;
    assert.throws(
      () => compile(src),
      /Exceeded safety loop limit/
    );
  });

  it("should re-evaluate cached AST across timeline ticks (AST Caching)", () => {
    const src = `
PVG 0.1
canvas 400 400

set pulse = 50 + 20 * sin(time * 3.0)

circle
  center [200, 200]
  radius pulse
  fill #00ffcc
`;
    const doc = parse(src);

    // Frame at t = 0 -> sin(0) = 0 -> radius = 50
    const dl0 = evaluate(doc, 0.0);
    assert.equal(dl0.items[0].radius, 50);

    // Frame at t = PI / 6 (~0.5236s) -> sin(PI/2) = 1 -> radius = 70
    const dlPeak = evaluate(doc, Math.PI / 6);
    assert.ok(Math.abs(dlPeak.items[0].radius - 70) < 1e-4);
  });
});