//! Observations of the safe area, per box (@ref LLP 1123 D2's band, D9).
//!
//! A content owner's host gives its root a band: the owner view's raw
//! `safeAreaInsets`, in its block's space, or none while it is unavailable.
//! A box that reads its observation (a calculation's `Observe` leaf) is a
//! reader, and its observation on an edge is what UIKit gives a view as
//! its `safeAreaInsets` there: the distance from its border box's edge, as
//! laid out with scroll offsets at zero, to its unsafe area's inner side,
//! at least 0 and at most the area's depth. The area is its owner's band
//! and what its ancestors reserve (D11); inside a scroller, which takes
//! the band it overlaps into its content inset on each edge it does not
//! ignore (D14, D4), it is there only what is reserved at or below the
//! scroller. A box outside every owner, a
//! document top-layer box's content included, observes the session view's
//! band on the viewport.
//!
//! A reader's border box depends on what it reads, so the kernel solves the
//! readers after each layout ([`Kernel::solve_observations`]): each input
//! comes within half a device pixel of its measurement, or the reader is
//! left unsettled after a bounded number of rounds. The same rounds measure
//! what each box that ignores the band takes past its place (D4) and lay it
//! out again with it.

use super::block::reservation;
use super::ignore;
use super::provenance::{self, Contribution};
use super::Kernel;
use crate::arena::NodeArena;
use crate::error::{KernelError, LayoutError};
use crate::id::{IdMap, IdSet, Offer, ViewId};
use crate::layout::{LayoutMirror, LayoutReceipt};
use crate::{Overflow, StyleId};

/// The rounds the solve lays its readers out in before it leaves the ones
/// still off unsettled (D9).
const MAX_ROUNDS: usize = 24;

/// What a reader observes on each edge, top right bottom left; `None` where
/// its band is unavailable, which makes the observation guaranteed-invalid.
pub(crate) type Observed = [Option<f32>; 4];

#[derive(Debug, Clone, Default)]
pub(crate) struct Observations {
    /// Each owner root's incoming band, top right bottom left in its
    /// block's space: its raw band less Exact's contributions (D11); `None`
    /// while it is unavailable.
    bands: IdMap<u32, Option<[f32; 4]>>,
    /// The session view's band, in the document's space.
    session: [f32; 4],
    /// Each reader's input, what its lengths resolve against.
    inputs: IdMap<u32, Observed>,
    /// Each reader's last measurement: its readout.
    readouts: IdMap<u32, Readout>,
    /// The readers the last solve left unsettled.
    unsettled: Vec<u32>,
    /// The layouts the last solve added.
    pub(crate) rounds: usize,
    /// Each owner's used size when its readers were last measured, which
    /// its band edges.
    spaces: IdMap<u32, (f32, f32)>,
    /// The readers last measured in a document top-layer box.
    top_layer: IdSet<u32>,
    /// Half a device pixel at the last solve, within which a reader is
    /// settled.
    tolerance: f32,
    /// The roots of the subtrees moved since the last solve.
    moved: Vec<u32>,
    /// What each box that reserves reserved and where its edges lay when
    /// its descendants were last measured (D11).
    reserving: IdMap<u32, (Option<reservation::Reserved>, [f32; 4])>,
    /// What each box that ignores and touches takes past its place (D4).
    taken: IdMap<u32, ignore::Taken>,
    /// Each box that ignores's used margins as its layout last placed it, an
    /// `auto` one 0, from whichever tree laid it out.
    margins: IdMap<u32, [f32; 4]>,
    /// The edges each box that ignores gave the band back on in this solve.
    released: IdMap<u32, [bool; 4]>,
    /// The boxes that ignore whose taking is current: their frames, their
    /// styles and the area above them held since they were measured, so the
    /// solve measures them again only when one of those moves.
    measured: IdSet<u32>,
    /// The edges on which each scroller that ignores passed the area to its
    /// content when its readers were last measured.
    passing: IdMap<u32, [bool; 4]>,
    /// The native targets owners and the session report.
    pub(crate) targets: super::targets::Targets,
}

impl Observations {
    /// What `slot`'s lengths read of its observations: its input, 0 on
    /// every edge before the solve gives one.
    pub(crate) fn input(&self, slot: u32) -> Observed {
        self.inputs.get(&slot).copied().unwrap_or([Some(0.0); 4])
    }

    pub(crate) fn band(&self, owner: u32) -> Option<[f32; 4]> {
        self.bands.get(&owner).copied().flatten()
    }

    /// Measure every reader again at its root's next layout, since what its
    /// measurement reads moved without its frame: the environment, a box
    /// starting or stopping to scroll, joining or leaving the top layer, or
    /// gaining or giving back a block.
    pub(crate) fn invalidate(&mut self) {
        self.readouts.clear();
        self.measured.clear();
    }

    /// Measure again the readers of `owner`'s band, the session's for `None`,
    /// and every box that ignores, which band changes are too rare to sort.
    fn invalidate_band(&mut self, owner: Option<u32>) {
        self.readouts.retain(|_, r| r.owner != owner);
        self.measured.clear();
    }

    /// Measure the readers under `slot` again, which moved: what holds them,
    /// an owner or a scroller, may have changed without their frames. So
    /// may what each box that ignores takes.
    pub(crate) fn moved(&mut self, slot: u32) {
        if !self.readouts.is_empty() {
            self.moved.push(slot);
        }
        self.measured.clear();
    }

    /// Measure again what `slot`, which ignores and was restyled, takes.
    pub(crate) fn restyled_taker(&mut self, slot: u32) {
        self.measured.remove(&slot);
    }

    /// Measure `slot` again: it reads an edge it did not, whose input may
    /// be stale.
    pub(crate) fn remeasure(&mut self, slot: u32) {
        self.readouts.remove(&slot);
    }

    /// `slot` reads no observation any more: a reader it becomes again is
    /// a new one, which starts at 0.
    pub(crate) fn stopped_reading(&mut self, slot: u32) {
        self.inputs.remove(&slot);
        self.readouts.remove(&slot);
        self.unsettled.retain(|s| *s != slot);
    }

    pub(crate) fn forget(&mut self, slot: u32) {
        self.targets.forget(slot);
        self.bands.remove(&slot);
        self.spaces.remove(&slot);
        self.forget_taken(slot);
        self.passing.remove(&slot);
        self.stopped_reading(slot);
    }

    /// What `slot` takes past its place, if it ignores and touches.
    pub(crate) fn taken(&self, slot: u32) -> Option<ignore::Taken> {
        match self.taken.is_empty() {
            true => None,
            false => self.taken.get(&slot).copied(),
        }
    }

    /// `slot` takes nothing until the solve measures it again.
    pub(crate) fn forget_taken(&mut self, slot: u32) {
        self.taken.remove(&slot);
        self.margins.remove(&slot);
        self.measured.remove(&slot);
    }

    /// Keep `slot`'s used margins, which a box that ignores touches by.
    pub(crate) fn set_margins(&mut self, slot: u32, margins: [f32; 4]) {
        // Measure it again where its margins moved and its frame need not
        if self.margins.insert(slot, margins) != Some(margins) {
            self.measured.remove(&slot);
        }
    }
}

impl Kernel {
    /// Set the band of the content owner whose root is `view`, top right
    /// bottom left in its block's space: its controller view's raw
    /// `safeAreaInsets` (LLP 1123 D2's owner update, D9), with Exact's
    /// contributions to it, which its boxes do not observe (D11,
    /// "Provenance"); `None` while it is unavailable, which makes its
    /// readers' observations guaranteed-invalid, so their fallbacks apply.
    /// The band less the contributions is what the owner's host clears for
    /// its content root (D3). Returns whether a layout is owed: that band
    /// changed, and it moved the content root, or some box reads an
    /// observation or ignores the band (D4). The update is refused whole,
    /// keeping the last, for
    /// nonfinite or negative insets, a nonfinite effect, or a
    /// contribution's identity twice.
    pub fn set_owner_band(
        &mut self,
        view: ViewId,
        band: Option<[f32; 4]>,
        contributions: &[Contribution],
    ) -> Result<bool, KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        if band.is_some_and(|b| !b.iter().all(|v| v.is_finite() && *v >= 0.0)) {
            return Err(LayoutError::InvalidEnv.into());
        }
        provenance::check(contributions)?;
        let cleared = super::block::clearance(&self.arena, slot);
        let observations = &mut self.arena.observations;
        let band = band.map(|raw| provenance::incoming(raw, contributions));
        if observations.bands.insert(slot, band) == Some(band) {
            return Ok(false);
        }
        observations.invalidate_band(Some(slot));
        let moved = super::block::clearance(&self.arena, slot) != cleared;
        Ok(moved || self.band_read())
    }

    /// Set the session view's band, top right bottom left in the document's
    /// space, which a box outside every owner observes (LLP 1123 D3, D9).
    /// Returns whether a layout is owed, as [`Kernel::set_owner_band`] does.
    pub fn set_session_band(&mut self, band: [f32; 4]) -> Result<bool, KernelError> {
        if !band.iter().all(|v| v.is_finite() && *v >= 0.0) {
            return Err(LayoutError::InvalidEnv.into());
        }
        if self.arena.observations.session == band {
            return Ok(false);
        }
        self.arena.observations.session = band;
        self.arena.observations.invalidate_band(None);
        Ok(self.band_read())
    }

    /// Whether a band's change can move a layout though the content root
    /// does not move: some box reads an observation, or some box inside a
    /// root ignores the band (an owner's root's taking is its clearance).
    fn band_read(&self) -> bool {
        let arena = &self.arena;
        !arena.readers.observers.is_empty()
            || (arena.ignoring.iter()).any(|s| !super::block::owns(arena, s))
    }

    /// What `view` observes of its safe area, top right bottom left, as the
    /// last layout measured it (D9's readout); `None` for a box that reads
    /// no observation, or that a content region's layout withholds them
    /// from.
    pub fn observation(&self, view: ViewId) -> Option<Observed> {
        let slot = self.arena.slot_of(view)?;
        self.arena
            .observations
            .readouts
            .get(&slot)
            .map(|r| r.observed)
    }

    /// The readers the last layout left unsettled, whose inputs are not
    /// within half a device pixel of their measurements, and the boxes
    /// whose taking of the band had not settled (D9, D4,
    /// `safeAreaUnsettled`).
    pub fn safe_area_unsettled(&self) -> Vec<ViewId> {
        let observations = &self.arena.observations;
        (observations.unsettled.iter())
            .map(|&s| self.arena.local_id(s))
            .collect()
    }

    /// Solve the readers under `slot` (D9), a root or a block's root, adding
    /// each round's frames to its `receipt`: each unsettled reader advances
    /// by [`Samples::step`] from the input it has, as UIKit lays a view out
    /// again from where it is, and `relayout` lays `slot` out again, until
    /// all settle or [`MAX_ROUNDS`] pass, the last only measuring. A reader
    /// still off keeps the input its frames were laid out with and is
    /// unsettled. The readers in `skip` are left out.
    pub(super) fn solve_observations(
        &mut self,
        slot: u32,
        relayout: &mut dyn FnMut(&mut Kernel) -> Result<LayoutReceipt, KernelError>,
        skip: &[u32],
        receipt: &mut LayoutReceipt,
    ) -> Result<(), KernelError> {
        self.arena.observations.rounds = 0;
        if self.arena.readers.observers.is_empty() && self.arena.ignoring.is_empty() {
            return Ok(());
        }
        let tolerance = 0.5 / self.arena.display_scale.unwrap_or(1.0);
        let observations = &mut self.arena.observations;
        if observations.tolerance != tolerance {
            observations.tolerance = tolerance;
            observations.invalidate();
        }
        let mut moved = std::mem::take(&mut observations.moved);
        while let Some(s) = moved.pop() {
            if self.arena.is_live(s) {
                self.arena.observations.readouts.remove(&s);
                moved.extend_from_slice(self.arena.children(s));
            }
        }
        self.arena.observations.released.clear();
        self.check_spaces(receipt);
        self.check_reservations();
        self.check_scrollers();
        let unsettled = std::mem::take(&mut self.arena.observations.unsettled);
        let (mine, others): (Vec<u32>, _) =
            (unsettled.into_iter()).partition(|&s| within(&self.arena, s, slot));
        self.arena.observations.unsettled = others;
        let mut candidates = self.readers_due(slot, receipt, mine, skip);
        let mut takers = self.takers_due(slot, receipt);
        let mut solving: IdMap<u32, Samples> = IdMap::default();
        let mut off = Vec::new();
        for round in 0..=MAX_ROUNDS {
            let mut moved = Vec::new();
            off.clear();
            for &s in &candidates {
                let space = band_at(self, s);
                let readout = measure(&self.arena, s, &space);
                let (measured, caps) = (readout.observed, readout.depths);
                let observations = &mut self.arena.observations;
                if readout.top_layer {
                    observations.top_layer.insert(s);
                }
                observations.readouts.insert(s, readout);
                let input = observations.input(s);
                let edges = self.arena.readers.observed_edges[&s];
                if settled(input, measured, tolerance, edges) {
                    continue;
                }
                off.push(s);
                if round == MAX_ROUNDS {
                    continue;
                }
                let samples = solving.entry(s).or_default();
                let next = samples.step(input, measured, caps, tolerance, edges);
                if next != input {
                    self.arena.observations.inputs.insert(s, next);
                    moved.push(s);
                }
            }
            // The boxes that took more or less lay out again with it (D4),
            // and at the bound are left as they are, unsettled
            let taking = self.check_taken(&takers, round < MAX_ROUNDS);
            if round == MAX_ROUNDS {
                off.extend(taking);
                break;
            }
            if moved.is_empty() && taking.is_empty() {
                break;
            }
            self.restyle_readers(&moved);
            self.restyle_taken(&taking);
            let laid = relayout(self)?;
            self.check_spaces(&laid);
            self.check_reservations();
            self.check_scrollers();
            let solving = solving.keys().copied();
            candidates = self.readers_due(slot, &laid, solving, skip);
            takers = self.takers_due(slot, &laid);
            merge(receipt, laid);
            self.arena.observations.rounds += 1;
        }
        self.arena.observations.unsettled.extend(off);
        Ok(())
    }

    /// The readers under `slot` to measure after `receipt`'s layout, in
    /// order: those whose frames it moved, those with no readout, those in a
    /// top-layer box, whose place in the viewport can move without their
    /// frames, and `also`; none of `skip`.
    fn readers_due(
        &mut self,
        slot: u32,
        receipt: &LayoutReceipt,
        also: impl IntoIterator<Item = u32>,
        skip: &[u32],
    ) -> Vec<u32> {
        let observations = &mut self.arena.observations;
        let readouts = &observations.readouts;
        observations
            .top_layer
            .retain(|s| readouts.get(s).is_some_and(|r| r.top_layer));
        let (arena, observations) = (&self.arena, &self.arena.observations);
        let observers = &arena.readers.observers;
        let mut out: Vec<u32> = (receipt.changed.iter())
            .map(|k| k.index)
            .filter(|&s| observers.contains(s))
            .collect();
        let mine = |s: &u32| within(arena, *s, slot);
        out.extend(observations.top_layer.iter().filter(|s| mine(s)));
        // Skip the walk over every reader while each has a readout
        if observations.readouts.len() < observers.len() {
            let unmeasured = observers
                .iter()
                .filter(|s| !observations.readouts.contains_key(s));
            out.extend(unmeasured.filter(mine));
        }
        // A box listed for what it takes is no reader
        out.extend(also.into_iter().filter(|&s| observers.contains(s)));
        out.sort_unstable();
        out.dedup();
        out.retain(|s| !skip.contains(s));
        out
    }

    /// Make the observations of the readers in a content region's
    /// branches unavailable, so their fallbacks apply, and return them: the
    /// region lays its content and placeholder out apart from the frames the
    /// solve measures (`compute_region_layout`).
    pub(super) fn withhold_observations(&mut self) -> Vec<u32> {
        let Some(binding) = self.region.as_ref().map(|r| r.binding) else {
            return Vec::new();
        };
        let arena = &self.arena;
        let branches = [binding.content.index, binding.pending.index];
        let withheld: Vec<u32> = (arena.readers.observers.iter())
            .filter(|&s| in_branches(arena, branches, s))
            .collect();
        let mut changed = Vec::new();
        for &s in &withheld {
            let observations = &mut self.arena.observations;
            observations.readouts.remove(&s);
            if observations.inputs.insert(s, [None; 4]) != Some([None; 4]) {
                changed.push(s);
            }
        }
        self.restyle_readers(&changed);
        withheld
    }

    /// Solve the readers of `result`'s layout outside the content region's
    /// branches (`withheld`), each round a region pass, and give `result`
    /// the last pass with every pass's frames. A pass that is superseded
    /// lets its publication go before the next, since a region holds two at
    /// most (`RegionProfile::SplitFacts`).
    pub(super) fn solve_region_observations(
        &mut self,
        slot: u32,
        offer: Offer,
        inputs: crate::RegionInputs,
        withheld: &[u32],
        result: &mut crate::RegionLayoutReceipt,
    ) -> Result<(), KernelError> {
        let placeholder = crate::region::RegionSelection::Pending(result.shell.root);
        let mut first = Some(std::mem::replace(&mut result.selection, placeholder));
        let mut last: Option<crate::RegionLayoutReceipt> = None;
        let relayout = &mut |k: &mut Kernel| {
            (first, last) = (None, None);
            let pass = k.region_pass(slot, offer, inputs)?;
            let shell = pass.shell.clone();
            last = Some(pass);
            Ok(shell)
        };
        self.solve_observations(slot, relayout, withheld, &mut result.shell)?;
        if let Some(mut pass) = last {
            std::mem::swap(&mut pass.shell, &mut result.shell);
            *result = pass;
        } else if let Some(selection) = first {
            result.selection = selection;
        }
        Ok(())
    }

    /// Whether the last layout measured each reader under `slot`: a band set
    /// since leaves the readers it edges to the next.
    pub(super) fn readers_measured(&self, slot: u32) -> bool {
        let (observers, readouts) = (
            &self.arena.readers.observers,
            &self.arena.observations.readouts,
        );
        readouts.len() >= observers.len()
            || (observers.iter())
                .all(|s| readouts.contains_key(&s) || !within(&self.arena, s, slot))
    }

    /// Measure again the readers of each owner whose size changed, since its
    /// band edges it: one whose root `receipt`'s layout moved or updated, as
    /// a block whose used size changed is, or whose size is not known.
    fn check_spaces(&mut self, receipt: &LayoutReceipt) {
        let observations = &self.arena.observations;
        let mut owners: Vec<u32> = (receipt.changed.iter().chain(&receipt.updated))
            .map(|k| k.index)
            .filter(|s| observations.bands.contains_key(s))
            .collect();
        let unknown = observations.bands.keys().copied();
        owners.extend(unknown.filter(|s| !observations.spaces.contains_key(s)));
        for s in owners {
            let size = self.used_size(s);
            let observations = &mut self.arena.observations;
            if observations.spaces.insert(s, size) != Some(size) {
                observations.invalidate_band(Some(s));
            }
        }
    }

    /// Measure again the readers under each box whose reservation or edges
    /// moved since its descendants were last measured, or that stopped
    /// reserving, which moves what they observe though their own frames
    /// need not move (D11): a custom property or an observation a
    /// reservation reads, or a reserving box that moves past an absolute
    /// reader.
    fn check_reservations(&mut self) {
        let arena = &self.arena;
        if arena.reserving.is_empty() && arena.observations.reserving.is_empty() {
            return;
        }
        // A box that stopped reserving moved what its descendants observe
        let held = &arena.observations.reserving;
        let stopped = held
            .keys()
            .copied()
            .filter(|&s| !arena.reserving.contains(s));
        let mut moved: IdSet<u32> = stopped.collect();
        let mut changed = Vec::new();
        for s in arena.reserving.iter() {
            let f = arena.frame(s);
            let [left, top, right, bottom] = arena.inset(s).unwrap_or_default().border;
            let edges = [
                f.y + top,
                f.x + f.width - right,
                f.y + f.height - bottom,
                f.x + left,
            ];
            let now = (reservation::reserved(arena, s), edges);
            if held.get(&s) != Some(&now) {
                moved.insert(s);
                changed.push((s, now));
            }
        }
        if moved.is_empty() {
            return;
        }
        let readouts = &arena.observations.readouts;
        let under = |r: u32| {
            let mut at = arena.parent(r);
            while let Some(a) = at {
                if moved.contains(&a) {
                    return true;
                }
                at = arena.parent(a);
            }
            false
        };
        let readers: Vec<u32> = (arena.readers.observers.iter())
            .filter(|r| readouts.contains_key(r) && under(*r))
            .collect();
        let reserving = &self.arena.reserving;
        let observations = &mut self.arena.observations;
        observations.reserving.retain(|s, _| reserving.contains(*s));
        observations.reserving.extend(changed);
        observations.measured.clear();
        for r in readers {
            observations.readouts.remove(&r);
        }
    }

    /// The boxes under `root` that ignore whose taking may have moved since
    /// they were measured: those whose frames `receipt`'s layout moved and
    /// those not measured since what they take from changed. In a content
    /// region, whose passes lay its branches out apart from the receipt,
    /// every one.
    fn takers_due(&self, root: u32, receipt: &LayoutReceipt) -> Vec<u32> {
        let arena = &self.arena;
        if arena.ignoring.is_empty() {
            return Vec::new();
        }
        let measured = &arena.observations.measured;
        let mut due: Vec<u32> = match self.region.is_some() {
            true => arena.ignoring.iter().collect(),
            false => (receipt.changed.iter())
                .map(|k| k.index)
                .filter(|&s| arena.ignoring.contains(s))
                .collect(),
        };
        // Skip the walk over every box that ignores while each is measured
        if measured.len() < arena.ignoring.len() {
            due.extend(arena.ignoring.iter().filter(|s| !measured.contains(s)));
        }
        due.sort_unstable();
        due.dedup();
        due.retain(|&s| within(arena, s, root));
        due
    }

    /// Measure what each box in `due` takes (D4), keep each measurement
    /// where `keep`, and return the boxes whose frames it moves by more than
    /// half a device pixel.
    fn check_taken(&mut self, due: &[u32], keep: bool) -> Vec<u32> {
        if due.is_empty() {
            return Vec::new();
        }
        let arena = &self.arena;
        let tolerance = arena.observations.tolerance;
        // Two agree where both extend or both move the box, by frames within tolerance
        let near = |a: Option<ignore::Taken>, b: Option<ignore::Taken>| match (a, b) {
            (Some(a), Some(b)) => {
                let (x, y) = (a.frame(), b.frame());
                a.offset.is_some() == b.offset.is_some()
                    && (0..4).all(|e| (x[e] - y[e]).abs() <= tolerance)
            }
            (a, b) => a == b,
        };
        let measured: Vec<(u32, Option<ignore::Taken>, bool)> = (due.iter())
            .map(|&s| {
                let (t, held) = taking(self, s);
                (s, t, held)
            })
            .collect();
        // One within tolerance of what it took keeps that, so it does not drift
        let changed: Vec<(u32, Option<ignore::Taken>)> = (measured.iter())
            .filter(|&&(s, t, _)| !near(arena.observations.taken(s), t))
            .map(|&(s, t, _)| (s, t))
            .collect();
        let observations = &mut self.arena.observations;
        for &(s, _, held) in &measured {
            match held && keep {
                true => observations.measured.insert(s),
                false => observations.measured.remove(&s),
            };
        }
        if keep {
            for &(s, t) in &changed {
                if let Some(was) = observations.taken.get(&s) {
                    let gave: [bool; 4] = std::array::from_fn(|e| {
                        was.by[e] > tolerance && t.is_none_or(|t| t.by[e] <= tolerance)
                    });
                    let released = observations.released.entry(s).or_default();
                    *released = std::array::from_fn(|e| released[e] || gave[e]);
                }
                match t {
                    Some(t) => observations.taken.insert(s, t),
                    None => observations.taken.remove(&s),
                };
            }
        }
        changed.into_iter().map(|(s, _)| s).collect()
    }

    /// Let the band fill again the padding each box kept whole where its
    /// parent is to be laid out anew, since what moved the box clear may have
    /// changed, so that the layout reaches what a fresh one of the tree does.
    /// A content region's branches are laid out by trees of their own, which
    /// it builds afresh, so a box in one is tested again only where its place
    /// moves (`taking`).
    pub(super) fn reopen_whole(&mut self) {
        let taken = &self.arena.observations.taken;
        if !taken.values().any(|t| t.whole.contains(&true)) {
            return;
        }
        let Some(tree) = self.layout.as_deref().and_then(LayoutMirror::tree_ref) else {
            return;
        };
        let arena = &self.arena;
        let branches =
            (self.region.as_ref()).map(|r| [r.binding.content.index, r.binding.pending.index]);
        let laid_anew = |slot: u32| {
            if branches.is_some_and(|b| in_branches(arena, b, slot)) {
                return false;
            }
            let mut at = arena.parent(slot);
            while let Some(p) = at {
                if let Some(node) = arena.taffy(p) {
                    return tree.is_dirty(node);
                }
                at = arena.parent(p);
            }
            true
        };
        let reopened: Vec<u32> = (taken.iter())
            .filter(|(_, t)| t.whole.contains(&true))
            .map(|(&s, _)| s)
            .filter(|&s| laid_anew(s))
            .collect();
        for &s in &reopened {
            if let Some(t) = self.arena.observations.taken.get_mut(&s) {
                t.whole = [false; 4];
            }
        }
        self.restyle_taken(&reopened);
    }

    /// Lay out and publish again each box that took more or less: an
    /// extended one's engine style carries it, and publication places a
    /// moved one.
    fn restyle_taken(&mut self, taking: &[u32]) {
        if taking.is_empty() {
            return;
        }
        if let Some(r) = &mut self.region {
            r.invalidate();
        }
        let mut unmirrored = crate::layout::Unmirrored;
        let layout = self.layout.as_deref_mut().unwrap_or(&mut unmirrored);
        for &s in taking {
            self.arena.layout_dirty.insert(s);
            if let Some(node) = self.arena.taffy(s) {
                layout.restyle(&self.arena, s, node);
            }
        }
    }
}

/// What `slot`, which ignores, takes past its place on the layout just made
/// (D4): on each edge it names, where its margin box, an `auto` margin
/// counting 0, meets the inner side of the area its parent passes it or
/// lies inside the area, that area to its outer side; `None` where it
/// takes nothing.
fn taking(kernel: &Kernel, slot: u32) -> (Option<ignore::Taken>, bool) {
    let arena = &kernel.arena;
    let hidden = arena.flags(slot).has(crate::id::NodeFlags::HIDDEN);
    if hidden
        || arena.taffy(slot).is_none()
        || arena.is_inline_run(slot)
        || super::block::owns(arena, slot)
    {
        return (None, true);
    }
    let space = band_at(kernel, slot);
    // One in a top-layer box can move in the viewport without its frame
    let held = space.owner.is_some() || space.root.is_none();
    let Some(band) = space.band else {
        return (None, held);
    };
    let sides = area(arena, slot, &space, band);
    let named = ignore::edges(arena, slot, crate::generated::SafeAreaIgnore::Container);
    // Its edges where its parent placed it: its frame less what it took
    let stored = arena.observations.taken(slot);
    let [top, right, bottom, left] = space.edges(arena, slot);
    let [x, y, width, height] = stored.map_or([0.0; 4], |t| t.frame());
    let placed = [top - y, right + x + width, bottom + y + height, left - x];
    let margins = (arena.observations.margins.get(&slot)).map_or([0.0; 4], |m| *m);
    let tolerance = arena.observations.tolerance;
    let at: [f32; 4] = std::array::from_fn(|e| placed[e] - margins[e]);
    let by: [f32; 4] = std::array::from_fn(|e| {
        let [outer, inner] = sides[e];
        match named[e] && at[e] <= inner + tolerance {
            true => (at[e] - outer).max(0.0),
            false => 0.0,
        }
    });
    // An edge it took, gave back and touches again in one solve keeps its
    // padding whole, since filling it moved the box clear, until its place
    // there moves
    let released = (arena.observations.released.get(&slot)).map_or([false; 4], |r| *r);
    let whole: [bool; 4] = std::array::from_fn(|e| {
        let kept = stored.is_some_and(|t| t.whole[e] && (t.at[e] - at[e]).abs() <= tolerance);
        by[e] > tolerance && (released[e] || kept)
    });
    let taken =
        (by.iter().any(|&b| b > tolerance)).then(|| ignore::Taken::of(arena, slot, by, whole, at));
    (taken, held)
}

impl Kernel {
    /// Measure again the readers under each scroller that ignores whose
    /// passing of the area to its content changed, which moves what they
    /// observe though their own frames need not move.
    fn check_scrollers(&mut self) {
        let arena = &self.arena;
        if arena.ignoring.is_empty() && arena.observations.passing.is_empty() {
            return;
        }
        let mut moved = Vec::new();
        for s in arena.ignoring.iter().filter(|&s| scroll_view(arena, s)) {
            let space = band_at(self, s);
            let now = space
                .band
                .map_or([false; 4], |b| passed(arena, s, &space, b));
            if arena
                .observations
                .passing
                .get(&s)
                .map_or([false; 4], |p| *p)
                != now
            {
                moved.push((s, now));
            }
        }
        // A scroller that stopped ignoring or scrolling passes nothing
        let gone = (arena.observations.passing.keys().copied())
            .filter(|&s| !arena.ignoring.contains(s) || !scroll_view(arena, s));
        moved.extend(gone.map(|s| (s, [false; 4])));
        let readouts = &arena.observations.readouts;
        let under: Vec<u32> = (arena.readers.observers.iter())
            .filter(|&r| readouts.contains_key(&r))
            .filter(|&r| moved.iter().any(|&(s, _)| arena.is_ancestor(s, r)))
            .collect();
        let observations = &mut self.arena.observations;
        if !moved.is_empty() {
            observations.measured.clear();
        }
        for (s, now) in moved {
            match now.contains(&true) {
                true => observations.passing.insert(s, now),
                false => observations.passing.remove(&s),
            };
        }
        for r in under {
            observations.readouts.remove(&r);
        }
    }
}

/// Whether `slot` lies in one of a content region's `branches`, stopping at a
/// box laid out on its own before one, as the region's tree is cut.
fn in_branches(arena: &NodeArena, branches: [u32; 2], slot: u32) -> bool {
    let mut at = Some(slot);
    while let Some(s) = at {
        if arena.blocks.holds(s) || crate::style::top_layer::is_box(arena, s) {
            return false;
        }
        if branches.contains(&s) {
            return true;
        }
        at = arena.parent(s);
    }
    false
}

fn within(arena: &NodeArena, slot: u32, root: u32) -> bool {
    slot == root || arena.is_ancestor(root, slot)
}

/// Whether `input` is within `tolerance` of `measured` on each of the
/// reader's `edges`, an unavailable edge matching only an unavailable one.
fn settled(input: Observed, measured: Observed, tolerance: f32, edges: u8) -> bool {
    (0..4)
        .filter(|e| edges & 1 << e != 0)
        .all(|e| match (input[e], measured[e]) {
            (Some(i), Some(m)) => (i - m).abs() <= tolerance,
            (None, None) => true,
            _ => false,
        })
}

/// A reader's last sample on each edge, for the secant step.
#[derive(Default)]
struct Samples {
    /// The input and residual of its round before last, per edge.
    last: [Option<(f32, f32)>; 4],
}

impl Samples {
    /// The next input, from this round's `input` and `measured`: on each of
    /// the reader's `edges` not within `tolerance`, the secant step on the
    /// residuals of this round and the one before, else plain substitution;
    /// an edge it does not read keeps its input. A step lands within what
    /// an observation can be, 0 to the area's depth `caps`; one whose
    /// candidate lies behind the input, against its residual, takes the end
    /// of that range the residual points to: the measurement outruns the
    /// line there, steeper than it, up to where it stops at that end.
    /// Telling a round that crosses a layout breakpoint (a wrap, a track
    /// change), which D9 also has substitute, needs a signal from layout
    /// that it does not give yet.
    fn step(
        &mut self,
        input: Observed,
        measured: Observed,
        caps: [f32; 4],
        tolerance: f32,
        edges: u8,
    ) -> Observed {
        let mut next = input;
        for e in (0..4).filter(|e| edges & 1 << e != 0) {
            let (Some(x), Some(m)) = (input[e], measured[e]) else {
                // Take an unavailable edge as measured: its band is unavailable throughout
                next[e] = measured[e];
                self.last[e] = None;
                continue;
            };
            let r = m - x;
            next[e] = Some(match self.last[e] {
                // Keep a settled edge's input while another moves
                _ if r.abs() <= tolerance => x,
                Some((x0, r0)) if x != x0 && r != r0 => {
                    let candidate = x - r * (x - x0) / (r - r0);
                    // Whether the step goes the way the residual points
                    let ahead = if r > 0.0 {
                        candidate > x
                    } else {
                        candidate < x
                    };
                    match (candidate.is_finite(), ahead) {
                        (true, true) => candidate.clamp(0.0, caps[e]),
                        // Take the range's end when the line's root lies behind
                        (true, false) if r > 0.0 => caps[e],
                        (true, false) => 0.0,
                        (false, _) => m,
                    }
                }
                _ => m,
            });
            self.last[e] = Some((x, r));
        }
        next
    }
}

/// A reader's measurement, and where its band came from.
#[derive(Debug, Clone, Copy)]
struct Readout {
    observed: Observed,
    /// The unsafe area's depth on each edge, past which no observation
    /// reaches and the solve tries none (`Samples::step`).
    depths: [f32; 4],
    /// The owner whose band it measured, `None` for the session's.
    owner: Option<u32>,
    /// Whether it is in a document top-layer box.
    top_layer: bool,
}

/// What `slot` observes on each edge of its `space` on the layout just
/// made, as UIKit gives a view its `safeAreaInsets`: the distance from the
/// box's edge to the unsafe area's inner side, at least 0 and at most the
/// area's depth, whatever the box's own size (D9).
fn measure(arena: &NodeArena, slot: u32, space: &Space) -> Readout {
    let (observed, depths) = match space.band {
        None => ([None; 4], [0.0; 4]),
        Some(band) => {
            let sides = area(arena, slot, space, band);
            let edges = space.edges(arena, slot);
            let observed = std::array::from_fn(|e| Some(depth(sides[e], edges[e])));
            (
                observed,
                sides.map(|[outer, inner]| (inner - outer).max(0.0)),
            )
        }
    };
    Readout {
        observed,
        depths,
        owner: space.owner,
        top_layer: space.root.is_some() && space.owner.is_none(),
    }
}

/// The depth an edge `at` in from the space's edge observes of an area
/// whose sides lie `outer` and `inner` in: at least 0, at most the area's
/// depth.
fn depth([outer, inner]: [f32; 2], at: f32) -> f32 {
    (inner - at).clamp(0.0, (inner - outer).max(0.0))
}

/// The unsafe area `slot` observes, on each edge how far in from the
/// space's edge its outer and inner sides lie (D9, D11): the band, from the
/// space's edge, or nothing below a scroller, which takes what reaches it;
/// then, on each edge where an ancestor at or below that reserves, from its
/// padding edge to its observation there plus its reservations, and from
/// further out where that edge lies in the area; an owner's root from the
/// allocation's edge, with the band as its observation. A box's own
/// reservations are not among them.
fn area(arena: &NodeArena, slot: u32, space: &Space, band: [f32; 4]) -> [[f32; 2]; 4] {
    let from = |open: [bool; 4]| -> [[f32; 2]; 4] {
        std::array::from_fn(|e| [0.0, if open[e] { band[e] } else { 0.0 }])
    };
    if arena.reserving.is_empty() {
        return from(open_edges(arena, slot, space, band));
    }
    let mut reserving = Vec::new();
    let mut open = [true; 4];
    let mut above = (space.root != Some(slot))
        .then(|| arena.parent(slot))
        .flatten();
    // What each box reserves this round, which `check_reservations` resolved
    let (held, tolerance) = (&arena.observations.reserving, arena.observations.tolerance);
    while let Some(s) = above {
        let r = match held.get(&s) {
            Some((r, _)) => *r,
            None => reservation::reserved(arena, s),
        };
        if let Some(r) = r {
            reserving.push((s, r, open));
        }
        if scroll_view(arena, s) {
            open = scrolled(arena, s, space, band, open);
            if !open.contains(&true) {
                break;
            }
        }
        above = (space.root != Some(s)).then(|| arena.parent(s)).flatten();
    }
    let mut sides = from(open);
    for (s, r, open) in reserving.into_iter().rev() {
        let edges = match space.owner == Some(s) {
            true => [0.0; 4],
            false => space.padding_edges(arena, s),
        };
        let observed: [f32; 4] = std::array::from_fn(|e| depth(sides[e], edges[e]));
        let inside = r.inside(observed);
        for e in 0..4 {
            if open[e] && (r.added[e] != 0.0 || r.padding[e] != 0.0) {
                // A padding edge at or past the area's inner side joins it
                let outer = match edges[e] <= sides[e][1] + tolerance {
                    true => sides[e][0].min(edges[e]),
                    false => edges[e],
                };
                sides[e] = [outer, edges[e] + inside[e]];
            }
        }
    }
    sides
}

/// The band a box observes and where it lies.
struct Space {
    /// The owner whose band it is, `None` for the session's.
    owner: Option<u32>,
    /// The root of the box's own layout: its owner's, or a document
    /// top-layer box's; `None` for the document.
    root: Option<u32>,
    /// Top right bottom left; `None` while it is unavailable.
    band: Option<[f32; 4]>,
    /// The width and height of the space the band edges.
    size: (f32, f32),
    /// What places the box's frames in that space: a top-layer box's
    /// translation into the viewport.
    origin: (f32, f32),
}

impl Space {
    /// How far in from this space's edges each of `slot`'s edges lies, top
    /// right bottom left.
    fn edges(&self, arena: &NodeArena, slot: u32) -> [f32; 4] {
        let ((width, height), f) = (self.size, arena.frame(slot));
        let (x, y) = (f.x + self.origin.0, f.y + self.origin.1);
        [y, width - x - f.width, height - y - f.height, x]
    }

    /// [`Self::edges`] for `slot`'s padding edges, inside its border.
    fn padding_edges(&self, arena: &NodeArena, slot: u32) -> [f32; 4] {
        let edges = self.edges(arena, slot);
        // A published inset's sides are left, top, right, bottom
        let [left, top, right, bottom] = arena.inset(slot).unwrap_or_default().border;
        let border = [top, right, bottom, left];
        std::array::from_fn(|e| edges[e] + border[e])
    }
}

/// The band `slot` observes: an owner's, in its block's space; or, for a
/// box outside every owner, the session's on the viewport, which a
/// document top-layer box is (D2). A top-layer box whose block its host
/// gave (a painted `position-area` popover) is where the host shows it,
/// which the kernel is not told, so its band is unavailable.
fn band_at(kernel: &Kernel, slot: u32) -> Space {
    let arena = &kernel.arena;
    let env = arena.env();
    let viewport = (env.viewport_width, env.viewport_height);
    let session = |root, origin| Space {
        owner: None,
        root,
        band: Some(arena.observations.session),
        size: viewport,
        origin,
    };
    match holder(arena, slot) {
        Holder::TopLayer(s) => {
            let origin = kernel.top_layer_origin(arena.local_id(s), viewport);
            let Some((x, y)) = origin else {
                return Space {
                    band: None,
                    ..session(Some(s), (0.0, 0.0))
                };
            };
            let f = arena.frame(s);
            session(Some(s), (x - f.x, y - f.y))
        }
        Holder::Owner(s) => Space {
            owner: Some(s),
            root: Some(s),
            band: arena.observations.band(s),
            size: kernel.used_size(s),
            origin: (0.0, 0.0),
        },
        Holder::Session => session(None, (0.0, 0.0)),
    }
}

/// What holds a box for its safe area (D2): the owner whose block it is in,
/// a document top-layer box, or the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Holder {
    Owner(u32),
    TopLayer(u32),
    Session,
}

/// What holds `slot`: its nearest ancestor that holds its own ([`holds`]),
/// or the session.
pub(crate) fn holder(arena: &NodeArena, slot: u32) -> Holder {
    let mut at = Some(slot);
    while let Some(s) = at {
        if let Some(h) = holds(arena, s) {
            return h;
        }
        at = arena.parent(s);
    }
    Holder::Session
}

/// What `slot` holds its own subtree as, if it does: a document top-layer
/// box laid out on its own, or a box its host gave a block.
pub(crate) fn holds(arena: &NodeArena, slot: u32) -> Option<Holder> {
    // Check for a top-layer box first: it is no owner, whatever block it has (D2)
    if arena.top_layer_slots.contains(slot) && arena.blocks.holds(slot) {
        return Some(Holder::TopLayer(slot));
    }
    (arena.blocks.given(slot).is_some()).then_some(Holder::Owner(slot))
}

/// The edges, top right bottom left, on which the band reaches `slot` past
/// every scroller between it and its layout's root: a scroller takes the
/// band that reaches it on each edge it does not ignore (D14, D4).
fn open_edges(arena: &NodeArena, slot: u32, space: &Space, band: [f32; 4]) -> [bool; 4] {
    let mut open = [true; 4];
    if space.root == Some(slot) {
        return open;
    }
    let mut at = arena.parent(slot);
    while let Some(s) = at {
        if scroll_view(arena, s) {
            open = scrolled(arena, s, space, band, open);
            if !open.contains(&true) {
                break;
            }
        }
        if space.root == Some(s) {
            break;
        }
        at = arena.parent(s);
    }
    open
}

/// `open` less the edges the scroller `slot` takes the area on (D9).
fn scrolled(
    arena: &NodeArena,
    slot: u32,
    space: &Space,
    band: [f32; 4],
    open: [bool; 4],
) -> [bool; 4] {
    let passes = passed(arena, slot, space, band);
    std::array::from_fn(|e| open[e] && passes[e])
}

/// The edges on which the scroller `slot` passes the area to its content:
/// those it ignores where its box reaches the area that reaches it, so that
/// what scrolls past an edge clear of the area does not reach it.
fn passed(arena: &NodeArena, slot: u32, space: &Space, band: [f32; 4]) -> [bool; 4] {
    let ignored = ignore::edges(arena, slot, crate::generated::SafeAreaIgnore::Container);
    if !ignored.contains(&true) {
        return [false; 4];
    }
    let (sides, edges) = (area(arena, slot, space, band), space.edges(arena, slot));
    let tolerance = arena.observations.tolerance;
    std::array::from_fn(|e| ignored[e] && edges[e] <= sides[e][1] + tolerance)
}

/// Whether the host presents `slot` as a scroll view: an axis whose
/// effective overflow is `auto` or `scroll` (CSS Overflow 3 §3.1), with
/// `auto` on the block axis of a node that scrolls by default and says no
/// `overflow-y`. A `hidden` box only clips, and UIKit passes the safe area
/// through a clipping view to its subviews.
pub(crate) fn scroll_view(arena: &NodeArena, slot: u32) -> bool {
    let s = arena.style(slot);
    let default = !s.mask.has(StyleId::OverflowY) && arena.node_type(slot).scrolls_by_default();
    let y = if default {
        Overflow::Auto
    } else {
        s.overflow_y
    };
    let scrolls = |o| matches!(o, Overflow::Auto | Overflow::Scroll);
    scrolls(s.overflow_x.computed(y)) || scrolls(y.computed(s.overflow_x))
}

/// `into` with `round`'s frames and publication candidates added, each
/// node once, and its passes counted.
fn merge(into: &mut LayoutReceipt, round: LayoutReceipt) {
    let mut seen: IdSet<_> = into.changed.iter().copied().collect();
    into.changed
        .extend(round.changed.into_iter().filter(|k| seen.insert(*k)));
    let mut seen: IdSet<_> = into.updated.iter().copied().collect();
    into.updated
        .extend(round.updated.into_iter().filter(|k| seen.insert(*k)));
    into.flow_changed.extend(round.flow_changed);
    into.flow_skipped.extend(round.flow_skipped);
    into.fragment_skipped.extend(round.fragment_skipped);
    into.flow_passes += round.flow_passes;
    into.flow_comparisons += round.flow_comparisons;
}

#[cfg(test)]
#[path = "observe_tests.rs"]
mod tests;
