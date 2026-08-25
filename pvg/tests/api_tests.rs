use pvg::*;

#[test]
fn test_basic_circle_and_svg() {
    let src = r#"
PVG 0.1
canvas 500 500
  background #0b0c10

circle
  center [250, 250]
  radius 100
  fill #00ffcc
  stroke #ffffff
  width 2.5
  opacity 0.85
"#;

    let doc = parse(src).expect("Failed to parse");
    assert_eq!(doc.canvas.width, 500.0);
    assert_eq!(doc.canvas.height, 500.0);
    assert!(!doc.is_animated());

    let dl = compile(src).expect("Failed to compile");
    assert_eq!(dl.len(), 1);

    if let DrawCmd::Circle { center, radius, style } = &dl.items[0] {
        assert_eq!(*center, (250.0, 250.0));
        assert_eq!(*radius, 100.0);
        assert_eq!(style.fill, Color::Rgba(0, 255, 204, 255));
        assert_eq!(style.stroke, Color::Rgba(255, 255, 255, 255));
        assert_eq!(style.width, 2.5);
        assert!((style.opacity - 0.85).abs() < 1e-4);
    } else {
        panic!("Expected DrawCmd::Circle");
    }

    let svg = dl.to_svg();
    assert!(svg.contains("<svg width=\"500\" height=\"500\""));
    assert!(svg.contains("<circle cx=\"250.00\" cy=\"250.00\" r=\"100.00\""));
}

#[test]
fn test_math_and_operators() {
    let src = r#"
PVG 0.1
canvas 400 400

set a = 10 + 5 * 2
set b = (2 ^ 3) + sqrt(16)
set c = min(a, b) + max(1, 2)
set deg_rad = 180deg

line
  from [0, 0]
  to [a + b, c]
  stroke #ff0055
  width 1
"#;

    let dl = compile(src).expect("Failed to compile math");
    assert_eq!(dl.len(), 1);
    if let DrawCmd::Line { from, to, .. } = &dl.items[0] {
        assert_eq!(*from, (0.0, 0.0));
        assert_eq!(*to, (32.0, 14.0)); // a = 20, b = 12, c = 12 + 2 = 14
    } else {
        panic!("Expected Line primitive");
    }
}

#[test]
fn test_procedural_for_loop() {
    let src = r#"
PVG 0.1
canvas 600 600

for i from 0 to 4
  circle
    center [100 + i * 50, 200]
    radius 10
    fill #00ffcc
"#;

    let dl = compile(src).expect("Failed to compile for loop");
    assert_eq!(dl.len(), 5);
}

#[test]
fn test_functions_and_scope() {
    let src = r#"
PVG 0.1
canvas 600 600

def make_dot(cx, cy, r, col)
  circle
    center [cx, cy]
    radius r
    fill col

make_dot(100, 100, 15, #ff3355)
make_dot(200, 200, 25, #00ffcc)
"#;

    let dl = compile(src).expect("Failed to compile functions");
    assert_eq!(dl.len(), 2);
}

#[test]
fn test_groups_and_affine_transforms() {
    let src = r#"
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
"#;

    let dl = compile(src).expect("Failed to compile group");
    assert_eq!(dl.len(), 1);
    if let DrawCmd::Circle { center, radius, .. } = &dl.items[0] {
        // [10, 0] scaled by 2 -> [20, 0], rotated 90deg -> [0, 20], translated by [100, 100] -> [100, 120]
        assert!((center.0 - 100.0).abs() < 1e-4);
        assert!((center.1 - 120.0).abs() < 1e-4);
        assert_eq!(*radius, 5.0);
    }
}

#[test]
fn test_ast_caching_animation() {
    let src = r#"
PVG 0.1
canvas 400 400

set pulse = 20 + 10 * sin(time * 3.0)

circle
  center [200, 200]
  radius pulse
  fill #00ffcc
"#;

    let doc = parse(src).expect("Parse error");
    assert!(doc.is_animated());

    // Frame 0: t = 0.0 -> sin(0) = 0 -> radius = 20
    let eval0 = Evaluator::new_with_time(0.0);
    let dl0 = eval0.evaluate_document(&doc).unwrap();
    if let DrawCmd::Circle { radius, .. } = &dl0.items[0] {
        assert!((radius - 20.0).abs() < 1e-4);
    }

    // Frame at t = PI / 6.0 (~0.5236s) -> sin(3 * PI / 6) = sin(PI/2) = 1.0 -> radius = 30
    let t_peak = std::f64::consts::PI / 6.0;
    let eval_peak = Evaluator::new_with_time(t_peak);
    let dl_peak = eval_peak.evaluate_document(&doc).unwrap();
    if let DrawCmd::Circle { radius, .. } = &dl_peak.items[0] {
        assert!((radius - 30.0).abs() < 1e-4);
    }
}

#[test]
fn test_paths_and_beziers() {
    let src = r#"
PVG 0.1
canvas 600 600

path
  fill #ff3355
  stroke #ffffff
  width 2.0
  start [50, 50]
  line [100, 50]
  quad [150, 100] [200, 50]
  curve [250, 20] [300, 80] [350, 50]
  arc [400, 50] 30 0deg 180deg
  close
"#;

    let dl = compile(src).expect("Failed to compile path");
    assert_eq!(dl.len(), 1);
    if let DrawCmd::Path { commands, .. } = &dl.items[0] {
        assert_eq!(commands.len(), 6);
        assert!(matches!(commands[0], DrawPathCommand::Start(_)));
        assert!(matches!(commands[1], DrawPathCommand::Line(_)));
        assert!(matches!(commands[2], DrawPathCommand::Quad { .. }));
        assert!(matches!(commands[3], DrawPathCommand::Curve { .. }));
        assert!(matches!(commands[4], DrawPathCommand::Arc { .. }));
        assert!(matches!(commands[5], DrawPathCommand::Close));
    }
}

#[test]
fn test_text_primitive() {
    let src = r#"
PVG 0.1
canvas 600 400

text
  pos [100, 150]
  content "TELEMETRY: " + 99 + "%"
  size 24
  font "mono"
  align "center"
  fill #00ffcc
"#;

    let dl = compile(src).expect("Failed to compile text");
    assert_eq!(dl.len(), 1);
    if let DrawCmd::Text { content, font_family, align, size, .. } = &dl.items[0] {
        assert_eq!(content, "TELEMETRY: 99%");
        assert_eq!(font_family, "mono");
        assert_eq!(*align, TextAlign::Center);
        assert_eq!(*size, 24.0);
    }
}

#[test]
fn test_error_diagnostics() {
    // Missing canvas
    let err = parse("PVG 0.1\ncircle\n  radius 10\n").unwrap_err();
    assert_eq!(err.kind, PvgErrorKind::Parse);

    // Tab indentation forbidden
    let err_tab = parse("PVG 0.1\ncanvas 100 100\n\tcircle\n").unwrap_err();
    assert_eq!(err_tab.kind, PvgErrorKind::Lex);
    assert!(err_tab.message.contains("Tabs are forbidden"));

    // Infinite loop limit
    let err_loop = compile("PVG 0.1\ncanvas 100 100\nwhile true\n  circle\n    center [0,0]\n    radius 1\n").unwrap_err();
    assert_eq!(err_loop.kind, PvgErrorKind::SafetyLimit);
}

#[test]
fn test_deterministic_random_seed() {
    let src1 = "PVG 0.1\ncanvas 100 100\nseed 12345\nset r = random(10, 50)\ncircle\n  center [r, r]\n  radius 5\n";
    let src2 = "PVG 0.1\ncanvas 100 100\nseed 12345\nset r = random(10, 50)\ncircle\n  center [r, r]\n  radius 5\n";

    let dl1 = compile(src1).unwrap();
    let dl2 = compile(src2).unwrap();

    assert_eq!(dl1.items, dl2.items);
}