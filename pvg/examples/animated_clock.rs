use pvg::compile_at_time;

fn main() {
    let source = r#"
PVG 0.1
canvas 400 400
  background #080a0f

set cx = 200
set cy = 200
set angle = time * (TAU / 60.0)

line
  from [cx, cy]
  to   [cx + 120 * sin(angle), cy - 120 * cos(angle)]
  stroke #ff3355
  width 3.0

circle
  center [cx, cy]
  radius 6
  fill #ffffff
"#;

    println!("Simulating animated clock at various timestamps:");
    for sec in [0.0, 15.0, 30.0, 45.0] {
        let dl = compile_at_time(source, sec).expect("Evaluation failed");
        println!("  • Time = {:>4.1}s -> {} shapes rendered", sec, dl.len());
    }
}