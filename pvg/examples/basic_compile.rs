use pvg::compile;

fn main() {
    let source = r#"
PVG 0.1
canvas 600 400
  background #0e0f13

# Draw a centered glowing ring
circle
  center [300, 200]
  radius 80
  fill none
  stroke #00ffcc
  width 4.0

# Add a label
text
  pos [300, 310]
  content "PVG CORE RUNTIME"
  size 18
  font "mono"
  align "center"
  fill #ffffff
"#;

    let draw_list = compile(source).expect("Compilation failed");
    println!("✓ Compiled canvas: {}x{}", draw_list.canvas_width, draw_list.canvas_height);
    println!("✓ Emitted {} primitives", draw_list.len());

    let svg = draw_list.to_svg();
    println!("\nGenerated W3C SVG:\n{}", svg);
}