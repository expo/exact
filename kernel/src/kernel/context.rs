//! The inset rule: the host clears each unsafe band once, and a safe-area
//! value an author reads is what is left of it where the length is laid out.
//! A content owner's root is laid out in what the route clears, and a
//! scroller that touches a band has the host inset its content by it on the
//! axes it scrolls, as UIKit's adjustment does (`.scrollableAxes`). So
//! `env(safe-area-inset-<edge>)` reads the remainder of a box's context: the
//! document's insets outside every owner, an owner's own (what its root does
//! not clear) inside one, and 0 in a scroller's padding and content on each
//! edge of an axis it scrolls that it does not ignore. The local insets that
//! `var(--exact-safe-area-inset-<edge>)` reads are 0 there too
//! (`owner::local_at`). A scroller's other edges are its context's: inside an
//! owner the root has cleared them, and outside every owner nothing insets
//! them, so content pads by the band itself. Taking an edge
//! (`-exact-safe-area-ignore`) leaves it to the author, readings included.

use crate::arena::NodeArena;
use crate::style::Edge;
use crate::{Overflow, SafeAreaIgnore, StyleId};

/// What `env(safe-area-inset-*)` reads in `slot`'s lengths, top right bottom
/// left, or in its content where `content`: a scroller's padding lies inside
/// its content inset, with its content.
pub(crate) fn context_at(arena: &NodeArena, slot: u32, content: bool) -> [f32; 4] {
    let mut found = [None; 4];
    let mut at = Some(slot);
    while let Some(s) = at {
        if s != slot || content {
            let taken = taken(arena, s);
            for e in 0..4 {
                if taken[e] {
                    found[e] = found[e].or(Some(0.0));
                }
            }
        }
        // A content owner's root: what its host leaves its content
        if let Some(host) = arena.blocks.local(s) {
            return std::array::from_fn(|e| found[e].unwrap_or(host[e]));
        }
        // Stop at a top-layer box: it is laid out against the viewport and is no owner (`block::owns`)
        if arena.top_layer_slots.contains(s) {
            break;
        }
        at = arena.parent(s);
    }
    let env = arena.env();
    std::array::from_fn(|e| found[e].unwrap_or(env.inset(Edge::ALL[e])))
}

/// `insets` as `slot`'s content reads them: 0 on the edges it takes as a
/// scroller.
pub(crate) fn inside(arena: &NodeArena, slot: u32, insets: [f32; 4]) -> [f32; 4] {
    let taken = taken(arena, slot);
    std::array::from_fn(|e| if taken[e] { 0.0 } else { insets[e] })
}

/// The edges `slot`'s host insets its content on as a scroller, top right
/// bottom left: those of each axis its author scrolls that it does not
/// ignore. UIKit adjusts an axis whose content passes the bounds or that
/// bounces (`UIScrollView.contentInsetAdjustmentBehavior`), and the host
/// bounces the authored axes only, so `overflow-x` that computes to `auto`
/// beside `overflow-y: scroll` (CSS Overflow 3 §3.1) leaves the sides to the
/// content, as `area::scroll_axes` does not.
fn taken(arena: &NodeArena, slot: u32) -> [bool; 4] {
    let (vertical, horizontal) = scroll_axes(arena, slot);
    if !vertical && !horizontal {
        return [false; 4];
    }
    let ignored = super::ignore::edges(arena, slot, SafeAreaIgnore::Container);
    let off = route_off(arena, slot);
    let axes = [vertical, horizontal, vertical, horizontal];
    std::array::from_fn(|e| axes[e] && !ignored[e] && !off[e])
}

/// The edges the content owner `slot` lies in turns off, top right bottom
/// left: those its root's `-exact-safe-area` names or its root ignores, and
/// the top under its transparent bar. There the host takes UIKit's adjustment
/// of the route's scrollers back (`RouteSafeArea`), so a scroller's content
/// reads the owner's inset and clears the band itself.
fn route_off(arena: &NodeArena, slot: u32) -> [bool; 4] {
    let mut at = arena.parent(slot);
    while let Some(s) = at {
        if arena.blocks.local(s).is_some() {
            let bits = arena.style(s).rare.safe_area;
            let ignored = super::ignore::edges(arena, s, SafeAreaIgnore::Container);
            let transparent =
                arena.props(s).str(crate::PropId::NavigationBar) == Some("transparent");
            return std::array::from_fn(|e| {
                bits & (1 << e) != 0 || ignored[e] || (e == 0 && transparent)
            });
        }
        if arena.top_layer_slots.contains(s) {
            break;
        }
        at = arena.parent(s);
    }
    [false; 4]
}

/// Whether `slot`'s author scrolls it vertically and sideways.
pub(crate) fn scroll_axes(arena: &NodeArena, slot: u32) -> (bool, bool) {
    let s = arena.style(slot);
    let y = match !s.mask.has(StyleId::OverflowY) && arena.node_type(slot).scrolls_by_default() {
        true => Overflow::Auto,
        false => s.overflow_y,
    };
    let scrolls = |o: Overflow| matches!(o, Overflow::Auto | Overflow::Scroll);
    (scrolls(y), scrolls(s.overflow_x))
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
