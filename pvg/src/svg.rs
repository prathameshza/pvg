use crate::ast::Color;
use crate::draw_list::{
    BlendMode, DrawCmd, DrawList, DrawPathCommand, DrawStyle, Glow, LineCap, LineJoin, Paint,
    Shadow, StrokeAlign, TextAlign,
};
use std::collections::HashMap;
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

fn cap_to_svg(cap: &LineCap) -> &'static str {
    match cap {
        LineCap::Butt => "butt",
        LineCap::Round => "round",
        LineCap::Square => "square",
    }
}

fn join_to_svg(join: &LineJoin) -> &'static str {
    match join {
        LineJoin::Miter => "miter",
        LineJoin::Round => "round",
        LineJoin::Bevel => "bevel",
    }
}

fn blend_to_css(blend: &BlendMode) -> Option<&'static str> {
    match blend {
        BlendMode::Normal => None,
        BlendMode::Add => Some("plus-lighter"),
        BlendMode::Multiply => Some("multiply"),
        BlendMode::Screen => Some("screen"),
        BlendMode::Overlay => Some("overlay"),
    }
}

/// Formats SVG fill, stroke, stroke-width, and opacity attributes.
///
/// Includes PVG 0.2 stroke topology (`linecap`, `linejoin`, `miterlimit`,
/// `dasharray`) and blend mode. Gradient paints fall back to a `url(...)`
/// placeholder; use [`emit_svg`] for fully-resolved `<defs>` output.
pub fn format_svg_attributes(style: &DrawStyle) -> String {
    let mut attrs = Vec::with_capacity(8);
    attrs.push(format!("fill=\"{}\"", paint_fallback(&style.fill)));

    if !style.stroke.is_none() && style.width > 0.0 {
        attrs.push(format!("stroke=\"{}\"", paint_fallback(&style.stroke)));
        attrs.push(format!("stroke-width=\"{:.2}\"", style.width));
    } else {
        attrs.push("stroke=\"none\"".to_string());
    }

    attrs.push(format!("stroke-linecap=\"{}\"", cap_to_svg(&style.cap)));
    attrs.push(format!("stroke-linejoin=\"{}\"", join_to_svg(&style.join)));
    if style.join == LineJoin::Miter && (style.miter - 4.0).abs() > 1e-6 {
        attrs.push(format!("stroke-miterlimit=\"{:.2}\"", style.miter));
    }
    if !style.dash.is_empty() {
        let d: Vec<String> = style.dash.iter().map(|v| format!("{:.2}", v)).collect();
        attrs.push(format!("stroke-dasharray=\"{}\"", d.join(" ")));
    }
    let _ = StrokeAlign::Center;

    if (style.opacity - 1.0).abs() > 0.001 {
        attrs.push(format!("opacity=\"{:.3}\"", style.opacity));
    }
    if let Some(css) = blend_to_css(&style.blend) {
        attrs.push(format!("mix-blend-mode=\"{}\"", css));
    }

    attrs.join(" ")
}

fn paint_fallback(paint: &Paint) -> String {
    match paint {
        Paint::Color(c) => c.to_svg_string(),
        Paint::Pattern(name) => format!("url(#pvg-pat-{})", name),
        _ => "url(#pvg-grad)".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Context-aware emitter with <defs> for gradients / filters / clip paths.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct FilterKey {
    blur_bits: u64,
    shadow: Option<(u64, u64, u64, Color)>,
    glow: Option<(u64, Color)>,
}

impl FilterKey {
    fn of(style: &DrawStyle) -> Option<Self> {
        let has_blur = style.blur > 1e-9;
        if !has_blur && style.shadow.is_none() && style.glow.is_none() {
            return None;
        }
        Some(Self {
            blur_bits: style.blur.to_bits(),
            shadow: style.shadow.as_ref().map(|s| {
                (
                    s.offset.0.to_bits(),
                    s.offset.1.to_bits(),
                    s.radius.to_bits(),
                    s.color.clone(),
                )
            }),
            glow: style.glow.as_ref().map(|g| (g.radius.to_bits(), g.color.clone())),
        })
    }
}

struct SvgContext {
    grad_ids: HashMap<String, String>,
    grad_order: Vec<Paint>,
    filter_ids: HashMap<String, String>,
    filter_order: Vec<FilterKey>,
    clip_counter: usize,
    grad_counter: usize,
    filter_counter: usize,
}

impl SvgContext {
    fn new() -> Self {
        Self {
            grad_ids: HashMap::new(),
            grad_order: Vec::new(),
            filter_ids: HashMap::new(),
            filter_order: Vec::new(),
            clip_counter: 0,
            grad_counter: 0,
            filter_counter: 0,
        }
    }

    fn paint_key(p: &Paint) -> String {
        match p {
            Paint::Pattern(name) => format!("pat:{}", name),
            Paint::Color(c) => format!("c:{}", c.to_svg_string()),
            Paint::Linear { start, end, stops } => {
                let st: Vec<String> =
                    stops.iter().map(|s| format!("{}:{}", s.offset, s.color.to_svg_string())).collect();
                format!("l:{:.4},{:.4}:{:.4},{:.4}:{}", start.0, start.1, end.0, end.1, st.join(";"))
            }
            Paint::Radial { center, radius, focal, stops } => {
                let st: Vec<String> =
                    stops.iter().map(|s| format!("{}:{}", s.offset, s.color.to_svg_string())).collect();
                let f = focal.map_or("none".to_string(), |v| format!("{:.4},{:.4}", v.0, v.1));
                format!("r:{:.4},{:.4}:{:.4}:{}:{}", center.0, center.1, radius, f, st.join(";"))
            }
            Paint::Angular { center, start_angle, stops } => {
                let st: Vec<String> =
                    stops.iter().map(|s| format!("{}:{}", s.offset, s.color.to_svg_string())).collect();
                format!("a:{:.4},{:.4}:{:.4}:{}", center.0, center.1, start_angle, st.join(";"))
            }
        }
    }

    fn filter_key_string(k: &FilterKey) -> String {
        format!("{:?}", k)
    }

    fn paint_id(&mut self, p: &Paint) -> Option<String> {
        match p {
            // Solid colors and pattern refs need no gradient def.
            Paint::Color(_) | Paint::Pattern(_) => None,
            _ => {
                let key = Self::paint_key(p);
                if let Some(id) = self.grad_ids.get(&key) {
                    return Some(id.clone());
                }
                let id = format!("pvg-g{}", self.grad_counter);
                self.grad_counter += 1;
                self.grad_ids.insert(key, id.clone());
                self.grad_order.push(p.clone());
                Some(id)
            }
        }
    }

    fn filter_id(&mut self, style: &DrawStyle) -> Option<String> {
        let key = FilterKey::of(style)?;
        let ks = Self::filter_key_string(&key);
        if let Some(id) = self.filter_ids.get(&ks) {
            return Some(id.clone());
        }
        let id = format!("pvg-f{}", self.filter_counter);
        self.filter_counter += 1;
        self.filter_ids.insert(ks, id.clone());
        self.filter_order.push(key);
        Some(id)
    }

    fn paint_ref(&mut self, p: &Paint) -> String {
        match p {
            Paint::Color(c) => c.to_svg_string(),
            Paint::Pattern(name) => format!("url(#pvg-pat-{})", name),
            _ => format!("url(#{})", self.paint_id(p).unwrap()),
        }
    }

    fn walk_cmd(&mut self, cmd: &DrawCmd) {
        match cmd {
            DrawCmd::Circle { style, .. }
            | DrawCmd::Ellipse { style, .. }
            | DrawCmd::Rectangle { style, .. }
            | DrawCmd::Line { style, .. }
            | DrawCmd::Polygon { style, .. }
            | DrawCmd::Path { style, .. }
            | DrawCmd::Spline { style, .. }
            | DrawCmd::Text { style, .. } => {
                self.paint_id(&style.fill);
                self.paint_id(&style.stroke);
                self.filter_id(style);
            }
            DrawCmd::Sprite { style, .. } => {
                self.filter_id(style);
            }
            DrawCmd::Clip { mask, content } => {
                self.walk_cmd(mask);
                for c in content {
                    self.walk_cmd(c);
                }
            }
        }
    }
}

fn emit_gradient_def(id: &str, p: &Paint) -> String {
    let stops: Vec<String> = match p {
        Paint::Pattern(_) => Vec::new(),
        Paint::Linear { stops, .. } | Paint::Radial { stops, .. } | Paint::Angular { stops, .. } => {
            stops
                .iter()
                .map(|s| {
                    let c = &s.color;
                    match c {
                        Color::Rgba(r, g, b, 255) => format!(
                            "<stop offset=\"{:.3}\" stop-color=\"#{:02x}{:02x}{:02x}\" />",
                            s.offset, r, g, b
                        ),
                        Color::Rgba(r, g, b, a) => format!(
                            "<stop offset=\"{:.3}\" stop-color=\"#{:02x}{:02x}{:02x}\" stop-opacity=\"{:.3}\" />",
                            s.offset,
                            r,
                            g,
                            b,
                            *a as f64 / 255.0
                        ),
                        Color::None => {
                            format!("<stop offset=\"{:.3}\" stop-color=\"none\" />", s.offset)
                        }
                    }
                })
                .collect()
        }
        Paint::Color(_) => Vec::new(),
    };
    let stops_str = stops.join("");
    match p {
        Paint::Linear { start, end, .. } => format!(
            "<linearGradient id=\"{}\" gradientUnits=\"userSpaceOnUse\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\">{}</linearGradient>",
            id, start.0, start.1, end.0, end.1, stops_str
        ),
        Paint::Radial { center, radius, focal, .. } => {
            let (fx, fy) = focal.unwrap_or(*center);
            format!(
                "<radialGradient id=\"{}\" gradientUnits=\"userSpaceOnUse\" cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fx=\"{:.2}\" fy=\"{:.2}\">{}</radialGradient>",
                id, center.0, center.1, radius, fx, fy, stops_str
            )
        }
        // SVG has no native conic gradient; approximate angular sweeps with
        // a linear gradient fallback carrying the same stops so output stays total.
        Paint::Angular { center, .. } => format!(
            "<linearGradient id=\"{}\" gradientUnits=\"userSpaceOnUse\" x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\">{}</linearGradient>",
            id,
            center.0 - 100.0,
            center.1,
            center.0 + 100.0,
            center.1,
            stops_str
        ),
        Paint::Color(_) | Paint::Pattern(_) => String::new(),
    }
}

fn emit_filter_def(id: &str, k: &FilterKey) -> String {
    let mut parts = Vec::new();
    if let Some((dxb, dyb, rb, c)) = &k.shadow {
        let dx = f64::from_bits(*dxb);
        let dy = f64::from_bits(*dyb);
        let r = f64::from_bits(*rb);
        parts.push(format!(
            "<feDropShadow dx=\"{:.2}\" dy=\"{:.2}\" stdDeviation=\"{:.2}\" flood-color=\"{}\" />",
            dx,
            dy,
            r / 2.0,
            color_no_alpha(c)
        ));
    }
    if let Some((rb, c)) = &k.glow {
        let r = f64::from_bits(*rb);
        parts.push(format!(
            "<feDropShadow dx=\"0\" dy=\"0\" stdDeviation=\"{:.2}\" flood-color=\"{}\" />",
            r / 2.0,
            color_no_alpha(c)
        ));
    }
    let blur = f64::from_bits(k.blur_bits);
    if blur > 1e-9 {
        parts.push(format!("<feGaussianBlur stdDeviation=\"{:.2}\" />", blur / 2.0));
    }
    format!(
        "<filter id=\"{}\" x=\"-60%\" y=\"-60%\" width=\"220%\" height=\"220%\">{}</filter>",
        id,
        parts.join("")
    )
}

fn color_no_alpha(c: &Color) -> String {
    match c {
        Color::Rgba(r, g, b, _) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        Color::None => "none".to_string(),
    }
}

fn style_attrs_ctx(style: &DrawStyle, ctx: &mut SvgContext) -> String {
    let mut attrs = Vec::with_capacity(10);
    attrs.push(format!("fill=\"{}\"", ctx.paint_ref(&style.fill)));

    if !style.stroke.is_none() && style.width > 0.0 {
        attrs.push(format!("stroke=\"{}\"", ctx.paint_ref(&style.stroke)));
        attrs.push(format!("stroke-width=\"{:.2}\"", style.width));
    } else {
        attrs.push("stroke=\"none\"".to_string());
    }

    // Emit non-default stroke topology only, keeping 0.1 output byte-stable.
    if style.cap != LineCap::Butt {
        attrs.push(format!("stroke-linecap=\"{}\"", cap_to_svg(&style.cap)));
    }
    if style.join != LineJoin::Miter {
        attrs.push(format!("stroke-linejoin=\"{}\"", join_to_svg(&style.join)));
    }
    if style.join == LineJoin::Miter && (style.miter - 4.0).abs() > 1e-6 {
        attrs.push(format!("stroke-miterlimit=\"{:.2}\"", style.miter));
    }
    if !style.dash.is_empty() {
        let d: Vec<String> = style.dash.iter().map(|v| format!("{:.2}", v)).collect();
        attrs.push(format!("stroke-dasharray=\"{}\"", d.join(" ")));
    }

    if (style.opacity - 1.0).abs() > 0.001 {
        attrs.push(format!("opacity=\"{:.3}\"", style.opacity));
    }
    if let Some(css) = blend_to_css(&style.blend) {
        attrs.push(format!("mix-blend-mode=\"{}\"", css));
    }
    if let Some(fid) = ctx.filter_id(style) {
        attrs.push(format!("filter=\"url(#{})\"", fid));
    }

    attrs.join(" ")
}

/// Smooth path data for a spline (Catmull-Rom -> cubic Bezier).
fn spline_path_data(points: &[(f64, f64)]) -> String {
    if points.is_empty() {
        return String::new();
    }
    if points.len() == 1 {
        return format!("M {:.2} {:.2}", points[0].0, points[0].1);
    }
    let mut d = format!("M {:.2} {:.2} ", points[0].0, points[0].1);
    for (c1, c2, ep) in crate::eval::spline_to_bezier(points) {
        d.push_str(&format!(
            "C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2} ",
            c1.0, c1.1, c2.0, c2.1, ep.0, ep.1
        ));
    }
    d.trim_end().to_string()
}

fn path_data(commands: &[DrawPathCommand]) -> String {
    let mut d_tokens = Vec::new();
    for p_cmd in commands {
        match p_cmd {
            DrawPathCommand::Start(p) => d_tokens.push(format!("M {:.2} {:.2}", p.0, p.1)),
            DrawPathCommand::Line(p) => d_tokens.push(format!("L {:.2} {:.2}", p.0, p.1)),
            DrawPathCommand::Quad { cp, ep } => {
                d_tokens.push(format!("Q {:.2} {:.2}, {:.2} {:.2}", cp.0, cp.1, ep.0, ep.1))
            }
            DrawPathCommand::Curve { c1, c2, ep } => d_tokens.push(format!(
                "C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2}",
                c1.0, c1.1, c2.0, c2.1, ep.0, ep.1
            )),
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
                    d_tokens.push(format!("A {:.2} {:.2} 0 {} {} {:.2} {:.2}", r, r, large_arc, sweep, end_x, end_y));
                }
            }
            DrawPathCommand::Close => d_tokens.push("Z".to_string()),
        }
    }
    d_tokens.join(" ")
}

/// Clip-mask geometry without paint/filter attributes (clipPath uses raw shape).
fn emit_clip_mask_shape(cmd: &DrawCmd) -> String {
    match cmd {
        DrawCmd::Circle { center, radius, .. } => {
            format!("<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" />", center.0, center.1, radius)
        }
        DrawCmd::Ellipse { center, radius, .. } => format!(
            "<ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" />",
            center.0, center.1, radius.0, radius.1
        ),
        DrawCmd::Rectangle { pos, size, corner_radius, .. } => {
            if *corner_radius > 0.0 {
                format!(
                    "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{:.2}\" />",
                    pos.0, pos.1, size.0, size.1, corner_radius
                )
            } else {
                format!(
                    "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" />",
                    pos.0, pos.1, size.0, size.1
                )
            }
        }
        DrawCmd::Polygon { points, .. } => {
            let pts: Vec<String> = points.iter().map(|p| format!("{:.2},{:.2}", p.0, p.1)).collect();
            format!("<polygon points=\"{}\" />", pts.join(" "))
        }
        DrawCmd::Path { commands, .. } => {
            format!("<path d=\"{}\" />", path_data(commands))
        }
        DrawCmd::Line { from, to, .. } => format!(
            "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" />",
            from.0, from.1, to.0, to.1
        ),
        DrawCmd::Text { pos, .. } => {
            format!("<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"1\" />", pos.0, pos.1)
        }
        DrawCmd::Sprite { pos, rows, scale, .. } => {
            let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as f64 * scale;
            let h = rows.len() as f64 * scale;
            format!(
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" />",
                pos.0, pos.1, w, h
            )
        }
        DrawCmd::Spline { points, .. } => {
            if points.is_empty() {
                return "<path d=\"\" />".to_string();
            }
            format!("<path d=\"{}\" />", spline_path_data(points))
        }
        DrawCmd::Clip { mask, .. } => emit_clip_mask_shape(mask),
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_cmd(cmd: &DrawCmd, ctx: &mut SvgContext, indent: &str, defs_out: &mut String) -> String {
    let mut out = String::new();
    match cmd {
        DrawCmd::Circle { center, radius, style } => {
            out.push_str(&format!(
                "{}<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" {} />\n",
                indent,
                center.0,
                center.1,
                radius,
                style_attrs_ctx(style, ctx)
            ));
        }
        DrawCmd::Ellipse { center, radius, style } => {
            out.push_str(&format!(
                "{}<ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" {} />\n",
                indent,
                center.0,
                center.1,
                radius.0,
                radius.1,
                style_attrs_ctx(style, ctx)
            ));
        }
        DrawCmd::Rectangle { pos, size, corner_radius, style } => {
            if *corner_radius > 0.0 {
                out.push_str(&format!(
                    "{}<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" {} />\n",
                    indent,
                    pos.0,
                    pos.1,
                    size.0,
                    size.1,
                    corner_radius,
                    corner_radius,
                    style_attrs_ctx(style, ctx)
                ));
            } else {
                out.push_str(&format!(
                    "{}<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" {} />\n",
                    indent,
                    pos.0,
                    pos.1,
                    size.0,
                    size.1,
                    style_attrs_ctx(style, ctx)
                ));
            }
        }
        DrawCmd::Line { from, to, style } => {
            out.push_str(&format!(
                "{}<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" {} />\n",
                indent,
                from.0,
                from.1,
                to.0,
                to.1,
                style_attrs_ctx(style, ctx)
            ));
        }
        DrawCmd::Polygon { points, style } => {
            if points.is_empty() {
                return out;
            }
            let pts_str: Vec<String> =
                points.iter().map(|p| format!("{:.2},{:.2}", p.0, p.1)).collect();
            out.push_str(&format!(
                "{}<polygon points=\"{}\" {} />\n",
                indent,
                pts_str.join(" "),
                style_attrs_ctx(style, ctx)
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
                indent,
                pos.0,
                pos.1,
                size,
                font_family,
                anchor,
                style_attrs_ctx(style, ctx),
                escape_xml(content)
            ));
        }
        DrawCmd::Path { commands, style } => {
            out.push_str(&format!(
                "{}<path d=\"{}\" {} />\n",
                indent,
                path_data(commands),
                style_attrs_ctx(style, ctx)
            ));
        }
        DrawCmd::Spline { points, style } => {
            if points.is_empty() {
                return out;
            }
            out.push_str(&format!(
                "{}<path d=\"{}\" fill=\"none\" {} />\n",
                indent,
                spline_path_data(points),
                style_attrs_ctx(style, ctx)
            ));
        }
        DrawCmd::Sprite { pos, palette, rows, scale, style } => {
            // Palette-indexed pixels as crisp rects. `.`/space = transparent,
            // `0`-`9` (+`a`-`z` for 10+) index into the palette.
            let opacity_attr = if (style.opacity - 1.0).abs() > 0.001 {
                format!(" opacity=\"{:.3}\"", style.opacity)
            } else {
                String::new()
            };
            let blend_attr = match blend_to_css(&style.blend) {
                Some(css) => format!(" mix-blend-mode=\"{}\"", css),
                None => String::new(),
            };
            out.push_str(&format!(
                "{}<g shape-rendering=\"crispEdges\"{}{}>\n",
                indent, opacity_attr, blend_attr
            ));
            let inner = format!("{}  ", indent);
            for (ry, row) in rows.iter().enumerate() {
                for (rx, ch) in row.chars().enumerate() {
                    if ch == '.' || ch == ' ' {
                        continue;
                    }
                    let idx = if ch.is_ascii_digit() {
                        (ch as u8 - b'0') as usize
                    } else if ('a'..='z').contains(&ch) {
                        (ch as u8 - b'a') as usize + 10
                    } else if ('A'..='Z').contains(&ch) {
                        (ch as u8 - b'A') as usize + 10
                    } else {
                        continue;
                    };
                    let color = match palette.get(idx) {
                        Some(c) if !c.is_transparent() && !c.is_none() => c,
                        _ => continue,
                    };
                    out.push_str(&format!(
                        "{}<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"{}\" />\n",
                        inner,
                        pos.0 + rx as f64 * scale,
                        pos.1 + ry as f64 * scale,
                        scale,
                        scale,
                        color.to_svg_string()
                    ));
                }
            }
            out.push_str(&format!("{}</g>\n", indent));
        }
        DrawCmd::Clip { mask, content } => {
            let cid = format!("pvg-clip{}", ctx.clip_counter);
            ctx.clip_counter += 1;
            defs_out.push_str(&format!(
                "<clipPath id=\"{}\">{}</clipPath>",
                cid,
                emit_clip_mask_shape(mask)
            ));
            out.push_str(&format!("{}<g clip-path=\"url(#{})\">\n", indent, cid));
            let inner = format!("{}  ", indent);
            for c in content {
                out.push_str(&emit_cmd(c, ctx, &inner, defs_out));
            }
            out.push_str(&format!("{}</g>\n", indent));
        }
    }
    out
}

/// Serializes an array of `DrawCmd` into SVG element tags with indentation.
pub fn emit_draw_commands(items: &[DrawCmd], indent: &str) -> String {
    let mut ctx = SvgContext::new();
    for cmd in items {
        ctx.walk_cmd(cmd);
    }
    let mut defs_out = String::new();
    let mut out = String::new();
    for cmd in items {
        out.push_str(&emit_cmd(cmd, &mut ctx, indent, &mut defs_out));
    }
    out
}

/// Serializes a `DrawList` into a standalone SVG document string.
pub fn emit_svg(draw_list: &DrawList) -> String {
    let mut ctx = SvgContext::new();
    for pat in &draw_list.patterns {
        for tile in &pat.tiles {
            ctx.walk_cmd(tile);
        }
    }
    for cmd in &draw_list.items {
        ctx.walk_cmd(cmd);
    }
    let mut out = String::with_capacity(1024 * 4);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    if draw_list.pixel_filter == crate::ast::PixelFilter::Nearest {
        out.push_str(&format!(
            "<svg width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" shape-rendering=\"crispEdges\" xmlns=\"http://www.w3.org/2000/svg\">\n",
            draw_list.canvas_width, draw_list.canvas_height, draw_list.canvas_width, draw_list.canvas_height
        ));
    } else {
        out.push_str(&format!(
            "<svg width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\">\n",
            draw_list.canvas_width, draw_list.canvas_height, draw_list.canvas_width, draw_list.canvas_height
        ));
    }

    if let Some(ref bg) = draw_list.background {
        out.push_str(&format!(
            "  <rect width=\"100%\" height=\"100%\" fill=\"{}\" />\n",
            bg.to_svg_string()
        ));
    }

    // <defs>: gradients, filters, then clip paths collected during body emit.
    let mut grad_defs = String::new();
    for paint in ctx.grad_order.clone() {
        let key = SvgContext::paint_key(&paint);
        let id = ctx.grad_ids.get(&key).unwrap().clone();
        grad_defs.push_str(&emit_gradient_def(&id, &paint));
    }
    let mut filter_defs = String::new();
    for fk in ctx.filter_order.clone() {
        let ks = SvgContext::filter_key_string(&fk);
        let id = ctx.filter_ids.get(&ks).unwrap().clone();
        filter_defs.push_str(&emit_filter_def(&id, &fk));
    }
    // Pattern tile defs (repeatable fills).
    let mut pattern_defs = String::new();
    let mut pattern_clip_defs = String::new();
    for pat in &draw_list.patterns {
        let mut tile_body = String::new();
        for tile in &pat.tiles {
            tile_body.push_str(&emit_cmd(tile, &mut ctx, "      ", &mut pattern_clip_defs));
        }
        pattern_defs.push_str(&format!(
            "<pattern id=\"pvg-pat-{}\" patternUnits=\"userSpaceOnUse\" width=\"{:.2}\" height=\"{:.2}\">{}</pattern>",
            pat.name, pat.width, pat.height, tile_body
        ));
    }
    // Body emit also collects clipPath defs.
    let mut clip_defs = String::new();
    let mut body = String::new();
    for cmd in &draw_list.items {
        body.push_str(&emit_cmd(cmd, &mut ctx, "  ", &mut clip_defs));
    }
    // Late-registered gradients/filters from clip content get appended too.
    // (Re-walk is cheap and keeps IDs stable for the common case.)
    if grad_defs.is_empty()
        && filter_defs.is_empty()
        && clip_defs.is_empty()
        && pattern_defs.is_empty()
        && pattern_clip_defs.is_empty()
    {
        out.push_str(&body);
    } else {
        // Re-collect any defs registered during body emit (clip content).
        let mut extra_grads = String::new();
        let seen_grads = grad_defs.clone();
        for paint in ctx.grad_order.clone() {
            let key = SvgContext::paint_key(&paint);
            let id = ctx.grad_ids.get(&key).unwrap().clone();
            let def = emit_gradient_def(&id, &paint);
            if !seen_grads.contains(&format!("id=\"{}\"", id)) {
                extra_grads.push_str(&def);
            }
        }
        let mut extra_filters = String::new();
        let seen_filters = filter_defs.clone();
        for fk in ctx.filter_order.clone() {
            let ks = SvgContext::filter_key_string(&fk);
            let id = ctx.filter_ids.get(&ks).unwrap().clone();
            let def = emit_filter_def(&id, &fk);
            if !seen_filters.contains(&format!("id=\"{}\"", id)) {
                extra_filters.push_str(&def);
            }
        }
        out.push_str(&format!(
            "  <defs>{}{}{}{}{}{}{}</defs>\n",
            grad_defs,
            extra_grads,
            filter_defs,
            extra_filters,
            pattern_defs,
            pattern_clip_defs,
            clip_defs,
        ));
        out.push_str(&body);
    }
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

// Silence dead-code warnings for FX descriptors only used via DrawStyle.
#[allow(dead_code)]
fn _fx_types(_s: Option<Shadow>, _g: Option<Glow>) {}
