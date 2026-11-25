//! Material 3's icon buttons (`IconButton.kt`, `IconButtonDefaults.kt`,
//! Compose Material 3 1.4): what a button holding only a symbol is under the
//! Material look (LLP 1115.001 D9).
use super::super::{look, Painter, Shape};
use super::{material_dp, material_number};
use crate::presenter::Material;
use exact_kernel::generated::metric;
use exact_kernel::{NodeRef, PressFace, PropId};
use tiny_skia::Transform;

/// Which of Material's icon buttons a face is, by its `buttonStyle`'s
/// emphasis: unsaid or `plain`, the standard `IconButton`; `tinted`, the
/// `FilledTonalIconButton`; `filled`, the `FilledIconButton`; any other said
/// style (`bordered`, `gray`), the `OutlinedIconButton`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IconButton {
    Standard,
    Tonal,
    Filled,
    Outlined,
}

/// The icon button a button is under the Material look: one holding a
/// symbol and no title. A raster image keeps the ordinary face.
pub(crate) fn material_icon(
    style: Option<&str>,
    face: &PressFace,
    rows: &exact_kernel::ButtonFaceStyle,
) -> Option<IconButton> {
    let lone = face.symbol.is_some() && face.title.is_none() && face.subtitle.is_none();
    // A radio of a segmented group keeps the segmented button's metrics.
    if !lone || rows.segment.is_some() || !look::material() {
        return None;
    }
    Some(match style.map(super::look) {
        None | Some("text" | "glass") => IconButton::Standard,
        Some("soft") => IconButton::Tonal,
        Some("fill" | "glass-fill") => IconButton::Filled,
        Some(_) => IconButton::Outlined,
    })
}

pub(super) fn role(role: Material, nearest: &str, dark: bool) -> [u8; 4] {
    crate::presenter::material(role, dark).unwrap_or_else(|| look::role(nearest, dark))
}

impl Painter {
    /// `kind`'s container and outline on `shape`, and the content's ink:
    /// `IconButtonDefaults`' colours, or its toggle's when the button has an
    /// `aria-pressed` (`iconToggleButtonColors` and the rest), and disabled
    /// `onSurface` at the `icon-button-disabled` rows' opacities. The standard
    /// and outlined buttons draw in `LocalContentColor`, which on a surface is
    /// `onSurface` (not the tokens' `onSurfaceVariant`, which only the
    /// `*VibrantColors` take). `accent` and
    /// `on_accent` are `primary` and `onPrimary` unless the author wrote an
    /// accent. Held, an authored press background over it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn material_icon_button(
        &mut self,
        node: &NodeRef<'_>,
        shape: &Shape,
        kind: IconButton,
        accent: [u8; 4],
        on_accent: [u8; 4],
        pressed_background: super::PressBackground,
        ts: Transform,
    ) -> [u8; 4] {
        let dark = self.dark;
        let disabled = node.props.bool(PropId::Disabled) == Some(true);
        let pressed = node.props.str(PropId::AccessibilityPressed);
        let (toggle, checked) = (
            matches!(pressed, Some("true" | "false" | "mixed")),
            pressed == Some("true"),
        );
        let on_surface = look::role("-exact-label", dark);
        let variant = role(Material::OnSurfaceVariant, "-exact-secondary-label", dark);
        // (container, content, outline) enabled.
        let (container, content, outline) = match kind {
            IconButton::Standard => (None, if checked { accent } else { on_surface }, false),
            IconButton::Tonal if checked => (
                Some(role(Material::Secondary, "AccentColor", dark)),
                role(Material::OnSecondary, "AccentColorText", dark),
                false,
            ),
            IconButton::Tonal => (
                Some(role(Material::SecondaryContainer, "ButtonFace", dark)),
                role(Material::OnSecondaryContainer, "-exact-label", dark),
                false,
            ),
            IconButton::Filled if toggle && !checked => (
                Some(role(
                    Material::SurfaceContainer,
                    "-exact-tertiary-background",
                    dark,
                )),
                variant,
                false,
            ),
            IconButton::Filled => (Some(accent), on_accent, false),
            IconButton::Outlined if checked => (
                Some(role(Material::InverseSurface, "-exact-label", dark)),
                role(Material::InverseOnSurface, "Canvas", dark),
                false,
            ),
            IconButton::Outlined => (None, on_surface, true),
        };
        let faded = |index| look::fade(on_surface, material_number(index));
        let (container, content) = match disabled {
            true => (
                container.map(|_| faded(metric::ICON_BUTTON_DISABLED_CONTAINER)),
                faded(metric::ICON_BUTTON_DISABLED),
            ),
            false => (container, content),
        };
        if let Some(fill) = pressed_background.mix(container) {
            self.backend.fill(shape, fill, ts);
        }
        if outline {
            // An `OutlinedIconButton`'s border is its content's colour
            // (`outlinedIconButtonBorder`).
            self.material_outline(shape, content, metric::ICON_BUTTON_OUTLINE, ts);
        }
        content
    }

    /// A Material border of the `width` row's dp: stroked inside `shape` and,
    /// as `Modifier.border` does, rounded up to whole pixels.
    pub(super) fn material_outline(
        &mut self,
        shape: &Shape,
        line: [u8; 4],
        width: usize,
        ts: Transform,
    ) {
        let width = (material_dp(width) * self.scale).ceil() / self.scale;
        let radii = shape.radii.map(|(r, _)| r);
        let d = super::segment::outline_path(shape.rect, radii, width / 2.0);
        let path = exact_kernel::svg::parse_d(&d);
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
    }
}
