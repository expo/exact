//! @ref LLP 1010 §6.12 — the lists whose rows ride a layout: those an author
//! changed in a runner commit (new keys, a mounted row's size, or its place
//! across the list or inside its wrapper), whose mounted rows moved from a
//! place they had. A list's own moves (its window, a measurement, a scroll),
//! a motion tick's, a resize's, a host fact's and a provisional layout ride
//! nothing. It reads the layout's own geometry, so it needs no motion link.

use super::*;

/// What a layout notes for its rides (`Host::emit_rides`).
#[derive(Debug, Default)]
pub(crate) struct Rides {
    /// Each virtualized list's data generation at the last layout.
    generations: Vec<(ViewId, u64)>,
    /// The lists whose keys the layout under way found new.
    renewed: IdSet<ViewId>,
    /// Each box the layout under way moved from a frame it had: its id, that
    /// frame and its new one, parent-relative (`x, y, width, height`).
    pub(super) moves: Vec<(ViewId, (f32, f32, f32, f32), (f32, f32, f32, f32))>,
    /// The layout under way is a resize's or a host fact's.
    blocked: bool,
}

impl<D: DataSource> Host<D> {
    /// As a layout begins: its keys judged against the last layout's, and
    /// whether it is a resize's or a host fact's, read before the resize's
    /// own step takes its flag.
    pub(crate) fn begin_rides(&mut self) {
        let now: Vec<(ViewId, u64)> = self
            .runner
            .collection_data()
            .into_iter()
            .map(|(view, generation, _)| (view, generation))
            .collect();
        self.rides.renewed.clear();
        for entry in &now {
            if !self.rides.generations.contains(entry) {
                self.rides.renewed.insert(entry.0);
            }
        }
        self.rides.generations = now;
        self.rides.moves.clear();
        self.rides.blocked = self.presence.snap || self.presence.unauthored;
    }

    /// After a layout that followed a runner commit's receipts: the lists an
    /// author changed whose mounted rows it moved from a place they had,
    /// which the presenter moves from where they showed, on one spring
    /// (`Batch::rides`), as a collection view's batch update moves its cells.
    pub(crate) fn emit_rides(&mut self, batch: &mut Batch) {
        let moves = std::mem::take(&mut self.rides.moves);
        if self.rides.blocked || batch.layout_provisional || moves.is_empty() || self.native_mode()
        {
            return;
        }
        let kernel = self.runner.kernel();
        let wraps =
            |n: exact_kernel::NodeRef| n.props.str(exact_kernel::PropId::ListItemKey).is_some();
        let across: IdMap<ViewId, bool> = self
            .runner
            .collection_data()
            .into_iter()
            .map(|(view, _, horizontal)| (view, horizontal))
            .collect();
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        let mut authored = self.rides.renewed.clone();
        let mut shifted: IdSet<ViewId> = IdSet::default();
        for (id, was, is) in moves {
            let Some(node) = kernel.node(id) else {
                continue;
            };
            let Some(parent) = node.parent.and_then(|p| kernel.node(p)) else {
                continue;
            };
            if wraps(node) {
                shifted.insert(id);
                let horizontal = node
                    .parent
                    .and_then(|l| across.get(&l).copied())
                    .unwrap_or(false);
                let cross = if horizontal {
                    (was.1, is.1)
                } else {
                    (was.0, is.0)
                };
                if !near(was.2, is.2) || !near(was.3, is.3) || !near(cross.0, cross.1) {
                    authored.extend(node.parent);
                }
            } else if wraps(parent) {
                authored.extend(parent.parent);
            }
        }
        let mut mounted = Vec::new();
        for head in self.runner.collections_shallow() {
            if !authored.contains(&head.view) {
                continue;
            }
            self.runner.collection_mounted(head.view, &mut mounted);
            if mounted.iter().any(|(wrapper, _)| shifted.contains(wrapper)) {
                batch.rides.push(head.view);
            }
        }
    }
}
