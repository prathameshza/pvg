use pvg::*;

// 1. Host params / uniforms
#[test]
fn verify_params() {
    let src = r#"
PVG 0.2
canvas 400 60

param health: 0.75
param shield = 0.40
param player_name: "Pilot_01"

rectangle
  pos [20, 20]
  size [200 * health, 12]
  fill health < 0.25 ? #ff3344 : #00e676
"#;
    let mut scene = Scene::from_source(src).expect("params parse");
    assert_eq!(scene.param_names(), vec!["health", "shield", "player_name"]);
    let dl = scene.evaluate().unwrap();
    assert_eq!(dl.len(), 1);
    if let DrawCmd::Rectangle { size, .. } = &dl.items[0] {
        assert!((size.0 - 150.0).abs() < 1e-9, "default health 0.75 -> {:?}", size);
    } else {
        panic!("expected rect");
    }
    // Host override without re-parsing.
    scene.set_param("health", 0.2);
    scene.set_param("shield", 0.9);
    scene.set_param("player_name", "Ace");
    let dl2 = scene.evaluate().unwrap();
    if let DrawCmd::Rectangle { size, style, .. } = &dl2.items[0] {
        assert!((size.0 - 40.0).abs() < 1e-9, "overridden health 0.2 -> {:?}", size);
        assert_eq!(style.fill, Paint::Color(Color::Rgba(255, 51, 68, 255)));
    } else {
        panic!("expected rect");
    }
    // compile_with_params helper + Evaluator::with_param parity.
    let dl3 = pvg::compile_with_params(src, &[("health", Value::Number(0.5))], 0.0).unwrap();
    if let DrawCmd::Rectangle { size, .. } = &dl3.items[0] {
        assert!((size.0 - 100.0).abs() < 1e-9);
    }
    let doc = parse(src).unwrap();
    let ev = Evaluator::new_with_time(0.0).with_param("health", 1.0);
    let dl4 = ev.evaluate_document(&doc).unwrap();
    if let DrawCmd::Rectangle { size, .. } = &dl4.items[0] {
        assert!((size.0 - 200.0).abs() < 1e-9);
    }
}

// 2. Deterministic noise
#[test]
fn verify_noise() {
    // Pure-function determinism + range.
    let a = pvg_noise2(1.5, -2.25);
    let b = pvg_noise2(1.5, -2.25);
    assert_eq!(a, b);
    assert!((-1.0..=1.0).contains(&a));
    let c = pvg_noise3(0.1, 0.2, 0.3);
    assert_eq!(c, pvg_noise3(0.1, 0.2, 0.3));
    assert!((-1.0..=1.0).contains(&c));
    // Varies across space.
    assert_ne!(pvg_noise2(0.0, 0.0), pvg_noise2(10.5, 3.7));

    let src = r#"
PVG 0.2
canvas 400 400
  background #07090e

seed 9921

set cx = 200
set cy = 200

path
  fill none
  stroke #00ffff
  width 2
  start [cx + 100, cy]
  line [cx + (100 + noise2d(1.0, time * 1.5) * 12), cy + 20]
"#;
    let dl = compile(src).expect("noise path compiles");
    assert_eq!(dl.len(), 1);
    // Builtins visible to evaluator.
    let src2 = "PVG 0.2\ncanvas 10 10\nset n = noise2d(3.0, 4.0)\nset m = noise3d(1.0, 2.0, time)\ncircle\n  center [5, 5]\n  radius 2 + n\n";
    assert_eq!(compile(src2).unwrap().len(), 1);
}

// 3. Pixel grid / sprite mode
#[test]
fn verify_sprite() {
    let src = r#"
PVG 0.2
canvas 256 240
  snap 1.0
  filter "nearest"

sprite
  pos [10.4, 20.6]
  palette [#00000000, #b84418, #fc9838]
  data "0110"
  data "1221"
"#;
    let dl = compile(src).expect("sprite compiles");
    assert_eq!(dl.len(), 1);
    assert!((dl.snap - 1.0).abs() < 1e-9);
    assert_eq!(dl.pixel_filter, PixelFilter::Nearest);
    if let DrawCmd::Sprite { pos, palette, rows, scale, .. } = &dl.items[0] {
        // Snap rounds 10.4 -> 10, 20.6 -> 21.
        assert_eq!(*pos, (10.0, 21.0));
        assert_eq!(palette.len(), 3);
        assert_eq!(rows.len(), 2);
        assert!((*scale - 1.0).abs() < 1e-9);
    } else {
        panic!("expected sprite");
    }
    let svg = dl.to_svg();
    assert!(svg.contains("crispEdges"));
    assert!(svg.contains("#b84418"));
}

#[test]
fn verify_triple_quoted_sprite_block() {
    // Multi-line `"""` art block: blank edge lines dropped, block indent stripped.
    let src = "PVG 0.2\ncanvas 64 64\nsprite\n  pos [0, 0]\n  palette [#00000000, #ff0000]\n  data \"\"\"\n  11\n  11\n  \"\"\"\n";
    let dl = compile(src).expect("triple-quoted sprite compiles");
    assert_eq!(dl.len(), 1);
    if let DrawCmd::Sprite { rows, .. } = &dl.items[0] {
        assert_eq!(rows, &vec!["11".to_string(), "11".to_string()]);
    } else {
        panic!("expected sprite");
    }
    // Same-line open+close also works.
    let src2 = "PVG 0.2\ncanvas 64 64\nsprite\n  pos [0, 0]\n  palette [#00000000, #ff0000]\n  data \"\"\"11\"\"\"\n";
    assert_eq!(compile(src2).unwrap().len(), 1);
    // Triple-quoted text content keeps inner newlines.
    let src3 = "PVG 0.2\ncanvas 64 64\ntext\n  pos [0, 0]\n  content \"\"\"\n  hi\n  there\n  \"\"\"\n";
    let dl3 = compile(src3).unwrap();
    if let DrawCmd::Text { content, .. } = &dl3.items[0] {
        assert_eq!(content, "hi\nthere");
    } else {
        panic!("expected text");
    }
    // Unclosed block is a clean lex error, not a hang.
    assert!(compile("PVG 0.2\ncanvas 64 64\nsprite\n  pos [0, 0]\n  data \"\"\"\n  11\n").is_err());
    // Trailing `#` comments allowed on opener and closer lines.
    let src4 = "PVG 0.2\ncanvas 64 64\nsprite\n  pos [0, 0]\n  palette [#00000000, #ff0000]\n  data \"\"\"  # drone art\n  11\n  \"\"\"  # end\n";
    let dl4 = compile(src4).expect("commented triple quotes compile");
    if let DrawCmd::Sprite { rows, .. } = &dl4.items[0] {
        assert_eq!(rows, &vec!["11".to_string()]);
    } else {
        panic!("expected sprite");
    }
}

#[test]
fn verify_soft_keyword_identifiers() {
    // Post-0.2 keywords stay usable as variable/function names (grid.pvg
    // uses `for row ...`); their special meaning applies only where their
    // syntax applies.
    let src = r#"
PVG 0.1
canvas 600 10

set data = 3
set filter = 4

def row(v)
  circle
    center [v, 5]
    radius 2

for row from 0 to 2
  set snap = row * 10 + data + filter
  row(snap)
  circle
    center [snap, 5]
    radius 1
"#;
    let dl = compile(src).expect("soft keywords as identifiers");
    // 3 loop iterations x (call circle + direct circle) = 6 shapes.
    assert_eq!(dl.len(), 6);
    if let DrawCmd::Circle { center, .. } = &dl.items[0] {
        // row=0: snap = 0 + 3 + 4 = 7.
        assert_eq!(*center, (7.0, 5.0));
    } else {
        panic!("expected circle");
    }

    // A variable literally named `pattern` still works as a fill value.
    let src2 = "PVG 0.2\ncanvas 10 10\nset pattern = #ff0000\ncircle\n  center [5, 5]\n  radius 2\n  fill pattern\n";
    let dl2 = compile(src2).unwrap();
    if let DrawCmd::Circle { style, .. } = &dl2.items[0] {
        assert_eq!(style.fill, Paint::Color(Color::Rgba(255, 0, 0, 255)));
    } else {
        panic!("expected circle");
    }
}

#[test]
fn verify_path_ternary_hint() {
    // `cond ? start a : line b` is invalid (commands aren't expressions);
    // the error must point at if/else instead of a bare token dump.
    let bad = "PVG 0.2\ncanvas 10 10\npath\n  fill none\n  stroke #ffffff\n  for i from 0 to 2\n    set pt = [i, i]\n    i == 0 ? start pt : line pt\n";
    let err = compile(bad).expect_err("ternary path commands must fail");
    let msg = err.to_string();
    assert!(msg.contains("if <cond>"), "hint missing: {}", msg);
}

#[test]
fn verify_path_control_flow() {
    // for + if/else inside path: organic closed perimeter (noise wobble).
    let src = r#"
PVG 0.2
canvas 400 400

param shield_power: 0.8
set cx = 200
set cy = 200

path
  fill none
  stroke #00ffff
  width 2
  for deg from 0 to 360 step 90
    set rad = deg * (PI / 180)
    set n = noise2d(cos(rad) * 1.8, sin(rad) * 1.8)
    set r = 100 + n * 10
    set pt = [cx + r * cos(rad), cy + r * sin(rad)]
    if deg == 0
      start pt
    else
      line pt
  close
"#;
    let dl = compile(src).expect("path control flow compiles");
    assert_eq!(dl.len(), 1);
    assert!(!parse(src).unwrap().is_animated(), "no time/t reference, so static");
    if let DrawCmd::Path { commands, .. } = &dl.items[0] {
        // 5 loop iterations (0,90,180,270,360): Start + 4x Line + Close.
        assert_eq!(commands.len(), 6);
        assert!(matches!(commands[0], DrawPathCommand::Start(_)));
        assert!(matches!(commands[5], DrawPathCommand::Close));
    } else {
        panic!("expected path");
    }

    // while + else-if inside path.
    let src2 = r#"
PVG 0.2
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
  close
"#;
    let dl2 = compile(src2).expect("while/else-if in path compiles");
    if let DrawCmd::Path { commands, .. } = &dl2.items[0] {
        assert_eq!(commands.len(), 4); // Start, Line, Line, Close
    } else {
        panic!("expected path");
    }

    // Style props belong in the path body, not inside control blocks.
    let bad = "PVG 0.2\ncanvas 10 10\npath\n  fill #ffffff\n  for i from 0 to 2\n    stroke #000000\n    line [i, i]\n";
    assert!(compile(bad).is_err());
}

// 4. Pattern fills
#[test]
fn verify_pattern() {
    let src = r#"
PVG 0.2
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
  fill pattern grid_pattern
"#;
    let doc = parse(src).expect("pattern parses");
    assert_eq!(doc.pattern_names(), vec!["grid_pattern"]);
    let dl = compile(src).expect("pattern compiles");
    assert_eq!(dl.len(), 1);
    assert_eq!(dl.patterns.len(), 1);
    assert_eq!(dl.patterns[0].tiles.len(), 2);
    if let DrawCmd::Rectangle { style, .. } = &dl.items[0] {
        assert_eq!(style.fill, Paint::Pattern("grid_pattern".into()));
    } else {
        panic!("expected rect");
    }
    let svg = dl.to_svg();
    assert!(svg.contains("pvg-pat-grid_pattern"));
}

// 5. Arrays + splines
#[test]
fn verify_arrays_spline() {
    let src = r#"
PVG 0.2
canvas 400 200

set history = [12, 45, 68, 30, 85, 92, 40]
set n = len(history)
set third = get(history, 2)

spline
  points history
  pos [20, 20]
  size [360, 120]
  stroke #00ffcc
  width 2
"#;
    let dl = compile(src).expect("spline compiles");
    assert_eq!(dl.len(), 1);
    if let DrawCmd::Spline { points, style } = &dl.items[0] {
        assert_eq!(points.len(), 7);
        // Auto-layout: first x at pos.x, last at pos.x + size.x.
        assert!((points[0].0 - 20.0).abs() < 1e-9);
        assert!((points[6].0 - 380.0).abs() < 1e-9);
        assert_eq!(style.width, 2.0);
    } else {
        panic!("expected spline");
    }
    let svg = dl.to_svg();
    assert!(svg.contains("<path d=\"M"));
    // Vec2 control points + explicit 2-elem array() builtin.
    let src2 = "PVG 0.2\ncanvas 100 100\nset pts = [[10, 10], [90, 90]]\nspline\n  points pts\n  stroke #ffffff\n  width 1\n";
    assert_eq!(compile(src2).unwrap().len(), 1);
    let src3 = "PVG 0.2\ncanvas 100 100\nset pair = array(12, 45)\nset n = len(pair)\ntext\n  pos [5, 5]\n  content \"\" + n\n";
    assert_eq!(compile(src3).unwrap().len(), 1);
}
