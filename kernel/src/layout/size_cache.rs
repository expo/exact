//! Size-cache comparisons and work counts (LLP 1165 D4).

pub use taffy::tree::{SizeCacheCounts, SizeCachePolicy};

#[cfg(test)]
mod cost_probe;
#[cfg(test)]
mod shapes_tests;

impl super::LayoutTree {
    pub(crate) fn reset_size_cache_counts(&mut self) {
        self.taffy.reset_size_cache_counts();
    }

    pub(crate) fn set_size_cache_policy(&mut self, policy: SizeCachePolicy) {
        self.taffy.set_size_cache_policy(policy);
        self.taffy.track_size_computations(true);
    }

    pub(crate) fn size_cache_history(&self) -> (usize, usize) {
        self.taffy.size_cache_history()
    }

    pub(crate) fn size_cache_counts(&self) -> SizeCacheCounts {
        self.taffy.size_cache_counts()
    }

    pub(crate) fn size_cache_occupancy(&self) -> (usize, usize) {
        self.taffy
            .size_cache_occupancy()
            .fold((0, 0), |(max, over), (_, n)| {
                (max.max(n), over + usize::from(n > 9))
            })
    }
}

#[cfg(test)]
pub(super) fn assert_same(a: &crate::Kernel, b: &crate::Kernel) {
    for slot in a.arena().iter_live() {
        let id = a.arena().local_id(slot);
        let other = b.arena().slot_of(id).unwrap();
        assert!(
            a.arena().frame(slot).bits_eq(b.arena().frame(other)),
            "frame {id}: {:?} != {:?}",
            a.arena().frame(slot),
            b.arena().frame(other)
        );
        let bits = |(w, h): (f32, f32)| (w.to_bits(), h.to_bits());
        assert_eq!(
            bits(a.arena().content(slot)),
            bits(b.arena().content(other)),
            "overflow {id}"
        );
    }
    assert!(a.layout_flow_matches(b), "flow results differ");
}

impl super::LayoutTree {
    pub(crate) fn height_proof_losses(
        &self,
        arena: &crate::arena::NodeArena,
        other: &Self,
        other_arena: &crate::arena::NodeArena,
    ) -> usize {
        arena
            .iter_live()
            .filter(|&slot| {
                let Some(other_slot) = other_arena.slot_of(arena.local_id(slot)) else {
                    return false;
                };
                match (arena.taffy(slot), other_arena.taffy(other_slot)) {
                    (Some(a), Some(b)) => !self.height_measured(a) && other.height_measured(b),
                    _ => false,
                }
            })
            .count()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Kernel, Offer};

    #[test]
    fn a_layout_counts_recomputed_sizes_and_resets_its_counts() {
        let mut kernel = Kernel::with_monospace();
        kernel
            .apply(
                0,
                1,
                &cost_probe::chain(
                    "RC",
                    8,
                    "A wrapping text with several words at the bottom of the chain.",
                ),
            )
            .unwrap();
        kernel.set_size_cache_policy(SizeCachePolicy::Nine);
        kernel
            .compute_layout(1, Offer::definite(100.0, 200.0))
            .unwrap();
        let counts = kernel.size_cache_counts();
        assert_eq!(counts.max_answers, 9);
        assert!(counts.repeated > 0);
        assert!(counts.computed > counts.repeated);
        kernel
            .compute_layout(1, Offer::definite(100.0, 200.0))
            .unwrap();
        assert_eq!(kernel.size_cache_counts().computed, 0);
    }
    #[test]
    fn counts_include_the_page_and_its_host_block() {
        use super::cost_probe::{chain, children, text, view};
        use crate::{BlockAxis, HostBlock, MonospaceMeasurer};
        let mut ops = chain("RC", 8, "Several wrapping words at the bottom of the page.");
        ops.push(view(100));
        ops.extend(text(101, "Host block text."));
        ops.extend([children(100, vec![101]), children(1, vec![2, 100])]);
        let mut together = Kernel::with_monospace();
        together.apply(0, 1, &ops).unwrap();
        together
            .set_host_block(
                100,
                Some(HostBlock {
                    width: BlockAxis::Is(80.0),
                    height: BlockAxis::Is(60.0),
                }),
            )
            .unwrap();
        together.set_size_cache_policy(SizeCachePolicy::Nine);
        let mut arena = together.arena().clone();
        let mut tree = together.tree().clone();
        let mut measurer = MonospaceMeasurer::default();
        let page_slot = arena.slot_of(1).unwrap();
        let block_slot = arena.slot_of(100).unwrap();
        super::super::compute(
            &mut arena,
            &mut tree,
            &mut measurer,
            page_slot,
            Offer::definite(100.0, 200.0),
        )
        .unwrap();
        let page = tree.size_cache_counts();
        tree.reset_size_cache_counts();
        super::super::compute(
            &mut arena,
            &mut tree,
            &mut measurer,
            block_slot,
            Offer::definite(80.0, 60.0),
        )
        .unwrap();
        let block = tree.size_cache_counts();
        together
            .compute_layout(1, Offer::definite(100.0, 200.0))
            .unwrap();
        let total = together.size_cache_counts();
        assert!(page.repeated > 0 && block.passes > 0);
        assert_eq!(total.computed, page.computed + block.computed);
        assert_eq!(total.repeated, page.repeated + block.repeated);
        assert_eq!(total.passes, page.passes + block.passes);
        assert_eq!(total.max_answers, page.max_answers.max(block.max_answers));
        together
            .compute_layout(1, Offer::definite(100.0, 200.0))
            .unwrap();
        assert_eq!(together.size_cache_counts().computed, 0);
    }

    #[test]
    fn height_proof_tallies_count_leaves_in_both_directions() {
        use super::cost_probe::{children, text, view};
        let mut ops = vec![view(1)];
        ops.extend(text(2, "First text."));
        ops.extend(text(3, "Second text."));
        ops.extend([children(1, vec![2, 3]), crate::Op::AttachRoot { id: 1 }]);
        let mut measured = Kernel::with_monospace();
        measured.apply(0, 1, &ops).unwrap();
        let unmeasured = measured.rehydrate(Box::new(crate::MonospaceMeasurer::default()));
        measured
            .compute_layout(1, Offer::definite(200.0, 400.0))
            .unwrap();
        assert_eq!(unmeasured.layout_height_proof_losses(&measured), 2);
        assert_eq!(measured.layout_height_proof_losses(&unmeasured), 0);
    }

    #[test]
    fn small_chains_agree_under_each_size_cache_policy() {
        for pattern in ["rc", "RC", "Bc", "wc", "B", "gc"] {
            for depth in [4, 8] {
                let ops = cost_probe::chain(
                    pattern,
                    depth,
                    "A wrapping text with several words at the bottom of the chain.",
                );
                let mut kernels = [
                    SizeCachePolicy::All,
                    SizeCachePolicy::Nine,
                    SizeCachePolicy::Off,
                ]
                .map(|policy| {
                    let mut k = Kernel::with_monospace();
                    k.apply(0, 1, &ops).unwrap();
                    k.set_size_cache_policy(policy);
                    k
                });
                for (round, width) in [390.0, 430.0, 390.0].into_iter().enumerate() {
                    for k in &mut kernels {
                        k.compute_layout(1, Offer::definite(width, 844.0)).unwrap();
                    }
                    super::assert_same(&kernels[0], &kernels[2]);
                    if pattern == "Bc" && depth == 8 && round == 0 {
                        let frame = kernels[2]
                            .arena()
                            .frame(kernels[2].arena().slot_of(9).unwrap());
                        assert_eq!(frame.width.to_bits(), 351.6001f32.to_bits());
                        assert_eq!(frame.height.to_bits(), 38.4f32.to_bits());
                        eprintln!(
                            "cold baseline All/Nine/Off: {:?}",
                            kernels
                                .iter()
                                .map(|k| k.arena().frame(k.arena().slot_of(9).unwrap()))
                                .collect::<Vec<_>>()
                        );
                    }
                    super::assert_same(&kernels[0], &kernels[1]);
                    assert_eq!(kernels[0].size_cache_counts().repeated, 0);
                    assert!(kernels[0].size_cache_counts().max_answers <= 64);
                }
            }
        }
    }
}
