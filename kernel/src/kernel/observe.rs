//! Observations of the safe area, the keyboard and the layout regions, per
//! box (@ref LLP 1123 D2's band, D7, D8, D9, D10).
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
//! band on the viewport. A box's readings of the keyboard are what UIKit
//! gives a view as its keyboard guide (D7, `keyboard.rs`), and its layout
//! regions what UIKit gives a view's margins and readable content (D8,
//! `margins.rs`), each family of observations four components of a
//! reader's input.
//!
//! A reader's border box depends on what it reads, so the kernel solves the
//! readers after each layout ([`Kernel::solve_observations`]): each input
//! comes within half a device pixel of its measurement, or the reader is
//! left unsettled after a bounded number of rounds. The same rounds measure
//! what each box that ignores the band takes past its place (D4), and what
//! each parent of bars observes, past which their slots begin (D12), and lay
//! it out again with it.

use super::area::{area, band_at, depth, passed, scroll_view, Space};
use super::block::reservation;
use super::ignore;
use super::provenance::{self, Contribution};
use super::Kernel;
use crate::arena::NodeArena;
use crate::error::{KernelError, LayoutError};
use crate::id::{IdMap, IdSet, Offer, ViewId};
use crate::layout::LayoutReceipt;
use crate::style::calc::{Components, Family, COMPONENTS};

/// The rounds the solve lays its readers out in before it leaves the ones
/// still off unsettled (D9).
const MAX_ROUNDS: usize = 24;

/// What a reader observes, each family on each edge top right bottom left
/// (`Observe::component`); `None` where it is unavailable, which makes the
/// observation guaranteed-invalid.
pub(crate) type Observed = [Option<f32>; COMPONENTS];

// A reader's components are bits of a `Components` (`Readers::observed`)
const _: () = assert!(COMPONENTS <= Components::BITS as usize);

/// What a box reads of one family, top right bottom left; `None` where it is
/// unavailable.
pub type Reading = [Option<f32>; 4];

/// What a box that reserves reserved, where its padding edges lay, top right
/// bottom left, and how deep its bars were.
type Held = (Option<reservation::Reserved>, [f32; 4], [f32; 4]);

#[derive(Debug, Clone, Default)]
pub(crate) struct Observations {
    /// Each owner root's incoming band, top right bottom left in its
    /// block's space: its raw band less Exact's contributions (D11); `None`
    /// while it is unavailable.
    bands: IdMap<u32, Option<[f32; 4]>>,
    /// Each owner root's own additional insets as its host measured their effect, where it wrote
    /// them (D11, D3): UIKit's floor can hold a negative one on a child controller.
    pub(crate) root_effects: IdMap<u32, [f32; 4]>,
    /// The session view's band, in the document's space.
    session: [f32; 4],
    /// Each reader's input, what its lengths resolve against.
    inputs: IdMap<u32, Observed>,
    /// Each reader's last measurement: its readout.
    readouts: IdMap<u32, Readout>,
    /// The readers, boxes that ignore and parents of bars the last solve left
    /// unsettled.
    unsettled: Vec<u32>,
    /// The layouts the last solve added.
    pub(crate) rounds: usize,
    /// Each owner's used size when its readers were last measured, which
    /// its band edges.
    spaces: IdMap<u32, (f32, f32)>,
    /// The readers measured at every layout, whose readings move with no
    /// change to their own frames: those last measured in a document
    /// top-layer box, and those whose margins project a superview's (D8).
    always: IdSet<u32>,
    /// Half a device pixel at the last solve, within which a reader is
    /// settled.
    tolerance: f32,
    /// The roots of the subtrees moved since the last solve.
    moved: Vec<u32>,
    /// What each box that reserves held when its descendants were last
    /// measured (D11, D12).
    reserving: IdMap<u32, Held>,
    /// What the boxes that ignore take, and what measuring it again needs
    /// (D4).
    pub(super) takings: ignore::Takings,
    /// The edges on which each scroller that ignores passed the area to its
    /// content when its readers were last measured.
    passing: IdMap<u32, [bool; 4]>,
    /// The scrollers that ignore and passed a docked keyboard to their
    /// content when its readers were last measured (D7).
    passing_keyboard: IdSet<u32>,
    /// The bars (D12): which boxes are styled as one, and their slots' depths.
    pub(crate) bars: super::bar::Bars,
    /// The navigation chrome beside each route, in points on the physical left and right.
    side_bars: IdMap<u32, [f32; 2]>,
    /// What each owner's host reports of its margins (D8).
    pub(crate) margins: IdMap<u32, super::margins::OwnerMargins>,
    /// The native targets owners and the session report.
    pub(crate) targets: super::targets::Targets,
    /// The segments each owner's host reports (D16).
    pub(crate) segments: IdMap<u32, std::sync::Arc<crate::style::env::SegmentGrid>>,
    /// The keyboard each owner's host reports, and the policy (D7).
    pub(crate) keyboards: super::keyboard::Keyboards,
    /// The chrome each owner's host reports (D6).
    pub(crate) chrome: IdMap<u32, super::owner_update::OwnerChrome>,
    /// Each owner's map into its window, and the document's (D6's `"window"` space).
    pub(crate) window_maps: IdMap<u32, super::window_map::WindowMap>,
    pub(crate) session_window_map: Option<super::window_map::WindowMap>,
}

impl Observations {
    /// What `slot`'s lengths read of its observations: its input, 0 on
    /// every component before the solve gives one.
    pub(crate) fn input(&self, slot: u32) -> Observed {
        self.inputs
            .get(&slot)
            .copied()
            .unwrap_or([Some(0.0); COMPONENTS])
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
    /// and every box that ignores, since a change to a band or a keyboard can
    /// move what any of them takes.
    pub(super) fn invalidate_band(&mut self, owner: Option<u32>) {
        self.readouts.retain(|_, r| r.owner != owner);
        self.takings.measured.clear();
    }

    /// Measure again the boxes that read the keyboard and whose owner,
    /// `None` outside every owner, is `of`: a change to the keyboard or the
    /// policy moves their readings, though it lays nothing out. `observed`
    /// is each reader's components. Returns whether there were any.
    pub(super) fn forget_keyboard_readings(
        &mut self,
        observed: &IdMap<u32, Components>,
        of: impl Fn(Option<u32>) -> bool,
    ) -> bool {
        let bits = Family::Keyboard.bits() | Family::KeyboardUndocked.bits();
        self.forget_readings(observed, bits, of)
    }

    /// Forget the readouts of the readers of any of `families` (bits of
    /// [`Family::bits`]) in the spaces `of` names, so the next solve
    /// measures them again. Whether any was forgotten.
    pub(super) fn forget_readings(
        &mut self,
        observed: &IdMap<u32, Components>,
        families: Components,
        of: impl Fn(Option<u32>) -> bool,
    ) -> bool {
        let before = self.readouts.len();
        let reads = |slot: &u32| (observed.get(slot)).is_some_and(|&c| c & families != 0);
        self.readouts
            .retain(|slot, r| !(of(r.owner) && reads(slot)));
        self.readouts.len() != before
    }

    /// The session view's band, in the document's space.
    pub(super) fn session(&self) -> [f32; 4] {
        self.session
    }

    /// What `slot` reserved when its descendants were last measured, if the
    /// solve resolved it (D11).
    pub(super) fn held_reservation(&self, slot: u32) -> Option<Option<reservation::Reserved>> {
        self.reserving.get(&slot).map(|(r, ..)| *r)
    }

    /// What a reset keeps: what the host reported for the session, its band,
    /// its view's targets, the registered targets, its keyboard, the
    /// keyboard policy and its window map, as the arena keeps the environment. What the tree
    /// held goes with it.
    pub(crate) fn after_reset(&mut self) -> Self {
        Observations {
            session: self.session,
            targets: self.targets.after_reset(),
            keyboards: self.keyboards.after_reset(),
            session_window_map: self.session_window_map,
            ..Default::default()
        }
    }

    /// Measure the readers and the boxes that ignore under `slot` again,
    /// which moved: what holds them, an owner or a scroller, may have changed
    /// without their frames.
    pub(crate) fn moved(&mut self, slot: u32) {
        if !self.readouts.is_empty() || !self.takings.measured.is_empty() {
            self.moved.push(slot);
        }
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
        self.root_effects.remove(&slot);
        self.spaces.remove(&slot);
        self.forget_taken(slot);
        self.passing.remove(&slot);
        self.passing_keyboard.remove(&slot);
        self.bars.forget(slot);
        self.keyboards.forget(slot);
        self.margins.remove(&slot);
        self.side_bars.remove(&slot);
        self.segments.remove(&slot);
        self.chrome.remove(&slot);
        self.window_maps.remove(&slot);
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
    /// Set the physical widths of host navigation beside a route (LLP 1123 D9).
    /// A change owes a layout only where the route has readers of this family.
    pub fn set_route_bar_insets(
        &mut self,
        view: ViewId,
        sides: [f32; 2],
    ) -> Result<bool, KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        if !sides.iter().all(|v| v.is_finite() && *v >= 0.0) {
            return Err(LayoutError::InvalidEnv.into());
        }
        let observations = &mut self.arena.observations;
        if observations
            .side_bars
            .get(&slot)
            .copied()
            .unwrap_or_default()
            == sides
        {
            return Ok(false);
        }
        if sides == [0.0; 2] {
            observations.side_bars.remove(&slot);
        } else {
            observations.side_bars.insert(slot, sides);
        }
        Ok(
            observations.forget_readings(&self.arena.readers.observed, Family::Bar.bits(), |o| {
                o == Some(slot)
            }),
        )
    }

    /// Set the band of the content owner whose root is `view`, top right
    /// bottom left in its block's space: its controller view's raw
    /// `safeAreaInsets` (LLP 1123 D2's owner update, D9), with Exact's
    /// contributions to it, which its boxes do not observe (D11,
    /// "Provenance"); `None` while it is unavailable, which makes its
    /// readers' observations guaranteed-invalid, so their fallbacks apply.
    /// The band less the contributions is what the owner's host clears for
    /// its content root (D3). Returns whether a layout is owed: that band
    /// changed, and it moved the content root or the root's bars' slots on
    /// an edge it takes (D12), or some box reads an observation, ignores the
    /// band (D4) or has bars past what it observes (D12). The update is refused whole,
    /// keeping the last, for
    /// nonfinite insets, a nonfinite effect, or a contribution's identity
    /// twice (`check_band`).
    /// A host reports this field with the owner's others through
    /// [`Kernel::update_owner`], which applies them whole or not at all.
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
        check_band(band.as_ref(), contributions)?;
        let cleared = super::block::clearance(&self.arena, slot);
        let band = band.map(|raw| provenance::incoming(raw, contributions));
        // The root's own additional insets as UIKit applied them, which D3 clears
        let root = self.arena.local_id(slot);
        let effect = (contributions.iter())
            .find(|c| c.node == root && c.operation == provenance::Operation::AdditionalInsets)
            .map(|c| c.effect);
        let observations = &mut self.arena.observations;
        let effect_moved = match effect {
            Some(e) => observations.root_effects.insert(slot, e) != Some(e),
            None => observations.root_effects.remove(&slot).is_some(),
        };
        if observations.bands.insert(slot, band) == Some(band) {
            // The root's own effect alone moved: what it moves is the clearance and its slots
            let moved = effect_moved && super::block::clearance(&self.arena, slot) != cleared;
            return Ok(
                moved || effect_moved && super::block::Blocks::slots_moved(&self.arena, slot)
            );
        }
        observations.invalidate_band(Some(slot));
        let moved = super::block::clearance(&self.arena, slot) != cleared;
        let slots = super::block::Blocks::slots_moved(&self.arena, slot);
        Ok(moved || slots || self.band_read())
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
    /// does not move: some box reads an observation, some box inside a
    /// root ignores the band (an owner's root's taking is its clearance),
    /// or some box other than an owner's root has bars, whose slots begin
    /// past what it observes (D12); an owner's root's own bars move with its
    /// band only where it takes an edge (`Blocks::slots_moved`).
    pub(super) fn band_read(&self) -> bool {
        let arena = &self.arena;
        !arena.readers.observers.is_empty()
            || (arena.ignoring.iter()).any(|s| !super::block::owns(arena, s))
            || super::bar::slotted(arena)
    }

    /// What `view` observes of its safe area, top right bottom left, as the
    /// last layout measured it (D9's readout); `None` for a box that reads
    /// none of it, or that a content region's layout withholds its
    /// observations from.
    pub fn observation(&self, view: ViewId) -> Option<Reading> {
        self.observation_of(view, Family::SafeArea)
    }

    /// What `view` reads of `family`, as [`Kernel::observation`] reads the
    /// safe area: the keyboard's readings among them (D7).
    pub fn observation_of(&self, view: ViewId, family: Family) -> Option<Reading> {
        let slot = self.arena.slot_of(view)?;
        let read = self.arena.readers.observed.get(&slot).copied().unwrap_or(0);
        if read & family.bits() == 0 {
            return None;
        }
        let readout = self.arena.observations.readouts.get(&slot)?;
        let at = family as usize * 4;
        Some(std::array::from_fn(|e| readout.observed[at + e]))
    }

    /// The space `view`'s frame is placed in (D2, D6): the owner whose
    /// space it is, its root's view, or `None` for the document's, outside
    /// every owner; and what places the frame there, a document top-layer
    /// box's translation into the viewport, or none. `None` for a view that
    /// is gone, or a top-layer box its host places (a painted
    /// `position-area` popover), where the kernel is not told.
    pub fn frame_space(&self, view: ViewId) -> Option<(Option<ViewId>, (f32, f32))> {
        let slot = self.arena.slot_of(view)?;
        let space = band_at(self, slot);
        let owner = space.owner.map(|o| self.arena.local_id(o));
        space.placed().then_some((owner, space.origin))
    }

    /// What `view` observes of each family on the layout just made, top
    /// right bottom left, measured now whether or not it reads any: a
    /// geometry fact's (D6), as every UIKit view has a safe area. A box that
    /// reads none cannot move its own frame by it, so the layout it was
    /// measured on stands. `None` for a view that is gone.
    pub fn observations_now(&self, view: ViewId) -> Option<Vec<(Family, Reading)>> {
        let slot = self.arena.slot_of(view)?;
        let all = Family::ALL.iter().fold(0, |bits, f| bits | f.bits());
        let readout = measure_families(&self.arena, slot, &band_at(self, slot), all);
        let reading = |f: Family| std::array::from_fn(|e| readout.observed[f as usize * 4 + e]);
        Some(Family::ALL.iter().map(|&f| (f, reading(f))).collect())
    }

    /// The readers the last layout left unsettled, whose inputs are not
    /// within half a device pixel of their measurements, the boxes whose
    /// taking of the band had not settled, and the parents whose bars'
    /// slots had not (D9, D4, D12, `exactSafeAreaUnsettled()`).
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
        let arena = &self.arena;
        let quiet = arena.readers.observers.is_empty() && arena.ignoring.is_empty();
        if quiet && !super::bar::slotted(arena) {
            // Nothing under `slot` reads, ignores or has slots past what it observes, so
            // nothing there is unsettled
            let unsettled = &arena.observations.unsettled;
            let others = (unsettled.iter().copied()).filter(|&s| !within(arena, s, slot));
            let others: Vec<u32> = others.collect();
            self.arena.observations.unsettled = others;
            return Ok(());
        }
        let tolerance = 0.5 / self.arena.display_scale.unwrap_or(1.0);
        let observations = &mut self.arena.observations;
        if observations.tolerance != tolerance {
            observations.tolerance = tolerance;
            observations.invalidate();
        }
        let mut moved = std::mem::take(&mut observations.moved);
        // Walk each moved subtree once, though a mount moves every box in it
        let mut seen: IdSet<u32> = IdSet::default();
        while let Some(s) = moved.pop() {
            if seen.insert(s) && self.arena.is_live(s) {
                let observations = &mut self.arena.observations;
                observations.readouts.remove(&s);
                observations.takings.measured.remove(&s);
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
                // A box with no layout box, `display: none` or under one, has
                // nothing to measure: it keeps its input until it has a box
                if self.arena.flags(s).has(crate::id::NodeFlags::HIDDEN) {
                    let owner = band_at(self, s).owner;
                    let observations = &mut self.arena.observations;
                    let kept = Readout::kept(observations.input(s), owner);
                    observations.readouts.insert(s, kept);
                    continue;
                }
                let space = band_at(self, s);
                let readout = measure(&self.arena, s, &space);
                let (measured, caps) = (readout.observed, readout.depths);
                let observations = &mut self.arena.observations;
                if readout.top_layer || readout.projects {
                    observations.always.insert(s);
                }
                observations.readouts.insert(s, readout);
                let input = observations.input(s);
                let components = self.arena.readers.observed[&s];
                if settled(input, measured, tolerance, components) {
                    continue;
                }
                off.push(s);
                if round == MAX_ROUNDS {
                    continue;
                }
                let samples = solving.entry(s).or_default();
                let next = samples.step(input, measured, caps, tolerance, components);
                if next != input {
                    self.arena.observations.inputs.insert(s, next);
                    moved.push(s);
                }
            }
            // The boxes that took more or less lay out again with it (D4),
            // and at the bound are left as they are, unsettled
            let taking = self.check_taken(&takers, round < MAX_ROUNDS);
            // And so do the parents whose bars' slots moved (D12)
            let slotting = self.check_slots(slot, round < MAX_ROUNDS);
            if round == MAX_ROUNDS {
                off.extend(taking);
                off.extend(slotting);
                break;
            }
            if moved.is_empty() && taking.is_empty() && slotting.is_empty() {
                break;
            }
            // A capture's readers take its declarations' values anew (D5)
            let captured = super::readers::Readers::captured(&mut self.arena, &moved);
            self.restyle_readers(&moved);
            self.restyle_readers(&captured);
            self.restyle_measured(&taking);
            self.restyle_measured(&slotting);
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
        // A box off as a reader and as a taker or a parent of bars is listed once
        off.sort_unstable();
        off.dedup();
        self.arena.observations.unsettled.extend(off);
        Ok(())
    }

    /// The readers under `slot` to measure after `receipt`'s layout, in
    /// order: those whose frames it moved, those with no readout, those
    /// measured at every layout (in a top-layer box, whose place in the
    /// viewport can move without their frames, or whose margins project a
    /// superview's), and `also`; none of `skip`.
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
            .always
            .retain(|s| readouts.get(s).is_some_and(|r| r.top_layer || r.projects));
        let (arena, observations) = (&self.arena, &self.arena.observations);
        let observers = &arena.readers.observers;
        let mut out: Vec<u32> = (receipt.changed.iter())
            .map(|k| k.index)
            .filter(|&s| observers.contains(s))
            .collect();
        let mine = |s: &u32| within(arena, *s, slot);
        out.extend(observations.always.iter().filter(|s| mine(s)));
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
            if observations.inputs.insert(s, [None; COMPONENTS]) != Some([None; COMPONENTS]) {
                changed.push(s);
            }
        }
        // A withheld capture is invalid, so its readers take their fallbacks (D5)
        let captured = super::readers::Readers::captured(&mut self.arena, &changed);
        self.restyle_readers(&changed);
        self.restyle_readers(&captured);
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
        if arena.reserving.is_empty()
            && arena.observations.bars.styled.is_empty()
            && arena.observations.reserving.is_empty()
        {
            return;
        }
        // The boxes that reserve, and those whose bars reserve (D12)
        let barred = super::bar::parents(arena);
        let reserves = |s: u32| arena.reserving.contains(s) || barred.binary_search(&s).is_ok();
        // A box that stopped reserving moved what its descendants observe
        let held = &arena.observations.reserving;
        let stopped = held.keys().copied().filter(|&s| !reserves(s));
        let mut moved: IdSet<u32> = stopped.collect();
        let mut changed = Vec::new();
        let mut all: Vec<u32> = arena.reserving.iter().collect();
        all.extend(
            barred
                .iter()
                .copied()
                .filter(|&s| !arena.reserving.contains(s)),
        );
        for s in all {
            let f = arena.frame(s);
            let [left, top, right, bottom] = arena.inset(s).unwrap_or_default().border;
            let edges = [
                f.y + top,
                f.x + f.width - right,
                f.y + f.height - bottom,
                f.x + left,
            ];
            let now = (
                reservation::reserved(arena, s),
                edges,
                super::bar::reserved(arena, s, None),
            );
            if held.get(&s) != Some(&now) {
                moved.insert(s);
                changed.push((s, now));
            }
        }
        if moved.is_empty() {
            return;
        }
        let observations = &mut self.arena.observations;
        observations
            .reserving
            .retain(|s, _| !moved.contains(s) || changed.iter().any(|(c, _)| c == s));
        observations.reserving.extend(changed);
        self.forget_under(&moved);
    }

    /// Measure again the readers and the boxes that ignore below any of
    /// `roots`, whose reservations or scrollers changed what reaches them.
    fn forget_under(&mut self, roots: &IdSet<u32>) {
        let arena = &self.arena;
        let under = |slot: u32| {
            let mut at = arena.parent(slot);
            while let Some(a) = at {
                if roots.contains(&a) {
                    return true;
                }
                at = arena.parent(a);
            }
            false
        };
        let observations = &arena.observations;
        let readers: Vec<u32> = (observations.readouts.keys().copied())
            .filter(|&r| under(r))
            .collect();
        let takers: Vec<u32> = (observations.takings.measured.iter().copied())
            .filter(|&t| under(t))
            .collect();
        let observations = &mut self.arena.observations;
        for r in readers {
            observations.readouts.remove(&r);
        }
        for t in takers {
            observations.takings.measured.remove(&t);
        }
    }
}

impl Kernel {
    /// Measure again the readers under each scroller that ignores whose
    /// passing of the area, or of a docked keyboard, to its content changed,
    /// which moves what they observe though their own frames need not move.
    fn check_scrollers(&mut self) {
        let arena = &self.arena;
        let observations = &arena.observations;
        if arena.ignoring.is_empty()
            && observations.passing.is_empty()
            && observations.passing_keyboard.is_empty()
        {
            return;
        }
        let (mut moved, mut keyboard) = (Vec::new(), Vec::new());
        for s in arena.ignoring.iter().filter(|&s| scroll_view(arena, s)) {
            let space = band_at(self, s);
            let now = space
                .band
                .map_or([false; 4], |b| passed(arena, s, &space, b));
            if observations.passing.get(&s).map_or([false; 4], |p| *p) != now {
                moved.push((s, now));
            }
            let passes = super::keyboard::passing(arena, s, &space);
            if passes != observations.passing_keyboard.contains(&s) {
                keyboard.push((s, passes));
            }
        }
        // A scroller that stopped ignoring or scrolling passes nothing
        let gone = |s: &u32| !arena.ignoring.contains(*s) || !scroll_view(arena, *s);
        let stopped = (observations.passing.keys()).filter(|s| gone(s));
        moved.extend(stopped.map(|&s| (s, [false; 4])));
        let stopped = (observations.passing_keyboard.iter()).filter(|s| gone(s));
        keyboard.extend(stopped.map(|&s| (s, false)));
        if moved.is_empty() && keyboard.is_empty() {
            return;
        }
        let roots: IdSet<u32> = (moved.iter().map(|&(s, _)| s))
            .chain(keyboard.iter().map(|&(s, _)| s))
            .collect();
        let observations = &mut self.arena.observations;
        for (s, now) in moved {
            match now.contains(&true) {
                true => observations.passing.insert(s, now),
                false => observations.passing.remove(&s),
            };
        }
        for (s, passes) in keyboard {
            match passes {
                true => observations.passing_keyboard.insert(s),
                false => observations.passing_keyboard.remove(&s),
            };
        }
        self.forget_under(&roots);
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
/// reader's `components`, an unavailable one matching only an unavailable
/// one.
fn settled(input: Observed, measured: Observed, tolerance: f32, components: Components) -> bool {
    (0..COMPONENTS)
        .filter(|e| components & 1 << e != 0)
        .all(|e| match (input[e], measured[e]) {
            (Some(i), Some(m)) => (i - m).abs() <= tolerance,
            (None, None) => true,
            _ => false,
        })
}

/// A reader's last sample on each component, for the secant step.
struct Samples {
    /// The input and residual of its round before last, per component.
    last: [Option<(f32, f32)>; COMPONENTS],
}

impl Default for Samples {
    fn default() -> Self {
        Samples {
            last: [None; COMPONENTS],
        }
    }
}

impl Samples {
    /// The next input, from this round's `input` and `measured`: on each of
    /// the reader's `components` not within `tolerance`, the secant step on
    /// the residuals of this round and the one before, else plain
    /// substitution; a component it does not read keeps its input. A step
    /// lands within what an observation can be, 0 to its most `caps` (the
    /// area's depth, the keyboard's for its readings, and for the layout
    /// regions their own margins with the area's depth); one whose
    /// candidate lies behind the input, against its residual, takes the end
    /// of that range the residual points to: the measurement outruns the
    /// line there, steeper than it, up to where it stops at that end. So
    /// does a residual that repeats while the input moved: the measurement
    /// moves one for one with the input, as a box's reading does where its
    /// own edge moves by it, and stops only at that end.
    /// Telling a round that crosses a layout breakpoint (a wrap, a track
    /// change), which D9 also has substitute, needs a signal from layout
    /// that it does not give yet.
    fn step(
        &mut self,
        input: Observed,
        measured: Observed,
        caps: [f32; COMPONENTS],
        tolerance: f32,
        components: Components,
    ) -> Observed {
        let mut next = input;
        for e in (0..COMPONENTS).filter(|e| components & 1 << e != 0) {
            let (Some(x), Some(m)) = (input[e], measured[e]) else {
                // Take an unavailable component as measured: its band is unavailable throughout
                next[e] = measured[e];
                self.last[e] = None;
                continue;
            };
            let r = m - x;
            next[e] = Some(match self.last[e] {
                // Keep a settled edge's input while another moves
                _ if r.abs() <= tolerance => x,
                Some((x0, r0)) if x != x0 && r == r0 => match r > 0.0 {
                    true => caps[e],
                    false => 0.0,
                },
                Some((x0, r0)) if x != x0 => {
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
    /// The most each component can be, past which the solve tries none
    /// (`Samples::step`): the unsafe area's depth on each edge, the
    /// keyboard's depth across it for its readings, and a layout region's
    /// own margins with the area's depth, or a superview's, for its.
    depths: [f32; COMPONENTS],
    /// The owner whose band it measured, `None` for the session's.
    owner: Option<u32>,
    /// Whether it is in a document top-layer box.
    top_layer: bool,
    /// Whether it reads margins that project a superview's, which move when
    /// a box above it does (D8).
    projects: bool,
}

impl Readout {
    /// The readout of a reader with no layout box: the input it keeps, in
    /// its `owner`.
    fn kept(input: Observed, owner: Option<u32>) -> Self {
        Readout {
            observed: input,
            depths: [0.0; COMPONENTS],
            owner,
            top_layer: false,
            projects: false,
        }
    }
}

/// What `slot` observes on each edge of its `space` on the layout just
/// made: of the safe area, as UIKit gives a view its `safeAreaInsets`, the
/// distance from the box's edge to the unsafe area's inner side, at least 0
/// and at most the area's depth, whatever the box's own size (D9); of the
/// keyboard, as UIKit gives a view its keyboard guide (D7); and of the layout
/// regions, as UIKit gives a view's margins and readable content (D8).
fn measure(arena: &NodeArena, slot: u32, space: &Space) -> Readout {
    // Each family is measured only for a box that reads it
    let read = arena.readers.observed.get(&slot).copied().unwrap_or(0);
    measure_families(arena, slot, space, read)
}

/// [`measure`] of the families whose components `read` holds, whether or
/// not the box reads them.
fn measure_families(arena: &NodeArena, slot: u32, space: &Space, read: Components) -> Readout {
    let (mut observed, mut depths) = ([None; COMPONENTS], [0.0; COMPONENTS]);
    // The margins families follow D8's rule where the owner's host reports
    // its margins, and resolve to the safe area where it reports none (D10,
    // "Other hosts")
    let reported = space.owner.and_then(|o| arena.observations.margins.get(&o));
    if let (Some(owner), Some(band)) = (reported, space.band.filter(|_| read & margins_bits() != 0))
    {
        let regions = super::margins::regions(arena, slot, space, band, owner);
        let families = Family::MARGINS.into_iter().chain([Family::SystemMinimum]);
        for (i, family) in families.enumerate() {
            let at = family as usize * 4;
            for e in 0..4 {
                observed[at + e] = Some(regions.readings[i][e]);
                depths[at + e] = regions.caps[i][e];
            }
        }
    }
    let families = || {
        std::iter::once(Family::SafeArea)
            .chain(Family::MARGINS.into_iter().filter(|_| reported.is_none()))
            .filter(|f| read & f.bits() != 0)
    };
    if let Some(band) = space.band.filter(|_| families().next().is_some()) {
        let sides = area(arena, slot, space, band);
        let edges = space.edges(arena, slot);
        for family in families() {
            let at = family as usize * 4;
            for e in 0..4 {
                observed[at + e] = Some(depth(sides[e], edges[e]));
                depths[at + e] = (sides[e][1] - sides[e][0]).max(0.0);
            }
        }
    }
    if read & Family::Bar.bits() != 0 {
        let [left, right] = space
            .owner
            .and_then(|owner| arena.observations.side_bars.get(&owner))
            .copied()
            .unwrap_or_default();
        let at = Family::Bar as usize * 4;
        let physical = [0.0, right, 0.0, left];
        observed[at..at + 4].copy_from_slice(&physical.map(Some));
        depths[at..at + 4].copy_from_slice(&physical);
    }
    let keyboard = super::keyboard::reads_keyboard(read)
        .then(|| super::keyboard::readings(arena, slot, space))
        .flatten();
    if let Some((readings, caps)) = keyboard {
        for (family, reading) in [Family::Keyboard, Family::KeyboardUndocked]
            .into_iter()
            .zip(readings)
        {
            let at = family as usize * 4;
            for e in 0..4 {
                observed[at + e] = Some(reading[e]);
                depths[at + e] = caps[e];
            }
        }
    }
    Readout {
        observed,
        depths,
        owner: space.owner,
        top_layer: space.root.is_some() && space.owner.is_none(),
        projects: reported.is_some()
            && read & margins_bits() != 0
            && super::margins::preserves(arena, slot),
    }
}

/// The components of D8's families, the system minimum included.
pub(crate) fn margins_bits() -> Components {
    Family::MARGINS
        .iter()
        .fold(Family::SystemMinimum.bits(), |bits, f| bits | f.bits())
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

/// Why an owner's band and Exact's contributions to it are refused, if they are:
/// nonfinite insets, a nonfinite effect, or a contribution's identity twice. A negative
/// raw inset is taken: UIKit gives one to a window's root under a negative addition past
/// its insets (D11), and the incoming band, raw less the effects, is at least 0.
pub(super) fn check_band(
    band: Option<&[f32; 4]>,
    contributions: &[Contribution],
) -> Result<(), KernelError> {
    if band.is_some_and(|b| !b.iter().all(|v| v.is_finite())) {
        return Err(LayoutError::InvalidEnv.into());
    }
    provenance::check(contributions)
}
