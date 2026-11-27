//! Whether a node shows: its computed `visibility`, or in a reordering
//! ghost, what the ghost reveals.
use exact_kernel::{Kernel, ViewId};

/// Whether `id` paints, is hit, and is exposed. In a ghost, [`revealed`];
/// otherwise the computed `visibility` (inherited). A missing node does not
/// veto a caller that already has nothing to draw.
pub(crate) fn paints(kernel: &Kernel, id: ViewId, reveal: Option<ViewId>) -> bool {
    match reveal {
        Some(root) => revealed(kernel, id, root),
        None => kernel.node(id).is_none_or(|n| {
            n.computed_row(exact_kernel::StyleId::Visibility, |s| s.visibility)
                == exact_kernel::Visibility::Visible
        }),
    }
}

/// Whether `id` shows in the ghost of `root`: the nearest `visibility` set
/// on it or an ancestor below the ghost's root decides, as in the web's
/// clone whose root alone is made visible (`group-glue.js`); none set shows.
pub(crate) fn revealed(kernel: &Kernel, id: ViewId, root: ViewId) -> bool {
    let arena = kernel.arena();
    let mut at = arena.key_of(id).map(|k| k.index);
    while let Some(slot) = at {
        if arena.local_id(slot) == root {
            return true;
        }
        let style = arena.style(slot);
        if style.mask.has(exact_kernel::StyleId::Visibility) {
            return style.visibility == exact_kernel::Visibility::Visible;
        }
        at = arena.parent(slot);
    }
    true
}
