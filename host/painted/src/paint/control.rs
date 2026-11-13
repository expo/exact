//! Form controls, painted (LLP 1069.001 D7): this host has no platform
//! controls to show. Where it has no platform at all (the Linux desktop,
//! Windows) each is the web's neutral look's (`neutral.rs`; LLP 1115.000
//! D17). Under the Material look a checkbox and a radio are Material's
//! (`material.rs`). Else, which is Android's reader until its other controls
//! are Material's,
//! a checkbox is Chrome's rounded square, a radio its circle and a switch a
//! pill with a thumb, filled with `accent-color`, else the platform's accent
//! where the host reported one ([`accents`]). `appearance: none` draws
//! no native chrome; a select or date field keeps its value in the author's box (D6).

use super::material::Icon;
use super::{look, neutral, rgba, Backend, Rect4, Shape};
use exact_kernel::generated::metric;
use exact_kernel::{Appearance, FieldChrome, NodeRef, NodeType, PropId, StyleMask};
use tiny_skia::Transform;

/// This painter's own accent, Chrome's default `#0075ff`: what a control
/// paints in where its node sets no `accent-color` and the host reported no
/// colours (desktop Linux, which has no platform accent to read; LLP 1104 D7).
const ACCENT: [u8; 4] = [0x00, 0x75, 0xff, 0xff];

/// The accent a control, a focus ring or a filled button paints in, and
/// what is drawn on it (a check, a switch's thumb, a filled button's
/// title). One accent (LLP 1115.001 D4): the node's `accent-color` where it
/// sets one (`authored`), with white on it as before; else the platform's
/// `AccentColor` with its `AccentColorText`, where the host reported the
/// device's colours; else this painter's own blue, with white.
pub(crate) fn accents(authored: Option<[u8; 4]>, dark: bool) -> ([u8; 4], [u8; 4]) {
    if look::neutral() {
        return look::accents(authored, dark);
    }
    match authored {
        Some(accent) => (accent, [0xff; 4]),
        None => crate::presenter::reported_accent(dark).unwrap_or((ACCENT, [0xff; 4])),
    }
}

// The existing system-ui stack selects the host's installed sans-serif face.
// Linux's own control size is the page's initial 16px, independent of ancestors.
pub(crate) fn control_text_styles() -> exact_kernel::ControlTextStyles {
    let font = exact_kernel::ControlFont {
        family: "system-ui".into(),
        family_id: 0,
        size: 16.0,
        weight: 400,
        style: exact_kernel::FontStyle::Normal,
    };
    exact_kernel::ControlTextStyles {
        field: font.clone(),
        textarea: font.clone(),
        button: font,
    }
}

/// An authored `::placeholder` colour (LLP 1104 D3), its alpha its own;
/// `None` leaves the platform's.
pub(crate) fn authored_placeholder(
    style: &exact_kernel::StyleProps,
    dark: bool,
) -> Option<[u8; 4]> {
    style
        .mask
        .has(exact_kernel::StyleId::PlaceholderColor)
        .then(|| super::rgba(style.rare.placeholder_color.resolve(dark)))
}

/// What a field has round its text. Under the neutral look (LLP 1115.000
/// D17) that is the control's line and the look's padding, of the field's
/// font size, as the page's sheet has them; else Material's filled field's
/// (`material.rs`).
pub(crate) fn field_chrome(font_size: f32, labelled: bool) -> FieldChrome {
    if !look::neutral() {
        return super::material::chrome(labelled);
    }
    let line = look::px(metric::CONTROL_LINE, font_size);
    let block = line + look::px(metric::FIELD_PADDING_BLOCK, font_size);
    let inline = line + look::px(metric::FIELD_PADDING_INLINE, font_size);
    FieldChrome {
        top: block,
        right: inline,
        bottom: block,
        left: inline,
        minimum_height: 0.0,
        provisional: false,
    }
}

/// A control's choice while its bound value is the one it had when the
/// person chose, as the web build writes an input's `value` only when the
/// bound value changes (LLP 1069.001 D4, amended 2026-10-04; kanban2 #5).
pub(crate) fn choice<'a>(
    node: &NodeRef<'_>,
    chosen: Option<&'a (String, String)>,
) -> Option<&'a str> {
    let bound = node.props.str(PropId::Value).unwrap_or("");
    chosen
        .filter(|(_, at)| at == bound)
        .map(|(c, _)| c.as_str())
}

/// A range's thumb's diameter in a box `w` by `h`: the neutral look's mark,
/// else 16 px. The painter and the pointer both read it and
/// [`range_mirrored`], so the thumb lands under a press.
pub(crate) fn range_thumb(node: &NodeRef<'_>, w: f32, h: f32) -> f32 {
    let d = match look::neutral() {
        true => look::px(metric::MARK_SIZE, neutral::em(node)),
        false => 16.0,
    };
    d.min(h).min(w)
}

/// Whether a range's minimum is at the right. A horizontal range runs from
/// its inline start, so under `direction: rtl` it is, as a browser draws it
/// and Android's `SeekBar` (`mirrorForRtl`) does.
pub(crate) fn range_mirrored(node: &NodeRef<'_>) -> bool {
    node.computed_row(exact_kernel::StyleId::Direction, |s| s.direction)
        == exact_kernel::Direction::Rtl
}

/// What a date control shows: its [`choice`], else the bound value. Native
/// neutral fields show HTML's format hint when empty; bare fields leave
/// room for their placeholder. Else the device formats the value (`dates.rs`).
pub(super) fn date_text(
    node: &NodeRef<'_>,
    chosen: Option<&(String, String)>,
    dates: Option<&super::dates::Dates>,
) -> String {
    let value = choice(node, chosen).unwrap_or_else(|| node.props.str(PropId::Value).unwrap_or(""));
    let kind = node.props.str(PropId::Type).unwrap_or("date");
    if value.is_empty() && node.style.appearance == Appearance::None {
        return String::new();
    }
    if !look::neutral() {
        let shown = dates.and_then(|d| d.show(kind, value));
        return shown.unwrap_or_else(|| value.to_string());
    }
    match (value, kind) {
        ("", "date") => "yyyy-mm-dd",
        ("", "time") => "--:--",
        ("", _) => "yyyy-mm-ddT--:--",
        (v, _) => v,
    }
    .to_string()
}

/// A select's label: its [`choice`]'s option, else the bound one's.
pub(super) fn select_label(
    kernel: &exact_kernel::Kernel,
    node: &NodeRef<'_>,
    chosen: Option<&(String, String)>,
) -> Option<String> {
    match choice(node, chosen) {
        Some(v) => kernel
            .select_choices(node.id)
            .into_iter()
            .find(|c| c.value == v)
            .map(|c| c.label),
        None => kernel.select_chosen(node.id).map(|c| c.label),
    }
}

/// A node's `accent-color`, where it sets one.
pub(crate) fn accent(node: &NodeRef<'_>, dark: bool) -> Option<[u8; 4]> {
    node.computed_style(StyleMask::INHERITED)
        .accent_color
        .map(|c| rgba(c.resolve(dark)))
}

/// An authored accent: an `accent-color` on the node or above it, else the
/// design's `AccentColor` (LLP 1121 D9); none where only the platform's.
/// `accent-color: AccentColor` names the platform's (or the design's), not
/// an authored one, except under an authored accent, which it keeps, as the
/// web's `var(--exact-accent, AccentColor)` does (stage 9 F1): the painting
/// scope holds that one ([`roles::scoped_accent`]).
pub(crate) fn authored_accent(node: &NodeRef<'_>, dark: bool) -> Option<[u8; 4]> {
    use exact_kernel::style::roles;
    let platform = roles::role("AccentColor");
    let accent = node.computed_style(StyleMask::INHERITED).accent_color;
    let role = matches!(accent, Some(exact_kernel::ColorValue::Role(id)) if Some(id) == platform);
    let written = if role {
        roles::scoped_accent()
            .filter(|c| !matches!(c, exact_kernel::ColorValue::Role(id) if Some(*id) == platform))
    } else {
        accent
    };
    written
        .map(|c| rgba(c.resolve(dark)))
        .or_else(|| roles::overridden(platform?).map(|c| rgba(c.resolve(dark))))
}

/// Whether the author took `node`'s focus ring off: CSS `outline: none`,
/// the row's one value (LLP 1104 D6), on any element that takes the focus.
pub(crate) fn ringless(node: &NodeRef<'_>) -> bool {
    node.style.mask.has(exact_kernel::StyleId::OutlineStyle)
}

/// A focused node's accent ([`accents`]), except native neutral fields
/// marked invalid, whose ring matches their system-red border.
pub(crate) fn ring(node: &NodeRef<'_>, dark: bool) -> [u8; 4] {
    if look::neutral()
        && (node.node_type == NodeType::TextInput
            || (node.node_type == NodeType::Control
                && matches!(
                    node.props.str(PropId::Type),
                    Some("select" | "date" | "time" | "datetime-local")
                )))
        && node.style.appearance != Appearance::None
        && node.props.str(PropId::AccessibilityInvalid) == Some("true")
    {
        return look::role("-exact-system-red", dark);
    }
    accents(accent(node, dark), dark).0
}

/// A select's open menu (LLP 1069.001 D7): its panel in viewport points,
/// one row per option, the chosen one marked with the accent.
#[derive(Debug, Clone)]
pub struct MenuPaint {
    /// The panel.
    pub rect: Rect4,
    /// Each row's height.
    pub row: f32,
    /// Each option's label and whether it is disabled.
    pub rows: Vec<(String, bool)>,
    /// The highlighted row: the one the keys moved to, else the chosen.
    pub chosen: Option<usize>,
    /// How far the rows are scrolled, in points.
    pub offset: f32,
    /// The select's inherited text style, which the rows are set in.
    pub style: exact_kernel::StyleProps,
    /// The select's accent, where it sets one.
    pub accent: Option<[u8; 4]>,
}

/// The menu's inset above its first row and below its last.
pub const MENU_PAD: f32 = 4.0;

impl super::Painter {
    /// Whether focused `node` paints no ring: a press left it without one,
    /// or its author wrote `outline: none` ([`ringless`]).
    pub(super) fn unringed(&self, node: &NodeRef<'_>) -> bool {
        self.ringless == Some(node.id) || ringless(node)
    }

    /// Fields and native controls ring their chrome; links ring their text
    /// fragments. Other focus targets, including a route's heading or its
    /// container (LLP 1115.000 D4.2), ring their outer box after its own clip.
    pub(super) fn node_ring(
        &mut self,
        walk: &super::Walk<'_, '_>,
        node: &NodeRef<'_>,
        outer: &Shape,
        ts: Transform,
    ) {
        if walk.scene.focus != Some(node.id)
            || matches!(node.node_type, NodeType::Control | NodeType::TextInput)
            || node.props.str(PropId::Href).is_some()
            || self.unringed(node)
            || node.props.bool(PropId::Disabled) == Some(true)
        {
            return;
        }
        self.field_ring(outer, ring(node, self.dark), ts);
    }

    /// Native text field chrome; text uses the kernel's published content rect.
    pub(super) fn text_field_chrome(
        &mut self,
        node: &NodeRef<'_>,
        rect: Rect4,
        focused: bool,
        ts: Transform,
    ) -> Shape {
        if look::neutral() {
            return neutral::field(self.backend.as_mut(), node, rect, ts, self.dark);
        }
        self.filled_field(node, rect, focused, false, ts)
    }

    /// A closed select, or a field that shows a value (LLP 1069.001 D7):
    /// the neutral look's field with the text, and a caret when it opens a
    /// menu; else Material's filled field with its trailing icon.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn field_control(
        &mut self,
        walk: &super::Walk<'_, '_>,
        node: &NodeRef<'_>,
        content: Rect4,
        outer: &Shape,
        focused: bool,
        ts: Transform,
        text: &str,
        chevron: bool,
    ) {
        let bare = node.style.appearance == Appearance::None;
        let dark = self.dark;
        if bare || look::neutral() {
            // Focus changes without a tree commit, so a row cannot keep this ring.
            self.row_refuse();
            // No appearance keeps the value and authored ink, without the field's inset or caret.
            let shape = if bare {
                *outer
            } else {
                neutral::field(self.backend.as_mut(), node, content, ts, dark)
            };
            let style = node.computed_style(StyleMask::INHERITED);
            let placeholder = bare && text.is_empty();
            let shown = if placeholder {
                node.props.str(PropId::Placeholder).unwrap_or("")
            } else {
                text
            };
            let spec = super::text_spec(&style, shown);
            let paragraph = self.text.borrow_mut().paragraph(&spec, None);
            let (x, y, w, h) = content;
            let em = neutral::em(node);
            let inset = if bare {
                0.0
            } else {
                look::px(metric::CONTROL_LINE, em) + look::px(metric::FIELD_PADDING_INLINE, em)
            };
            let rtl = style.direction == exact_kernel::Direction::Rtl;
            let ox = if bare {
                x + crate::text::field::line_left(
                    style.text_align,
                    style.direction,
                    w,
                    paragraph.width,
                )
            } else if rtl {
                x + w - inset - paragraph.width
            } else {
                x + inset
            };
            let oy = y + ((h - paragraph.height) / 2.0).max(0.0);
            let palette = [crate::text::RunPaint {
                color: if placeholder {
                    authored_placeholder(node.style, dark)
                        .unwrap_or_else(|| look::role("-exact-placeholder", dark))
                } else if bare {
                    super::presented_color(walk, node, dark)
                        .unwrap_or_else(|| rgba(node.text_color().resolve(dark)))
                } else {
                    neutral::field_ink(node, dark)
                },
                source: node.id,
            }];
            self.backend.push_clip(&Shape::rect(content), ts);
            self.backend.text(
                &mut self.text.borrow_mut(),
                &paragraph,
                &palette,
                (ox, oy),
                ts,
            );
            self.backend.pop_clip();
            if chevron && !bare {
                neutral::caret(self.backend.as_mut(), node, content, ts, dark);
            }
            let unringed = if bare {
                ringless(node)
            } else {
                self.unringed(node)
            };
            if focused && !unringed && node.props.bool(PropId::Disabled) != Some(true) {
                self.field_ring(&shape, ring(node, dark), ts);
            }
            return;
        }
        let icon = match node.props.str(PropId::Type) {
            Some("select") => Icon::Menu,
            Some("time") => Icon::Clock,
            _ => Icon::Calendar,
        };
        self.filled_value(node, content, focused, ts, text, icon);
    }

    /// A range (LLP 1069.001 D7): Chrome's track, filled with the accent to
    /// a 16 px thumb at the value.
    pub(super) fn range_control(
        &mut self,
        node: &NodeRef<'_>,
        content: Rect4,
        ts: Transform,
        chosen: Option<&(String, String)>,
    ) {
        if node.style.appearance == Appearance::None {
            return;
        }
        let dark = self.dark;
        let disabled = node.props.bool(PropId::Disabled) == Some(true);
        let alpha = |mut c: [u8; 4]| {
            if disabled {
                c[3] /= 2;
            }
            c
        };
        let range = exact_kernel::Range::of(node.props);
        let value = choice(node, chosen)
            .and_then(|c| c.parse::<f64>().ok())
            .map_or_else(|| range.shown(node.props), |v| range.sanitize(v));
        let t = if range.max > range.min {
            ((value - range.min) / (range.max - range.min)) as f32
        } else {
            0.0
        };
        let (x, y, w, h) = content;
        let (d, mirrored) = (range_thumb(node, w, h), range_mirrored(node));
        let along = if mirrored { 1.0 - t } else { t };
        if look::neutral() {
            neutral::range(self.backend.as_mut(), node, content, ts, dark, d, along);
            return;
        }
        let cy = y + h / 2.0;
        let tx = x + (w - d) * along;
        let track =
            |from: f32, to: f32| Shape::new((from, cy - 2.0, (to - from).max(0.0), 4.0), [2.0; 4]);
        let rest = if dark {
            [0x78, 0x78, 0x80, 0x5c]
        } else {
            [0x78, 0x78, 0x80, 0x33]
        };
        self.backend.fill(&track(x, x + w), alpha(rest), ts);
        let (accent, _) = accents(accent(node, dark), dark);
        // The fill runs from the minimum to the thumb.
        let filled = match mirrored {
            true => track(tx + d / 2.0, x + w),
            false => track(x, tx + d / 2.0),
        };
        self.backend.fill(&filled, alpha(accent), ts);
        let thumb = Shape::new((tx, cy - d / 2.0, d, d), [d / 2.0; 4]);
        self.backend.fill(&thumb, alpha(accent), ts);
    }

    /// A select's open menu, over everything (LLP 1069.001 D7).
    pub(super) fn menu(&mut self, menu: &MenuPaint) {
        let ts = Transform::identity();
        // Painted after the walk: the select's own scheme (LLP 1034 §8), as
        // its inherited style computes it, else the app's.
        let dark = match menu.style.color_scheme {
            exact_kernel::ColorScheme::Dark => true,
            exact_kernel::ColorScheme::Light => false,
            exact_kernel::ColorScheme::Normal => self.dark,
        };
        let previous = std::mem::replace(&mut self.dark, dark);
        if look::neutral() {
            self.neutral_menu(menu, dark);
            self.dark = previous;
            return;
        }
        let panel = Shape::new(menu.rect, [6.0; 4]);
        self.backend.fill(&panel, [0, 0, 0, 0x40], ts);
        let bg = if dark {
            [0x2c, 0x2c, 0x2e, 0xff]
        } else {
            [0xff, 0xff, 0xff, 0xff]
        };
        self.backend.fill(&panel.inset(1.0), bg, ts);
        let (accent, on_accent) = accents(menu.accent, dark);
        let (x, y, w, h) = menu.rect;
        self.backend.push_clip(&panel, ts);
        for (i, (label, disabled)) in menu.rows.iter().enumerate() {
            let top = y + MENU_PAD + i as f32 * menu.row - menu.offset;
            if top + menu.row < y || top > y + h {
                continue;
            }
            let chosen = menu.chosen == Some(i);
            if chosen {
                let row = Shape::new((x + 4.0, top, w - 8.0, menu.row), [4.0; 4]);
                self.backend.fill(&row, accent, ts);
            }
            let mut ink = if chosen {
                on_accent
            } else if dark {
                [0xff, 0xff, 0xff, 0xff]
            } else {
                [0x00, 0x00, 0x00, 0xff]
            };
            if *disabled {
                ink[3] = 0x66;
            }
            let spec = super::text_spec(&menu.style, label);
            let paragraph = self.text.borrow_mut().paragraph(&spec, None);
            let oy = top + ((menu.row - paragraph.height) / 2.0).max(0.0);
            let mut engine = self.text.borrow_mut();
            let palette = [crate::text::RunPaint {
                color: ink,
                source: 0,
            }];
            self.backend
                .text(&mut engine, &paragraph, &palette, (x + 12.0, oy), ts);
        }
        self.backend.pop_clip();
        self.dark = previous;
    }
}

/// Paint `node`'s control in its content box; `held` is an unbound
/// checkbox's own state, which the host keeps as a browser does, and
/// `hovered` whether a pointer that hovers is over it. `scale` is device
/// pixels a point.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint(
    backend: &mut dyn Backend,
    node: &NodeRef<'_>,
    content: Rect4,
    ts: Transform,
    dark: bool,
    held: Option<bool>,
    hovered: bool,
    scale: f32,
) {
    if node.style.appearance == Appearance::None {
        return;
    }
    let on = node.props.bool(PropId::Checked).or(held).unwrap_or(false);
    if look::neutral() {
        if node.props.str(PropId::AccessibilityRole) == Some("switch") {
            neutral::switch(backend, node, content, ts, dark, on);
        } else {
            neutral::mark(backend, node, content, ts, dark, on, hovered);
        }
        return;
    }
    let switch = node.props.str(PropId::AccessibilityRole) == Some("switch");
    let radio = node.props.str(PropId::Type) == Some("radio");
    if look::material() && !switch {
        return match radio {
            true => super::material::radio(backend, node, content, ts, dark, on, scale),
            false => super::material::checkbox(backend, node, content, ts, dark, on, scale),
        };
    }
    let disabled = node.props.bool(PropId::Disabled) == Some(true);
    let (accent, on_accent) = accents(accent(node, dark), dark);
    let dim = |mut c: [u8; 4]| {
        if disabled {
            c[3] /= 2;
        }
        c
    };
    let (x, y, w, h) = content;
    if node.props.str(PropId::AccessibilityRole) == Some("switch") {
        let track = Shape::new(content, [h / 2.0; 4]);
        // UIKit's `systemFill` (Safari's off track): translucent, so it
        // reads on a grey surface as on white.
        let off = if dark {
            [0x78, 0x78, 0x80, 0x5c]
        } else {
            [0x78, 0x78, 0x80, 0x33]
        };
        backend.fill(&track, dim(if on { accent } else { off }), ts);
        let d = (h - 4.0).max(0.0);
        let tx = if on { x + w - 2.0 - d } else { x + 2.0 };
        let thumb = Shape::new((tx, y + 2.0, d, d), [d / 2.0; 4]);
        backend.fill(&thumb, dim(if on { on_accent } else { [0xff; 4] }), ts);
        return;
    }
    let (border, fill) = if dark {
        ([0x85, 0x85, 0x85, 0xff], [0x3b, 0x3b, 0x3b, 0xff])
    } else {
        ([0x76, 0x76, 0x76, 0xff], [0xff, 0xff, 0xff, 0xff])
    };
    // A radio is Chrome's circle (x2apps survey #2): checked, an accent
    // ring around the field's own fill and an accent dot a fifth in.
    if node.props.str(PropId::Type) == Some("radio") {
        let circle = Shape::new(content, [w.min(h) / 2.0; 4]);
        backend.fill(&circle, dim(if on { accent } else { border }), ts);
        backend.fill(&circle.inset(1.0), dim(fill), ts);
        if on {
            backend.fill(&circle.inset(w.min(h) * 0.2), dim(accent), ts);
        }
        return;
    }
    let square = Shape::new(content, [2.0; 4]);
    if on {
        backend.fill(&square, dim(accent), ts);
        // The check: two strokes, as rotated bars about their joint.
        let s = w.min(h);
        let (cx, cy) = (x + w * 0.42, y + h * 0.7);
        let bar = (s * 0.14).max(1.5);
        let check = dim(on_accent);
        let short = Transform::from_rotate_at(45.0, cx, cy);
        backend.fill(
            &Shape::rect((cx - s * 0.3, cy - bar / 2.0, s * 0.3 + bar / 2.0, bar)),
            check,
            ts.pre_concat(short),
        );
        let long = Transform::from_rotate_at(-50.0, cx, cy);
        backend.fill(
            &Shape::rect((cx - bar / 2.0, cy - bar / 2.0, s * 0.62, bar)),
            check,
            ts.pre_concat(long),
        );
    } else {
        backend.fill(&square, dim(border), ts);
        backend.fill(&square.inset(1.0), dim(fill), ts);
    }
}

/// An indeterminate `progress` (LLP 1069.001, amended 2026-10-07): UIKit's
/// activity indicator as one still frame, eight spokes round the centre of
/// the box's shorter side in `color`, the tail fading behind the brightest
/// one (counterclockwise: the indicator turns clockwise). Linux paints on
/// change, not per frame, so it does not turn; every frame is the same,
/// which is what the agent's held clock shows on the other hosts. Under
/// the Material look it is Material's indicator ([`material_spinner`]).
pub(super) fn progress(
    backend: &mut dyn Backend,
    node: &NodeRef<'_>,
    content: Rect4,
    ts: Transform,
    dark: bool,
) {
    if look::neutral() {
        return neutral::spinner(backend, node, content, ts, dark);
    }
    let (x, y, w, h) = content;
    let s = w.min(h);
    if s <= 0.0 {
        return;
    }
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    // Its own `color`, else an authored accent, else the platform's
    // (Material's spinner is in the primary colour): an inherited colour is
    // not the spinner's (LLP 1115 D4, LLP 1121 D9).
    let ink = if node.style.written.has(exact_kernel::StyleId::TextColor) {
        rgba(node.text_color().resolve(dark))
    } else {
        accents(authored_accent(node, dark), dark).0
    };
    if look::material() {
        return material_spinner(backend, node.key, (cx, cy, s), ink, ts);
    }
    let (thick, long) = (s * 0.1, s * 0.27);
    let spoke = Shape::new(
        (cx - thick / 2.0, cy - s / 2.0, thick, long),
        [thick / 2.0; 4],
    );
    for i in 0..8 {
        let mut c = ink;
        c[3] = (c[3] as f32 * (1.0 - i as f32 * 0.1)) as u8;
        let turn = Transform::from_rotate_at(-45.0 * i as f32, cx, cy);
        backend.fill(&spoke, c, ts.pre_concat(turn));
    }
}

/// Material's indeterminate circular indicator (LLP 1115.001): a 4 dp
/// square-capped arc round a 40 dp circle, scaled to the box's shorter side
/// `s`, whose head and tail chase each other as it turns. The arc is the
/// first stretch of a dash `[c, c]` along the circle from its top, `c` its
/// length: a phase `p` leaves `c - p` of it showing. A backend whose reader
/// plays layers draws it in one, which the reader turns and whose phase it
/// moves every frame by [`crate::host::lower::spinner_tracks`]; elsewhere,
/// and before the tracks reach it, it is the arc at its longest.
fn material_spinner(
    backend: &mut dyn Backend,
    key: exact_kernel::NodeKey,
    (cx, cy, s): (f32, f32, f32),
    ink: [u8; 4],
    ts: Transform,
) {
    use exact_kernel::svg::{Path, Seg};
    let width = s / 10.0;
    let r = (s - width) / 2.0;
    let k = r * 0.552_284_8;
    let path = Path(vec![
        Seg::Move(cx, cy - r),
        Seg::Cubic(cx + k, cy - r, cx + r, cy - k, cx + r, cy),
        Seg::Cubic(cx + r, cy + k, cx + k, cy + r, cx, cy + r),
        Seg::Cubic(cx - k, cy + r, cx - r, cy + k, cx - r, cy),
        Seg::Cubic(cx - r, cy - k, cx - k, cy - r, cx, cy - r),
    ]);
    let c = std::f32::consts::TAU * r;
    let paint = super::SvgPaint {
        path: &path,
        fill: None,
        even_odd: false,
        stroke: Some(super::Ink::Solid(ink)),
        order: [0, 1, 2],
        width,
        cap: 2,
        join: 0,
        miter: 4.0,
        dash: vec![c, c],
        phase: c * crate::host::lower::SPINNER_REST as f32,
    };
    let layer = crate::host::lower::spinner_layer(key);
    let base = [0.0, 0.0, 1.0, 0.0, 1.0, 0.0, c];
    let layered = backend.layer_begin(layer, ts, (cx, cy), base);
    backend.svg_path(&paint, ts);
    if layered {
        backend.layer_end();
    }
}

/// A determinate `progress` (LLP 1116 D8): UIKit's progress view, drawn as
/// the range's track is here — a 4 px pill across the content box, centred,
/// in the system fill, and the accent to the value from the leading edge.
pub(super) fn progress_bar(
    backend: &mut dyn Backend,
    node: &NodeRef<'_>,
    content: Rect4,
    ts: Transform,
    dark: bool,
) {
    if look::neutral() {
        return neutral::progress_bar(backend, node, content, ts, dark);
    }
    let (x, y, w, h) = content;
    let t = 4f32.min(h);
    if w <= 0.0 || t <= 0.0 {
        return;
    }
    let fraction = exact_kernel::Progress::of(node.props).fraction() as f32;
    let rtl = node.computed_row(exact_kernel::StyleId::Direction, |s| s.direction)
        == exact_kernel::Direction::Rtl;
    let filled = w * fraction;
    let bar = |from: f32, width: f32| Shape::new((from, y + (h - t) / 2.0, width, t), [t / 2.0; 4]);
    let rest = if dark {
        [0x78, 0x78, 0x80, 0x5c]
    } else {
        [0x78, 0x78, 0x80, 0x33]
    };
    backend.fill(&bar(x, w), rest, ts);
    if filled > 0.0 {
        let from = if rtl { x + w - filled } else { x };
        let (accent, _) = accents(accent(node, dark), dark);
        backend.fill(&bar(from, filled), accent, ts);
    }
}

#[cfg(test)]
mod field_tests {
    use crate::text::{Measurer, TextEngine};
    use exact_kernel::{
        ControlFont, FieldChrome, FieldChromeRequest, FieldKind, FontStyle, TextMeasurer,
    };

    #[test]
    fn the_host_answers_every_field_kind_and_font_without_a_provisional_frame() {
        let mut measurer = Measurer(TextEngine::shared());
        for kind in [
            FieldKind::Field,
            FieldKind::SecureField,
            FieldKind::SearchField,
            FieldKind::Textarea,
        ] {
            for size in [10.0, 16.0, 32.0] {
                let chrome = measurer.field_chrome(&FieldChromeRequest {
                    kind,
                    font: ControlFont {
                        family: String::new(),
                        family_id: 0,
                        size,
                        weight: 700,
                        style: FontStyle::Italic,
                    },
                    cell: false,
                    labelled: false,
                });
                // Material's filled field, whatever the font (LLP 1115.001 D9).
                assert_eq!(
                    chrome,
                    FieldChrome {
                        top: 16.0,
                        right: 16.0,
                        bottom: 16.0,
                        left: 16.0,
                        minimum_height: 56.0,
                        provisional: false,
                    }
                );
            }
        }
    }
}

/// LLP 1104 D3: the authored `::placeholder` colour, its alpha kept; none
/// is the platform's (the field's paint and the hosted field's hint).
#[cfg(test)]
#[test]
fn an_authored_placeholder_colour_keeps_its_alpha_and_none_is_the_platforms() {
    use exact_kernel::{StyleId, StyleProps, StyleValue};
    let mut style = StyleProps::default();
    assert_eq!(authored_placeholder(&style, false), None);
    let pair = StyleValue::Text("light-dark(#00800080, #eeeeee)".into());
    style.set_dynamic(StyleId::PlaceholderColor, &pair).unwrap();
    assert_eq!(
        authored_placeholder(&style, false),
        Some([0, 0x80, 0, 0x80])
    );
    assert_eq!(
        authored_placeholder(&style, true),
        Some([0xee, 0xee, 0xee, 0xff])
    );
}
