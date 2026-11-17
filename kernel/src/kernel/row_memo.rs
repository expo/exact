//! A list's rows laid out by replaying rows like them (Taffy patch 29).

use super::Kernel;
use crate::layout::LayoutMirror;

impl Kernel {
    /// Lay out each list row by replaying a row like it where one was laid
    /// out before (the default), or by its own algorithm every time. The
    /// frames are the same either way; a host turns it off to compare.
    pub fn set_row_layout_memo(&mut self, on: bool) {
        if let Some(tree) = self.layout.as_deref_mut().and_then(LayoutMirror::tree) {
            tree.set_row_memo(on);
        }
    }

    /// List rows laid out by a replay, and those computed and recorded,
    /// since the engine tree was made.
    pub fn row_layout_memo_counts(&self) -> (usize, usize) {
        self.layout
            .as_deref()
            .and_then(LayoutMirror::tree_ref)
            .map_or((0, 0), |tree| tree.row_memo_counts())
    }
}

use crate::layout::size_cache::{SizeCacheCounts, SizeCachePolicy};

impl Kernel {
    /// Select the engine's size cache policy for a comparison.
    pub fn set_size_cache_policy(&mut self, policy: SizeCachePolicy) {
        if let Some(tree) = self.layout.as_deref_mut().and_then(LayoutMirror::tree) {
            tree.set_size_cache_policy(policy);
        }
    }

    /// Size computations summed over the engine calls of the last layout.
    pub fn size_cache_counts(&self) -> SizeCacheCounts {
        self.layout
            .as_deref()
            .and_then(LayoutMirror::tree_ref)
            .map_or_else(SizeCacheCounts::default, |tree| tree.size_cache_counts())
    }

    /// Compare flow shapes and refusals, alongside frames and scroll extents.
    pub fn layout_flow_matches(&self, other: &Self) -> bool {
        self.arena().iter_live().all(|slot| {
            other
                .arena()
                .slot_of(self.arena().local_id(slot))
                .is_some_and(|b| {
                    crate::flow::shapes_eq(
                        self.arena().flow_shapes(slot),
                        other.arena().flow_shapes(b),
                    ) && self.arena().flow_refusal(slot) == other.arena().flow_refusal(b)
                })
        })
    }

    /// Number of leaves lacking a height proof that the reference layout holds.
    pub fn layout_height_proof_losses(&self, other: &Self) -> usize {
        match (
            self.layout.as_deref().and_then(LayoutMirror::tree_ref),
            other.layout.as_deref().and_then(LayoutMirror::tree_ref),
        ) {
            (Some(a), Some(b)) => a.height_proof_losses(self.arena(), b, other.arena()),
            _ => 0,
        }
    }

    /// Peak answers and distinct nodes holding more than nine since tracking began.
    pub fn size_cache_history(&self) -> (usize, usize) {
        self.layout
            .as_deref()
            .and_then(LayoutMirror::tree_ref)
            .map_or((0, 0), |tree| tree.size_cache_history())
    }

    /// Most answers held by one live node, and nodes holding more than nine.
    pub fn size_cache_occupancy(&self) -> (usize, usize) {
        self.layout
            .as_deref()
            .and_then(LayoutMirror::tree_ref)
            .map_or((0, 0), |tree| tree.size_cache_occupancy())
    }
}
