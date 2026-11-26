//! @ref LLP 1123 D13 — what the painted hosts' executors hold of the safe area, per node: a box's reach, which grows
//! its surface (its background, border and the clip it puts on its children) and the frame that takes its hits past
//! its laid-out border box, its layout, its children and what reads its geometry staying where they are. Tests set it;
//! activation (stage 5) sets it from the kernel's `Kernel::box_reach` after each layout.
use exact_kernel::{Kernel, NodeKey, ViewId};
use std::collections::HashMap;

/// Each reaching box's reach, top right bottom left in points, by its node.
#[derive(Debug, Default)]
pub(crate) struct SafeArea {
    reach: HashMap<NodeKey, [f32; 4]>,
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

    /// Drop the reach of the views `renewed`, which a commit made new (LLP 1078).
    pub(crate) fn renewed(&mut self, kernel: &Kernel, renewed: &[ViewId]) {
        for key in renewed
            .iter()
            .filter_map(|id| kernel.node(*id))
            .map(|n| n.key)
        {
            self.generation += u64::from(self.reach.remove(&key).is_some());
        }
    }

    /// What a still frame keys this on.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}
