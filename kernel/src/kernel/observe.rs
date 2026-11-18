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

use super::area::{area, band_at, depth, passed, scroll_view, Space};
use super::block::reservation;
use super::ignore;
use super::provenance::{self, Contribution};
use super::Kernel;
use crate::arena::NodeArena;
use crate::error::{KernelError, LayoutError};
use crate::id::{IdMap, IdSet, Offer, ViewId};
use crate::layout::LayoutReceipt;

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
    /// What the boxes that ignore take, and what measuring it again needs
    /// (D4).
    pub(super) takings: ignore::Takings,
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
        self.takings.measured.clear();
    }

    /// Measure again the readers of `owner`'s band, the session's for `None`,
    /// and every box that ignores, which band changes are too rare to sort.
    fn invalidate_band(&mut self, owner: Option<u32>) {
        self.readouts.retain(|_, r| r.owner != owner);
        self.takings.measured.clear();
    }

    /// Half a device pixel at the last solve, within which a measurement is
    /// settled.
    pub(super) fn tolerance(&self) -> f32 {
        self.tolerance
    }

    /// The session view's band, in the document's space.
    pub(super) fn session(&self) -> [f32; 4] {
        self.session
    }

    /// What `slot` reserved when its descendants were last measured, if the
    /// solve resolved it (D11).
    pub(super) fn held_reservation(&self, slot: u32) -> Option<Option<reservation::Reserved>> {
        self.reserving.get(&slot).map(|(r, _)| *r)
    }

    /// What a reset keeps: what the host reported for the session, its band,
    /// its view's targets and the registered targets, as the arena keeps the
    /// environment. What the tree held goes with it.
    pub(crate) fn after_reset(&mut self) -> Self {
        Observations {
            session: self.session,
            targets: self.targets.after_reset(),
            ..Default::default()
        }
    }

    /// Measure the readers under `slot` again, which moved: what holds them,
    /// an owner or a scroller, may have changed without their frames. So
    /// may what each box that ignores takes.
    pub(crate) fn moved(&mut self, slot: u32) {
        if !self.readouts.is_empty() {
            self.moved.push(slot);
        }
        self.takings.measured.clear();
    }

    /// Measure again what `slot`, which ignores and was restyled, takes.
    pub(crate) fn restyled_taker(&mut self, slot: u32) {
        self.takings.measured.remove(&slot);
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
        self.takings.taken(slot)
    }

    /// `slot` takes nothing until the solve measures it again.
    pub(crate) fn forget_taken(&mut self, slot: u32) {
        self.takings.forget(slot);
    }

    /// Keep `slot`'s used margins, which a box that ignores touches by.
    pub(crate) fn set_margins(&mut self, slot: u32, margins: [f32; 4]) {
        self.takings.set_margins(slot, margins);
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
        self.arena.observations.takings.released.clear();
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
        observations.takings.measured.clear();
        for r in readers {
            observations.readouts.remove(&r);
        }
    }
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
            observations.takings.measured.clear();
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
pub(super) fn in_branches(arena: &NodeArena, branches: [u32; 2], slot: u32) -> bool {
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

pub(super) fn within(arena: &NodeArena, slot: u32, root: u32) -> bool {
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

/// `into` with `round`'s frames, publication candidates and flow changes
/// added, each node once, its refusals `round`'s, and its passes counted.
fn merge(into: &mut LayoutReceipt, round: LayoutReceipt) {
    let union = |into: &mut Vec<_>, round: Vec<_>| {
        let mut seen: IdSet<_> = into.iter().copied().collect();
        into.extend(round.into_iter().filter(|k| seen.insert(*k)));
    };
    union(&mut into.changed, round.changed);
    union(&mut into.updated, round.updated);
    union(&mut into.flow_changed, round.flow_changed);
    // Each pass lists every refusal under its root, so the last round's stand
    into.flow_skipped = round.flow_skipped;
    into.fragment_skipped = round.fragment_skipped;
    into.flow_passes += round.flow_passes;
    into.flow_comparisons += round.flow_comparisons;
}

#[cfg(test)]
#[path = "observe_tests.rs"]
mod tests;
