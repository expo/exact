//! CSS `order` (feed F19): a flex or grid container lays its items out in
//! order-modified document order — ascending `order`, ties in document order
//! (CSS Flexbox §5.4.1, CSS Grid §8.1). The kernel hands the layout engine a
//! container's children in that order; the arena keeps document order, which
//! painting and focus follow (LLP 1001 §2 declares the paint half).

use crate::arena::NodeArena;
use crate::{Display, StyleId};
use taffy::prelude::NodeId;

/// `parent`'s children's engine handles in the order its layout takes them;
/// `before` is generated content before its authored children;
/// `handle` is a child slot's handle in that engine tree.
pub(super) fn laid_out(
    arena: &NodeArena,
    parent: u32,
    before: Option<NodeId>,
    handle: impl Fn(u32) -> Option<NodeId>,
) -> Vec<NodeId> {
    // A child a host gave a containing block is out of this layout, the
    // root of its own (LLP 1115.002 D8.2).
    let items = before
        .into_iter()
        .map(|node| (0, node))
        .chain(
            arena
                .children(parent)
                .iter()
                .filter(|&&c| !arena.blocks.holds(c))
                .filter_map(|&c| Some((arena.style(c).order, handle(c)?))),
        )
        .collect();
    ordered(arena, parent, items)
}

/// `items`, each child's `order` and engine handle in document order, in
/// the order `parent`'s layout takes them.
pub(crate) fn ordered(
    arena: &NodeArena,
    parent: u32,
    mut items: Vec<(i32, NodeId)>,
) -> Vec<NodeId> {
    // A newly mounted parent's engine style may not have been derived yet.
    let orders = matches!(
        arena.computed_source(parent, StyleId::Display).display,
        Display::Flex | Display::Grid
    );
    if orders && items.iter().any(|(order, _)| *order != 0) {
        // A stable sort: equal `order`s keep document order.
        items.sort_by_key(|(order, _)| *order);
    }
    items.into_iter().map(|(_, n)| n).collect()
}
