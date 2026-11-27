//! Authored menu container overrides and WCAG-legible ink resolution for
//! Android's projected Material 3 dropdown menus (LLP 1115.001 D10, #1956).
use super::Role;
use crate::paint::look::{ink_on, luminance};
use crate::presenter::bars::look::ink::over;
use crate::presenter::top::shape::MenuSurface;

const TEXT_CONTRAST: f32 = 4.5;
const SYMBOL_CONTRAST: f32 = 3.0;
const RULE_CONTRAST: f32 = 1.2;

fn contrast_ratio(ink: [u8; 4], ground: [u8; 4]) -> f32 {
    let fg = luminance(over(ink, ground)) + 0.05;
    let bg = luminance(ground) + 0.05;
    fg.max(bg) / fg.min(bg)
}

fn readable(
    candidate: [u8; 4],
    ground: [u8; 4],
    lit: [u8; 4],
    minimum: f32,
    fallback: [u8; 4],
) -> [u8; 4] {
    if contrast_ratio(candidate, ground) >= minimum && contrast_ratio(candidate, lit) >= minimum {
        candidate
    } else {
        fallback
    }
}

/// Resolved container and row inks for a projected menu.
pub(super) struct MenuPalette {
    pub container: [u8; 4],
    pub on_surface: [u8; 4],
    pub on_surface_variant: [u8; 4],
    pub icon_variant: [u8; 4],
    pub error: [u8; 4],
    pub error_icon: [u8; 4],
    pub disabled: [u8; 4],
    pub rule: [u8; 4],
    pub state_layer: [u8; 4],
}

impl MenuPalette {
    /// Resolve Material or WinUI menu colors, honoring an explicitly written
    /// Android `background-color` composited opaquely over `SurfaceContainer`.
    pub(super) fn resolve(surface: Option<&MenuSurface>, dark: bool, desktop: bool) -> Self {
        let default = |scheme: bool| Self {
            container: if desktop {
                Role::Surface.color(scheme)
            } else {
                Role::SurfaceContainer.color(scheme)
            },
            on_surface: Role::OnSurface.color(scheme),
            on_surface_variant: Role::OnSurfaceVariant.color(scheme),
            icon_variant: Role::OnSurfaceVariant.color(scheme),
            error: Role::Error.color(scheme),
            error_icon: Role::Error.color(scheme),
            disabled: Role::Disabled.color(scheme),
            rule: Role::OutlineVariant.color(scheme),
            state_layer: Role::OnSurface.color(scheme),
        };
        if desktop {
            return default(dark);
        }
        let Some(surface) = surface else {
            return default(dark);
        };
        let Some(bg) = surface.background else {
            return default(dark);
        };
        let paint = {
            let _accent = exact_kernel::style::roles::authored_accent(surface.accent);
            crate::paint::rgba(bg.resolve(dark))
        };
        if paint[3] == 0 {
            return default(dark);
        }
        let base = Role::SurfaceContainer.color(dark);
        let container = if paint[3] == 255 {
            paint
        } else {
            over(paint, base)
        };
        let plain = ink_on(container);
        let ground_dark = plain[0] == 255;
        let layer_alpha = (255.0 * super::STATE_LAYER).round() as u8;
        let same_layer = [plain[0], plain[1], plain[2], layer_alpha];
        let state_layer = if contrast_ratio(plain, over(same_layer, container)) >= TEXT_CONTRAST {
            plain
        } else {
            [255 - plain[0], 255 - plain[1], 255 - plain[2], 255]
        };
        let lit = over(
            [state_layer[0], state_layer[1], state_layer[2], layer_alpha],
            container,
        );
        let on_surface = readable(
            Role::OnSurface.color(ground_dark),
            container,
            lit,
            TEXT_CONTRAST,
            plain,
        );
        let on_surface_variant = readable(
            Role::OnSurfaceVariant.color(ground_dark),
            container,
            lit,
            TEXT_CONTRAST,
            plain,
        );
        let icon_variant = readable(
            Role::OnSurfaceVariant.color(ground_dark),
            container,
            lit,
            SYMBOL_CONTRAST,
            plain,
        );
        let err_same = Role::Error.color(ground_dark);
        let err_other = Role::Error.color(!ground_dark);
        let error = readable(
            err_same,
            container,
            lit,
            TEXT_CONTRAST,
            readable(err_other, container, lit, TEXT_CONTRAST, plain),
        );
        let error_icon = readable(
            err_same,
            container,
            lit,
            SYMBOL_CONTRAST,
            readable(err_other, container, lit, SYMBOL_CONTRAST, plain),
        );
        let disabled =
            if contrast_ratio(Role::OnSurface.color(ground_dark), container) >= TEXT_CONTRAST {
                Role::Disabled.color(ground_dark)
            } else {
                [plain[0], plain[1], plain[2], 97]
            };
        let rule = if contrast_ratio(Role::OutlineVariant.color(ground_dark), container)
            >= RULE_CONTRAST
        {
            Role::OutlineVariant.color(ground_dark)
        } else {
            [plain[0], plain[1], plain[2], 38]
        };
        Self {
            container,
            on_surface,
            on_surface_variant,
            icon_variant,
            error,
            error_icon,
            disabled,
            rule,
            state_layer,
        }
    }
}

impl super::Layout {
    /// Whether `other` has the same outer size and interactive target boxes,
    /// so a paint-only re-measure can keep scroll offset and active input state.
    pub(in crate::presenter) fn same_targets(&self, other: &Self) -> bool {
        self.size == other.size
            && self.targets.len() == other.targets.len()
            && (self.targets.iter().zip(&other.targets))
                .all(|(a, b)| a.node == b.node && a.rect == b.rect && a.enabled == b.enabled)
    }
}

/// Resolve the menu container corner radius from an authored uniform circular
/// `border-radius`, clamping to half the final shorter dimension. Returns
/// `(radius, non_uniform_ignored)`.
pub(super) fn menu_radius(
    surface: Option<&MenuSurface>,
    width: f32,
    height: f32,
    desktop: bool,
) -> (f32, bool) {
    if desktop {
        return (8.0, false);
    }
    let Some((radii, env)) = surface.and_then(|s| s.radii.as_ref()) else {
        return (4.0, false);
    };
    let corners = crate::paint::border::resolve_radii(radii.clone(), env, width, height);
    let first = corners[0].0;
    let uniform = first.is_finite()
        && first >= 0.0
        && corners.iter().all(|&(rx, ry)| {
            rx.is_finite()
                && ry.is_finite()
                && rx >= 0.0
                && ry >= 0.0
                && (rx - ry).abs() < 0.01
                && (rx - first).abs() < 0.01
        });
    if !uniform {
        return (4.0, true);
    }
    let max_radius = (width.min(height) / 2.0).max(0.0);
    (first.clamp(0.0, max_radius), false)
}
