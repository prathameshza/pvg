#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub category: String,
    pub description: String,
    pub source: String,
    pub is_animated: bool,
}

pub fn get_reference_presets() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "radar".to_string(),
            category: "Preset".to_string(),
            description: "Radar Scanner with rotating sweep, phosphor decay trail, and orbiting beacons".to_string(),
            source: include_str!("../../presets/radar.pvg").to_string(),
            is_animated: true,
        },
        TestCase {
            name: "dial".to_string(),
            category: "Preset".to_string(),
            description: "Technical Dashboard Dial with circular arcs, ternary ticks, and needle gauge".to_string(),
            source: include_str!("../../presets/dial.pvg").to_string(),
            is_animated: false,
        },
        TestCase {
            name: "grid".to_string(),
            category: "Preset".to_string(),
            description: "Procedural 8x8 Grid with pseudo-random Xorshift radii and rounded rectangles".to_string(),
            source: include_str!("../../presets/grid.pvg").to_string(),
            is_animated: false,
        },
        TestCase {
            name: "spiral".to_string(),
            category: "Preset".to_string(),
            description: "Logarithmic Golden Spiral with exponential calculations and fading opacity".to_string(),
            source: include_str!("../../presets/spiral.pvg").to_string(),
            is_animated: false,
        },
        TestCase {
            name: "paths".to_string(),
            category: "Preset".to_string(),
            description: "Path Primitives including quadratic Béziers, cubic Béziers, and polygons".to_string(),
            source: include_str!("../../presets/paths.pvg").to_string(),
            is_animated: false,
        },
        TestCase {
            name: "gears".to_string(),
            category: "Preset".to_string(),
            description: "User-defined procedural functions `def draw_gear` with trigonometric cogs".to_string(),
            source: include_str!("../../presets/gears.pvg").to_string(),
            is_animated: false,
        },
    ]
}

pub fn get_stress_benchmarks() -> Vec<TestCase> {
    let mut complex_paths_src = String::from("PVG 0.1\ncanvas 1200 1200\n  background #101014\n\n");
    for p in 0..20 {
        let y = 60 + p * 55;
        complex_paths_src.push_str(&format!(
            "path\n  fill none\n  stroke #ff9f1c\n  width 2.5\n  opacity 0.85\n  start [50, {y}]\n  quad [150, {}] [250, {y}]\n  curve [350, {}] [450, {}] [550, {y}]\n  line [650, {y}]\n  arc [720, {y}] 40 0deg 180deg\n  quad [850, {}] [950, {y}]\n  curve [1000, {}] [1050, {}] [1100, {y}]\n  close\n\n",
            y - 35,
            y + 45, y - 45,
            y - 30,
            y + 35, y - 35
        ));
    }

    vec![
        TestCase {
            name: "stress_10k_primitives".to_string(),
            category: "Stress".to_string(),
            description: "Generates 10,000 geometric shapes (5,000 circles + 5,000 rectangles) via procedural loops".to_string(),
            source: r#"PVG 0.1
canvas 1920 1080
  background #050508

seed 998877

for i from 0 to 4999
  set cx = random(50, 1870)
  set cy = random(50, 1030)
  set cr = random(2, 12)
  circle
    center [cx, cy]
    radius cr
    fill #00ffcc
    opacity 0.6
    stroke #ffffff
    width 1

  rectangle
    pos [cx - 5, cy - 5]
    size [10, 10]
    radius 2
    fill none
    stroke #ff007f
    width 1
    opacity 0.4
"#.to_string(),
            is_animated: false,
        },
        TestCase {
            name: "stress_nested_groups_transforms".to_string(),
            category: "Stress".to_string(),
            description: "Evaluates 2,000 hierarchical affine matrix compositions with translation, rotation, and scaling".to_string(),
            source: r#"PVG 0.1
canvas 800 800
  background #0a0a0f

for i from 0 to 999
  group
    pos [400, 400]
    rot i * 0.05rad
    scale [1.0 + (i % 5) * 0.1, 1.0 + (i % 5) * 0.1]
    line
      from [0, 0]
      to [150 + (i % 50), 0]
      stroke #00d2ff
      width 1.5
      opacity 0.25
    circle
      center [150 + (i % 50), 0]
      radius 4
      fill #ff3355
"#.to_string(),
            is_animated: false,
        },
        TestCase {
            name: "stress_math_and_trig".to_string(),
            category: "Stress".to_string(),
            description: "High-density arithmetic and trigonometric evaluations: sin, cos, tan, pow, sqrt, floor, ceil".to_string(),
            source: r#"PVG 0.1
canvas 1000 1000
  background #000000

for i from 0 to 1499
  set a = i * 0.015
  set r = sqrt(i) * 18.0
  set x = 500 + r * cos(a) + sin(a * 5) * 10
  set y = 500 + r * sin(a) + cos(a * 5) * 10
  set sz = 2.0 + (i % 10) * 0.5
  circle
    center [x, y]
    radius sz
    fill #00e676
    stroke #ffffff
    width 0.5
    opacity 0.75
"#.to_string(),
            is_animated: false,
        },
        TestCase {
            name: "stress_complex_paths".to_string(),
            category: "Stress".to_string(),
            description: "Constructs 20 multi-segment paths containing Bézier curves, lines, circular arcs, and closes".to_string(),
            source: complex_paths_src,
            is_animated: false,
        },
    ]
}