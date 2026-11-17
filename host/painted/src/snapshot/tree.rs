//! The snapshot tree (LLP 1126.003 D2): each box at its frame in its
//! parent's, its paint as `RemoteViews` can express it, each text node as
//! the kernel's lines, and the leaves the platform draws.
//!
//! Colours are read from the kernel's unresolved `ColorValue`: a role is
//! carried as the Material role the reader's palette gives it, so the
//! launcher resolves it (`{"m": index, "a": percent}`); anything else as its
//! light and dark ARGB (`{"l", "d"}`).

use crate::paint::text_spec;
use crate::presenter::palette;
use crate::text::{Run, Shared};
use exact_kernel::style::roles;
use exact_kernel::{
    BorderStyle, ColorValue, Kernel, NodeRef, NodeType, ObjectFit, Overflow, PropId, PropValue,
    StyleId, StyleMask, StyleProps, ViewId,
};
use serde_json::{json, Map, Value};

pub(super) fn walk(kernel: &Kernel, roots: &[ViewId], text: &Shared) -> Value {
    let nodes: Vec<Value> = roots
        .iter()
        .filter_map(|&id| node(kernel, text, id, (0.0, 0.0)))
        .collect();
    match <[Value; 1]>::try_from(nodes) {
        Ok([root]) => root,
        Err(nodes) => json!({ "x": 0, "y": 0, "children": nodes }),
    }
}

fn node(kernel: &Kernel, text: &Shared, id: ViewId, origin: (f32, f32)) -> Option<Value> {
    let n = kernel.node(id)?;
    if n.node_type == NodeType::Head {
        return None;
    }
    let f = n.frame;
    let s = n.computed_style(StyleMask::EMPTY);
    let mut out = Map::new();
    out.insert("x".into(), num(f.x - origin.0));
    out.insert("y".into(), num(f.y - origin.1));
    out.insert("w".into(), num(f.width));
    out.insert("h".into(), num(f.height));
    paint(kernel, &n, &s, &mut out);
    access(&n, &mut out);
    if let Some(href) = n.props.str(PropId::Href).filter(|h| !h.is_empty()) {
        out.insert("href".into(), href.into());
    }
    match n.node_type {
        NodeType::Text => {
            if n.props.str(PropId::SemanticTag) == Some("time") {
                if let Some(at) = n.props.get(PropId::Datetime).and_then(PropValue::as_float) {
                    out.insert(
                        "time".into(),
                        json!({ "datetime": at, "format": n.props.str(PropId::TimeFormat).unwrap_or("") }),
                    );
                }
            }
            out.insert("lines".into(), Value::Array(lines(kernel, text, &n, &s)));
        }
        NodeType::Image => {
            let src = n
                .props
                .str(PropId::ImageSource)
                .or_else(|| n.props.str(PropId::Src))
                .unwrap_or("");
            let fit = match s.object_fit {
                ObjectFit::Fill => "fill",
                ObjectFit::Contain => "contain",
                ObjectFit::Cover => "cover",
                ObjectFit::None => "none",
                ObjectFit::ScaleDown => "scale-down",
            };
            let color = n.computed_row(StyleId::TextColor, |s| s.text_color);
            out.insert(
                "image".into(),
                json!({ "src": src, "fit": fit, "color": color_json(color) }),
            );
        }
        NodeType::Control if n.props.str(PropId::Type) == Some("progress") => {
            let p = exact_kernel::Progress::of(n.props);
            let accent = s.accent_color.unwrap_or(ColorValue::Role(
                roles::role("AccentColor").unwrap_or_default(),
            ));
            out.insert(
                "progress".into(),
                json!({ "value": p.value, "max": p.max, "color": color_json(accent) }),
            );
        }
        _ => {
            let children: Vec<Value> = n
                .children()
                .into_iter()
                .filter_map(|c| node(kernel, text, c, (f.x, f.y)))
                .collect();
            if !children.is_empty() {
                out.insert("children".into(), Value::Array(children));
            }
        }
    }
    Some(Value::Object(out))
}

/// The box's own paint: its background, its corner radius, a uniform solid
/// border, its opacity and whether it clips. What `RemoteViews` cannot draw
/// is named under `"picture"`, for a picture leaf (D2).
fn paint(kernel: &Kernel, n: &NodeRef<'_>, s: &StyleProps, out: &mut Map<String, Value>) {
    if let Some(bg) = s.background_color.filter(|c| visible(*c)) {
        out.insert("background".into(), color_json(bg));
    }
    let mut picture = Vec::new();
    if !s.background_image.layers().is_empty() {
        picture.push("background-image");
    }
    let env = kernel.env_of(n.id);
    let radii = crate::paint::border::resolve_radii(
        [
            s.border_radius_top_left.clone(),
            s.border_radius_top_right.clone(),
            s.border_radius_bottom_right.clone(),
            s.border_radius_bottom_left.clone(),
        ]
        .map(|d| d.resolve(&env)),
        &env,
        n.frame.width,
        n.frame.height,
    );
    let r = radii[0].0;
    if radii.iter().all(|&(h, v)| h == r && v == r) {
        if r > 0.0 {
            out.insert("radius".into(), num(r));
        }
    } else {
        picture.push("border-radius");
    }
    let widths = s.border_widths();
    if widths.iter().any(|w| *w > 0.0) {
        let current = n.computed_row(StyleId::TextColor, |s| s.text_color);
        let colors = s.border_colors(current);
        let styles = [
            s.border_style_top,
            s.border_style_right,
            s.border_style_bottom,
            s.border_style_left,
        ];
        let uniform = widths.iter().all(|w| *w == widths[0])
            && colors.iter().all(|c| *c == colors[0])
            && styles.iter().all(|st| *st == BorderStyle::Solid);
        if uniform {
            out.insert(
                "border".into(),
                json!({ "width": widths[0], "color": color_json(colors[0]) }),
            );
        } else {
            picture.push("border");
        }
    }
    if s.opacity < 1.0 {
        out.insert("opacity".into(), num(s.opacity));
    }
    if matches!(s.overflow_x, Overflow::Hidden | Overflow::Clip) {
        out.insert("clip".into(), true.into());
    }
    if let Some((left, top, right, bottom)) = kernel.resolved_padding(n.key) {
        if [left, top, right, bottom].iter().any(|p| *p != 0.0) {
            out.insert("padding".into(), json!([top, right, bottom, left]));
        }
    }
    if !picture.is_empty() {
        out.insert("picture".into(), json!(picture));
    }
}

/// What TalkBack reads (LLP 1126 D5): the node's label, role and heading
/// level, and whether it is hidden.
fn access(n: &NodeRef<'_>, out: &mut Map<String, Value>) {
    for (prop, key) in [
        (PropId::AccessibilityLabel, "label"),
        (PropId::AccessibilityRole, "role"),
    ] {
        if let Some(v) = n.props.str(prop).filter(|v| !v.is_empty()) {
            out.insert(key.into(), v.into());
        }
    }
    let level = n
        .props
        .str(PropId::AccessibilityHeadingLevel)
        .and_then(|l| l.parse::<u8>().ok());
    if let Some(level) = level {
        out.insert("heading".into(), level.into());
    }
    if n.props.str(PropId::AccessibilityElementsHidden) == Some("true") {
        out.insert("hidden".into(), true.into());
    }
}

/// The text node's lines as the kernel broke them, in the node's box: each
/// line's text, its box and baseline, and its runs' styles, UTF-16 ranges of
/// the line's text and their extents in the node's box (a line of more than
/// one colour is drawn a view per run, D2).
fn lines(kernel: &Kernel, text: &Shared, n: &NodeRef<'_>, s: &StyleProps) -> Vec<Value> {
    let widths = s.border_widths();
    let (pl, pt, pr, _) = kernel.resolved_padding(n.key).unwrap_or_default();
    let mut left = widths[3] + pl;
    let top = widths[0] + pt + kernel.text_line_offset(n);
    let mut width = (n.frame.width - left - widths[1] - pr).max(0.0);
    if let Some(item) = kernel.text_item_box(n) {
        left += item.x;
        width = item.width;
    }
    let computed = n.computed_style(StyleMask::INHERITED);
    let mut spec = text_spec(&computed, "");
    spec.strut = Run::from_style("", exact_kernel::TextStyle::from_style(&computed));
    spec.runs = n.text_runs().iter().map(Run::from_kernel).collect();
    let spec = spec.shown(n.props.str(PropId::Markup) == Some("markdown"));
    if spec.is_empty() {
        return Vec::new();
    }
    let paragraph = text.borrow_mut().paragraph(&spec, Some(width));
    let mut colors = Vec::new();
    run_colors(kernel, n, &mut colors);
    let mut out = Vec::new();
    for line in paragraph.layout_runs() {
        // A clamp's or an overflow's ellipsis is glyphs with an empty range at
        // the line's original end (`Shaped::ellipsize`): the line's text is
        // what the kept glyphs cover, then "…", the last run in logical order
        // taking it.
        let kept = || line.glyphs.iter().filter(|g| g.start != g.end);
        let ellipsis = line.glyphs.iter().any(|g| g.start == g.end);
        let Some(start) = kept().map(|g| g.start as usize).min() else {
            continue;
        };
        let end = kept().map(|g| g.end as usize).max().unwrap_or(start);
        let Some(shown) = line.text.get(start..end) else {
            continue;
        };
        let x = line
            .glyphs
            .iter()
            .map(|g| g.x)
            .fold(f32::INFINITY, f32::min);
        // Each canonical run's byte range in this line, in logical order.
        let mut spans: Vec<(usize, usize, usize)> = Vec::new();
        let mut logical: Vec<_> = kept().collect();
        logical.sort_by_key(|g| g.start);
        for g in logical {
            let (a, b) = (g.start as usize - start, g.end as usize - start);
            match spans.last_mut() {
                Some((run, _, e)) if *run == g.run() => *e = (*e).max(b),
                _ => spans.push((g.run(), a, b)),
            }
        }
        let units = |bytes: usize| {
            shown
                .get(..bytes)
                .map_or(bytes, |p| p.encode_utf16().count())
        };
        let last = spans.len().saturating_sub(1);
        let runs: Vec<Value> = spans
            .iter()
            .enumerate()
            .filter_map(|(i, &(run, a, b))| {
                let r = paragraph.runs().get(run)?;
                let takes = ellipsis && i == last;
                let glyphs = || {
                    line.glyphs
                        .iter()
                        .filter(move |g| g.run() == run || (takes && g.start == g.end))
                };
                let size = glyphs().next().map_or(r.size, |g| g.font_size);
                let from = glyphs().map(|g| g.x).fold(f32::INFINITY, f32::min);
                let to = glyphs()
                    .map(|g| g.x + g.w)
                    .fold(f32::NEG_INFINITY, f32::max);
                Some(json!({
                    "x": num(left + from),
                    "width": num(to - from),
                    "start": units(a),
                    "end": units(b) + usize::from(takes),
                    "size": size,
                    "weight": r.weight,
                    "italic": r.italic,
                    "tabular": r.font_variant_numeric & 1 != 0,
                    "letterSpacing": r.letter_spacing,
                    "color": colors.get(run).copied().map(color_json),
                }))
            })
            .collect();
        let text = if ellipsis {
            format!("{shown}\u{2026}")
        } else {
            shown.to_owned()
        };
        out.push(json!({
            "text": text,
            "x": left + x,
            "top": top + line.line_top,
            "height": line.line_height,
            "baseline": top + line.line_y,
            "width": line.line_w,
            "rtl": line.rtl,
            "runs": runs,
        }));
    }
    out
}

/// Each canonical run's colour, in the paragraph's run order (the order
/// `paint::inline::text_palette` resolves them in), unresolved.
fn run_colors(kernel: &Kernel, n: &NodeRef<'_>, out: &mut Vec<ColorValue>) {
    if n.props.str(PropId::Text).is_some() {
        out.push(n.text_color());
        return;
    }
    for child in n.children() {
        let Some(child) = kernel.node(child) else {
            continue;
        };
        if child.node_type == NodeType::Text {
            run_colors(kernel, &child, out);
        } else if child.inline_box().is_some() {
            out.push(ColorValue::Fixed(exact_kernel::style::Color(0)));
        }
    }
}

fn visible(c: ColorValue) -> bool {
    c.resolve(false).a() != 0 || c.resolve(true).a() != 0
}

fn color_json(c: ColorValue) -> Value {
    if let ColorValue::Role(id) = c {
        if let Some((material, percent)) =
            roles::role_of(id).and_then(|role| palette::material_of(role.name))
        {
            return json!({ "m": material as usize, "a": percent });
        }
    }
    let argb = |dark: bool| {
        let [r, g, b, a] = crate::paint::rgba(c.resolve(dark));
        i64::from(u32::from_be_bytes([a, r, g, b]) as i32)
    };
    json!({ "l": argb(false), "d": argb(true) })
}

fn num(v: f32) -> Value {
    // Hundredths of a point: more is noise, and the tree stays small.
    json!(((v as f64) * 100.0).round() / 100.0)
}
