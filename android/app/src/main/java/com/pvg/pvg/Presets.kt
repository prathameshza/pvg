package com.pvg.pvg

data class Preset(
    val name: String,
    val description: String,
    val isAnimated: Boolean,
    val code: String
)

object Presets {
    val list = listOf(
        Preset(
            name = "📊 Telemetry Monitor Card",
            description = "System-font text, live RPM/temp values, multi-alignment footer (mirrors presets/telemetry_card.pvg)",
            isAnimated = true,
            code = """
PVG 0.1
canvas 600 400
  background #0b0c10

# Card Background Container
rectangle
  pos [40, 40]
  size [520, 320]
  radius 12
  fill #12141c
  stroke #1f2333
  width 1.5

# Top Header Bar
rectangle
  pos [40, 40]
  size [520, 52]
  radius 12
  fill #181b26
  stroke none

# Card Header Title (Sans-Serif Font)
text
  pos [60, 56]
  content "TELEMETRY MONITOR"
  size 16
  font "sans"
  align "left"
  fill #00ffcc

# Live Status Badge
circle
  center [480, 66]
  radius 4
  fill #00e676

text
  pos [495, 58]
  content "LIVE"
  size 13
  font "mono"
  align "left"
  fill #00e676

# Dynamic Animated Values
set rpm = floor(3200 + 400 * sin(time * 3.0))
set temp = floor(68 + 8 * cos(time * 2.0))

# Left Metric Card
rectangle
  pos [60, 115]
  size [225, 110]
  radius 8
  fill #171922
  stroke #282c3f
  width 1

text
  pos [80, 130]
  content "ENGINE SPEED"
  size 12
  font "sans"
  align "left"
  fill #8f96b0

text
  pos [80, 155]
  content "" + rpm + " RPM"
  size 28
  font "mono"
  align "left"
  fill #ffffff

# Right Metric Card
rectangle
  pos [315, 115]
  size [225, 110]
  radius 8
  fill #171922
  stroke #282c3f
  width 1

text
  pos [335, 130]
  content "CORE TEMP"
  size 12
  font "sans"
  align "left"
  fill #8f96b0

text
  pos [335, 155]
  content "" + temp + " °C"
  size 28
  font "mono"
  align "left"
  fill #ff3355

# Multi-alignment Footer Test
text
  pos [60, 310]
  content "LEFT: System OK"
  size 12
  font "mono"
  align "left"
  fill #5e6278

text
  pos [300, 310]
  content "CENTER: 60 FPS"
  size 12
  font "mono"
  align "center"
  fill #5e6278

text
  pos [540, 310]
  content "RIGHT: Time " + floor(time) + "s"
  size 12
  font "mono"
  align "right"
  fill #5e6278
""".trimIndent()
        ),
        Preset(
            name = "🦖 Chrome Dino Runner",
            description = "Sine gravity jump curve and procedural moving ground dashes",
            isAnimated = true,
            code = """
PVG 0.1
canvas 80 72
  background #000000

set fg = #f97316
set t2 = time % 2.0
set in_jump = (t2 >= 0.6) and (t2 <= 1.2)
set jump_y = in_jump ? (-30 * sin(((t2 - 0.6) / 0.6) * PI)) : 0
set leg = (time % 0.2) < 0.1

# UI Borders & Indicators
rect
  pos [0.5, 0.5]
  size [79, 71]
  stroke fg
  opacity 0.2
  fill none
rect
  pos [0.5, 0.5]
  size [79, 4]
  stroke fg
  opacity 0.5
  fill none
rect
  pos [2, 2]
  size [8, 1]
  fill fg
rect
  pos [66, 2]
  size [12, 1]
  fill fg

# Procedural Moving Ground Track
for i from 0 to 21
  set gx = i * 4 - ((time * 50) % 4)
  line
    from [gx, 54]
    to   [gx + 1, 54]
    stroke fg
    opacity 0.3

# Moving Cactus
group
  pos [80 - (t2 / 2.0) * 140, 18]
  rect
    pos [0, 26]
    size [3, 10]
    fill fg
  rect
    pos [-2, 28]
    size [2, 4]
    fill fg
  rect
    pos [3, 29]
    size [2, 3]
    fill fg

# Animated Dino
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
    fill fg
""".trimIndent()
        ),
        Preset(
            name = "🌀 Radar Scanner",
            description = "Concentric range rings, sweep phosphor trail, and orbiting beacons",
            isAnimated = true,
            code = """
PVG 0.1
canvas 600 600
  background #080a0f

set cx = 300
set cy = 300
set sweep = time * 2.0

# Radar Concentric Range Rings
for r_idx from 1 to 4
  circle
    center [cx, cy]
    radius r_idx * 55
    fill none
    stroke #103b42
    width 1.5
    opacity 0.7

# Crosshair Lines
line
  from [cx - 240, cy]
  to   [cx + 240, cy]
  stroke #155560
  width 1
  opacity 0.5

line
  from [cx, cy - 240]
  to   [cx, cy + 240]
  stroke #155560
  width 1
  opacity 0.5

# Rotating Phosphor Sweep Trail
for trail from 0 to 20
  set a = sweep - trail * 0.035
  line
    from [cx, cy]
    to   [cx + 230 * cos(a), cy + 230 * sin(a)]
    stroke #00ffcc
    width 2
    opacity (1.0 - trail / 20) * 0.45

# Main Sweep Line
line
  from [cx, cy]
  to   [cx + 230 * cos(sweep), cy + 230 * sin(sweep)]
  stroke #ffffff
  width 2.5

# Orbiting Satellites with Pulsing Beacons
for b from 0 to 4
  set orbit_r = 65 + b * 40
  set speed = 0.6 + b * 0.25
  set b_angle = (time * speed) + b * 1.5
  set bx = cx + orbit_r * cos(b_angle)
  set by = cy + orbit_r * sin(b_angle)
  
  set pulse = 4 + 3 * sin(time * 8 + b * 2)
  circle
    center [bx, by]
    radius pulse
    fill #ff0055
    opacity 0.85
    stroke #ffffff
    width 1.5

  circle
    center [bx, by]
    radius pulse + 6
    fill none
    stroke #ff0055
    width 1
    opacity 0.35

# Central Hub Beacon
circle
  center [cx, cy]
  radius 8
  fill #00ffcc
  stroke #ffffff
  width 2
""".trimIndent()
        ),
        Preset(
            name = "🎛️ Technical Dial",
            description = "Forward circular arcs, tick loop, and pointer needle",
            isAnimated = false,
            code = """
PVG 0.1
canvas 600 600
  background #141419

set cx = 300
set cy = 300
set outer_r = 200
set inner_r = 170

# Outer Background Track
path
  stroke #2c2d35
  width 14
  fill none
  start [cx + outer_r * cos(135deg), cy + outer_r * sin(135deg)]
  arc [cx, cy] outer_r 135deg 405deg

# Colored Value Arc
path
  stroke #00d2ff
  width 14
  fill none
  start [cx + outer_r * cos(135deg), cy + outer_r * sin(135deg)]
  arc [cx, cy] outer_r 135deg 325deg

# Procedurally Generated Ticks
for i from 0 to 24
  set angle = 135deg + i * (270deg / 24)
  set is_major = (i % 4 == 0)
  set tick_len = is_major ? 18 : 8
  
  line
    from [cx + inner_r * cos(angle), cy + inner_r * sin(angle)]
    to   [cx + (inner_r - tick_len) * cos(angle), cy + (inner_r - tick_len) * sin(angle)]
    stroke is_major ? #ffffff : #666677
    width is_major ? 3 : 1
    opacity is_major ? 1.0 : 0.5

# Central Hub
circle
  center [cx, cy]
  radius 18
  fill #ffffff
  stroke #00d2ff
  width 4

# Gauge Pointer Needle
path
  fill #ff3355
  stroke none
  
  set needle_angle = 325deg
  set nx = cos(needle_angle)
  set ny = sin(needle_angle)
  set px = -ny * 7
  set py =  nx * 7
  
  start [cx + px, cy + py]
  line  [cx + nx * (inner_r - 25), cy + ny * (inner_r - 25)]
  line  [cx - px, cy - py]
  close
""".trimIndent()
        ),
        Preset(
            name = "⚙️ Gears & Functions",
            description = "User-defined procedural def functions with trigonometric cogs",
            isAnimated = false,
            code = """
PVG 0.1
canvas 600 600
  background #111116

def draw_gear(gx, gy, teeth, outer_r, inner_r, col)
  circle
    center [gx, gy]
    radius outer_r - 10
    fill col
    stroke #ffffff
    width 2

  for t from 0 to (teeth - 1)
    set angle = t * (TAU / teeth)
    set tx = gx + outer_r * cos(angle)
    set ty = gy + outer_r * sin(angle)
    circle
      center [tx, ty]
      radius 8
      fill col

  circle
    center [gx, gy]
    radius inner_r
    fill #111116
    stroke #ffffff
    width 2

draw_gear(220, 300, 12, 110, 30, #ff5722)
draw_gear(410, 300, 8, 75, 20, #03a9f4)
""".trimIndent()
        ),
        Preset(
            name = "ðŸ”² Procedural Grid",
            description = "Nested 2D loops with 64-bit Xorshift pseudorandom radii",
            isAnimated = false,
            code = """
PVG 0.1
canvas 600 600
  background #0b0c10

seed 42

for row from 0 to 7
  for col from 0 to 7
    set x = 60 + col * 68
    set y = 60 + row * 68
    set r = 10 + random(0, 18)
    
    circle
      center [x, y]
      radius r
      fill #66fcf1
      opacity 0.25 + (col + row) * 0.05
      stroke #45a29e
      width 1.5

    rectangle
      pos [x - 20, y - 20]
      size [40, 40]
      radius 4
      fill none
      stroke #c5c6c7
      width 1
      opacity 0.2
""".trimIndent()
        ),
        Preset(
            name = "ðŸŒ€ Golden Spiral",
            description = "Logarithmic spiral evaluation with fading opacity",
            isAnimated = false,
            code = """
PVG 0.1
canvas 600 600
  background #000000

set cx = 300
set cy = 300
set a = 3.0
set b = 0.12

for i from 0 to 60
  set theta = i * 0.2
  set r = a * (2.71828 ^ (b * theta)) * 8.0
  set x = cx + r * cos(theta)
  set y = cy + r * sin(theta)
  
  circle
    center [x, y]
    radius 4 + (i * 0.2)
    fill #ff007f
    stroke #00ffff
    width 1
    opacity 0.85 - (i * 0.008)
""".trimIndent()
        ),
        Preset(
            name = "🛡️ Sci-Fi Shield Core (0.2 FX)",
            description = "Gradients, clip mask, dashed stroke, drop shadow, additive glow and blur",
            isAnimated = true,
            code = """
PVG 0.2
canvas 512 512
  background #07090e

set cx = 256
set cy = 256

# 1. Ambient background glow (additive, blurred)
circle
  center [cx, cy]
  radius 160
  fill #00aaff
  blur 45
  blend "add"
  opacity 0.25

# 2. Outer armor plate with drop shadow and a metallic linear gradient
rectangle
  pos [106, 106]
  size [300, 300]
  radius 36
  fill linear [106, 106] [406, 406]
    stop 0.0 #2b3040
    stop 0.5 #171922
    stop 1.0 #0e1017
  stroke #48526e
  width 2
  join "miter"
  shadow [0, 18] 24 #000000bb

# 3. Clipped core window: a radial sphere behind a scanline sweep
clip
  circle
    center [cx, cy]
    radius 105
  for i from -5 to 5
    line
      from [cx - 150, cy + i * 30]
      to [cx + 150, cy + i * 30 + 60]
      stroke #00ffff
      width 1.5
      opacity 0.12
  circle
    center [cx, cy]
    radius 90
    fill radial [cx, cy] 90
      stop 0.0 #00ffff
      stop 0.6 #0033aa
      stop 1.0 #07090e

# 4. Mechanical retainer ring: dashed stroke plus neon glow
circle
  center [cx, cy]
  radius 112
  fill none
  stroke #00d2ff
  width 4
  cap "butt"
  dash [28, 8, 12, 8]
  glow 8 #00d2ff80

# 5. Rotating additive energy prisms
group
  pos [cx, cy]
  rot time * 1.5
  blend "add"
  for p from 0 to 2
    set angle = p * (360deg / 3)
    path
      fill linear [0, 0] [cos(angle) * 70, sin(angle) * 70]
        stop 0.0 #ffffff
        stop 1.0 #0066ff00
      stroke #ffffff
      width 1
      join "miter"
      set r_inner = 35
      set r_outer = 68
      start [r_inner * cos(angle - 15deg), r_inner * sin(angle - 15deg)]
      line [r_outer * cos(angle), r_outer * sin(angle)]
      line [r_inner * cos(angle + 15deg), r_inner * sin(angle + 15deg)]
      close

# 6. Specularity highlight
circle
  center [cx - 6, cy - 6]
  radius 14
  fill #ffffff
  blur 4
  blend "screen"
""".trimIndent()
        ),
        Preset(
            name = "🎛️ Tactical HUD (0.2 Full)",
            description = "Full reference HUD: params, pattern chassis, drone sprite, noise shield, spline, system-font labels",
            isAnimated = true,
            code = """
PVG 0.2
canvas 512 512
  background #06080d

param shield_power: 0.82
param hull_hp: 0.65
param flux_temp: 74
param pilot_tag: "VIPER-7"

set cx = 256
set cy = 230

# Repeatable carbon-mesh tile
pattern carbon_mesh 16 16
  line
    from [0, 0] to [16, 16]
    stroke #ffffff0a
    width 1
  line
    from [16, 0] to [0, 16]
    stroke #ffffff0a
    width 1
  rectangle
    pos [0, 0]
    size [16, 16]
    fill none
    stroke #00ffff08
    width 1

# Chassis card with a real pattern fill and a drop shadow
rectangle
  pos [26, 26]
  size [460, 460]
  radius 24
  fill pattern carbon_mesh
  stroke #1b2336
  width 2
  shadow [0, 16] 24 #000000ee

# 16x16 retro targeting drone (indexed-color sprite, one data row per line)
sprite
  pos [410, 48]
  scale 2
  palette [#00000000, #ff1a4b, #1e2333, #ffffff, #ffaa00]
  data "..11........11.."
  data ".1441......1441."
  data "142241....142241"
  data "1423241..1423241"
  data ".12222111122221."
  data "..122222222221.."
  data "...1222222221..."
  data "...1244224421..."
  data "...1222222221..."
  data "...1221111221..."
  data "..1221....1221.."
  data ".14221....12241."
  data "142221....122241"
  data "142241....142241"
  data ".1441......1441."
  data "..11........11.."

# Ambient core glow
circle
  center [cx, cy]
  radius 115
  fill #0066ff
  blur 38
  opacity 0.22
  blend "add"

# Fixed tactical reticle (dashed ring)
circle
  center [cx, cy]
  radius 125
  fill none
  stroke #00d2ff
  width 2
  cap "butt"
  dash [24, 10, 6, 10]
  glow 6 #00d2ff66

# Organic fluctuating shield perimeter (noise2d + shield_power)
path
  fill none
  stroke shield_power > 0.3 ? #00ffff : #ff2255
  width 2.5
  glow 10 (shield_power > 0.3 ? #00ffff80 : #ff225580)
  blend "add"
  for deg from 0 to 360 step 6
    set rad = deg * (PI / 180)
    set n = noise2d(cos(rad) * 1.8 + time * 0.9, sin(rad) * 1.8 + time * 0.9)
    set r = 100 + n * (18 * shield_power)
    set pt = [cx + r * cos(rad), cy + r * sin(rad)]
    if deg == 0
      start pt
    else
      line pt
  close

# Central radar hub
circle
  center [cx, cy]
  radius 12
  fill #ffffff
  blur 4
  blend "screen"

# Labels render with on-device system fonts (mono/sans/serif).
text
  pos [50, 48]
  content "TACTICAL HUD // " + pilot_tag
  size 14
  font "mono"
  align "left"
  fill #00ffff

# Hull bar track + fill driven by hull_hp
rectangle
  pos [50, 78]
  size [160, 10]
  radius 3
  fill #111522
  stroke #232c42
  width 1

rectangle
  pos [50, 78]
  size [160 * hull_hp, 10]
  radius 3
  fill hull_hp > 0.3 ? #00e676 : #ff3344
  glow 4 (hull_hp > 0.3 ? #00e67680 : #ff334480)

text
  pos [220, 76]
  content "" + floor(hull_hp * 100) + "% HULL"
  size 11
  font "mono"
  fill #8fa2c7

text
  pos [50, 375]
  content "HARMONIC FLUX: " + flux_temp + " °C"
  size 12
  font "mono"
  fill #00d2ff

# Telemetry waveform from an array of samples
set wave_samples = [430, 418, 435, 395, 420, 405, 445, 410, 428, 390, 425]

spline
  pos [50, 395]
  size [412, 50]
  points wave_samples
  stroke #00ffcc
  width 2.5
  glow 8 #00ffcc66
""".trimIndent()
        )
    )
}
