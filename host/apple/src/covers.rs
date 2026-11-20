//! What a native container covers of a box (LLP 1075.003 §3.5): the iOS
//! navigation and tab containers report their bars over a route, and the
//! header a bar replaces; one layout for all of a turn's reports. And the
//! containing block a native container gives a subtree it shows in a view
//! of its own (LLP 1115.002 D8.2): a split's pane, a sheet, and where it
//! shows that subtree's root when it does not lift it to the top layer (a
//! stack's route, LLP 1123 D2), which only the runner's geometry reads.
//! And the margins UIKit's own list gives a grouped list's cards and cells,
//! and which split placeholders a pane shows (LLP 1116.006 D2). And the
//! size a person dragged a box's `resize` grip to (`Runner::resize_box`).
//! And how a split shows each column's route against its stack's top (LLP
//! 1127 D8), which the runner's liveness reads.

use super::Host;
use exact_kernel::style::targets::NativeTarget;
use exact_kernel::{
    BlockMeasure, BlockShown, GroupedMargins, HostBlock, HostCover, KernelError, LayoutError,
    ViewId,
};
use exact_runner::DataSource;

/// A host's restyle of a box under the claims' layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Restyle {
    /// Show (`true`) or hide a split's own placeholder
    /// (`Runner::show_split_placeholder`).
    Placeholder(bool),
    /// The border box's width and height a person dragged its `resize`
    /// grip to; `None` keeps an axis (`Runner::resize_box`).
    Size([Option<f32>; 2]),
}

/// What a call reports of content owners and native targets beside its
/// blocks (LLP 1123 D2, D9).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OwnerReports {
    /// Content owners' updates, each by its root's view.
    pub updates: Vec<(ViewId, exact_kernel::OwnerUpdate)>,
    /// Targets native code registered by name, `None` disposing of one
    /// (D9, "Registration").
    pub registrations: Vec<(String, Option<NativeTarget>)>,
}

impl<D: DataSource> Host<D> {
    /// Several boxes' covers (`None` clears one) under one layout. A refusal
    /// for one view (it is gone) leaves the others set and is the batch's
    /// error.
    pub fn set_covers(&mut self, covers: &[(ViewId, Option<HostCover>)]) -> String {
        self.set_claims(
            covers,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            OwnerReports::default(),
        )
    }

    /// What UIKit covers of list `view`'s port at its start and end
    /// (`Runner::set_list_covered`). Nothing is laid out or sent.
    pub fn set_list_covered(&mut self, view: ViewId, covered: [f64; 2]) {
        self.runner.set_list_covered(view, covered);
    }

    /// The kernel's layout stamp folded to one number (`Kernel::layout_stamp`):
    /// a measure kept while it stands is the one the kernel would answer.
    pub fn layout_stamp(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.runner.kernel().layout_stamp().hash(&mut h);
        h.finish()
    }

    /// What `view`'s subtree would measure in `block` (`Kernel::measure_block`):
    /// no layout of the page, no batch; `None` when refused.
    pub fn measure_block(&mut self, view: ViewId, block: HostBlock) -> Option<BlockMeasure> {
        self.runner.count_kernel_layout();
        self.runner.kernel_mut().measure_block(view, block).ok()
    }

    /// Several subtrees' containing blocks (`None` takes one back) under one
    /// layout: each root's frame comes back in its block's space, with the
    /// block's used size.
    pub fn set_blocks(&mut self, blocks: &[(ViewId, Option<HostBlock>)]) -> String {
        self.set_claims(
            &[],
            blocks,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            OwnerReports::default(),
        )
    }

    /// Covers, containing blocks and the blocks' placements under one
    /// layout, and the canvases' work after it: a canvas in a box given
    /// back is laid out and drawn in this batch. And content owners' local
    /// insets (`None` for the session root's; LLP 1123 D9), which change
    /// only the lengths that read them. And content owners' updates (LLP
    /// 1123 D2), each applied after its block, and registered targets (D9):
    /// every one is checked before the call changes anything, and one the
    /// kernel refuses, or two for one owner or one name, refuses the call; an
    /// update whose block the kernel refuses is not applied, and one whose
    /// view is gone is not applied and is the batch's error
    /// (`Host::blocks_into`).
    #[allow(clippy::too_many_arguments)]
    pub fn set_claims(
        &mut self,
        covers: &[(ViewId, Option<HostCover>)],
        blocks: &[(ViewId, Option<HostBlock>)],
        placements: &[(ViewId, Option<exact_runner::BlockPlacement>)],
        margins: &[(ViewId, Option<GroupedMargins>)],
        list_margins: &[(ViewId, Option<[f32; 2]>)],
        restyles: &[(ViewId, Restyle)],
        locals: &[(Option<ViewId>, Option<[f32; 4]>)],
        shown: &[(ViewId, BlockShown)],
        owners: OwnerReports,
    ) -> String {
        if let Some(why) = self.refused_report(&owners) {
            return self.finish(crate::batch::Batch::new(), Some(why));
        }
        self.presence.unauthored = true;
        let mut local_layout = false;
        let mut error = None;
        for &(view, insets) in locals {
            let set = match view {
                Some(view) => self.runner.kernel_mut().set_local_insets(view, insets),
                None => self.runner.kernel_mut().set_root_local_insets(insets),
            };
            match set {
                Ok(owed) => local_layout |= owed,
                Err(e) => drop(error.get_or_insert(format!("local insets: {e:?}"))),
            }
        }
        let mut needs_layout = local_layout
            || !(covers.is_empty()
                && blocks.is_empty()
                && margins.is_empty()
                && list_margins.is_empty());
        if needs_layout {
            self.height_targets_dirty = true;
        }
        let mut batch = crate::batch::Batch::new();
        // A placeholder's or a resized box's style changes as a commit does,
        // so the presenter hears its rows; it is laid out with the claims
        // below, once.
        let mut receipts = Vec::new();
        for &(view, restyle) in restyles {
            let restyled = match restyle {
                Restyle::Placeholder(shown) => self.runner.show_split_placeholder(view, shown),
                Restyle::Size([width, height]) => self.runner.resize_box(view, width, height),
            };
            match restyled {
                Ok(Some(receipt)) => receipts.push(exact_runner::Timed {
                    at_ms: self.now_ms,
                    receipt,
                }),
                Ok(None) => {}
                Err(e) => drop(error.get_or_insert(format!("restyle: {e:?}"))),
            }
        }
        for (view, cover) in self.top_layer_claims(covers) {
            if let Err(e) = self.runner.kernel_mut().set_host_cover(view, cover) {
                error.get_or_insert(format!("cover: {e:?}"));
            }
        }
        let (blocked, owed) = self.blocks_into(blocks, placements, owners);
        // Liveness alone, once the blocks stand: the runner reads it, layout does not
        for &(view, how) in shown {
            self.runner.kernel_mut().set_host_block_shown(view, how);
        }
        error = error.or(blocked);
        needs_layout |= owed;
        for &(view, list) in margins {
            if let Err(e) = self.runner.kernel_mut().set_grouped_margins(view, list) {
                error.get_or_insert(format!("grouped margins: {e:?}"));
            }
        }
        // @ref LLP 1138 — a cells list's rows, the platform's cell margin.
        for &(view, list) in list_margins {
            if let Err(e) = self.runner.kernel_mut().set_list_margins(view, list) {
                error.get_or_insert(format!("list margins: {e:?}"));
            }
        }
        if !receipts.is_empty() {
            return self.commit_into(&receipts, error, batch);
        }
        if needs_layout {
            if let Err(e) = self.layout(&mut batch) {
                error.get_or_insert(e);
            }
            if let Some(canvas) = self.turn.canvas {
                (canvas.turn)(self, &mut batch);
            }
        }
        self.finish(batch, error)
    }
    /// Set `blocks` and `placements`, then the registered targets and the
    /// content owners' updates (LLP 1123 D2, D9), before the caller's single
    /// layout, so a keyboard is held against the bounds its block gives; an
    /// update whose view's block the kernel refuses is not applied. A layout
    /// a report owes is a host's fact, as a block's is: a box it moves takes
    /// its place without a transition. Returns the first refusal, and
    /// whether a report owes a layout.
    pub(super) fn blocks_into(
        &mut self,
        blocks: &[(ViewId, Option<HostBlock>)],
        placements: &[(ViewId, Option<exact_runner::BlockPlacement>)],
        owners: OwnerReports,
    ) -> (Option<String>, bool) {
        let mut error = None;
        if !blocks.is_empty() {
            self.presence.unauthored = true;
            self.height_targets_dirty = true;
        }
        let mut refused = Vec::new();
        for &(view, block) in blocks {
            let captured = self.native_captured_id(view);
            match self.runner.kernel_mut().set_host_block(view, block) {
                // Its frame is in another space now: sent whatever it is.
                Ok(()) => self.queue_layout(view),
                Err(e) => {
                    refused.push(view);
                    error.get_or_insert(format!("block: {e:?}"));
                }
            }
            if captured != self.native_captured_id(view) {
                self.native_moved();
            }
        }
        self.runner.place_blocks(placements);
        let mut owed = false;
        for (name, target) in owners.registrations {
            match self
                .runner
                .kernel_mut()
                .set_registered_target(&name, target)
            {
                Ok(o) => owed |= o,
                Err(e) => drop(error.get_or_insert(format!("registered target: {e:?}"))),
            }
        }
        let updates = owners.updates.into_iter();
        for (view, update) in updates.filter(|(v, _)| !refused.contains(v)) {
            match self.runner.kernel_mut().update_owner(view, update) {
                Ok(o) => owed |= o,
                Err(e) => drop(error.get_or_insert(format!("owner update: {e:?}"))),
            }
        }
        if owed {
            self.presence.unauthored = true;
            self.height_targets_dirty = true;
        }
        (error, owed)
    }

    /// Why a call carrying `owners` is refused before it changes anything,
    /// if it is: an update or a registration the kernel refuses
    /// (`Kernel::check_owner_update`, `Kernel::check_registered_target`), or
    /// two for one owner or one name, since the first changes what the
    /// second is checked against. An update for a view that is gone is
    /// passed over here, as a block for one is: it has no owner to leave half
    /// updated, and `blocks_into` reports it beside what the call applies.
    pub(super) fn refused_report(&self, owners: &OwnerReports) -> Option<String> {
        let kernel = self.runner.kernel();
        let updates = &owners.updates;
        let update = updates.iter().enumerate().find_map(|(i, (view, update))| {
            if updates[..i].iter().any(|(v, _)| v == view) {
                return Some(format!(
                    "owner update: two for view {view} in one call; send one per owner"
                ));
            }
            match kernel.check_owner_update(*view, update) {
                Ok(()) | Err(KernelError::Layout(LayoutError::UnknownView(_))) => None,
                Err(e) => Some(format!(
                    "owner update: {e:?}; nothing in the call was applied"
                )),
            }
        });
        let registered = &owners.registrations;
        update.or_else(|| {
            registered
                .iter()
                .enumerate()
                .find_map(|(i, (name, target))| {
                    if registered[..i].iter().any(|(n, _)| n == name) {
                        return Some(format!(
                            "registered target: two for `{name}` in one call; send one per name"
                        ));
                    }
                    let refused = kernel
                        .check_registered_target(name, target.as_ref())
                        .err()?;
                    Some(format!(
                        "registered target: {refused:?}; nothing in the call was applied"
                    ))
                })
        })
    }
}
