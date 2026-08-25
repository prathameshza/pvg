/**
 * Procedural Vector Graphics (PVG) 0.1 - Reference Presets
 */

const PVG_PRESETS = [
  {
    name: '🦖 Chrome Dino Runner (Anim)',
    code: `PVG 0.1
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
    fill fg`,
  },
  {
    name: '🌀 Radar Scanner (Anim)',
    code: `PVG 0.1
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
  width 2`,
  },
  {
    name: 'Dashboard Dial',
    code: `PVG 0.1
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
  close`,
  },
  {
    name: 'Procedural Grid',
    code: `PVG 0.1
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
      opacity 0.2`,
  },
  {
    name: 'Golden Spiral',
    code: `PVG 0.1
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
    opacity 0.85 - (i * 0.008)`,
  },
  {
    name: 'Paths & Curves',
    code: `PVG 0.1
canvas 600 600
  background #1e1e24

# Smooth Quadratic Curve Wave
path
  fill none
  stroke #ff9f1c
  width 4
  start [50, 200]
  quad [150, 50] [250, 200]
  quad [350, 350] [450, 200]
  line [550, 200]

# Cubic Bézier S-Shape
path
  fill none
  stroke #2ec4b6
  width 5
  start [100, 450]
  curve [150, 250] [450, 550] [500, 350]

# Filled Smooth Polygon
polygon
  points [300, 80] [360, 160] [320, 220] [280, 220] [240, 160]
  fill #e71d36
  stroke #ffffff
  width 2
  opacity 0.8`,
  },
  {
    name: 'Gears & Groups',
    code: `PVG 0.1
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
draw_gear(410, 300, 8, 75, 20, #03a9f4)`,
  },
];

window.PVG_PRESETS = PVG_PRESETS;