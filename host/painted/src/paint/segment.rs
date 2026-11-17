//! The chrome of a radio of a segmented `radiogroup`
//! (`exact_kernel::segmented`), under the ordinary button's face: under
//! the Material look one of Material 3's segmented buttons, under the
//! neutral look a segment of the web's neutral segmented control
//! (`host/web/neutral.css`), on the track the group's own box paints.
use super::super::{look, Painter, Rect4, Shape};
use super::{material_dp, material_number};
use crate::presenter::Material;
use exact_kernel::generated::metric;
use exact_kernel::{NodeRef, PropId, SegmentPlace, StyleProps};
use tiny_skia::Transform;

/// Material's check (`Icons.Filled.Check`), in 24 units.
const CHECK: &str = "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z";

/// The room a Material segment keeps beside its label for its check: the
/// `segmented-button-icon` row and the `-icon-gap` row after it
/// (`SegmentedButton.kt`'s `IconSpacing`).
pub(super) fn check_room(scale: f32) -> f32 {
    (material_dp(metric::SEGMENTED_BUTTON_ICON) + material_dp(metric::SEGMENTED_BUTTON_ICON_GAP))
        * scale
}

/// A segment's corners (top left, top right, bottom right, bottom left) at
/// `height`: under Material round at the group's ends (`CornerFull`) and
/// square between, under the neutral look the segment's radius.
pub(super) fn radii(
    place: SegmentPlace,
    rtl: bool,
    height: f32,
    neutral: bool,
    em: f32,
) -> [f32; 4] {
    if neutral {
        return [look::px(metric::SEGMENT_RADIUS, em); 4];
    }
    let round = height / 2.0;
    let (start, end) = match place {
        SegmentPlace::First => (round, 0.0),
        SegmentPlace::Middle => (0.0, 0.0),
        SegmentPlace::Last => (0.0, round),
        SegmentPlace::Only => (round, round),
    };
    let (left, right) = if rtl { (end, start) } else { (start, end) };
    [left, right, right, left]
}

/// A Material colour role: the device's, as its shell told it, else the
/// role of Exact's nearest it (LLP 1115.001 D4's table, which a told
/// palette also fills, so both paths agree where it maps the role).
fn material_role(role: Material, nearest: &str, dark: bool) -> [u8; 4] {
    crate::presenter::material(role, dark).unwrap_or_else(|| look::role(nearest, dark))
}

/// A box's outline as a path, `by` inside its edge, each corner round by
/// its radius less `by` (top left, top right, bottom right, bottom left).
fn outline_path(rect: Rect4, radii: [f32; 4], by: f32) -> String {
    let (l, t) = (rect.0 + by, rect.1 + by);
    let (r, b) = (rect.0 + rect.2 - by, rect.1 + rect.3 - by);
    let [tl, tr, br, bl] = radii.map(|v| (v - by).max(0.0));
    let arc = |radius: f32, x: f32, y: f32| match radius > 0.0 {
        true => format!("A{radius} {radius} 0 0 1 {x} {y}"),
        false => format!("L{x} {y}"),
    };
    format!(
        "M{} {t}H{}{}V{}{}H{}{}V{}{}Z",
        l + tl,
        r - tr,
        arc(tr, r, t + tr),
        b - br,
        arc(br, r - br, b),
        l + bl,
        arc(bl, l, b - bl),
        t + tl,
        arc(tl, l + tl, t),
    )
}

impl Painter {
    /// Material 3's segmented button (`SegmentedButton.kt`, Compose
    /// Material 3 1.4; `OutlinedSegmentedButtonTokens`) under its face: a
    /// 1 dp outline in `outline`; checked, a `secondaryContainer` fill and
    /// the content in `onSecondaryContainer`; unchecked, the content in
    /// `onSurface` on nothing; disabled, `onSurface` at the tokens' disabled
    /// opacities. Held, an authored press background, else the ripple in
    /// the content's colour (`ripple.rs`); hovered or focused, the state layer.
    /// Compose slides the label and grows the check as the choice moves;
    /// here they change at once. The content's ink.
    pub(super) fn material_segment(
        &mut self,
        node: &NodeRef<'_>,
        shape: &Shape,
        checked: bool,
        focused: bool,
        pressed_background: Option<[u8; 4]>,
        ts: Transform,
    ) -> [u8; 4] {
        let dark = self.dark;
        let disabled = node.props.bool(PropId::Disabled) == Some(true);
        let faded = |c: [u8; 4], index| look::fade(c, material_number(index));
        let on_surface = look::role("-exact-label", dark);
        let ink = if disabled {
            faded(on_surface, metric::SEGMENTED_BUTTON_DISABLED_LABEL)
        } else if checked {
            material_role(Material::OnSecondaryContainer, "-exact-label", dark)
        } else {
            on_surface
        };
        // Disabled too: Compose 1.4's `disabledActiveContainerColor` is the
        // selected container (`SegmentedButtonDefaults`).
        if checked {
            // `ButtonFace` is `secondaryContainer` (LLP 1115.001 D4).
            self.backend.fill(shape, look::role("ButtonFace", dark), ts);
        }
        if let Some(ground) = pressed_background {
            self.backend.fill(shape, ground, ts);
        } else if !disabled {
            // A focus's layer is the pressed state's opacity, as Compose's.
            let layer = if focused {
                Some(metric::STATE_PRESSED_OPACITY)
            } else if self.feedback.0 == Some(node.id) {
                Some(metric::STATE_HOVER_OPACITY)
            } else {
                None
            };
            if let Some(index) = layer {
                self.backend.fill(shape, faded(ink, index), ts);
            }
            self.ripple(node, shape, ink, ts);
        }
        let line = match disabled {
            true => faded(on_surface, metric::SEGMENTED_BUTTON_DISABLED_OUTLINE),
            false => material_role(Material::Outline, "-exact-separator", dark),
        };
        // Compose strokes its border inside the shape (`BorderStroke`).
        let width = material_dp(metric::SEGMENTED_BUTTON_OUTLINE);
        let radii = shape.radii.map(|(r, _)| r);
        let path = exact_kernel::svg::parse_d(&outline_path(shape.rect, radii, width / 2.0));
        self.backend.svg_path(
            &super::super::SvgPaint {
                path: &path,
                fill: None,
                even_odd: false,
                stroke: Some(super::super::Ink::Solid(line)),
                order: [0, 1, 2],
                width,
                cap: 0,
                join: 0,
                miter: 4.0,
                dash: Vec::new(),
                phase: 0.0,
            },
            ts,
        );
        ink
    }

    /// The web's neutral segment (`host/web/neutral.css`, "A segmented
    /// control") under its face: nothing of its own unchecked, its label the
    /// secondary label's colour and the label's under a pointer; checked, a
    /// chip of the page's ground (the fill's in dark) with a short shadow
    /// under it, its label the label's colour; disabled, grey. Held, an
    /// authored press background; the sheet gives a segment no wash of its
    /// own. The content's ink.
    pub(super) fn neutral_segment(
        &mut self,
        node: &NodeRef<'_>,
        shape: &Shape,
        hover: bool,
        pressed_background: Option<[u8; 4]>,
        ts: Transform,
    ) -> [u8; 4] {
        let dark = self.dark;
        let disabled = node.props.bool(PropId::Disabled) == Some(true);
        let checked = node.props.bool(PropId::AccessibilityChecked) == Some(true);
        let em = super::super::neutral::em(node);
        if checked {
            let (x, y, w, h) = shape.rect;
            let shadow = Shape {
                rect: (x, y + look::px(metric::SEGMENT_SHADOW_Y, em), w, h),
                ..*shape
            };
            let ink = look::fade([0, 0, 0, 255], look::number(metric::SEGMENT_SHADOW_ALPHA));
            // The page's blur radius is twice a Gaussian's deviation.
            let sigma = look::px(metric::SEGMENT_SHADOW_BLUR, em) / 2.0;
            self.backend.blurred_shadow(&shadow, ink, sigma, shape, ts);
            let chip = if dark {
                "-exact-fill"
            } else {
                "-exact-background"
            };
            self.backend.fill(shape, look::role(chip, dark), ts);
        }
        if let Some(ground) = pressed_background {
            self.backend.fill(shape, ground, ts);
        }
        let ink = if disabled {
            "GrayText"
        } else if checked || hover {
            "-exact-label"
        } else {
            "-exact-secondary-label"
        };
        look::role(ink, dark)
    }

    /// Material's check in `rect`, in `tint` at `style`'s weight.
    pub(super) fn segment_check(
        &mut self,
        rect: Rect4,
        style: &StyleProps,
        tint: [u8; 4],
        ts: Transform,
    ) {
        let path = super::super::material::role_path(CHECK, true);
        self.glyph_face(path, style, rect, tint, ts);
    }
}
