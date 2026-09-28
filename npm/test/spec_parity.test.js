import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { compile, evaluate, parse } from "../dist/index.js";

/**
 * Rust-core parity regressions (PVG_SPECS.md conformance).
 * Each case mirrors `pvg/src/eval.rs` / `pvg/src/parser.rs` behavior so the
 * npm engine cannot silently drift from the Rust reference implementation.
 */

describe("block-level param behaves like set (spec Section 18.1 / EBNF)", () => {
  it("accepts param inside a group body", () => {
    const dl = compile(`PVG 0.2
canvas 100 100
group
  pos [10, 20]
  param local_w: 7
  circle
    center [0, 0]
    radius local_w`);
    assert.equal(dl.items.length, 1);
    assert.equal(dl.items[0].radius, 7);
  });

  it("accepts param inside a function body", () => {
    const dl = compile(`PVG 0.2
canvas 100 100
def make_r()
  param rr: 9
  circle
    center [10, 10]
    radius rr
make_r()`);
    assert.equal(dl.items.length, 1);
    assert.equal(dl.items[0].radius, 9);
  });

  it("still treats top-level param as a host uniform", () => {
    const doc = parse(`PVG 0.2
canvas 100 100
param health: 0.5
rectangle
  pos [0, 0]
  size [100 * health, 10]`);
    assert.deepEqual(
      doc.params.map((p) => p.name),
      ["health"]
    );
    assert.equal(evaluate(doc).items[0].size[0], 50);
  });
});

describe("pattern tiles are top-level only (spec Section 18.4)", () => {
  it("rejects a bare pattern() call in block position", () => {
    assert.throws(
      () =>
        compile(`PVG 0.2
canvas 10 10
group
  pos [0, 0]
  pattern(1, 2)`),
      /top level/
    );
  });
});

describe("equality coerces numbers/bools, errors otherwise (Rust as_f64)", () => {
  it("treats true == 1 and 0 != true with numeric coercion", () => {
    const dl = compile(`PVG 0.2
canvas 10 10
set a = true == 1
set b = 0 != false
set c = 2 != 3
text
  pos [0, 0]
  content "" + a + b + c`);
    assert.equal(dl.items[0].content, "truefalsetrue");
  });

  it("rejects string and color equality like the Rust core", () => {
    assert.throws(
      () =>
        compile(`PVG 0.2
canvas 10 10
set x = "a" == "a"
circle
  center [0, 0]
  radius 1`),
      /Expected number/
    );
    assert.throws(
      () =>
        compile(`PVG 0.2
canvas 10 10
set x = #ff0000 == #ff0000
circle
  center [0, 0]
  radius 1`),
      /Expected number/
    );
  });
});

describe("string concatenation uses display conversion (Rust as_string)", () => {
  it("formats arrays as [a, b] and bools as true/false", () => {
    const dl = compile(`PVG 0.2
canvas 10 10
set arr = [5, 7, 9]
set label = "" + arr + "|" + true + "|" + 3200
text
  pos [0, 0]
  content label`);
    assert.equal(dl.items[0].content, "[5, 7, 9]|true|3200");
  });

  it("rejects color/None operands instead of printing garbage", () => {
    assert.throws(
      () =>
        compile(`PVG 0.2
canvas 10 10
set x = "c: " + #ff0000
circle
  center [0, 0]
  radius 1`),
      /displayable/
    );
  });
});

describe("empty arrays are falsy (Rust is_truthy)", () => {
  it("branches on empty vs non-empty arrays", () => {
    const dl = compile(`PVG 0.2
canvas 10 10
set empty = array()
set full = [1]
set r = "none"
if empty
  set r = "bad-empty"
else
  if full
    set r = "ok"
text
  pos [0, 0]
  content r`);
    assert.equal(dl.items[0].content, "ok");
  });
});

describe("seed edge cases match the Rust core", () => {
  it("treats seed 0 as the engine default seed (same as no seed line)", () => {
    const run = (seedLine) =>
      compile(
        `PVG 0.2
canvas 10 10
${seedLine}${seedLine ? "\n" : ""}set r = random(0, 1000)
circle
  center [r, 0]
  radius 1`
      ).items[0].center[0];
    // `seed 0` selects the default state, identical to omitting `seed`.
    assert.equal(run("seed 0"), run(""));
    // …and distinct from an explicit `seed 42` (the old npm fallback bug).
    assert.notEqual(run("seed 0"), run("seed 42"));
  });
});

describe("functional rgb()/rgba() colors (spec Section 2.6)", () => {
  it("builds opaque colors and maps alpha [0,1] to [0,255]", () => {
    const fillOf = (expr) =>
      compile(`PVG 0.2
canvas 10 10
circle
  center [5, 5]
  radius 2
  fill ${expr}`).items[0].style.fill;
    const c0 = fillOf("rgb(255, 128, 0)").color;
    assert.deepEqual(
      [c0.r, c0.g, c0.b, c0.a],
      [255, 128, 0, 255]
    );
    assert.equal(fillOf("rgba(255, 0, 0, 0.5)").color.a, 128);
    assert.equal(fillOf("rgba(255, 0, 0, 0.0)").color.a, 0);
    assert.equal(fillOf("rgba(255, 0, 0, 1.0)").color.a, 255);
  });

  it("clamps out-of-range channels and rounds fractions", () => {
    const c = compile(`PVG 0.2
canvas 10 10
circle
  center [5, 5]
  radius 2
  fill rgb(300, -20, 12.7)`).items[0].style.fill.color;
    assert.deepEqual([c.r, c.g, c.b, c.a], [255, 0, 13, 255]);
    const a = compile(`PVG 0.2
canvas 10 10
circle
  center [5, 5]
  radius 2
  fill rgba(0, 0, 0, 2.5)`).items[0].style.fill.color;
    assert.equal(a.a, 255);
  });

  it("enforces arity and numeric operands", () => {
    assert.throws(
      () => compile("PVG 0.2\ncanvas 10 10\nset c = rgb(1, 2)\ncircle\n  center [0, 0]\n  radius c"),
      /rgb\(r, g, b\) needs 3 arguments/
    );
    assert.throws(
      () => compile("PVG 0.2\ncanvas 10 10\nset c = rgba(1, 2, 3)\ncircle\n  center [0, 0]\n  radius c"),
      /rgba\(r, g, b, a\) needs 4 arguments/
    );
    assert.throws(
      () => compile('PVG 0.2\ncanvas 10 10\nset c = rgb("a", 0, 0)\ncircle\n  center [0, 0]\n  radius 1'),
      /Expected number/
    );
  });
});

describe("call-stack depth guard: 64 frames (spec Section 15)", () => {
  const recurse = (n) => `PVG 0.2
canvas 10 10
def boom(k)
  if k <= 0
    return 0
  else
    return boom(k - 1)
boom(${n})`;

  it("allows 60 nested calls inside the budget", () => {
    assert.doesNotThrow(() => compile(recurse(60)));
  });

  it("trips the guard on deep and infinite recursion", () => {
    assert.throws(() => compile(recurse(200)), /call stack/);
    assert.throws(() => compile("PVG 0.2\ncanvas 10 10\ndef f()\n  f()\nf()"), /call stack/);
  });
});

describe("scene primitive budget: 50000 draw commands (spec Section 15)", () => {
  it("accepts exactly 50000 primitives and rejects the 50001st", () => {
    const at = compile("PVG 0.2\ncanvas 10 10\nfor i from 0 to 49999\n  circle\n    center [0, 0]\n    radius 1");
    assert.equal(at.items.length, 50000);
    assert.throws(
      () => compile("PVG 0.2\ncanvas 10 10\nfor i from 0 to 50000\n  circle\n    center [0, 0]\n    radius 1"),
      /primitive/
    );
  });
});
