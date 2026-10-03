use pvg::*;

// Functional color form: rgb(r, g, b) / rgba(r, g, b, a) (Section 2.6).
#[test]
fn verify_rgb_builtin() {
    let dl = compile(
        r#"
PVG 0.2
canvas 100 100
circle
  center [50, 50]
  radius 10
  fill rgb(255, 128, 0)
"#,
    )
    .unwrap();
    assert_eq!(dl.len(), 1);
    if let DrawCmd::Circle { style, .. } = &dl.items[0] {
        assert_eq!(style.fill, Paint::Color(Color::Rgba(255, 128, 0, 255)));
    } else {
        panic!("expected circle");
    }
}

#[test]
fn verify_rgba_alpha_mapping() {
    // Alpha in [0.0, 1.0] maps to [0, 255]: 0.5 -> 128, 0.0 -> 0, 1.0 -> 255.
    for (alpha, byte) in [("0.5", 128), ("0.0", 0), ("1.0", 255)] {
        let src = format!(
            "PVG 0.2\ncanvas 10 10\ncircle\n  center [5, 5]\n  radius 2\n  fill rgba(255, 0, 0, {})",
            alpha
        );
        let dl = compile(&src).unwrap();
        if let DrawCmd::Circle { style, .. } = &dl.items[0] {
            assert_eq!(
                style.fill,
                Paint::Color(Color::Rgba(255, 0, 0, byte)),
                "alpha {}",
                alpha
            );
        } else {
            panic!("expected circle");
        }
    }
}

#[test]
fn verify_rgb_clamping_and_rounding() {
    // Out-of-range channels clamp; fractions round to nearest.
    let dl = compile(
        "PVG 0.2\ncanvas 10 10\ncircle\n  center [5, 5]\n  radius 2\n  fill rgb(300, -20, 12.7)",
    )
    .unwrap();
    if let DrawCmd::Circle { style, .. } = &dl.items[0] {
        assert_eq!(style.fill, Paint::Color(Color::Rgba(255, 0, 13, 255)));
    } else {
        panic!("expected circle");
    }
    // Alpha clamps to [0, 1] before byte mapping.
    let dl = compile(
        "PVG 0.2\ncanvas 10 10\ncircle\n  center [5, 5]\n  radius 2\n  fill rgba(0, 0, 0, 2.5)",
    )
    .unwrap();
    if let DrawCmd::Circle { style, .. } = &dl.items[0] {
        assert_eq!(style.fill, Paint::Color(Color::Rgba(0, 0, 0, 255)));
    } else {
        panic!("expected circle");
    }
}

#[test]
fn verify_rgb_arity_and_type_errors() {
    assert!(compile("PVG 0.2\ncanvas 10 10\nset c = rgb(1, 2)\ncircle\n  center [0, 0]\n  radius c")
        .unwrap_err()
        .to_string()
        .contains("rgb(r, g, b) needs 3 arguments"));
    assert!(compile("PVG 0.2\ncanvas 10 10\nset c = rgba(1, 2, 3)\ncircle\n  center [0, 0]\n  radius c")
        .unwrap_err()
        .to_string()
        .contains("rgba(r, g, b, a) needs 4 arguments"));
    assert!(compile("PVG 0.2\ncanvas 10 10\nset c = rgb(\"a\", 0, 0)\ncircle\n  center [0, 0]\n  radius 1")
        .is_err());
}

// Call-stack depth guard: 64 frames (Section 15).
#[test]
fn verify_call_stack_limit() {
    let recurse = |n: i64| {
        format!(
            "PVG 0.2\ncanvas 10 10\ndef boom(k)\n  if k <= 0\n    return 0\n  else\n    return boom(k - 1)\nboom({})",
            n
        )
    };
    // 60 nested calls fit inside the 64-frame budget.
    assert!(compile(&recurse(60)).is_ok());
    // 200 nested calls trip the guard instead of overflowing the host stack.
    let err = compile(&recurse(200)).unwrap_err().to_string();
    assert!(err.contains("call stack"), "unexpected error: {}", err);
    // Self-recursion without a base case trips the same guard.
    let err = compile("PVG 0.2\ncanvas 10 10\ndef f()\n  f()\nf()")
        .unwrap_err()
        .to_string();
    assert!(err.contains("call stack"), "unexpected error: {}", err);
}

// Scene primitive budget: 50,000 draw commands (Section 15).
#[test]
fn verify_scene_primitive_limit() {
    // Exactly at the budget: 50,000 circles evaluate fine.
    let at_budget = "PVG 0.2\ncanvas 10 10\nfor i from 0 to 49999\n  circle\n    center [0, 0]\n    radius 1";
    let dl = compile(at_budget).unwrap();
    assert_eq!(dl.len(), 50_000);
    // One more trips the guard.
    let over = "PVG 0.2\ncanvas 10 10\nfor i from 0 to 50000\n  circle\n    center [0, 0]\n    radius 1";
    let err = compile(over).unwrap_err().to_string();
    assert!(err.contains("primitive"), "unexpected error: {}", err);
}
