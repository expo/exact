//! The area a box observes and takes (LLP 1123 D9): the space it lies in,
//! the band its holder gives that space, and what of the band and of its
//! ancestors' reservations reaches it past the scrollers above it. The
//! readers' solve (`observe.rs`), what boxes that ignore take (`ignore.rs`)
//! and native targets (`targets.rs`) measure in it.
use super::block::reservation;
use super::ignore;
use super::Kernel;
use crate::arena::NodeArena;
use crate::{Overflow, StyleId};

/// The depth an edge `at` in from the space's edge observes of an area
/// whose sides lie `outer` and `inner` in: at least 0, at most the area's
/// depth.
pub(super) fn depth([outer, inner]: [f32; 2], at: f32) -> f32 {
    (inner - at).clamp(0.0, (inner - outer).max(0.0))
}

/// The unsafe area `slot` observes, on each edge how far in from the
/// space's edge its outer and inner sides lie (D9, D11): the band, from the
/// space's edge, or nothing below a scroller, which takes what reaches it;
/// then, on each edge where an ancestor at or below that reserves, from its
/// padding edge to its observation there plus its reservations, and from
/// further out where that edge lies in the area; an owner's root from the
/// allocation's edge, with the band as its observation. A box's own
/// reservations are not among them.
pub(super) fn area(arena: &NodeArena, slot: u32, space: &Space, band: [f32; 4]) -> [[f32; 2]; 4] {
    let from = |open: [bool; 4]| -> [[f32; 2]; 4] {
        std::array::from_fn(|e| [0.0, if open[e] { band[e] } else { 0.0 }])
    };
    if arena.reserving.is_empty() {
        return from(open_edges(arena, slot, space, band));
    }
    let mut reserving = Vec::new();
    let mut open = [true; 4];
    let mut above = (space.root != Some(slot))
        .then(|| arena.parent(slot))
        .flatten();
    // What each box reserves this round, which `check_reservations` resolved
    let (observations, tolerance) = (&arena.observations, arena.observations.tolerance());
    while let Some(s) = above {
        let r =
            (observations.held_reservation(s)).unwrap_or_else(|| reservation::reserved(arena, s));
        if let Some(r) = r {
            reserving.push((s, r, open));
        }
        if scroll_view(arena, s) {
            open = scrolled(arena, s, space, band, open);
            if !open.contains(&true) {
                break;
            }
        }
        above = (space.root != Some(s)).then(|| arena.parent(s)).flatten();
    }
    let mut sides = from(open);
    for (s, r, open) in reserving.into_iter().rev() {
        let edges = match space.owner == Some(s) {
            true => [0.0; 4],
            false => space.padding_edges(arena, s),
        };
        let observed: [f32; 4] = std::array::from_fn(|e| depth(sides[e], edges[e]));
        let inside = r.inside(observed);
        for e in 0..4 {
            if open[e] && (r.added[e] != 0.0 || r.padding[e] != 0.0) {
                // A padding edge at or past the area's inner side joins it
                let outer = match edges[e] <= sides[e][1] + tolerance {
                    true => sides[e][0].min(edges[e]),
                    false => edges[e],
                };
                sides[e] = [outer, edges[e] + inside[e]];
            }
        }
    }
    sides
}

/// The band a box observes and where it lies.
pub(super) struct Space {
    /// The owner whose band it is, `None` for the session's.
    pub(super) owner: Option<u32>,
    /// The root of the box's own layout: its owner's, or a document
    /// top-layer box's; `None` for the document.
    pub(super) root: Option<u32>,
    /// Top right bottom left; `None` while it is unavailable.
    pub(super) band: Option<[f32; 4]>,
    /// The width and height of the space the band edges.
    pub(super) size: (f32, f32),
    /// What places the box's frames in that space: a top-layer box's
    /// translation into the viewport.
    pub(super) origin: (f32, f32),
}

impl Space {
    /// How far in from this space's edges each of `slot`'s edges lies, top
    /// right bottom left.
    pub(super) fn edges(&self, arena: &NodeArena, slot: u32) -> [f32; 4] {
        let ((width, height), f) = (self.size, arena.frame(slot));
        let (x, y) = (f.x + self.origin.0, f.y + self.origin.1);
        [y, width - x - f.width, height - y - f.height, x]
    }

    /// [`Self::edges`] for `slot`'s padding edges, inside its border.
    pub(super) fn padding_edges(&self, arena: &NodeArena, slot: u32) -> [f32; 4] {
        let edges = self.edges(arena, slot);
        // A published inset's sides are left, top, right, bottom
        let [left, top, right, bottom] = arena.inset(slot).unwrap_or_default().border;
        let border = [top, right, bottom, left];
        std::array::from_fn(|e| edges[e] + border[e])
    }
}

/// The band `slot` observes: an owner's, in its block's space; or, for a
/// box outside every owner, the session's on the viewport, which a
/// document top-layer box is (D2). A top-layer box whose block its host
/// gave (a painted `position-area` popover) is where the host shows it,
/// which the kernel is not told, so its band is unavailable.
pub(super) fn band_at(kernel: &Kernel, slot: u32) -> Space {
    let arena = &kernel.arena;
    let env = arena.env();
    let viewport = (env.viewport_width, env.viewport_height);
    let session = |root, origin| Space {
        owner: None,
        root,
        band: Some(arena.observations.session()),
        size: viewport,
        origin,
    };
    match holder(arena, slot) {
        Holder::TopLayer(s) => {
            let origin = kernel.top_layer_origin(arena.local_id(s), viewport);
            let Some((x, y)) = origin else {
                return Space {
                    band: None,
                    ..session(Some(s), (0.0, 0.0))
                };
            };
            let f = arena.frame(s);
            session(Some(s), (x - f.x, y - f.y))
        }
        Holder::Owner(s) => Space {
            owner: Some(s),
            root: Some(s),
            band: arena.observations.band(s),
            size: kernel.used_size(s),
            origin: (0.0, 0.0),
        },
        Holder::Session => session(None, (0.0, 0.0)),
    }
}

/// What holds a box for its safe area (D2): the owner whose block it is in,
/// a document top-layer box, or the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Holder {
    Owner(u32),
    TopLayer(u32),
    Session,
}

/// What holds `slot`: its nearest ancestor that holds its own ([`holds`]),
/// or the session.
pub(crate) fn holder(arena: &NodeArena, slot: u32) -> Holder {
    let mut at = Some(slot);
    while let Some(s) = at {
        if let Some(h) = holds(arena, s) {
            return h;
        }
        at = arena.parent(s);
    }
    Holder::Session
}

/// What `slot` holds its own subtree as, if it does: a document top-layer
/// box laid out on its own, or a box its host gave a block.
pub(crate) fn holds(arena: &NodeArena, slot: u32) -> Option<Holder> {
    // Check for a top-layer box first: it is no owner, whatever block it has (D2)
    if arena.top_layer_slots.contains(slot) && arena.blocks.holds(slot) {
        return Some(Holder::TopLayer(slot));
    }
    (arena.blocks.given(slot).is_some()).then_some(Holder::Owner(slot))
}

/// The edges, top right bottom left, on which the band reaches `slot` past
/// every scroller between it and its layout's root: a scroller takes the
/// band that reaches it on each edge it does not ignore (D14, D4).
pub(super) fn open_edges(arena: &NodeArena, slot: u32, space: &Space, band: [f32; 4]) -> [bool; 4] {
    let mut open = [true; 4];
    if space.root == Some(slot) {
        return open;
    }
    let mut at = arena.parent(slot);
    while let Some(s) = at {
        if scroll_view(arena, s) {
            open = scrolled(arena, s, space, band, open);
            if !open.contains(&true) {
                break;
            }
        }
        if space.root == Some(s) {
            break;
        }
        at = arena.parent(s);
    }
    open
}

/// `open` less the edges the scroller `slot` takes the area on (D9).
pub(super) fn scrolled(
    arena: &NodeArena,
    slot: u32,
    space: &Space,
    band: [f32; 4],
    open: [bool; 4],
) -> [bool; 4] {
    let passes = passed(arena, slot, space, band);
    std::array::from_fn(|e| open[e] && passes[e])
}

/// The edges on which the scroller `slot` passes the area to its content:
/// those it ignores where its box reaches the area that reaches it, so that
/// what scrolls past an edge clear of the area does not reach it.
pub(super) fn passed(arena: &NodeArena, slot: u32, space: &Space, band: [f32; 4]) -> [bool; 4] {
    let ignored = ignore::edges(arena, slot, crate::generated::SafeAreaIgnore::Container);
    if !ignored.contains(&true) {
        return [false; 4];
    }
    let (sides, edges) = (area(arena, slot, space, band), space.edges(arena, slot));
    let tolerance = arena.observations.tolerance();
    std::array::from_fn(|e| ignored[e] && edges[e] <= sides[e][1] + tolerance)
}

/// Whether the host presents `slot` as a scroll view: an axis whose
/// effective overflow is `auto` or `scroll` (CSS Overflow 3 §3.1), with
/// `auto` on the block axis of a node that scrolls by default and says no
/// `overflow-y`. A `hidden` box only clips, and UIKit passes the safe area
/// through a clipping view to its subviews.
pub(crate) fn scroll_view(arena: &NodeArena, slot: u32) -> bool {
    let s = arena.style(slot);
    let default = !s.mask.has(StyleId::OverflowY) && arena.node_type(slot).scrolls_by_default();
    let y = if default {
        Overflow::Auto
    } else {
        s.overflow_y
    };
    let scrolls = |o| matches!(o, Overflow::Auto | Overflow::Scroll);
    scrolls(s.overflow_x.computed(y)) || scrolls(y.computed(s.overflow_x))
}
