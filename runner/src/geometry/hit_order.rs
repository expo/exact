//! The order a point meets a box's children.

use exact_kernel::{BlockShown, Kernel, PropId, ViewId};
use std::collections::HashMap;

/// `slot`'s children as a point meets them, front first: by paint rank, then
/// document order, except that a route its host shows beside the stack's top
/// comes just in front of the stack's last route, whose view spans the
/// route's column on iOS 26 (a split's list over its detail, LLP 1127 D8). A
/// child after the routes, such as an overlay, stays in front of both, and so
/// does one after a later block the host places that is no route (a popover).
pub(super) fn hit_order(kernel: &Kernel, ranks: &HashMap<ViewId, i64>, slot: u32) -> Vec<u32> {
    let arena = kernel.arena();
    let children = arena.children(slot);
    let last_route = children
        .iter()
        .rposition(|&c| arena.props(c).str(PropId::NavigationKey).is_some());
    let mut order: Vec<(i64, usize, bool, u32)> = children
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let view = arena.local_id(c);
            let beside = kernel.host_block_shown(view) == BlockShown::Beside;
            let at = if beside { last_route.unwrap_or(i) } else { i };
            (ranks.get(&view).copied().unwrap_or(0), at, beside, c)
        })
        .collect();
    order.sort_unstable();
    order.into_iter().rev().map(|(.., c)| c).collect()
}
