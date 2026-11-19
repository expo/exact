//! The native targets owners and the session report, and what their raw
//! readers resolve to (@ref LLP 1123 D2's owner update, D9 "The native
//! chain").
//!
//! A raw observation is a target view's insets in its own coordinates, so a
//! reader of one needs no solve: its value is its holder's report, which
//! its environment carries ([`Targets::scope`]). A reader is derived again
//! when a report changes an edge it reads or a target's availability, and
//! when a move, a block gained or given back, or a box joining or leaving
//! the top layer gives it another holder.

use super::area::{holder, holds, Holder};
use super::Kernel;
use crate::arena::NodeArena;
use crate::error::{KernelError, LayoutError};
use crate::id::{IdMap, ViewId};
use crate::style::targets::{self, NativeTarget, NativeTargets, Registry, Scope, Target};
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub(crate) struct Targets {
    /// Each owner root's reports.
    owners: IdMap<u32, Reports>,
    /// The session view's, which a box outside every owner reads.
    session: Reports,
    /// The targets native code registered for the session; a removed one
    /// stays, unavailable, so its generation still refuses stale reports.
    registered: Arc<Registry>,
    /// The boxes that joined or left the top layer since the last layout,
    /// whose subtrees may have another holder.
    pub(crate) moved: Vec<u32>,
}

impl Targets {
    /// The session's reports and the registry, which a reset keeps.
    pub(crate) fn after_reset(&mut self) -> Self {
        Targets {
            session: std::mem::take(&mut self.session),
            registered: self.registered.clone(),
            ..Default::default()
        }
    }
}

/// An owner's or the session's last report, and the highest generation any
/// report gave each target, which refuses a stale report even after its
/// target left the reports.
#[derive(Debug, Clone, Default)]
struct Reports {
    last: Option<Arc<NativeTargets>>,
    seen: IdMap<u64, u64>,
}

impl Targets {
    /// The highest generation the owner `slot`'s reports gave each target,
    /// against which a new report is checked (`NativeTargets::refusal`).
    pub(crate) fn seen(&self, slot: u32) -> Option<&crate::id::IdMap<u64, u64>> {
        self.owners.get(&slot).map(|r| &r.seen)
    }

    /// What `slot` reads raw insets from, where it reads any.
    pub(crate) fn scope(&self, arena: &NodeArena, slot: u32) -> Option<Scope> {
        if !arena.readers.targets.contains_key(&slot) {
            return None;
        }
        Some(Scope {
            placed: self.placed(holder(arena, slot)),
            registered: self.registered.clone(),
        })
    }

    fn placed(&self, holder: Holder) -> Option<Arc<NativeTargets>> {
        match holder {
            Holder::Owner(s) => self.owners.get(&s)?.last.clone(),
            Holder::TopLayer(_) | Holder::Session => self.session.last.clone(),
        }
    }

    /// Whether any owner or the session reported targets, which a box reads
    /// by where it is.
    pub(crate) fn placed_any(&self) -> bool {
        self.session.last.is_some() || !self.owners.is_empty()
    }

    pub(crate) fn forget(&mut self, slot: u32) {
        self.owners.remove(&slot);
    }
}

impl Kernel {
    /// Set the native targets of the content owner whose root is `view`
    /// (LLP 1123 D2's owner update, D9 "The native chain"): its containment
    /// chain from its controller up and its relational targets, each with
    /// its identity, generation and raw insets, `None` while unavailable.
    /// Its readers of a target whose insets changed are derived again;
    /// returns whether any was (a layout is owed then). A report is refused
    /// whole: nonfinite or negative insets, a target at a generation older
    /// than an earlier report gave it, a chain entry of kind `owner` or
    /// `parent` (entries 1 and 2 by position) or of a kind bit past the
    /// kinds, a relation twice, or a chain past 255.
    /// A host reports this field with the owner's others through
    /// [`Kernel::update_owner`], which applies them whole or not at all.
    pub fn set_owner_targets(
        &mut self,
        view: ViewId,
        targets: NativeTargets,
    ) -> Result<bool, KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        let reports = self.arena.observations.targets.owners.get(&slot);
        let readers = self.report(&targets, reports, self.held_by(slot))?;
        let store = &mut self.arena.observations.targets;
        store.owners.entry(slot).or_default().take(targets);
        let readers = self.with_captures(readers);
        self.restyle_readers(&readers);
        Ok(!readers.is_empty())
    }

    /// Set the session view's native targets, which a box outside every
    /// owner reads (LLP 1123 D9), as [`Kernel::set_owner_targets`] does an
    /// owner's.
    pub fn set_session_targets(&mut self, targets: NativeTargets) -> Result<bool, KernelError> {
        let arena = &self.arena;
        let outside = (arena.readers.targets.keys().copied())
            .filter(|&s| matches!(holder(arena, s), Holder::TopLayer(_) | Holder::Session))
            .collect();
        let reports = Some(&self.arena.observations.targets.session);
        let readers = self.report(&targets, reports, outside)?;
        self.arena.observations.targets.session.take(targets);
        let readers = self.with_captures(readers);
        self.restyle_readers(&readers);
        Ok(!readers.is_empty())
    }

    /// Register a native target for the session under `name`, which
    /// `--exact-target-<name>-safe-area-inset-*` reads anywhere (LLP 1123
    /// D9, "Registration"), or remove it (`None`). Returns whether a layout
    /// is owed, as [`Kernel::set_owner_targets`] does. Refused: a name other
    /// than ASCII lowercase letters, digits and inner hyphens, up to 64; a
    /// generation older than the name's last registration, a removed one
    /// included; and another view at that generation, since a replacement
    /// takes a newer one.
    pub fn set_registered_target(
        &mut self,
        name: &str,
        target: Option<NativeTarget>,
    ) -> Result<bool, KernelError> {
        if !targets::valid_name(name) {
            return Err(LayoutError::NativeTarget(
                "a registered name must be ASCII lowercase letters, digits and inner hyphens, at most 64; rename the target",
            )
            .into());
        }
        let registered = &self.arena.observations.targets.registered;
        let last = registered.get(name).copied();
        let refusal = target.and_then(|t| targets::registration_refusal(&t, last.as_ref()));
        if let Some(why) = refusal {
            return Err(LayoutError::NativeTarget(why).into());
        }
        // Keep a removed registration, unavailable, for its generation
        let Some(next) = target.or(last.map(|l| NativeTarget { insets: None, ..l })) else {
            return Ok(false);
        };
        if last == Some(next) {
            return Ok(false);
        }
        let key: Arc<str> = name.into();
        let read = Target::Registered(key.clone());
        let (old, new) = (last.and_then(|l| l.insets), next.insets);
        let raw = self.arena.readers.targets.keys().copied().collect();
        let readers = self.reading(raw, |t, edges| *t == read && changes_read(old, new, edges));
        let registered = Arc::make_mut(&mut self.arena.observations.targets.registered);
        registered.insert(key, next);
        let readers = self.with_captures(readers);
        self.restyle_readers(&readers);
        Ok(!readers.is_empty())
    }

    /// `readers`, whose read insets a report or a holder change moved, and
    /// below each that captures a target, each node whose lengths take the
    /// capture's value (LLP 1123 D5).
    pub(super) fn with_captures(&mut self, mut readers: Vec<u32>) -> Vec<u32> {
        readers.extend(super::readers::Readers::captured_targets(
            &mut self.arena,
            &readers,
        ));
        readers.sort_unstable();
        readers.dedup();
        readers
    }

    /// Of `held`, the raw readers a report holds, those whose read insets
    /// `new` changes from the last of `reports`; or why `new` is refused.
    fn report(
        &self,
        new: &NativeTargets,
        reports: Option<&Reports>,
        held: Vec<u32>,
    ) -> Result<Vec<u32>, KernelError> {
        let none = Reports::default();
        let reports = reports.unwrap_or(&none);
        if let Some(why) = new.refusal(&reports.seen) {
            return Err(LayoutError::NativeTarget(why).into());
        }
        let empty = NativeTargets::default();
        let old = reports.last.as_deref().unwrap_or(&empty);
        Ok(self.reading(held, |t, edges| {
            changes_read(old.insets(t), new.insets(t), edges)
        }))
    }

    /// Those of `readers` for which `changed` takes a target they read and
    /// the edges they read of it, in order.
    fn reading(&self, readers: Vec<u32>, changed: impl Fn(&Target, u8) -> bool) -> Vec<u32> {
        let raw = &self.arena.readers.targets;
        let mut out: Vec<u32> = (readers.into_iter())
            .filter(|s| {
                raw.get(s)
                    .is_some_and(|read| read.iter().any(|(t, e)| changed(t, *e)))
            })
            .collect();
        out.sort_unstable();
        out
    }

    /// The raw readers `owner` holds.
    fn held_by(&self, owner: u32) -> Vec<u32> {
        match holder(&self.arena, owner) == Holder::Owner(owner) {
            true => self.held_through(owner),
            false => Vec::new(),
        }
    }

    /// The raw readers whose holder is `top`'s, or `top` itself: those in
    /// its subtree short of the boxes below it that hold their own. A walk
    /// down finds them where the subtree is small, as a local inset's
    /// readers are found (`readers::walk_down`); else each reader's
    /// ancestors are checked.
    pub(super) fn held_through(&self, top: u32) -> Vec<u32> {
        let raw = &self.arena.readers.targets;
        let readers = raw.keys().copied();
        held_among(
            &self.arena,
            top,
            raw.len(),
            |s| raw.contains_key(&s),
            readers,
        )
    }

    /// Derive again the raw readers and the owner segments' readers whose
    /// holder is `slot`'s or `slot` ([`Kernel::held_through`]), after a block
    /// gained or given back, or a top-layer change, that may give them another.
    pub(super) fn targets_moved(&mut self, slot: u32) {
        let mut readers = Vec::new();
        if self.arena.observations.targets.placed_any() {
            readers.extend(self.held_through(slot));
        }
        // An owner segment's reader reads its holder's report as a raw
        // reader does (D16)
        if super::owner::segments_differ(&self.arena) {
            readers.extend(self.segment_readers_through(slot));
        }
        let readers = self.with_captures(readers);
        self.restyle_readers(&readers);
    }
}

impl Reports {
    /// Take `report` as the last, raising each target's highest generation.
    fn take(&mut self, report: NativeTargets) {
        for t in report.all() {
            let seen = self.seen.entry(t.id).or_insert(t.generation);
            *seen = (*seen).max(t.generation);
        }
        self.last = Some(Arc::new(report));
    }
}

/// The readers whose holder is `top`'s or `top` itself, of a set of `count`
/// for which `is_reader` holds and which `readers` lists, found by a walk
/// down from `top` where that is shorter than the set
/// ([`Kernel::held_through`]).
pub(super) fn held_among(
    arena: &NodeArena,
    top: u32,
    count: usize,
    is_reader: impl Fn(u32) -> bool,
    readers: impl Iterator<Item = u32>,
) -> Vec<u32> {
    if count == 0 {
        return Vec::new();
    }
    let enter = |c: u32| holds(arena, c).is_none();
    let mut out: Vec<u32> = match super::readers::walk_down(arena, top, 2 * count + 1, enter) {
        Some(nodes) => nodes.into_iter().filter(|&s| is_reader(s)).collect(),
        None => readers.filter(|&s| through(arena, s, top)).collect(),
    };
    out.sort_unstable();
    out
}

/// Whether `reader` is `top` or below it short of a box that holds its own.
fn through(arena: &NodeArena, reader: u32, top: u32) -> bool {
    let mut at = Some(reader);
    while let Some(s) = at {
        if s == top {
            return true;
        }
        if holds(arena, s).is_some() {
            return false;
        }
        at = arena.parent(s);
    }
    false
}

/// Whether a target's insets `old` to `new` change what a reader of
/// `edges` (a bit per edge) reads: its availability, or a value on one.
fn changes_read(old: Option<[f32; 4]>, new: Option<[f32; 4]>, edges: u8) -> bool {
    match (old, new) {
        (Some(old), Some(new)) => (0..4).any(|e| edges & 1 << e != 0 && old[e] != new[e]),
        (old, new) => old.is_some() != new.is_some(),
    }
}
