//! Reservations (LLP 1123 D11): what a box adds to the unsafe area inside
//! it, edge by edge, top right bottom left. A box lays its in-flow children
//! out clear of them, as with that much more padding; an owner's root has
//! its host clear them around its content root instead (D3); and the
//! descendants of either observe them (D9).
use crate::arena::NodeArena;
use crate::generated::{Rare, StyleProps};
use crate::style::{Dimension, Env};

/// What a box reserves on each edge: its additional insets, which may be
/// negative, and its safe-area padding, after them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Reserved {
    pub(crate) added: [f32; 4],
    pub(crate) padding: [f32; 4],
}

impl Reserved {
    /// The depth of the unsafe area inside a box on each edge, from its own
    /// edge, where it observes `observed` there: the additional inset taken
    /// from or added to the observation, clamped at 0, then the padding.
    pub(crate) fn inside(&self, observed: [f32; 4]) -> [f32; 4] {
        self.inside_covered(observed, [0.0; 4])
    }

    /// [`Self::inside`] where a keyboard covers `cover` of each edge (D3,
    /// D7): it covers the observation and the additional inset where it is
    /// deeper, as UIKit's keyboard guide covers a controller's
    /// `additionalSafeAreaInsets`, and the padding stacks on it.
    pub(crate) fn inside_covered(&self, observed: [f32; 4], cover: [f32; 4]) -> [f32; 4] {
        std::array::from_fn(|e| {
            (observed[e] + self.added[e]).max(0.0).max(cover[e]) + self.padding[e]
        })
    }
}

/// What `slot` reserves, or `None` where it reserves nothing.
pub(crate) fn reserved(arena: &NodeArena, slot: u32) -> Option<Reserved> {
    if !arena.reserving.contains(slot) {
        return None;
    }
    // A reservation row has no structural default, so the box's own style
    // is the one it takes
    let style = arena.style(slot);
    resolve(style, &crate::kernel::owner::env_for(arena, slot, style))
}

/// What `style` reserves with its lengths resolved against `env`, or `None`
/// where it reserves nothing.
fn resolve(style: &StyleProps, env: &Env) -> Option<Reserved> {
    if !reserves(style) {
        return None;
    }
    let (added, padding) = (added(&style.rare), padding(&style.rare));
    let default = env.default_padding.unwrap_or([0.0; 4]);
    Some(Reserved {
        added: added.map(|d| d.at_basis(env, 0.0)),
        // An authored `auto` is the platform's default; a calculation that
        // is invalid where it is read is 0, as CSS's initial value
        padding: std::array::from_fn(|e| match padding[e] {
            Dimension::Auto => default[e],
            d => d.at_basis(env, 0.0).max(0.0),
        }),
    })
}

/// Lay `slot`'s in-flow children out clear of what it reserves: on each
/// edge its positive addition and then its safe-area padding, inside its
/// border and outside its padding, as UIKit measures a view's layout
/// margins from its safe area. An owner's root lays out none of it, since
/// its host clears it around the content root (D3). Returns what it added
/// on each edge, top right bottom left, past which its bars lie (D12).
pub(crate) fn lay_out(
    arena: &NodeArena,
    slot: u32,
    style: &StyleProps,
    env: &Env,
    s: &mut taffy::Style,
) -> [f32; 4] {
    if !arena.reserving.contains(slot) || super::owns(arena, slot) {
        return [0.0; 4];
    }
    let Some(r) = resolve(style, env) else {
        return [0.0; 4];
    };
    let added = r.inside([0.0; 4]).map(|reserved| reserved.max(0.0));
    let p = &mut s.padding;
    for (side, reserved) in [&mut p.top, &mut p.right, &mut p.bottom, &mut p.left]
        .into_iter()
        .zip(added)
    {
        if reserved > 0.0 {
            *side = crate::style::plus(*side, reserved);
        }
    }
    added
}

/// Whether `style` reserves on some edge.
pub(crate) fn reserves(style: &StyleProps) -> bool {
    let none = Dimension::Points(0.0);
    let rare = &style.rare;
    (added(rare).iter().chain(&padding(rare))).any(|d| **d != none)
}

/// Whether `style` pads by the environment's default on some edge.
pub(crate) fn pads_by_default(style: &StyleProps) -> bool {
    (padding(&style.rare).iter()).any(|d| **d == Dimension::Auto)
}

fn added(rare: &Rare) -> [&Dimension; 4] {
    [
        &rare.additional_safe_area_top,
        &rare.additional_safe_area_right,
        &rare.additional_safe_area_bottom,
        &rare.additional_safe_area_left,
    ]
}

fn padding(rare: &Rare) -> [&Dimension; 4] {
    [
        &rare.safe_area_padding_top,
        &rare.safe_area_padding_right,
        &rare.safe_area_padding_bottom,
        &rare.safe_area_padding_left,
    ]
}
