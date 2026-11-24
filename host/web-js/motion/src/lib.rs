//! The JS target's motion capability (LLP 1071 §7): springs, holds and a
//! pan's release velocity from the one `exact_motion` Engine every host runs.
//!
//! The page (`host/web-js/motion.js`) is the kernel seam here: it tells the
//! engine each registered node's `transition` row and its four compositor
//! targets as commits change them (the kernel's `motion_sync`), moves holds
//! for the web host's own `motion-glue.js`, and plays what [`Motion::lower`]
//! lowers, as `host/web/src/motion.rs`'s `Springs` does for the wasm host:
//! nothing here runs per frame; a spring is compiled to frames once.
//!
//! Nodes are the runtime's view ids. Time is seconds on the page's clock.

use exact_motion::{
    Change, Engine, EngineError, HoldEnd, HoldToken, Property, SpringDescriptor, TransformHold,
    Transitions, Value, VelocityTracker,
};
use std::collections::{BTreeMap, BTreeSet};

/// The compositor properties the seam feeds, in the page's order.
pub const TARGETS: [Property; 4] = [
    Property::Translate,
    Property::Scale,
    Property::Rotate,
    Property::Opacity,
];

/// The engine, the page's live holds by serial, the springs it plays, and
/// one pan contact's samples.
pub struct Motion {
    engine: Engine,
    holds: BTreeMap<u64, HoldToken>,
    playing: BTreeMap<(u64, Property), (SpringDescriptor, f64)>,
    pan: Option<(u32, VelocityTracker)>,
    /// Transform pairs by their translate serial.
    pairs: BTreeMap<u64, TransformHold>,
    enabled_translate: BTreeSet<u64>,
    /// Springs lowered held (LLP 1003.001 D9), by their curve without its
    /// start, which moves with the clock while held: lowered once.
    held: BTreeMap<(u64, Property), SpringDescriptor>,
    /// Springs started at the browser's ready time, or released at their
    /// commit: the started op's `at`.
    presented: BTreeMap<(u64, Property), f64>,
}

impl Default for Motion {
    fn default() -> Self {
        Motion::new()
    }
}

impl Motion {
    /// Empty, at clock zero; the browser runs CSS transitions and
    /// animations itself (LLP 1055 D7), so the engine lowers only springs.
    pub fn new() -> Motion {
        // This module reads the springs a `transition` row names; it is its
        // own wasm, so it links the grammar itself (LLP 1047 D2).
        exact_motion::link_transitions();
        let mut engine = Engine::new();
        engine.set_lowered(true);
        Motion {
            engine,
            holds: BTreeMap::new(),
            playing: BTreeMap::new(),
            pan: None,
            pairs: BTreeMap::new(),
            enabled_translate: BTreeSet::new(),
            held: BTreeMap::new(),
            presented: BTreeMap::new(),
        }
    }

    /// Turn the first-frame rule on (the page's clock owns time) or off
    /// (the agent's): a spring a commit begins waits for the browser's
    /// ready time (LLP 1003.001 D7, D9).
    pub fn start_on_frame(&mut self, on: bool) -> Result<(), EngineError> {
        let at = self.engine.now();
        self.engine.set_start_on_frame(on, at).map(|_| ())
    }

    /// Whether the browser will present anything: not while the page is
    /// hidden or frozen, when every hold starts at its commit (D9, "The
    /// bound"); the next lowering re-emits what that started.
    pub fn presentable(&mut self, presentable: bool) -> Result<(), EngineError> {
        self.engine.set_presentable(presentable)?;
        self.note_released();
        Ok(())
    }

    /// The browser made a held spring's animation ready at `at` seconds:
    /// the engine's curve starts there. Whether it was held.
    pub fn present(&mut self, node: u64, property: Property, at: f64) -> Result<bool, EngineError> {
        let started = self.engine.present_curve(node, property, at)?;
        if started {
            self.presented.insert((node, property), at);
        }
        Ok(started)
    }

    /// Release what waited [`exact_motion::HOLD_DEADLINE`] by `now` for a
    /// ready time that has not come.
    pub fn release_overdue(&mut self, now: f64) -> Result<(), EngineError> {
        self.engine.release_overdue(now.max(self.engine.now()))?;
        self.note_released();
        Ok(())
    }

    /// Held springs the engine released start, in their op, at the curve's
    /// own start, not at a ready time.
    fn note_released(&mut self) {
        for key in self.held.keys() {
            if let Some(start) = self.engine.curve_start(key.0, key.1) {
                self.presented.insert(*key, start);
            }
        }
    }

    fn seek(&mut self, now: f64) -> Result<(), EngineError> {
        self.engine.advance(now.max(self.engine.now()))
    }

    /// A node's `transition` row as CSS text. Empty, unset and invalid rows
    /// take CSS's initial `all 0s` coverage.
    pub fn transitions(&mut self, node: u64, text: &str) -> bool {
        let parsed = if text.trim().is_empty() || text.trim().eq_ignore_ascii_case("unset") {
            Ok(Transitions::initial())
        } else {
            Transitions::parse(text)
        };
        let was = self.engine.has_spring_transition(node, Property::Translate);
        let ok = parsed.is_ok();
        let applied = self
            .engine
            .set_transitions(node, parsed.unwrap_or_else(|_| Transitions::initial()))
            .is_ok();
        if !was && self.engine.has_spring_transition(node, Property::Translate) {
            self.enabled_translate.insert(node);
        } else {
            self.enabled_translate.remove(&node);
        }
        applied && ok
    }

    /// A node's targets after a commit, at `now`: translate x and y, its x
    /// and y percentages of the box, scale, rotate (degrees), opacity, rotation axis. The
    /// first observation of a node is taken as it is (nothing transitions),
    /// as the kernel's `adopt` does.
    pub fn observe(
        &mut self,
        node: u64,
        values: [f64; 11],
        velocity: Option<Value>,
        now: f64,
    ) -> Result<(), EngineError> {
        self.seek(now)?;
        let enabled = self.enabled_translate.remove(&node);
        let [x, y, px, py, sx, sy, rotate, opacity, ax, ay, az] = values;
        let targets = [
            Value::four(x, y, px, py),
            Value::new(sx, sy),
            Value::four(rotate, ax, ay, az),
            Value::scalar(opacity),
        ];
        for (property, value) in TARGETS.into_iter().zip(targets) {
            self.engine.observe(Change {
                node,
                property,
                value,
                velocity: if property == Property::Translate
                    && (enabled || self.engine.target(node, property) != Some(value))
                {
                    velocity
                } else {
                    None
                },
            })?;
        }
        Ok(())
    }

    /// The height owner's numeric border-box height at `now` (the kernel's
    /// `height_motion_sync`): a height drag holds it and releases it by the
    /// node's `transition`.
    pub fn height(&mut self, node: u64, height: f64, now: f64) -> Result<(), EngineError> {
        self.seek(now)?;
        self.engine.observe(Change {
            node,
            property: Property::Height,
            value: Value::scalar(height),
            velocity: None,
        })
    }

    /// The node owns height no more: its height motion retires.
    pub fn retire_height(&mut self, node: u64) -> bool {
        self.playing.remove(&(node, Property::Height));
        self.engine.remove_property(node, Property::Height)
    }

    /// A node left the tree: forgotten, with its springs.
    pub fn remove(&mut self, node: u64) {
        self.engine.remove(node);
        self.enabled_translate.remove(&node);
        self.playing.retain(|(n, _), _| *n != node);
        self.holds.retain(|_, token| token.node() != node);
    }

    fn token(&self, serial: u64) -> Option<HoldToken> {
        self.holds
            .get(&serial)
            .copied()
            .filter(|t| self.engine.has_hold(*t))
    }

    /// Capture a presented value; the hold's serial and the value it took,
    /// or `None` for a node the engine doesn't know.
    pub fn begin(
        &mut self,
        node: u64,
        property: Property,
        presented: Value,
        now: f64,
    ) -> Result<Option<(u64, Value)>, EngineError> {
        if self.engine.value(node, property).is_none() {
            return Ok(None);
        }
        let Some(start) = self
            .engine
            .begin_hold(node, property, now, Some(presented))?
        else {
            return Ok(None);
        };
        self.holds.retain(|_, token| self.engine.has_hold(*token));
        self.holds.insert(start.token.serial(), start.token);
        // A curve crossing its target is cancelled on takeover too.
        self.playing.remove(&(node, property));
        Ok(Some((start.token.serial(), start.value)))
    }

    /// Capture a node's translate and scale as one pair (the photo pair,
    /// LLP 1057.001 §4): the two serials and the values taken.
    pub fn begin_pair(
        &mut self,
        node: u64,
        values: [f64; 3],
        now: f64,
    ) -> Result<Option<(u64, u64, [f64; 3])>, EngineError> {
        let presented = [
            Value::new(values[0], values[1]),
            Value::new(values[2], values[2]),
        ];
        let Some(pair) = self
            .engine
            .begin_transform_hold(node, now, Some(presented))?
        else {
            return Ok(None);
        };
        self.holds.retain(|_, token| self.engine.has_hold(*token));
        self.pairs.retain(|_, p| {
            self.engine.has_hold(p.translate().token) || self.engine.has_hold(p.scale().token)
        });
        let (t, s) = (pair.translate(), pair.scale());
        for start in [t, s] {
            self.holds.insert(start.token.serial(), start.token);
            self.playing.remove(&(node, start.token.property()));
        }
        self.pairs.insert(t.token.serial(), pair);
        Ok(Some((
            t.token.serial(),
            s.token.serial(),
            [t.value.x, t.value.y, s.value.x],
        )))
    }

    /// Move a pair, by its translate serial; `false` when either is stale.
    pub fn update_pair(
        &mut self,
        serial: u64,
        values: [f64; 3],
        now: f64,
    ) -> Result<bool, EngineError> {
        let Some(pair) = self.pairs.get(&serial).copied() else {
            return Ok(false);
        };
        let values = [
            Value::new(values[0], values[1]),
            Value::new(values[2], values[2]),
        ];
        self.engine.update_transform_hold(pair, now, values)
    }

    /// Move a hold; `false` for a stale one.
    pub fn update(&mut self, serial: u64, value: Value, now: f64) -> Result<bool, EngineError> {
        match self.token(serial) {
            Some(token) => self.engine.update_hold(token, now, value),
            None => Ok(false),
        }
    }

    /// Record what a constrained display shows for a hold (LLP 1057.001 §3).
    pub fn track(&mut self, serial: u64, now: f64, shown: Value) -> bool {
        self.token(serial)
            .is_some_and(|token| self.engine.track_hold(token, now, shown))
    }

    /// The engine's velocity over a hold's values, where the platform
    /// measures none (LLP 1057.001 §3).
    pub fn measured(&self, serial: u64, now: f64) -> Value {
        self.token(serial)
            .and_then(|token| self.engine.hold_velocity(token, now))
            .filter(|v| v.x.is_finite() && v.y.is_finite())
            .unwrap_or(Value::ZERO)
    }

    /// Release (a velocity) or cancel (`None`) a hold, to the newest target.
    pub fn end(
        &mut self,
        serial: u64,
        velocity: Option<Value>,
        now: f64,
    ) -> Result<bool, EngineError> {
        let Some(token) = self.token(serial) else {
            return Ok(false);
        };
        let end = velocity.map_or(HoldEnd::Cancel, |velocity| HoldEnd::Release { velocity });
        let accepted = self.engine.end_hold(token, now, end)?;
        if accepted {
            self.holds.remove(&serial);
        }
        Ok(accepted)
    }

    /// Whether a hold is live.
    pub fn live(&self, serial: u64) -> bool {
        self.token(serial).is_some()
    }

    /// A live hold's node and property.
    pub fn held(&self, serial: u64) -> Option<(u64, Property)> {
        self.token(serial).map(|t| (t.node(), t.property()))
    }

    /// When the last spring in flight ends, seconds.
    pub fn settle_time(&self) -> Option<f64> {
        self.engine.settle_time()
    }

    /// Seek to `now` and lower what changed, as the wasm host's batch ops
    /// (`Springs::lower_current`, `Host::emit_lowered`): a spring that
    /// started is its frames, one that stopped before its end is
    /// cancelled. Each op is numbers in `out`: `[1, node, property, at,
    /// delay, duration, n, x0, y0, …]` (seconds), `[2, node, property]`, or
    /// a held spring (LLP 1003.001 D9), `[3, node, property, due, delay,
    /// duration, n, …]`, which the browser starts at its ready time and the
    /// engine releases at its commit by `due`.
    pub fn lower(&mut self, now: f64, out: &mut Vec<f64>) -> Result<(), EngineError> {
        self.seek(now)?;
        // Only springs wait for the browser's ready time; the CSS transitions
        // the browser runs itself start at their commit in the engine.
        self.engine.release_held_but_springs()?;
        let now = self.engine.now();
        let index = |p: Property| TARGETS.iter().position(|t| *t == p).unwrap_or(4) as f64;
        for p in self.engine.frame() {
            let key = (p.node, p.property);
            if self.engine.is_held(p.node, p.property) {
                self.playing.remove(&key);
                self.held.remove(&key);
                continue;
            }
            match self.engine.spring_descriptor(p.node, p.property) {
                Some(descriptor) if self.engine.curve_start(p.node, p.property).is_none() => {
                    let curve = SpringDescriptor {
                        start: 0.0,
                        ..descriptor
                    };
                    if self.held.get(&key) == Some(&curve) {
                        continue;
                    }
                    let (Some(frames), Some(due)) = (
                        self.engine.spring_frames(p.node, p.property),
                        self.engine.held_deadline(),
                    ) else {
                        continue;
                    };
                    self.held.insert(key, curve);
                    self.playing.remove(&key);
                    out.extend([
                        3.0,
                        p.node as f64,
                        index(p.property),
                        due,
                        (frames.start - now).max(0.0),
                        frames.duration,
                        frames.values.len() as f64,
                    ]);
                    for v in &frames.values {
                        out.extend([v.x, v.y, v.z, v.w]);
                    }
                }
                Some(descriptor) => {
                    self.held.remove(&key);
                    if self.playing.get(&key).map(|(d, _)| d) == Some(&descriptor) {
                        continue;
                    }
                    let Some(frames) = self.engine.spring_frames(p.node, p.property) else {
                        continue;
                    };
                    self.playing
                        .insert(key, (descriptor, frames.start + frames.duration));
                    // Started at the browser's ready time, or released at its
                    // commit: the page pins the animation there.
                    let at = self.presented.remove(&key).unwrap_or(now);
                    out.extend([
                        1.0,
                        p.node as f64,
                        index(p.property),
                        at,
                        (frames.start - at).max(0.0),
                        frames.duration,
                        frames.values.len() as f64,
                    ]);
                    // Four numbers a frame: translate's lengths and
                    // percentages; one of them for the rest.
                    for v in &frames.values {
                        out.extend([v.x, v.y, v.z, v.w]);
                    }
                }
                None => {
                    // A spring lowered held runs in the browser until cancelled.
                    let held = self.held.remove(&key).is_some();
                    let playing = self.playing.remove(&key);
                    if held
                        || playing
                            .is_some_and(|(previous, end)| now < end || p.value != previous.target)
                    {
                        out.extend([2.0, p.node as f64, index(p.property)]);
                    }
                }
            }
        }
        Ok(())
    }

    /// A pan's pointer sample at `ms`, the event's own time (LLP 1057 §10.6,
    /// `host/web/src/pan_velocity.rs`); `first` begins the contact on `view`.
    pub fn pan_sample(&mut self, view: u32, first: bool, x: f64, y: f64, ms: f64) {
        if first || self.pan.as_ref().is_none_or(|(v, _)| *v != view) {
            self.pan = Some((view, VelocityTracker::new()));
        }
        if let Some((_, tracker)) = self.pan.as_mut() {
            tracker.push(ms / 1000.0, Value::new(x, y));
        }
    }

    /// The contact's release velocity at `ms`, CSS pixels per second, then
    /// forgotten; zero for another view or too few samples.
    pub fn pan_release(&mut self, view: u32, ms: f64) -> Value {
        let v = self
            .pan
            .take()
            .filter(|(v, _)| *v == view)
            .map_or(Value::ZERO, |(_, t)| t.estimate(ms / 1000.0));
        if v.x.is_finite() && v.y.is_finite() {
            v
        } else {
            Value::ZERO
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod abi;

#[cfg(test)]
mod tests;
