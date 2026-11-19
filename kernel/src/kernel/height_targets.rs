//! Measuring the heights an automatic-height transition animates between,
//! without publishing a layout (`measure_height_targets`).
use super::*;

impl Kernel {
    /// Measure current authored border-box heights without publishing layout.
    /// Frames, overflow, flags, authored styles, receipts and epoch remain
    /// unchanged, but for what the next layout at `offer` would set first:
    /// the viewport, and each popover's block in it (`imply_blocks`). Where
    /// readers or boxes that ignore the safe area exist, the heights are
    /// measured in a layout of copies with their solve (LLP 1123 D9), so they
    /// are what the next layout gives, and the kernel spends only text
    /// revisions. Otherwise one unpublished pass measures them: it removes
    /// active presentations and restores them before returning, including
    /// after failure, and gives each block its clearance as the next layout
    /// would, with the restyle and whole publication a changed one owes
    /// (`clear_block`). Results are used border-box sizes after constraints,
    /// not unconstrained CSS intrinsic sizes; a target that extends past its
    /// place measures without what it takes, which publication adds.
    ///
    /// Call when a target's content or offer changes, not on every motion tick.
    /// This does not register owners or define an automatic `height: auto`
    /// transition policy. The caller still chooses which targets to animate.
    pub fn measure_height_targets(
        &mut self,
        root: ViewId,
        offer: Offer,
        owners: &[NodeKey],
    ) -> Result<Vec<PresentedHeight>, KernelError> {
        self.mirror();
        let slot = self.layout_root_slot(root, offer)?;
        for (i, owner) in owners.iter().enumerate() {
            self.validate_presented_height(
                slot,
                PresentedHeight {
                    node: *owner,
                    epoch: self.epoch,
                    px: 0.0,
                },
            )?;
            if self.arena.style(owner.index).box_sizing != BoxSizing::BorderBox {
                return Err(LayoutError::UnsupportedHeightMeasurement(*owner).into());
            }
            if owners[..i].contains(owner) {
                return Err(LayoutError::DuplicatePresentedHeight(*owner).into());
            }
        }
        if owners.is_empty() {
            return Ok(Vec::new());
        }
        // The offer's viewport, as the layout that follows will have it: a
        // popover's block is that viewport (`imply_blocks`).
        self.replace_env(self.arena.env().with_viewport(offer))?;
        self.imply_blocks();
        if !self.arena.readers.observers.is_empty() || !self.arena.ignoring.is_empty() {
            return self.measure_heights_solved(root, offer, owners);
        }
        let previous = engine(&mut self.layout).height_samples(self.epoch);
        engine(&mut self.layout).present_heights(&self.arena, &[]);
        let measure = |kernel: &mut Self| {
            let root = kernel
                .arena
                .taffy(slot)
                .ok_or_else(|| LayoutError::Engine("root has no engine node".into()))?;
            engine(&mut kernel.layout).compute(
                root,
                offer,
                &kernel.arena,
                kernel.measurer.as_mut(),
            )?;
            kernel.lay_out_blocks_unpublished(slot)?;
            owners
                .iter()
                .map(|&node| kernel.measured_height(node))
                .collect()
        };
        let result = match measure(self) {
            Err(LayoutError::Engine(_)) => {
                self.layout = Some(Box::new(LayoutTree::rebuild(&mut self.arena)));
                measure(self)
            }
            result => result,
        };
        if result.is_err() {
            // A contained invalid metric may have populated Taffy's cache with
            // zero. Rebuild so the next measurement actually invokes the host.
            self.layout = Some(Box::new(LayoutTree::rebuild(&mut self.arena)));
        }
        engine(&mut self.layout).present_heights(&self.arena, &previous);
        result.map_err(KernelError::from)
    }

    /// `measure_height_targets` in a layout of copies of the arena and the
    /// engine tree, with no presented heights and its readers solved, as
    /// `measure_block` measures; the kernel keeps its own, but for the text
    /// revisions the copies' measurements spent.
    fn measure_heights_solved(
        &mut self,
        root: ViewId,
        offer: Offer,
        owners: &[NodeKey],
    ) -> Result<Vec<PresentedHeight>, KernelError> {
        let tree = self.layout.as_deref().and_then(LayoutMirror::tree_ref);
        let tree = tree.ok_or_else(|| LayoutError::Engine("no engine tree to measure".into()))?;
        let copies = (self.arena.trial_copy(), tree.clone());
        let kept = (
            std::mem::replace(&mut self.arena, copies.0),
            self.layout.replace(Box::new(copies.1)),
            self.region.take(),
            (self.provisional_layouts, self.layouts, self.symbol_revision),
        );
        let measured = self
            .compute_layout_presented(root, offer, &[])
            .and_then(|_| {
                let heights = owners.iter().map(|&node| self.measured_height(node));
                Ok(heights.collect::<Result<_, _>>()?)
            });
        let trial = std::mem::replace(&mut self.arena, kept.0);
        (self.layout, self.region) = (kept.1, kept.2);
        (self.provisional_layouts, self.layouts, self.symbol_revision) = kept.3;
        self.arena.spend_text_revisions(&trial);
        measured
    }

    /// The border-box height the engine laid `node` out at.
    fn measured_height(&mut self, node: NodeKey) -> Result<PresentedHeight, LayoutError> {
        let engine_node = (self.arena.taffy(node.index))
            .ok_or_else(|| LayoutError::Engine("height target has no engine node".into()))?;
        let px = engine(&mut self.layout).layout(engine_node).size.height;
        if !px.is_finite() || px < 0.0 {
            return Err(LayoutError::InvalidPresentedHeight);
        }
        Ok(PresentedHeight {
            node,
            epoch: self.epoch,
            px,
        })
    }
}
