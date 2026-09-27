import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  compile,
  compileWithParams,
  evaluate,
  parse,
  pvgNoise2,
  pvgNoise3,
  PvgScene,
  splineToBezier,
  toSvg,
} from "../dist/index.js";

/**
 * Post-0.2 (spec section 18) feature tests. These mirror
 * `pvg/tests/new_features_verify.rs` so the npm engine cannot silently
 * drift from the Rust core.
 */

describe("18.1 host params (uniforms)", () => {
  const src = `PVG 0.2
canvas 400 60

param health: 0.75
param shield = 0.40
param pilot: "Ace"

rectangle
  pos [20, 20]
  size [200 * health, 12]
  fill health < 0.25 ? #ff3344 : #00e676`;

  it("applies declared defaults and lists the declared names", () => {
    const doc = parse(src);
    assert.deepEqual(doc.params.map((p) => p.name), ["health", "shield", "pilot"]);
    const dl = evaluate(doc);
    assert.equal(dl.items.length, 1);
    assert.equal(dl.items[0].size[0], 150);
  });

  it("lets the host override a param without re-parsing", () => {
    const scene = PvgScene.fromSource(src);
    scene.setParam("health", 0.2);
    const dl = scene.evaluate();
    assert.equal(dl.items[0].size[0], 40);
    assert.equal(dl.items[0].style.fill.kind, "color");
    assert.equal(dl.items[0].style.fill.color.toSvgString(), "#ff3344");

    // Clearing the override restores the document default.
    scene.clearParam("health");
    assert.equal(scene.evaluate().items[0].size[0], 150);
  });

  it("supports compileWithParams and compile(..., params)", () => {
    assert.equal(compileWithParams(src, { health: 0.5 }).items[0].size[0], 100);
    assert.equal(compile(src, 0, { health: 1.0 }).items[0].size[0], 200);
  });
});

describe("18.2 deterministic noise", () => {
  it("is a pure function of its coordinates and stays in [-1, 1]", () => {
    const a = pvgNoise2(1.5, -2.25);
    assert.equal(a, pvgNoise2(1.5, -2.25));
    assert.ok(a >= -1 && a <= 1);
    assert.notEqual(pvgNoise2(0, 0), pvgNoise2(10.5, 3.7));
    const n3 = pvgNoise3(0.1, 0.2, 0.3);
    assert.equal(n3, pvgNoise3(0.1, 0.2, 0.3));
    assert.ok(n3 >= -1 && n3 <= 1);
    assert.equal(pvgNoise2(Number.NaN, 0), 0);
  });

  it("matches the Rust core bit-for-bit (u64 lattice hashes)", () => {
    // Captured from pvg::pvg_noise2 / pvg::pvg_noise3 in the Rust engine.
    assert.equal(pvgNoise2(0, 0), -1.0);
    assert.equal(pvgNoise2(1.5, -2.25), 0.05296408067571079);
    assert.equal(pvgNoise2(1.0, 2.0), 0.601562507379483);
    assert.equal(pvgNoise2(10.5, 3.7), 0.41718126214449147);
    assert.equal(pvgNoise3(0.1, 0.2, 0.3), -0.7249852614609553);
  });

  it("is reachable from expressions", () => {
    const dl = compile(`PVG 0.2
canvas 200 200
set a = noise2d(1.0, 2.0)
set b = noise3d(1.0, 2.0, time)
circle
  center [a, b]
  radius 2`);
    assert.equal(dl.items.length, 1);
    assert.equal(dl.items[0].radius, 2);
  });
});

describe("18.3 sprites, pixel grid and filter", () => {
  it("snaps positions, keeps palette rows and records the pixel filter", () => {
    const dl = compile(`PVG 0.2
canvas 256 240
  snap 1.0
  filter "nearest"

sprite
  pos [10.4, 20.6]
  palette [#00000000, #b84418, #fc9838]
  data "0110"
  data "1221"`);
    assert.equal(dl.items.length, 1);
    assert.equal(dl.snap, 1.0);
    assert.equal(dl.pixelFilter, "nearest");
    const sprite = dl.items[0];
    assert.deepEqual(sprite.pos, [10, 21]); // 10.4 -> 10, 20.6 -> 21
    assert.equal(sprite.palette.length, 3);
    assert.deepEqual(sprite.rows, ["0110", "1221"]);
    assert.equal(sprite.scale, 1);
    assert.equal(sprite.style.fill.color.toSvgString(), "#ffffff");
    assert.equal(sprite.style.stroke.color.isNone, true);
  });

  it("supports multi-row triple-quoted art blocks", () => {
    const dl = compile(`PVG 0.2
canvas 64 64

sprite
  pos [0, 0]
  palette [#00000000, #ff1a4b, #fc9838]
  data """
  ..11..
  .1221..
  ..11..
  """`);
    assert.equal(dl.items[0].rows.length, 3);
    assert.deepEqual(dl.items[0].rows[1], ".1221..");
  });

  it("emits crisp per-pixel rects into SVG", () => {
    const svg = toSvg(`PVG 0.2
canvas 32 32
  filter "nearest"
sprite
  pos [0, 0]
  scale 2
  palette [#00000000, #ff1a4b]
  data "10"`);
    // '1' is at rx=0 (index 1 -> #ff1a4b); '0' is at rx=1 and palette entry 0 is
    // fully transparent, so that pixel is skipped entirely.
    assert.ok(svg.includes('shape-rendering="crispEdges"'));
    assert.ok(svg.includes('<rect x="0.00" y="0.00" width="2.00" height="2.00" fill="#ff1a4b"'));
    assert.ok(!svg.includes('fill="#00000000"'));
    assert.equal((svg.match(/<rect /g) || []).length, 1);
  });

  it("honours sprite scale and opacity", () => {
    const dl = compile(`PVG 0.2
canvas 32 32
sprite
  pos [1, 1]
  scale 3
  opacity 0.5
  palette [#ffffff]
  data "1"`);
    assert.equal(dl.items[0].scale, 3);
    assert.equal(dl.items[0].style.opacity, 0.5);
  });
});

describe("18.4 pattern fills", () => {
  const src = `PVG 0.2
canvas 480 380

pattern grid_pattern 16 16
  line
    from [0, 0]
    to [16, 0]
    stroke #ffffff
    width 1
  line
    from [0, 0]
    to [0, 16]
    stroke #ffffff
    width 1

rectangle
  pos [40, 40]
  size [400, 300]
  fill pattern grid_pattern`;

  it("parses the tile, evaluates it in isolation and resolves the paint", () => {
    const doc = parse(src);
    assert.deepEqual(
      doc.patterns.map((p) => p.name),
      ["grid_pattern"]
    );
    const dl = evaluate(doc);
    assert.equal(dl.patterns.length, 1);
    assert.equal(dl.patterns[0].name, "grid_pattern");
    assert.equal(dl.patterns[0].width, 16);
    assert.equal(dl.patterns[0].tiles.length, 2);
    assert.equal(dl.items[0].style.fill.kind, "pattern");
    assert.equal(dl.items[0].style.fill.name, "grid_pattern");
  });

  it("emits a real SVG pattern def and references it", () => {
    const svg = toSvg(src);
    assert.ok(
      svg.includes(
        '<pattern id="pvg-pat-grid_pattern" patternUnits="userSpaceOnUse" width="16.00" height="16.00">'
      )
    );
    assert.ok(svg.includes('fill="url(#pvg-pat-grid_pattern)"'));
  });

  it("rejects unknown pattern names at evaluation time", () => {
    assert.throws(
      () =>
        compile(`PVG 0.2
canvas 10 10
rectangle
  pos [0, 0]
  size [10, 10]
  fill pattern missing_tile`),
      /Unknown pattern 'missing_tile'/
    );
  });
});

describe("18.5 arrays and splines", () => {
  it("auto-lays bare numbers across pos/size", () => {
    const dl = compile(`PVG 0.2
canvas 400 200

set history = [12, 45, 68, 30, 85, 92, 40]

spline
  points history
  pos [20, 20]
  size [360, 120]
  stroke #00ffcc
  width 2`);
    const spline = dl.items[0];
    assert.equal(spline.type, "Spline");
    assert.equal(spline.points.length, 7);
    assert.equal(spline.points[0][0], 20);
    assert.equal(spline.points[6][0], 380);
    // y is down-positive: the max value (92) sits at the TOP of the box.
    assert.equal(spline.points[5][1], 20);
    // The min value (12) sits at the bottom.
    assert.equal(spline.points[0][1], 140);
    assert.equal(spline.style.width, 2);
  });

  it("accepts [x, y] control vectors", () => {
    const dl = compile(`PVG 0.2
canvas 200 200
spline
  points [[10, 10], [60, 90], [120, 40], [190, 150]]
  stroke #ffffff
  width 2`);
    assert.deepEqual(dl.items[0].points[0], [10, 10]);
    assert.deepEqual(dl.items[0].points[3], [190, 150]);
  });

  it("emits a cubic-bezier SVG path for splines", () => {
    const svg = toSvg(`PVG 0.2
canvas 200 200
spline
  points [0, 0, 50, 100, 100, 0]
  pos [10, 10]
  size [180, 180]
  stroke #00ffcc
  width 2`);
    assert.ok(svg.includes('<path d="M '));
    assert.ok(svg.includes("C "));
    assert.ok(svg.includes('fill="none"'));
  });

  it("implements len / get / array with negative-wrap indexing", () => {
    const dl = compile(`PVG 0.2
canvas 200 100
set data = [5, 7, 9]
set n = len(data)
set first = get(data, 0)
set wrapped = get(data, -1)
set pair = array(1, 2)
set pn = len(pair)
set text_value = "" + n + first + wrapped + pn
text
  pos [5, 5]
  content text_value
  size 12`);
    assert.equal(dl.items[0].content, "3592"); // len=3, get(0)=5, get(-1)=9, len(array)=2
  });

  it("keeps 2 scalar brackets as Vec2 and 3+ as an Array", () => {
    const doc = parse(`PVG 0.2
canvas 100 100
set v = [1, 2]
set a = [1, 2, 3]
set nested = [[10, 10], [90, 90]]`);
    const stmts = doc.statements;
    assert.equal(stmts[0].expr.type, "Vec2");
    assert.equal(stmts[1].expr.type, "Array");
    assert.equal(stmts[2].expr.type, "Array");
    assert.equal(stmts[2].expr.items.length, 2);
    assert.equal(stmts[2].expr.items[0].type, "Vec2");
  });

  it("converts control points to cubic beziers (Catmull-Rom)", () => {
    const segs = splineToBezier([
      [0, 0],
      [10, 20],
      [30, 0],
    ]);
    assert.equal(segs.length, 2);
    assert.equal(segs.length, 2);
    // Segment 0: p0 = p1 = [0,0], p2 = [10,20], p3 = [30,0].
    //   c1 = p1 + (p2 - p0)/6, c2 = p2 - (p3 - p1)/6, end = p2
    assert.deepEqual(segs[0][0], [0 + 10 / 6, 0 + 20 / 6]);
    assert.deepEqual(segs[0][1], [10 - 30 / 6, 20 - 0 / 6]);
    assert.deepEqual(segs[0][2], [10, 20]);
    // Two points collapse to a single degenerate cubic.
    assert.equal(splineToBezier([[0, 0], [5, 5]]).length, 1);
    assert.equal(splineToBezier([[1, 1]]).length, 0);
  });
});

describe("18.6 control flow inside path bodies", () => {
  it("expands a for loop into path commands sharing the path locals", () => {
    const dl = compile(`PVG 0.2
canvas 200 200

path
  fill none
  stroke #ffffff
  width 2
  for deg from 0 to 360 step 90
    set pt = [100 + 50 * cos(deg), 100 + 50 * sin(deg)]
    if deg == 0
      start pt
    else
      line pt
  close`);
    const path = dl.items[0];
    assert.equal(path.type, "Path");
    // 5 iterations (0, 90, 180, 270, 360): Start + 4x Line + Close.
    assert.equal(path.commands.length, 6);
    assert.equal(path.commands[0].cmd, "Start");
    assert.equal(path.commands[5].cmd, "Close");
  });

  it("supports while loops and else-if chains inside a path", () => {
    const dl = compile(`PVG 0.2
canvas 100 100
path
  fill none
  stroke #ffffff
  set i = 0
  while i < 3
    set i = i + 1
    if i == 1
      start [i * 10, 0]
    else
      if i == 2
        line [i * 10, 10]
      else
        line [i * 10, 20]
  close`);
    assert.equal(dl.items[0].commands.length, 4);
  });

  it("keeps style properties out of control blocks (parse error)", () => {
    assert.throws(
      () =>
        compile(`PVG 0.2
canvas 10 10
path
  fill #ffffff
  for i from 0 to 2
    stroke #000000
    line [i, i]`),
      /Invalid path command/
    );
  });
});

describe("soft keywords and lexer additions", () => {
  it("keeps soft keywords usable as identifiers", () => {
    const dl = compile(`PVG 0.2
canvas 200 200

set row = 5
set data = 3
set snap = 2
for sprite from 0 to 2
  circle
    center [row * 20, data * 20]
    radius snap + sprite
    fill #00ffcc`);
    assert.equal(dl.items.length, 3);
    assert.equal(dl.items[0].center[0], 100);
  });

  it("rejects unclosed triple-quoted blocks", () => {
    assert.throws(
      () =>
        compile(`PVG 0.2
canvas 32 32
sprite
  pos [0, 0]
  palette [#ffffff]
  data """
  11
  `),
      /Unclosed triple-quoted string/
    );
  });

  it("requires a sprite palette", () => {
    assert.throws(
      () =>
        compile(`PVG 0.2
canvas 32 32
sprite
  pos [0, 0]
  data "11"`),
      /palette/
    );
  });
});

describe("new built-ins", () => {
  it("supports radians/degrees/deg_to_rad as both constant and call", () => {
    const dl = compile(`PVG 0.2
canvas 100 100
set a = radians(180)
set b = degrees(PI)
set c = deg_to_rad(90)
set d = deg_to_rad * 2
line
  from [a, b]
  to [c, d]
  stroke #ffffff`);
    const line = dl.items[0];
    assert.ok(Math.abs(line.from[0] - Math.PI) < 1e-12);
    assert.ok(Math.abs(line.from[1] - 180) < 1e-12);
    assert.ok(Math.abs(line.to[0] - Math.PI / 2) < 1e-12);
    assert.ok(Math.abs(line.to[1] - 2 * (Math.PI / 180)) < 1e-12);
  });

  it("validates arity for noise and get", () => {
    assert.throws(
      () =>
        compile(`PVG 0.2
canvas 10 10
set n = noise2d(1.0)
circle
  center [0, 0]
  radius n`),
      /noise2d\(x, y\) needs 2 arguments/
    );
    assert.throws(
      () =>
        compile(`PVG 0.2
canvas 10 10
set n = get([1, 2])
circle
  center [0, 0]
  radius n`),
      /get\(arr, i\) needs 2 arguments/
    );
  });

  it("still honours the loop safety limit", () => {
    assert.throws(
      () =>
        compile(`PVG 0.2
canvas 10 10
while true
  circle
    center [0, 0]
    radius 1`),
      /safety loop limit/
    );
  });
});

describe("0.1/0.2 back-compatibility is preserved", () => {
  it("still accepts 0.1 headers and default style semantics", () => {
    const dl = compile(`PVG 0.1
canvas 100 100
line
  from [0, 0]
  to [10, 10]
  stroke #ffffff
  width 4`);
    assert.equal(dl.items[0].style.cap, "butt");
    assert.equal(dl.items[0].style.join, "miter");
    assert.deepEqual(dl.items[0].style.dash, []);
    assert.equal(dl.items[0].style.blend, "normal");
    assert.equal(dl.items[0].style.blur, 0);
    assert.equal(dl.snap, 0);
    assert.equal(dl.pixelFilter, "linear");
  });
});
