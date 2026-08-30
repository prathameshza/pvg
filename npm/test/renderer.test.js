import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { toSvg, toAnimatedSvg, escapeXml, detectLoopDuration } from "../dist/index.js";

describe("Renderer & SVG Transpilation", () => {
  it("should escape XML entities properly", () => {
    assert.equal(
      escapeXml('AT&T <speed> "100%" & \'ok\''),
      "AT&amp;T &lt;speed&gt; &quot;100%&quot; &amp; &apos;ok&apos;"
    );
  });

  it("should detect animation loop durations from modulo expressions", () => {
    assert.equal(detectLoopDuration("set t2 = time % 2.5"), 2.5);
    assert.equal(detectLoopDuration("set t2 = time % 4"), 4.0);
    assert.equal(detectLoopDuration("circle\n  radius 10"), 2.0); // Default fallback
  });

  it("should transpile PVG to valid static W3C SVG XML", () => {
    const src = `
PVG 0.1
canvas 600 400
  background #0b0c10

circle
  center [300, 200]
  radius 80
  fill #00ffcc
  stroke #ffffff
  width 2

text
  pos [300, 310]
  content "TELEMETRY & STATUS"
  size 14
  font "mono"
  align "center"
  fill #ffffff
`;
    const svg = toSvg(src);

    assert.ok(svg.startsWith("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert.ok(svg.includes("<svg viewBox=\"0 0 600 400\""));
    assert.ok(svg.includes("<rect width=\"100%\" height=\"100%\" fill=\"#0b0c10\""));
    assert.ok(svg.includes("<circle cx=\"300.00\" cy=\"200.00\" r=\"80.00\""));
    assert.ok(svg.includes("<text x=\"300.00\" y=\"310.00\" font-size=\"14.00\""));
    assert.ok(svg.includes("TELEMETRY &amp; STATUS"));
    assert.ok(svg.endsWith("</svg>\n"));
  });

  it("should generate W3C SMIL animated SVGs with <animate> tags", () => {
    const src = `
PVG 0.1
canvas 400 400
  background #000000

set pulse = 40 + 20 * sin(time * 3.14)

circle
  center [200, 200]
  radius pulse
  fill #ff0055
`;
    const animatedSvg = toAnimatedSvg(src, { duration: 2.0, fps: 15 });

    assert.ok(animatedSvg.includes("<svg"));
    assert.ok(animatedSvg.includes("<animate attributeName=\"visibility\""));
    assert.ok(animatedSvg.includes("dur=\"2.00s\""));
    assert.ok(animatedSvg.includes("repeatCount=\"indefinite\""));
  });
});