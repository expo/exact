//! Exact's own writes to an owner's controller and what they take out of
//! its band (@ref LLP 1123 D11, "Provenance").
//!
//! The host applies each of Exact's contributions to an owner's controller
//! (its additional insets, safe-area padding, bar reservations, keyboard
//! insets) with an identity, and records its marginal effect: the owner
//! view's insets with the contribution less without it. The owner's
//! incoming band, which its boxes observe, is the raw band less the sum of
//! the effects, so a reservation that UIKit echoes into the raw band
//! changes nothing Exact lays out with. A write to an outer owner's
//! controller is incoming to a nested owner, as an app's write is.
//!
//! An update is the owner's current snapshot, since the host rejects a
//! stale native callback before it publishes (D9). Generations do not tell
//! a stale update from a current one, since an unchanged write keeps its
//! generation when its effect is remeasured, so the kernel checks only
//! what one update shows.

use crate::error::{KernelError, LayoutError};
use crate::id::ViewId;

/// One of Exact's writes to an owner's controller, as the owner update
/// reports it (D11). Its native target is that controller, and the host
/// keeps its generation, so it carries neither.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contribution {
    /// The node whose declaration the write carries.
    pub node: ViewId,
    /// What it writes.
    pub operation: Operation,
    /// The owner view's insets with the contribution less without it, both
    /// with the contributions recorded before it applied, top right bottom
    /// left.
    pub effect: [f32; 4],
}

/// What a contribution writes; with the node and the owner, its identity,
/// so one node's several contributions stay apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// `-exact-additional-safe-area` on the owner's root.
    AdditionalInsets,
    /// `-exact-safe-area-padding` on the owner's root.
    SafeAreaPadding,
    /// A bar's reservation (D12).
    BarReservation,
    /// The keyboard's inset (D7).
    KeyboardInsets,
}

impl Operation {
    /// Every operation, in wire order (`OwnerUpdate::encode`).
    pub const ALL: [Operation; 4] = [
        Operation::AdditionalInsets,
        Operation::SafeAreaPadding,
        Operation::BarReservation,
        Operation::KeyboardInsets,
    ];
}

/// `raw` less the sum of `contributions`' effects, at least 0 on each edge
/// against rounding in the host's subtractions.
pub(crate) fn incoming(raw: [f32; 4], contributions: &[Contribution]) -> [f32; 4] {
    let mut band = raw;
    for c in contributions {
        for (edge, effect) in band.iter_mut().zip(c.effect) {
            *edge -= effect;
        }
    }
    band.map(|v| v.max(0.0))
}

/// Why `contributions` are refused, if they are: a nonfinite effect, or an
/// identity twice. A contribution whose node has gone is taken, since its
/// write stays in the raw band until the host removes it.
pub(crate) fn check(contributions: &[Contribution]) -> Result<(), KernelError> {
    let refused = |why| Err(LayoutError::Contribution(why).into());
    for (i, c) in contributions.iter().enumerate() {
        if !c.effect.iter().all(|v| v.is_finite()) {
            return refused("a contribution's effect is not finite; report the owner view's insets with it less without it");
        }
        let twice =
            (contributions[..i].iter()).any(|d| d.node == c.node && d.operation == c.operation);
        if twice {
            return refused("one node's operation is reported twice; report each contribution once, with its whole effect");
        }
    }
    Ok(())
}
