//! Material 3's buttons with a label (`Button.kt`, `ButtonDefaults`, Compose
//! Material 3 1.4): what a button is under the Material look (LLP 1115.001
//! D9), by its `buttonStyle`'s emphasis.
use super::super::{look, Painter, Shape};
use super::icon_button::role;
use super::material_number;
use crate::presenter::Material;
use exact_kernel::generated::metric;
use exact_kernel::{ButtonFaceStyle, NodeRef, PropId};
use tiny_skia::Transform;

/// `filled` and the prominent styles are the filled `Button`, `tinted` the
/// `FilledTonalButton`, `plain` and `borderless` the `TextButton`, and the
/// rest (unsaid, `bordered`, `gray`, the glass styles that stand in for
/// `bordered`) the `OutlinedButton`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaterialButton {
    Filled,
    Tonal,
    Outlined,
    Text,
}

/// The Material button a button of `style` is, under the Material look. A
/// segmented group's radio is the segmented button instead.
pub(crate) fn material_button(
    style: Option<&str>,
    rows: &ButtonFaceStyle,
) -> Option<MaterialButton> {
    if !look::material() || rows.segment.is_some() {
        return None;
    }
    Some(match super::look(style.unwrap_or("bordered")) {
        "fill" | "glass-fill" => MaterialButton::Filled,
        "soft" => MaterialButton::Tonal,
        "text" => MaterialButton::Text,
        _ => MaterialButton::Outlined,
    })
}

impl Painter {
    /// A tonal, outlined or text button's container and outline on `shape`,
    /// and its label's ink: `ButtonDefaults`' `filledTonalButtonColors`,
    /// `outlinedButtonColors` with `outlinedButtonBorder`, and
    /// `textButtonColors`, each disabled at its tokens' opacities. A text
    /// button's label is `accent`, `primary` unless the author wrote one.
    /// Held, an authored press background over it.
    pub(super) fn material_button(
        &mut self,
        node: &NodeRef<'_>,
        shape: &Shape,
        kind: MaterialButton,
        accent: [u8; 4],
        pressed_background: Option<[u8; 4]>,
        ts: Transform,
    ) -> [u8; 4] {
        let dark = self.dark;
        let disabled = node.props.bool(PropId::Disabled) == Some(true);
        let on_surface = look::role("-exact-label", dark);
        let variant = role(Material::OnSurfaceVariant, "-exact-secondary-label", dark);
        let faded = |c, index| look::fade(c, material_number(index));
        let label = metric::FILLED_BUTTON_DISABLED_LABEL;
        let (container, content) = match (kind, disabled) {
            (MaterialButton::Tonal, false) => (
                Some(role(Material::SecondaryContainer, "ButtonFace", dark)),
                role(Material::OnSecondaryContainer, "-exact-label", dark),
            ),
            (MaterialButton::Tonal, true) => (
                Some(faded(on_surface, metric::TONAL_BUTTON_DISABLED_CONTAINER)),
                faded(on_surface, label),
            ),
            (MaterialButton::Text, false) => (None, accent),
            (_, false) => (None, variant),
            (_, true) => (None, faded(variant, label)),
        };
        if let Some(fill) = pressed_background.or(container) {
            self.backend.fill(shape, fill, ts);
        }
        if kind == MaterialButton::Outlined {
            let line = role(Material::OutlineVariant, "-exact-separator", dark);
            let line = match disabled {
                true => faded(line, metric::OUTLINED_BUTTON_DISABLED_OUTLINE),
                false => line,
            };
            self.material_outline(shape, line, metric::OUTLINED_BUTTON_OUTLINE, ts);
        }
        content
    }
}
