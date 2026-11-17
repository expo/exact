//! Motion starts at the first frame that shows it (LLP 1003.001).
//!
//! On a host whose display drives the clock, a curve an author's commit
//! begins between frames is pending (held): every reading of it is at the
//! time it began, so it does not move, and the present of the first picture
//! that carries it starts it there (D1, D2, D9). The engine keeps two times
//! (D5): `now`, the input clock, which inputs, holds and releases move and
//! check against; and `shown`, the latest frame it presented. Every sample
//! is at the later of the two. Where nothing will present, a hold is
//! released at its own commit (D9, "The bound").

use super::{Engine, EngineError};
use crate::property::Property;

/// How long, in seconds, a held curve waits for the present of a picture
/// that carries it before it is released at its commit (LLP 1003.001 D9:
/// the Android reader's deadline for a buffer's first present).
pub const HOLD_DEADLINE: f64 = 2.0;

impl Engine {
    /// The time every sample is taken at: the input clock, or the latest
    /// presented frame if that is later (LLP 1003.001 D5).
    pub fn sample_time(&self) -> f64 {
        self.now.max(self.shown)
    }

    /// Whether curves an author's commit begins wait for the first presented
    /// frame.
    pub fn starts_on_frame(&self) -> bool {
        self.start_on_frame
    }

    /// Whether a curve a commit begins now is held: the rule is on and
    /// something will present it.
    pub(super) fn holds(&self) -> bool {
        self.start_on_frame && !self.unpresentable
    }

    /// Turn the rule on or off. Off (the agent's takeover, D7), everything
    /// pending starts at `at`, or at its own begin if that is later, and the
    /// engine forgets the frames it presented: from here it samples on the
    /// agent's clock alone. Returns the nodes whose plays started, for their
    /// lowered specs.
    pub fn set_start_on_frame(&mut self, on: bool, at: f64) -> Result<Vec<u64>, EngineError> {
        let started = if on || !self.start_on_frame {
            Vec::new()
        } else {
            if !at.is_finite() {
                return Err(EngineError::NonFinite);
            }
            // Sampled where the agent's clock takes over, as Core Animation
            // is: not at a frame this forgets (D7).
            // `shown` is forgotten first, so nothing that ends between the
            // takeover and that frame is retired by the start's own sample.
            self.shown = f64::NEG_INFINITY;
            self.now = self.now.max(at);
            let started = self.start_pending(f64::INFINITY, Some(at))?;
            self.shown = f64::NEG_INFINITY;
            started
        };
        self.start_on_frame = on;
        Ok(started)
    }

    /// Whether anything will present what a commit begins (D9, "The bound").
    /// Not (the page is hidden or frozen, the host has no surface), every
    /// hold is released at its own begin and none is made until it will
    /// again. Returns the nodes whose plays started, as [`Engine::release_held`].
    pub fn set_presentable(&mut self, presentable: bool) -> Result<Vec<u64>, EngineError> {
        let started = if !presentable && !self.unpresentable {
            self.release_held(f64::INFINITY)?
        } else {
            Vec::new()
        };
        self.unpresentable = !presentable;
        Ok(started)
    }

    /// Whether anything will present what a commit begins.
    pub fn presentable(&self) -> bool {
        !self.unpresentable
    }

    /// The present at `at` of a picture produced when the engine sampled at
    /// `carried` (D9): raise `shown` to `at`, start every held curve that
    /// began at or before `carried` there, and sample every curve at the new
    /// time (D2). A curve born after the picture waits for a later picture's
    /// present. A present whose picture is older than one already applied
    /// starts nothing. Returns the nodes whose plays started, for a lowering
    /// host to re-emit. On Apple a tick names its picture and its present
    /// together (`carried == at`), and a second call at the same instant
    /// starts only what began since.
    pub fn present_frame(&mut self, carried: f64, at: f64) -> Result<Vec<u64>, EngineError> {
        if !carried.is_finite() || !at.is_finite() {
            return Err(EngineError::NonFinite);
        }
        if carried < self.carried {
            return Ok(Vec::new());
        }
        self.carried = carried;
        self.shown = self.shown.max(at);
        self.start_pending(carried, Some(at))
    }

    /// Release, at its own begin, every held curve and clock origin that
    /// began at or before `before` (D9, "The bound": nothing will present
    /// it, or its present failed). `shown` and every start already made are
    /// kept. Returns the nodes whose plays started.
    pub fn release_held(&mut self, before: f64) -> Result<Vec<u64>, EngineError> {
        if before.is_nan() {
            return Err(EngineError::NonFinite);
        }
        self.start_pending(before, None)
    }

    /// Release, at its own begin, everything held but running springs (D9,
    /// "The web"): the browser presents only the springs a host lowers to
    /// Web Animations; the CSS transitions and animations it runs itself it
    /// starts at its own ready time, so the engine's copies start at their
    /// commit. Returns the nodes whose plays started.
    pub fn release_held_but_springs(&mut self) -> Result<Vec<u64>, EngineError> {
        let springs: Vec<(u64, Property)> = (self.pending.iter().copied())
            .filter(|key| {
                self.slots
                    .get(key)
                    .and_then(|s| s.running())
                    .is_some_and(|r| {
                        r.pending.is_some()
                            && matches!(r.curve, crate::transition::Curve::Spring { .. })
                    })
            })
            .collect();
        for key in &springs {
            self.pending.remove(key);
        }
        let started = self.release_held(f64::INFINITY);
        self.pending.extend(springs);
        started
    }

    /// The earliest begin of anything held, or `None` when nothing is.
    pub fn held_since(&self) -> Option<f64> {
        let curves = self
            .pending
            .iter()
            .filter_map(|key| self.slots.get(key)?.running()?.pending);
        let plays = self
            .animating
            .iter()
            .flat_map(|node| &self.animations[node])
            .filter_map(|play| play.pending);
        curves
            .chain(plays)
            .chain(self.clock_held_since())
            .reduce(f64::min)
    }

    /// The latest begin of anything held, or `None` when nothing is: a
    /// picture produced at or after it carries every hold.
    pub fn held_latest(&self) -> Option<f64> {
        let curves = self
            .pending
            .iter()
            .filter_map(|key| self.slots.get(key)?.running()?.pending);
        let plays = self
            .animating
            .iter()
            .flat_map(|node| &self.animations[node])
            .filter_map(|play| play.pending);
        curves
            .chain(plays)
            .chain(self.clocks_held().map(|(_, latest)| latest))
            .reduce(f64::max)
    }

    /// When the earliest hold's present is overdue ([`HOLD_DEADLINE`] after
    /// its begin), or `None` when nothing is held. A host whose loop sleeps
    /// wakes for it with no frame.
    pub fn held_deadline(&self) -> Option<f64> {
        self.held_since().map(|begin| begin + HOLD_DEADLINE)
    }

    /// Release what has waited [`HOLD_DEADLINE`] by `now` for a present
    /// that has not come (D9). Returns the nodes whose plays started.
    pub fn release_overdue(&mut self, now: f64) -> Result<Vec<u64>, EngineError> {
        // The latest begin that is overdue: `begin + HOLD_DEADLINE <= now`
        // itself, not `begin <= now - HOLD_DEADLINE`, which rounds otherwise.
        let overdue = |begin: f64| (begin + HOLD_DEADLINE <= now).then_some(begin);
        let curves = self
            .pending
            .iter()
            .filter_map(|key| self.slots.get(key)?.running()?.pending);
        let plays = self
            .animating
            .iter()
            .flat_map(|node| &self.animations[node])
            .filter_map(|play| play.pending);
        let latest = curves
            .chain(plays)
            .chain(self.clock_held_since())
            .filter_map(overdue)
            .reduce(f64::max);
        match latest {
            Some(before) => self.release_held(before),
            None => Ok(Vec::new()),
        }
    }

    /// Start one held curve, `node`'s `property`, at `at` (or its begin if
    /// that is later): the web's present, where the browser decides when
    /// each lowered animation is ready (D9, "The web"). Whether it was held.
    pub fn present_curve(
        &mut self,
        node: u64,
        property: Property,
        at: f64,
    ) -> Result<bool, EngineError> {
        if !at.is_finite() {
            return Err(EngineError::NonFinite);
        }
        let key = (node, property);
        let Some(curve) = self
            .slots
            .get_mut(&key)
            .and_then(|slot| slot.live.as_mut())
            .and_then(|live| live.running.as_mut())
        else {
            return Ok(false);
        };
        let Some(begin) = curve.pending.take() else {
            return Ok(false);
        };
        curve.start += at.max(begin) - begin;
        self.pending.remove(&key);
        self.dirty.insert(key);
        self.advance(self.now)?;
        Ok(true)
    }

    /// Start what began at or before `cohort`: at `at`, or at its own begin
    /// if that is later; with no `at`, at its own begin (a release).
    fn start_pending(&mut self, cohort: f64, at: Option<f64>) -> Result<Vec<u64>, EngineError> {
        let at = move |begin: f64| (begin <= cohort).then_some(at.map_or(begin, |t| t.max(begin)));
        let keys = std::mem::take(&mut self.pending);
        for key in keys {
            let Some(curve) = self
                .slots
                .get_mut(&key)
                .and_then(|slot| slot.live.as_mut())
                .and_then(|live| live.running.as_mut())
            else {
                continue;
            };
            if let Some(begin) = curve.pending {
                match at(begin) {
                    Some(t) => {
                        curve.start += t - begin;
                        curve.pending = None;
                        self.dirty.insert(key);
                    }
                    None => {
                        self.pending.insert(key);
                    }
                }
            }
        }
        self.start_clocks(at);
        let mut started = Vec::new();
        let nodes: Vec<u64> = self.animating.iter().copied().collect();
        for node in nodes {
            let starts = self.animations.get(&node).is_some_and(|plays| {
                plays
                    .iter()
                    .any(|p| p.pending.is_some_and(|b| at(b).is_some()))
            });
            if !starts {
                continue;
            }
            let clock_origin = self.clock_origin(node);
            let Some(plays) = self.animations.get_mut(&node) else {
                continue;
            };
            let mut any = false;
            for play in plays.iter_mut() {
                if let Some((begin, t)) = play.pending.and_then(|b| Some((b, at(b)?))) {
                    // A member that joined a waiting clock origin takes its
                    // phase at the frame, from the origin that frame started
                    // (D8); any other play starts at its own frame.
                    play.start = match clock_origin.filter(|_| play.clock_wait) {
                        Some(origin) => super::clock::boundary(&play.animation, t, origin),
                        None => play.start + (t - begin),
                    };
                    play.pending = None;
                    play.clock_wait = false;
                    any = true;
                }
            }
            if any {
                for play in self.animations[&node].iter() {
                    for p in play.animation.keyframes.properties() {
                        self.dirty.insert((node, p));
                    }
                }
                self.schedule_animations(node);
                started.push(node);
            }
        }
        // Values an earlier seek cached are behind the frame: sample again.
        self.advance(self.now)?;
        Ok(started)
    }

    /// When a property's running curve starts moving (its begin plus delay),
    /// or `None` while it is pending or nothing runs (D3: a flight lands from
    /// its curve's own start).
    pub fn curve_start(&self, node: u64, property: Property) -> Option<f64> {
        let curve = self.slots.get(&(node, property))?.running()?;
        curve.pending.is_none().then_some(curve.start)
    }

    /// When a leaving node's exit ends: the latest end of its last `count`
    /// plays, the ones `play_exit` appended; `None` while any of them is
    /// pending (D3).
    pub fn exit_end(&self, node: u64, count: usize) -> Option<f64> {
        let plays = self.animations.get(&node)?;
        let exit = &plays[plays.len().saturating_sub(count)..];
        if exit.iter().any(|p| p.pending.is_some()) {
            return None;
        }
        Some(
            exit.iter()
                .map(|p| p.start + p.animation.end_time())
                .fold(f64::NEG_INFINITY, f64::max),
        )
    }
}
