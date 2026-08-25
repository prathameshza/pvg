use crate::ast::Color;
use crate::draw_list::{DrawCmd, DrawList, DrawPathCommand, DrawStyle, TextAlign};
use std::f64::consts::PI;

/// Escapes XML special characters (`&`, `<`, `>`, `"`, `'`).
pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Formats SVG fill, stroke, stroke-width, and opacity attributes.
pub fn format_svg_attributes(style: &DrawStyle) -> String {
    let mut attrs = Vec::with_capacity(4);
    attrs.push(format!("fill=\"{}\"", style.fill.to_svg_string()));

    if style.stroke != Color::None && style.width > 0.0 {
        attrs.push(format!("stroke=\"{}\"", style.stroke.to_svg_string()));
        attrs.push(format!("stroke-width=\"{:.2}\"", style.width));
    } else {
        attrs.push("stroke=\"none\"".to_string());
    }

    if (style.opacity - 1.0).abs() > 0.001 {
        attrs.push(format!("opacity=\"{:.3}\"", style.opacity));
    }

    attrs.join(" ")
}

/// Serializes an array of `DrawCmd` into SVG element tags with indentation.
pub fn emit_draw_commands(items: &[DrawCmd], indent: &str) -> String {
    let mut out = String::new();

    for cmd in items {
        match cmd {
            DrawCmd::Circle { center, radius, style } => {
                out.push_str(&format!(
                    "{}<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" {} />\n",
                    indent, center.0, center.1, radius, format_svg_attributes(style)
                ));
            }
            DrawCmd::Ellipse { center, radius, style } => {
                out.push_str(&format!(
                    "{}<ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" {} />\n",
                    indent, center.0, center.1, radius.0, radius.1, format_svg_attributes(style)
                ));
            }
            DrawCmd::Rectangle { pos, size, corner_radius, style } => {
                if *corner_radius > 0.0 {
                    out.push_str(&format!(
                        "{}<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" {} />\n",
                        indent, pos.0, pos.1, size.0, size.1, corner_radius, corner_radius, format_svg_attributes(style)
                    ));
                } else {
                    out.push_str(&format!(
                        "{}<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" {} />\n",
                        indent, pos.0, pos.1, size.0, size.1, format_svg_attributes(style)
                    ));
                }
            }
            DrawCmd::Line { from, to, style } => {
                out.push_str(&format!(
                    "{}<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" {} />\n",
                    indent, from.0, from.1, to.0, to.1, format_svg_attributes(style)
                ));
            }
            DrawCmd::Polygon { points, style } => {
                if points.is_empty() {
                    continue;
                }
                let pts_str: Vec<String> = points
                    .iter()
                    .map(|p| format!("{:.2},{:.2}", p.0, p.1))
                    .collect();
                out.push_str(&format!(
                    "{}<polygon points=\"{}\" {} />\n",
                    indent,
                    pts_str.join(" "),
                    format_svg_attributes(style)
                ));
            }
            DrawCmd::Text { pos, content, size, font_family, align, style } => {
                let anchor = match align {
                    TextAlign::Left => "start",
                    TextAlign::Center => "middle",
                    TextAlign::Right => "end",
                };
                out.push_str(&format!(
                    "{}<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" font-family=\"{}\" text-anchor=\"{}\" dominant-baseline=\"hanging\" {}>{}</text>\n",
                    indent, pos.0, pos.1, size, font_family, anchor, format_svg_attributes(style), escape_xml(content)
                ));
            }
            DrawCmd::Path { commands, style } => {
                let mut d_tokens = Vec::new();

                for p_cmd in commands {
                    match p_cmd {
                        DrawPathCommand::Start(p) => {
                            d_tokens.push(format!("M {:.2} {:.2}", p.0, p.1));
                        }
                        DrawPathCommand::Line(p) => {
                            d_tokens.push(format!("L {:.2} {:.2}", p.0, p.1));
                        }
                        DrawPathCommand::Quad { cp, ep } => {
                            d_tokens.push(format!("Q {:.2} {:.2}, {:.2} {:.2}", cp.0, cp.1, ep.0, ep.1));
                        }
                        DrawPathCommand::Curve { c1, c2, ep } => {
                            d_tokens.push(format!(
                                "C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2}",
                                c1.0, c1.1, c2.0, c2.1, ep.0, ep.1
                            ));
                        }
                        DrawPathCommand::Arc { center, radius, start_angle, end_angle } => {
                            let r = *radius;
                            let delta = end_angle - start_angle;
                            let end_x = center.0 + r * end_angle.cos();
                            let end_y = center.1 + r * end_angle.sin();

                            if delta.abs() >= (2.0 * PI - 1e-4) {
                                let mid_angle = start_angle + delta / 2.0;
                                let mid_x = center.0 + r * mid_angle.cos();
                                let mid_y = center.1 + r * mid_angle.sin();
                                let sweep = if delta > 0.0 { 1 } else { 0 };
                                d_tokens.push(format!("A {:.2} {:.2} 0 0 {} {:.2} {:.2}", r, r, sweep, mid_x, mid_y));
                                d_tokens.push(format!("A {:.2} {:.2} 0 0 {} {:.2} {:.2}", r, r, sweep, end_x, end_y));
                            } else {
                                let large_arc = if delta.abs() > PI { 1 } else { 0 };
                                let sweep = if delta > 0.0 { 1 } else { 0 };
                                d_tokens.push(format!(
                                    "A {:.2} {:.2} 0 {} {} {:.2} {:.2}",
                                    r, r, large_arc, sweep, end_x, end_y
                                ));
                            }
                        }
                        DrawPathCommand::Close => {
                            d_tokens.push("Z".to_string());
                        }
                    }
                }

                out.push_str(&format!(
                    "{}<path d=\"{}\" {} />\n",
                    indent,
                    d_tokens.join(" "),
                    format_svg_attributes(style)
                ));
            }
        }
    }

    out
}

/// Serializes a `DrawList` into a standalone SVG document string.
pub fn emit_svg(draw_list: &DrawList) -> String {
    let mut out = String::with_capacity(1024 * 4);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<svg width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\">\n",
        draw_list.canvas_width, draw_list.canvas_height, draw_list.canvas_width, draw_list.canvas_height
    ));

    if let Some(ref bg) = draw_list.background {
        out.push_str(&format!(
            "  <rect width=\"100%\" height=\"100%\" fill=\"{}\" />\n",
            bg.to_svg_string()
        ));
    }

    out.push_str(&emit_draw_commands(&draw_list.items, "  "));
    out.push_str("</svg>\n");
    out
}

/// Serializes a sequence of animation frames into a standalone animated SVG using W3C SMIL.
pub fn emit_animated_svg(frames: &[DrawList], duration_sec: f64) -> String {
    if frames.is_empty() {
        return String::new();
    }
    let first = &frames[0];
    let frame_count = frames.len();
    let mut out = String::new();

    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<svg width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\">\n",
        first.canvas_width, first.canvas_height, first.canvas_width, first.canvas_height
    ));

    if let Some(ref bg) = first.background {
        out.push_str(&format!(
            "  <rect width=\"100%\" height=\"100%\" fill=\"{}\" />\n",
            bg.to_svg_string()
        ));
    }

    let n = frame_count as f64;
    for (i, frame) in frames.iter().enumerate() {
        let i_f = i as f64;

        let (values_str, keytimes_str) = if i == 0 {
            let t1 = 1.0 / n;
            ("visible;hidden".to_string(), format!("0; {:.4}", t1))
        } else if i == frame_count - 1 {
            let t0 = (n - 1.0) / n;
            ("hidden;visible".to_string(), format!("0; {:.4}", t0))
        } else {
            let t0 = i_f / n;
            let t1 = (i_f + 1.0) / n;
            ("hidden;visible;hidden".to_string(), format!("0; {:.4}; {:.4}", t0, t1))
        };

        out.push_str("  <g>\n");
        out.push_str(&format!(
            "    <animate attributeName=\"visibility\" values=\"{}\" keyTimes=\"{}\" dur=\"{:.2}s\" repeatCount=\"indefinite\" calcMode=\"discrete\" />\n",
            values_str, keytimes_str, duration_sec
        ));
        out.push_str(&emit_draw_commands(&frame.items, "    "));
        out.push_str("  </g>\n");
    }

    out.push_str("</svg>\n");
    out
}