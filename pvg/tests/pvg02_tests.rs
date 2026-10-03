use pvg::*;

// PVG 0.2 keeps backwards compatibility with 0.1 documents.
#[test]
fn test_version_headers() {
    let v1 = "PVG 0.1\ncanvas 100 100\ncircle\n  center [50, 50]\n  radius 10\n";
    let doc1 = parse(v1).expect("0.1 header must parse");
    assert_eq!(doc1.version, (0, 1));

    let v2 = "PVG 0.2\ncanvas 100 100\ncircle\n  center [50, 50]\n  radius 10\n";
    let doc2 = parse(v2).expect("0.2 header must parse");
    assert_eq!(doc2.version, (0, 2));

    assert_eq!(compile(v1).unwrap().len(), 1);
    assert_eq!(compile(v2).unwrap().len(), 1);
}

#[test]
fn test_stroke_topology() {
    let src = r#"
PVG 0.2
canvas 200 200

line
  from [10, 10]
  to [190, 10]
  stroke #ffffff
  width 4
  cap "round"
  join "bevel"
  miter 6
  dash [28, 8, 12, 8]
  align "inside"
"#;
    let dl = compile(src).expect("stroke topology must compile");
    assert_eq!(dl.len(), 1);
    if let DrawCmd::Line { style, .. } = &dl.items[0] {
        assert_eq!(style.cap, LineCap::Round);
        assert_eq!(style.join, LineJoin::Bevel);
        assert!((style.miter - 6.0).abs() < 1e-9);
        assert_eq!(style.dash, vec![28.0, 8.0, 12.0, 8.0]);
        assert_eq!(style.stroke_align, StrokeAlign::Inside);
    } else {
        panic!("expected line");
    }

    // Defaults stay 0.1-compatible.
    let plain = "PVG 0.1\ncanvas 50 50\nline\n  from [0, 0]\n  to [10, 10]\n";
    let dl0 = compile(plain).unwrap();
    if let DrawCmd::Line { style, .. } = &dl0.items[0] {
        assert_eq!(style.cap, LineCap::Butt);
        assert_eq!(style.join, LineJoin::Miter);
        assert!(style.dash.is_empty());
        assert_eq!(style.blend, BlendMode::Normal);
        assert_eq!(style.blur, 0.0);
    }
}

#[test]
fn test_linear_gradient() {
    let src = r#"
PVG 0.2
canvas 300 300

rectangle
  pos [0, 0]
  size [300, 300]
  fill linear [0, 0] [0, 300]
    stop 0.0 #2a2e3d
    stop 1.0 #12141c
"#;
    let dl = compile(src).expect("linear gradient must compile");
    assert_eq!(dl.len(), 1);
    if let DrawCmd::Rectangle { style, .. } = &dl.items[0] {
        match &style.fill {
            Paint::Linear { start, end, stops } => {
                assert_eq!(*start, (0.0, 0.0));
                assert_eq!(*end, (0.0, 300.0));
                assert_eq!(stops.len(), 2);
                assert!((stops[0].offset - 0.0).abs() < 1e-9);
                assert!((stops[1].offset - 1.0).abs() < 1e-9);
            }
            other => panic!("expected linear paint, got {:?}", other),
        }
    }
    let svg = dl.to_svg();
    assert!(svg.contains("<linearGradient"), "svg must carry gradient defs");
    assert!(svg.contains("stop-color"));
}

#[test]
fn test_radial_and_angular_gradients() {
    let src = r#"
PVG 0.2
canvas 200 200

circle
  center [100, 100]
  radius 90
  fill radial [100, 100] 90
    stop 0.0 #00ffff
    stop 0.6 #0033aa
    stop 1.0 #07090e

circle
  center [100, 100]
  radius 40
  fill angular [100, 100] 0deg
    stop 0.0 #00ffcc
    stop 0.75 #00ffcc
    stop 1.0 transparent
"#;
    let dl = compile(src).expect("radial/angular must compile");
    assert_eq!(dl.len(), 2);
    if let DrawCmd::Circle { style, .. } = &dl.items[0] {
        assert!(matches!(style.fill, Paint::Radial { .. }));
    }
    if let DrawCmd::Circle { style, .. } = &dl.items[1] {
        assert!(matches!(style.fill, Paint::Angular { .. }));
    }
}

#[test]
fn test_fx_blur_shadow_glow_blend() {
    let src = r#"
PVG 0.2
canvas 200 200

circle
  center [100, 100]
  radius 50
  fill #00aaff
  blur 12
  blend "add"
  opacity 0.5

rectangle
  pos [10, 10]
  size [80, 80]
  fill #ffffff
  shadow [4, 6] 8 #00000080
  glow 6 #00ffcc
  blend "screen"
"#;
    let dl = compile(src).expect("fx props must compile");
    assert_eq!(dl.len(), 2);
    if let DrawCmd::Circle { style, .. } = &dl.items[0] {
        assert!((style.blur - 12.0).abs() < 1e-9);
        assert_eq!(style.blend, BlendMode::Add);
    }
    if let DrawCmd::Rectangle { style, .. } = &dl.items[1] {
        let sh = style.shadow.as_ref().expect("shadow");
        assert_eq!(sh.offset, (4.0, 6.0));
        assert!((sh.radius - 8.0).abs() < 1e-9);
        let gl = style.glow.as_ref().expect("glow");
        assert!((gl.radius - 6.0).abs() < 1e-9);
        assert_eq!(style.blend, BlendMode::Screen);
    }
    let svg = dl.to_svg();
    assert!(svg.contains("mix-blend-mode"));
    assert!(svg.contains("<filter"));
}

#[test]
fn test_clip_block() {
    let src = r#"
PVG 0.2
canvas 300 120
  background #07090e

clip
  rectangle
    pos [100, 100]
    size [200, 20]
    radius 6
  for s from 0 to 3
    line
      from [80 + s * 20, 90]
      to [110 + s * 20, 130]
      stroke #00e676
      width 6
"#;
    let dl = compile(src).expect("clip must compile");
    assert_eq!(dl.len(), 1);
    if let DrawCmd::Clip { mask, content } = &dl.items[0] {
        assert!(matches!(**mask, DrawCmd::Rectangle { .. }));
        assert_eq!(content.len(), 4);
    } else {
        panic!("expected clip");
    }
    let svg = dl.to_svg();
    assert!(svg.contains("<clipPath"));
    assert!(svg.contains("clip-path"));
}

#[test]
fn test_group_blend_and_animated_gradient() {
    let src = r#"
PVG 0.2
canvas 400 400

group
  pos [200, 200]
  rot time * 1.5
  blend "add"
  circle
    center [0, 0]
    radius 30 + 10 * sin(time * 2.0)
    fill radial [0, 0] 30
      stop 0.0 #ffffff
      stop 1.0 #0066ff
"#;
    let doc = parse(src).expect("group blend must parse");
    assert!(doc.is_animated());
    let dl0 = Evaluator::new_with_time(0.0).evaluate_document(&doc).unwrap();
    assert_eq!(dl0.len(), 1);
    if let DrawCmd::Circle { style, radius, .. } = &dl0.items[0] {
        assert_eq!(style.blend, BlendMode::Add);
        assert!((radius - 30.0).abs() < 1e-9);
    }
}
