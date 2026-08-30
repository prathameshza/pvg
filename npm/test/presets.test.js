import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { parse, evaluate, toSvg, toAnimatedSvg } from "../dist/index.js";

const PRESETS = [
  {
    name: "Chrome Dino Runner",
    code: `PVG 0.1
canvas 80 72
  background #000000
set fg = #f97316
set t2 = time % 2.0
set in_jump = (t2 >= 0.6) and (t2 <= 1.2)
set jump_y = in_jump ? (-30 * sin(((t2 - 0.6) / 0.6) * PI)) : 0
set leg = (time % 0.2) < 0.1
rect
  pos [0.5, 0.5]
  size [79, 71]
  stroke fg
  opacity 0.2
  fill none
for i from 0 to 21
  set gx = i * 4 - ((time * 50) % 4)
  line
    from [gx, 54]
    to   [gx + 1, 54]
    stroke fg
    opacity 0.3
group
  pos [80 - (t2 / 2.0) * 140, 18]
  rect
    pos [0, 26]
    size [3, 10]
    fill fg
group
  pos [0, 30.2222 + jump_y]
  polygon
    fill fg
    points [12.56,12.22] [13.44,12.22] [13.44,14] [14.33,14] [14.33,14.89] [15.22,14.89] [15.22,15.78] [17,15.78] [17,14.89] [17.89,14.89] [17.89,14] [19.22,14] [19.22,13.11] [20.56,13.11] [20.56,12.22] [21.44,12.22] [21.44,6.44] [22.33,6.44] [22.33,5.56] [29.44,5.56] [29.44,6.44] [30.33,6.44] [30.33,10.44] [25.89,10.44] [25.89,11.33] [28.56,11.33] [28.56,12.22] [25,12.22] [25,14] [26.78,14] [26.78,15.78] [25.89,15.78] [25.89,14.89] [25,14.89] [25,18] [24.11,18] [24.11,19.33] [23.22,19.33] [23.22,20.22] [22.33,20.22] [22.33,21.11] [15.22,21.11] [15.22,20.22] [14.33,20.22] [14.33,19.33] [13.44,19.33] [13.44,18.44] [12.56,18.44] [12.56,17.56]
  rect
    pos [23.22, 6.89]
    size [0.89, 0.89]
    fill #000
  rect
    pos [17, 21.11]
    size [1.78, leg ? 2.67 : 0.89]
    fill fg
  rect
    pos [21.44, 21.11]
    size [1.78, leg ? 0.89 : 2.67]
    fill fg`,
  },
  {
    name: "Radar Scanner",
    code: `PVG 0.1
canvas 600 600
  background #080a0f
set cx = 300
set cy = 300
set sweep = time * 2.0
for r_idx from 1 to 4
  circle
    center [cx, cy]
    radius r_idx * 55
    fill none
    stroke #103b42
    width 1.5
for trail from 0 to 20
  set a = sweep - trail * 0.035
  line
    from [cx, cy]
    to   [cx + 230 * cos(a), cy + 230 * sin(a)]
    stroke #00ffcc
    width 2
    opacity (1.0 - trail / 20) * 0.45
line
  from [cx, cy]
  to   [cx + 230 * cos(sweep), cy + 230 * sin(sweep)]
  stroke #ffffff
  width 2.5
circle
  center [cx, cy]
  radius 8
  fill #00ffcc`,
  },
  {
    name: "Technical Dial",
    code: `PVG 0.1
canvas 600 600
  background #141419
set cx = 300
set cy = 300
set outer_r = 200
set inner_r = 170
path
  stroke #2c2d35
  width 14
  fill none
  start [cx + outer_r * cos(135deg), cy + outer_r * sin(135deg)]
  arc [cx, cy] outer_r 135deg 405deg
path
  stroke #00d2ff
  width 14
  fill none
  start [cx + outer_r * cos(135deg), cy + outer_r * sin(135deg)]
  arc [cx, cy] outer_r 135deg 325deg
for i from 0 to 24
  set angle = 135deg + i * (270deg / 24)
  set is_major = (i % 4 == 0)
  set tick_len = is_major ? 18 : 8
  line
    from [cx + inner_r * cos(angle), cy + inner_r * sin(angle)]
    to   [cx + (inner_r - tick_len) * cos(angle), cy + (inner_r - tick_len) * sin(angle)]
    stroke is_major ? #ffffff : #666677
    width is_major ? 3 : 1
circle
  center [cx, cy]
  radius 18
  fill #ffffff
  stroke #00d2ff
  width 4`,
  },
];

describe("Official Reference Presets End-to-End Test", () => {
  for (const preset of PRESETS) {
    it(`should compile, evaluate and transpile preset: ${preset.name}`, () => {
      const ast = parse(preset.code);
      assert.ok(ast.statements.length > 0);

      const dl = evaluate(ast, 0.5);
      assert.ok(dl.items.length > 0, "Must produce visual primitives");

      const svg = toSvg(preset.code, 0.0);
      assert.ok(svg.includes("<svg"), "Must produce valid SVG");

      const animatedSvg = toAnimatedSvg(preset.code, { duration: 2.0, fps: 15 });
      assert.ok(animatedSvg.includes("<svg"), "Must produce valid SMIL animated SVG");
    });
  }
});