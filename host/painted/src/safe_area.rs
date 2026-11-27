//! @ref LLP 1123 D13, D14 — what the painted hosts' executors hold of the safe area, per node. A box's reach grows
//! its surface (its background, border and the clip it puts on its children) and the frame that takes its hits past
//! its laid-out border box, its layout, its children and what reads its geometry staying where they are. A scroller's
//! claim is its reach, which grows its scrollport and the frame that takes its hits while its decoration keeps its box,
//! and D14's content insets, which widen its scroll range; CSS `scrollTop` and `scrollLeft` stay the offsets the
//! presenter keeps. Tests set them; activation (stage 5) sets them from the kernel's `Kernel::box_reach` and
//! `Kernel::scroll_terms` after each layout.
use exact_kernel::{Kernel, NodeKey, ViewId};
use std::collections::HashMap;

/// A scroller's claim (D13, D14): how far its scrollport reaches past its laid-out box and its content insets, each
/// top right bottom left in points. Its displacement and range follow (`ScrollClaim::displacement`, `ScrollBounds`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct ScrollClaim {
    pub(crate) reach: [f32; 4],
    pub(crate) insets: [f32; 4],
}

impl ScrollClaim {
    /// How far the scroller's content is moved at CSS offset `(left, top)`: the offset, plus the reach on the leading
    /// edge, which moves the scrollport's origin out, less the content inset there, which moves the content's start in.
    pub(crate) fn displacement(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let ([top, _, _, left], [inset_top, _, _, inset_left]) = (self.reach, self.insets);
        (x + left - inset_left, y + top - inset_top)
    }

    /// The CSS offset whose displacement is `(x, y)`.
    pub(crate) fn offset(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let ([top, _, _, left], [inset_top, _, _, inset_left]) = (self.reach, self.insets);
        (x - left + inset_left, y - top + inset_top)
    }

    /// How much longer the range is on each axis, x then y: the insets on its two edges less the reach there.
    pub(crate) fn range_change(&self) -> (f32, f32) {
        let ([t, r, b, l], [it, ir, ib, il]) = (self.reach, self.insets);
        (il + ir - l - r, it + ib - t - b)
    }
}

/// Each reaching box's reach and each scroller's claim, by its node.
#[derive(Debug, Default)]
pub(crate) struct SafeArea {
    reach: HashMap<NodeKey, [f32; 4]>,
    scrollers: HashMap<NodeKey, ScrollClaim>,
    extension: HashMap<NodeKey, [f32; 4]>,
    /// Moves on with each change, so a still frame, which nothing else of this moves, is no longer still.
    generation: u64,
}

impl SafeArea {
    /// How far the box `key` reaches past its border box on each edge; 0 on each for one that does not.
    pub(crate) fn reach(&self, key: NodeKey) -> [f32; 4] {
        self.reach.get(&key).copied().unwrap_or_default()
    }

    /// Set or clear (`None`, or 0 on each edge) the box `key`'s reach; whether it changed. Each call drops the reach
    /// of every box no longer live, which a node's reach goes with.
    pub(crate) fn set_reach(
        &mut self,
        kernel: &Kernel,
        key: NodeKey,
        reach: Option<[f32; 4]>,
    ) -> bool {
        self.reach.retain(|k, _| kernel.node_by_key(*k).is_some());
        let reach =
            reach.filter(|r| *r != [0.0; 4] && r.iter().all(|v| v.is_finite() && *v >= 0.0));
        let changed = match reach {
            Some(r) => self.reach.insert(key, r) != Some(r),
            None => self.reach.remove(&key).is_some(),
        };
        self.generation += u64::from(changed);
        changed
    }

    /// The box `key`'s reach, its claim for a claimed scroller, and its colour-only extension.
    pub(crate) fn of(&self, key: NodeKey) -> ([f32; 4], Option<ScrollClaim>, [f32; 4]) {
        let extension = self.extension.get(&key).copied().unwrap_or_default();
        (self.reach(key), self.scroll_claim(key), extension)
    }

    /// Set or clear (`None`, or 0 on each edge) how far the box `key`'s colour-only extension reaches past its border
    /// box; whether it changed. As `set_reach`, each call drops those of boxes no longer live.
    pub(crate) fn set_extension(
        &mut self,
        kernel: &Kernel,
        key: NodeKey,
        extension: Option<[f32; 4]>,
    ) -> bool {
        self.extension
            .retain(|k, _| kernel.node_by_key(*k).is_some());
        let extension =
            extension.filter(|e| *e != [0.0; 4] && e.iter().all(|v| v.is_finite() && *v >= 0.0));
        let changed = match extension {
            Some(e) => self.extension.insert(key, e) != Some(e),
            None => self.extension.remove(&key).is_some(),
        };
        self.generation += u64::from(changed);
        changed
    }

    /// The scroller `key`'s claim, while it has one.
    pub(crate) fn scroll_claim(&self, key: NodeKey) -> Option<ScrollClaim> {
        self.scrollers.get(&key).copied()
    }

    /// Each claimed scroller and its claim.
    pub(crate) fn scroll_claims(&self) -> impl Iterator<Item = (NodeKey, ScrollClaim)> + '_ {
        self.scrollers.iter().map(|(k, c)| (*k, *c))
    }

    /// Set or release (`None`) the scroller `key`'s claim; whether it changed. As `set_reach`, each call drops the
    /// claims of scrollers no longer live.
    pub(crate) fn set_scroll_claim(
        &mut self,
        kernel: &Kernel,
        key: NodeKey,
        claim: Option<ScrollClaim>,
    ) -> bool {
        self.scrollers
            .retain(|k, _| kernel.node_by_key(*k).is_some());
        let finite = |c: &ScrollClaim| c.reach.iter().chain(&c.insets).all(|v| v.is_finite());
        let claim = claim.filter(|c| finite(c) && c.reach.iter().all(|v| *v >= 0.0));
        let changed = match claim {
            Some(c) => self.scrollers.insert(key, c) != Some(c),
            None => self.scrollers.remove(&key).is_some(),
        };
        self.generation += u64::from(changed);
        changed
    }

    /// Drop the reach and the claims of the views `renewed`, which a commit made new (LLP 1078).
    pub(crate) fn renewed(&mut self, kernel: &Kernel, renewed: &[ViewId]) {
        for key in renewed
            .iter()
            .filter_map(|id| kernel.node(*id))
            .map(|n| n.key)
        {
            let dropped = self.reach.remove(&key).is_some()
                | self.scrollers.remove(&key).is_some()
                | self.extension.remove(&key).is_some();
            self.generation += u64::from(dropped);
        }
    }

    /// What a still frame keys this on.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}
