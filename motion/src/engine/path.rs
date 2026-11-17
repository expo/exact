//! A list value under `transition`: a path's `d` (LLP 1055.000 D15) and a
//! `filter` (LLP 1003.002 D6).
//!
//! @ref CSS Transitions 1 §3 (starting, interrupting, reversing)
//!
//! Neither is a [`Value`], so its transition runs in two parts: the node's
//! slot for the property ([`Property::D`], [`Property::Filter`]) runs the
//! curve, a progress from 0 to 1 under the matching declaration, and a
//! [`Track`] holds the values at its ends. The presented value is the one
//! between them at that progress. The slot takes part in everything a curve
//! does — the clock, `frame`, `settle_time`, removal — so a host repaints
//! the value as it would a radius.
//!
//! CSS's rules, by value: a node first seen takes its value with no
//! transition; a new value starts one from the value presented now, when
//! the two interpolate and a declaration covers the property; otherwise
//! the value changes at once and anything running stops. A change back to
//! where a running transition began reverses it with CSS's shortening.
//! Neither springs as physics: a `-exact-spring()` plays its curve from
//! rest (LLP 1062 D3).

use super::{Engine, Live, Slot};
use crate::filter::FilterValue;
use crate::path::PathValue;
use crate::property::{Property, Value};
use crate::transition::{Curve, Running};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A value the engine moves as a progress between two ends.
pub(super) trait Tracked: Clone + PartialEq + Sized {
    /// The property whose slot runs the progress.
    const PROPERTY: Property;
    fn interpolable(&self, other: &Self) -> bool;
    fn lerp(&self, to: &Self, progress: f64) -> Self;
    fn is_finite(&self) -> bool;
    fn tracks(engine: &Engine) -> &BTreeMap<u64, Track<Self>>;
    fn tracks_mut(engine: &mut Engine) -> &mut BTreeMap<u64, Track<Self>>;
    /// The value as the track keeps it.
    fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }
    /// Whether a settled track keeps the value: a settled `none` is what
    /// no track means for a node the engine knew.
    fn kept(&self) -> bool {
        true
    }
}

/// `none`, which every node whose `transition` covers `filter` and that
/// has none keeps: one allocation, not one a node.
static NO_FILTER: std::sync::LazyLock<Arc<FilterValue>> =
    std::sync::LazyLock::new(|| Arc::new(FilterValue::default()));

impl Tracked for PathValue {
    const PROPERTY: Property = Property::D;
    fn interpolable(&self, other: &Self) -> bool {
        PathValue::interpolable(self, other)
    }
    fn lerp(&self, to: &Self, progress: f64) -> Self {
        PathValue::lerp(self, to, progress)
    }
    fn is_finite(&self) -> bool {
        PathValue::is_finite(self)
    }
    fn tracks(engine: &Engine) -> &BTreeMap<u64, Track<Self>> {
        &engine.paths
    }
    fn tracks_mut(engine: &mut Engine) -> &mut BTreeMap<u64, Track<Self>> {
        &mut engine.paths
    }
}

impl Tracked for FilterValue {
    const PROPERTY: Property = Property::Filter;
    fn interpolable(&self, other: &Self) -> bool {
        FilterValue::interpolable(self, other)
    }
    fn lerp(&self, to: &Self, progress: f64) -> Self {
        FilterValue::lerp(self, to, progress)
    }
    fn is_finite(&self) -> bool {
        FilterValue::is_finite(self)
    }
    fn tracks(engine: &Engine) -> &BTreeMap<u64, Track<Self>> {
        &engine.filters
    }
    fn tracks_mut(engine: &mut Engine) -> &mut BTreeMap<u64, Track<Self>> {
        &mut engine.filters
    }
    fn shared(self) -> Arc<Self> {
        if self.0.is_empty() {
            NO_FILTER.clone()
        } else {
            Arc::new(self)
        }
    }
    fn kept(&self) -> bool {
        !self.0.is_empty()
    }
}

/// One node's value and its transition's ends.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Track<T> {
    /// The value the row says now.
    pub(super) target: Arc<T>,
    /// The value the running transition starts from.
    pub(super) from: Arc<T>,
    /// CSS §3.2: the value a reversal is measured against.
    reversing_adjusted_start: Arc<T>,
}

impl<T> Track<T> {
    fn settled(value: Arc<T>) -> Track<T> {
        Track {
            from: value.clone(),
            reversing_adjusted_start: value.clone(),
            target: value,
        }
    }
}

impl Engine {
    /// A committed path's `d`: `None` when it is not a path that moves (its
    /// `transition` covers no `d`, or the data has an error), which forgets
    /// the path and stops its transition. Otherwise CSS Transitions §3, as
    /// [`Engine::observe`] for a value.
    pub fn observe_path(&mut self, node: u64, path: Option<PathValue>) {
        (self.links.observe_path)(self, node, path)
    }

    pub(super) fn observe_path_full(&mut self, node: u64, path: Option<PathValue>) {
        self.observe_track(node, path, None)
    }

    /// A committed `filter` (LLP 1003.002 D6), `none` included: `None` for
    /// one that never interpolates (it names a `filter` element), which
    /// forgets it and stops its transition. `new`: the engine has not heard
    /// of the node before (it was created or renewed), so its first filter
    /// starts nothing; a node it knew and kept no filter for had `none`.
    /// Otherwise as [`Engine::observe_path`].
    pub fn observe_filter(&mut self, node: u64, filter: Option<FilterValue>, new: bool) {
        (self.links.observe_filter)(self, node, filter, new)
    }

    pub(super) fn observe_filter_full(
        &mut self,
        node: u64,
        filter: Option<FilterValue>,
        new: bool,
    ) {
        self.observe_track(node, filter, (!new).then(FilterValue::default))
    }

    /// CSS Transitions §3 for a list value. `before`: what a node with no
    /// track had (`None`: the node is first seen, and takes its value with
    /// no transition).
    fn observe_track<T: Tracked>(&mut self, node: u64, value: Option<T>, before: Option<T>) {
        let key = (node, T::PROPERTY);
        let Some(value) = value.filter(T::is_finite) else {
            if T::tracks_mut(self).remove(&node).is_some() {
                self.running.remove(&key);
                self.played.remove(&key);
                self.slots.remove(&key);
                self.dirty.remove(&key);
            }
            return;
        };
        let track = match (T::tracks(self).get(&node).cloned(), before) {
            (Some(track), _) => track,
            (None, Some(before)) if before != value => Track::settled(before.shared()),
            (None, _) => {
                if value.kept() {
                    T::tracks_mut(self).insert(node, Track::settled(value.shared()));
                }
                return;
            }
        };
        if *track.target == value {
            let running = self.slots.get(&key).and_then(Slot::running).is_some();
            if !running && !value.kept() {
                T::tracks_mut(self).remove(&node);
                if self.slots.remove(&key).is_some() {
                    self.dirty.insert(key);
                }
                return;
            }
            if self
                .transitions
                .get(&node)
                .is_some_and(|t| t.matching(T::PROPERTY).is_none())
            {
                self.played.remove(&key);
                self.running.remove(&key);
                self.pending.remove(&key);
                self.slots.remove(&key);
                self.dirty.insert(key);
            }
            return;
        }
        // Any new value takes the curve back from the host
        self.played.remove(&key);
        let now = self.sample_time();
        let running = self.slots.get(&key).and_then(Slot::running).cloned();
        let current = match &running {
            Some(r) => Arc::new(track.from.lerp(&track.target, r.sample(now).value.x)),
            None => track.target.clone(),
        };
        let declaration = self
            .transitions
            .get(&node)
            .and_then(|t| t.matching(T::PROPERTY))
            .filter(|t| t.starts())
            .map(|t| t.governing(T::PROPERTY));
        let value = value.shared();
        let Some(declaration) =
            declaration.filter(|_| current.interpolable(&value) && *current != *value)
        else {
            // Not interpolable, or nothing covers the property: at once.
            if value.kept() {
                T::tracks_mut(self).insert(node, Track::settled(value));
            } else {
                T::tracks_mut(self).remove(&node);
            }
            self.running.remove(&key);
            if self.slots.remove(&key).is_some() {
                self.dirty.insert(key);
            }
            return;
        };
        let reverses = running
            .as_ref()
            .filter(|r| matches!(r.curve, Curve::Easing { .. }))
            .filter(|_| *value == *track.reversing_adjusted_start);
        let (adjusted, factor) = match reverses {
            // CSS §3.2, the reversing case.
            Some(r) => (
                track.target.clone(),
                (r.easing_progress(now) * r.reversing_shortening + (1.0 - r.reversing_shortening))
                    .abs()
                    .clamp(0.0, 1.0),
            ),
            None => (current.clone(), 1.0),
        };
        let mut curve = Running::start(
            &declaration,
            T::PROPERTY,
            (Value::scalar(0.0), Value::scalar(1.0)),
            Value::ZERO,
            now,
            Value::scalar(0.0),
            factor,
        );
        T::tracks_mut(self).insert(
            node,
            Track {
                target: value,
                from: current,
                reversing_adjusted_start: adjusted,
            },
        );
        curve.pending = self.holds().then_some(now);
        if curve.pending.is_some() {
            self.pending.insert(key);
        }
        let presented = curve.sample(now).value;
        self.keep(
            key,
            Slot {
                target: Value::scalar(1.0),
                live: Some(Box::new(Live {
                    presented,
                    running: Some(curve),
                    owner: None,
                })),
            },
        );
        self.running.insert(key);
        self.dirty.insert(key);
    }

    /// The value a host draws for `node` now: the one between a running
    /// transition's ends, or `None` while nothing runs (the row shows).
    fn presented_track<T: Tracked>(&self, node: u64) -> Option<T> {
        let slot = self.slots.get(&(node, T::PROPERTY))?;
        slot.running()?;
        let track = T::tracks(self).get(&node)?;
        Some(track.from.lerp(&track.target, slot.presented().x))
    }

    /// A running `d` transition's two paths, start and target.
    pub fn path_ends(&self, node: u64) -> Option<(Arc<PathValue>, Arc<PathValue>)> {
        self.slots.get(&(node, Property::D))?.running()?;
        let track = self.paths.get(&node)?;
        Some((track.from.clone(), track.target.clone()))
    }

    /// The path a host draws for `node` now: the one between a running
    /// transition's ends, or `None` while nothing runs (the row shows).
    pub fn presented_path(&self, node: u64) -> Option<PathValue> {
        self.presented_track(node)
    }

    /// The filter a host draws for `node` now (LLP 1003.002): the list
    /// between a running transition's ends, or `None` while nothing runs
    /// (the row shows).
    pub fn presented_filter(&self, node: u64) -> Option<FilterValue> {
        self.presented_track(node)
    }
}
