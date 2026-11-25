//! Springs on the web: the evaluator runs once per release, as a compiler.
//!
//! @ref LLP 1002 D2 (on the web a spring is lowered, not evaluated per frame)
//! @ref LLP 1003 §4 (the seam: `Kernel::motion_sync`)
//!
//! CSS plays every easing transition itself. A `-exact-spring()` it cannot, so the
//! host keeps the same [`Engine`] every native host runs, feeds it each
//! commit through the kernel's seam, seeks it to the commit's clock, and asks
//! it for the frames of any spring that just started — including one that
//! interrupts a transition in flight, whose release value and velocity the
//! engine takes from the curve it was on (CSS Transitions §3, the same rule
//! natively). Nothing here runs per frame: the engine is sampled once per
//! commit and the browser interpolates the frames.
//!
//! Motion is a linked capability (LLP 1047 D3): the host holds a
//! [`Motion`], which is [`Still`] unless the app's entry registered
//! [`springs`], so an app that uses no spring or hold carries no engine.

use exact_kernel::motion::{motion_node, receipt_nodes, targets, transitions, MotionSync};
use exact_kernel::{CommitReceipt, Kernel, NodeKey, StyleId, ViewId};
use exact_motion::{
    Change, Engine, EngineError, HoldEnd, HoldStart, HoldToken, Property, SpringDescriptor,
    TransformHold, Value,
};
use std::collections::{BTreeMap, BTreeSet};

/// What the page must do about one property's spring after a commit.
#[derive(Debug, Clone, PartialEq)]
pub enum Lowered {
    /// Play these frames, evenly spaced over `duration` seconds after
    /// `delay` seconds, replacing whatever was playing on the property.
    Start {
        /// The node.
        view: ViewId,
        /// The property.
        property: Property,
        /// The engine's clock when this was lowered, seconds: `delay`
        /// counts from here.
        at: f64,
        /// Seconds before the first frame.
        delay: f64,
        /// Seconds from the first frame to the last.
        duration: f64,
        /// The frames; the last is the target.
        values: Vec<Value>,
    },
    /// A spring the engine holds until the browser presents it (LLP
    /// 1003.001 D9, "The web"): play these frames from the animation's own
    /// ready time, not pinned, and report that time
    /// ([`Motion::present_curve`]).
    Held {
        /// The node.
        view: ViewId,
        /// The property.
        property: Property,
        /// Seconds before the first frame, from the ready time.
        delay: f64,
        /// Seconds from the first frame to the last.
        duration: f64,
        /// The frames; the last is the target.
        values: Vec<Value>,
        /// When the engine releases the hold at its commit if no ready time
        /// came ([`exact_motion::HOLD_DEADLINE`]), on its clock, seconds.
        due: f64,
    },
    /// Stop playing: the property is no longer under a spring (the style is
    /// its value, or a CSS transition is).
    Cancel {
        /// The node.
        view: ViewId,
        /// The property.
        property: Property,
    },
    /// Eligibility or explicit ownership ended. Remove held DOM overrides as
    /// well as playback, even when removing the Engine slot emitted no frame.
    Retire {
        /// The node's last live view id.
        view: ViewId,
        /// Only this property is retired.
        property: Property,
    },
    /// A commit while drag timelines are bound, or one that unbound the
    /// last (LLP 1057.003 D4): the page's glue, which seeks consumers
    /// itself, follows them again.
    Timelines,
}

/// Springs and holds: the motion capability's seam (LLP 1047 D3). The host
/// calls it at every commit and for every hold; [`Still`] answers when the
/// artifact doesn't link motion, and [`springs`] is the engine an entry
/// registers when its plan uses it.
pub trait Motion {
    /// The engine's clock, seconds.
    fn now(&self) -> f64;
    /// When the last spring in flight ends, seconds; `None` when none is.
    fn settle_time(&self) -> Option<f64>;
    /// The spring engine, when the artifact links one.
    fn linked_engine(&self) -> Option<&Engine>;
    /// Keyframe compilations so far.
    #[cfg(test)]
    fn frame_compilations(&self) -> usize;
    /// Number of property springs retained for the current mounted tree.
    fn playing_count(&self) -> usize;
    /// The registered numeric-height owner.
    fn height_owner(&self) -> Option<(NodeKey, ViewId)>;
    /// Register the numeric-height owner, or clear it.
    fn set_height_owner(
        &mut self,
        kernel: &Kernel,
        view: Option<ViewId>,
    ) -> Result<Vec<Lowered>, &'static str>;
    /// A live hold's token.
    fn token(&self, serial: u64) -> Option<HoldToken>;
    /// A live hold's measured velocity at `now` (LLP 1057.001 §3).
    fn hold_velocity(&self, serial: u64, now: f64) -> Option<Value>;
    /// Record the value a constrained display actually shows for a hold.
    fn track_hold(&mut self, serial: u64, now: f64, shown: Value) -> bool;
    /// Capture a presented value.
    fn begin_hold(
        &mut self,
        kernel: &Kernel,
        view: ViewId,
        property: Property,
        presented: Value,
        now: f64,
    ) -> Result<Option<HoldStart>, EngineError>;
    /// Move a hold.
    fn update_hold(&mut self, serial: u64, value: Value, now: f64) -> Result<bool, EngineError>;
    /// Capture a translate and scale pair.
    fn begin_transform_hold(
        &mut self,
        node: u64,
        values: [Value; 2],
        now: f64,
    ) -> Result<Option<TransformHold>, EngineError>;
    /// Move a translate and scale pair.
    fn update_transform_hold(
        &mut self,
        pair: TransformHold,
        values: [Value; 2],
        now: f64,
    ) -> Result<bool, EngineError>;
    /// Synchronize, then take a transform target's newest authored values.
    fn synchronize_transform(&mut self, kernel: &Kernel, target: NodeKey, now: f64)
        -> Vec<Lowered>;
    /// Release or cancel a hold.
    fn end_hold(&mut self, serial: u64, end: HoldEnd, now: f64) -> Result<bool, EngineError>;
    /// Take the tree at boot as it is.
    fn adopt(&mut self, kernel: &Kernel, views: &[ViewId]);
    /// Synchronize accepted receipts while holds still own presentation.
    fn synchronize(
        &mut self,
        kernel: &Kernel,
        receipts: &[CommitReceipt],
        now: f64,
    ) -> Vec<Lowered>;
    /// Synchronize and lower changed properties once.
    fn commit(&mut self, kernel: &Kernel, receipts: &[CommitReceipt], now: f64) -> Vec<Lowered>;
    /// Lower what changed at the current clock.
    fn lower_current(&mut self, kernel: &Kernel) -> Vec<Lowered>;
    /// Turn the first-frame rule on (the page's clock owns time) or off
    /// (the agent's), as at a boot (LLP 1003.001 D7, D9).
    fn set_start_on_frame(&mut self, on: bool);
    /// Whether the browser will present anything: not while the page is
    /// hidden or frozen, when every hold is released at its commit (D9,
    /// "The bound"). The next lowering re-emits the released springs.
    fn set_presentable(&mut self, presentable: bool);
    /// The browser made a held spring's animation ready at `at`, engine
    /// seconds: start the engine's curve there. Whether it was held.
    fn present_curve(&mut self, kernel: &Kernel, view: ViewId, property: Property, at: f64)
        -> bool;
    /// Release what waited [`exact_motion::HOLD_DEADLINE`] by `now` for a
    /// ready time that has not come.
    fn release_overdue(&mut self, now: f64);
}

impl dyn Motion + '_ {
    /// The spring engine, for inspection: presentation values as the page
    /// shows them. Only an artifact that links motion has one.
    pub fn engine(&self) -> &Engine {
        self.linked_engine().expect("this artifact links motion")
    }
}

/// Motion in an artifact that doesn't link it: no springs and no holds.
/// Admission (LLP 1047 D6) keeps a plan that needs them off such an artifact;
/// the clock still follows the commits.
#[derive(Debug, Default)]
pub struct Still {
    now: f64,
}

impl Motion for Still {
    fn now(&self) -> f64 {
        self.now
    }

    fn settle_time(&self) -> Option<f64> {
        None
    }

    fn linked_engine(&self) -> Option<&Engine> {
        None
    }

    #[cfg(test)]
    fn frame_compilations(&self) -> usize {
        0
    }

    fn playing_count(&self) -> usize {
        0
    }

    fn height_owner(&self) -> Option<(NodeKey, ViewId)> {
        None
    }

    fn set_height_owner(
        &mut self,
        _: &Kernel,
        view: Option<ViewId>,
    ) -> Result<Vec<Lowered>, &'static str> {
        view.map_or(Ok(Vec::new()), |_| Err("motion is not linked"))
    }

    fn hold_velocity(&self, _: u64, _: f64) -> Option<Value> {
        None
    }

    fn track_hold(&mut self, _: u64, _: f64, _: Value) -> bool {
        false
    }

    fn token(&self, _: u64) -> Option<HoldToken> {
        None
    }

    fn begin_hold(
        &mut self,
        _: &Kernel,
        _: ViewId,
        _: Property,
        _: Value,
        _: f64,
    ) -> Result<Option<HoldStart>, EngineError> {
        Ok(None)
    }

    fn update_hold(&mut self, _: u64, _: Value, _: f64) -> Result<bool, EngineError> {
        Ok(false)
    }

    fn begin_transform_hold(
        &mut self,
        _: u64,
        _: [Value; 2],
        _: f64,
    ) -> Result<Option<TransformHold>, EngineError> {
        Ok(None)
    }

    fn update_transform_hold(
        &mut self,
        _: TransformHold,
        _: [Value; 2],
        _: f64,
    ) -> Result<bool, EngineError> {
        Ok(false)
    }

    fn synchronize_transform(&mut self, _: &Kernel, _: NodeKey, now: f64) -> Vec<Lowered> {
        self.now = self.now.max(now);
        Vec::new()
    }

    fn end_hold(&mut self, _: u64, _: HoldEnd, _: f64) -> Result<bool, EngineError> {
        Ok(false)
    }

    fn adopt(&mut self, _: &Kernel, _: &[ViewId]) {}

    fn synchronize(&mut self, _: &Kernel, _: &[CommitReceipt], now: f64) -> Vec<Lowered> {
        self.now = self.now.max(now);
        Vec::new()
    }

    fn commit(&mut self, _: &Kernel, _: &[CommitReceipt], now: f64) -> Vec<Lowered> {
        self.now = self.now.max(now);
        Vec::new()
    }

    fn lower_current(&mut self, _: &Kernel) -> Vec<Lowered> {
        Vec::new()
    }

    fn set_start_on_frame(&mut self, _: bool) {}

    fn set_presentable(&mut self, _: bool) {}

    fn present_curve(&mut self, _: &Kernel, _: ViewId, _: Property, _: f64) -> bool {
        false
    }

    fn release_overdue(&mut self, _: f64) {}
}

/// The spring engine, for an entry that links motion (`exact-web-capabilities`
/// registers it). Nothing else names it, so an artifact without motion
/// carries none of it.
pub fn springs() -> Box<dyn Motion> {
    Box::new(Springs::new())
}

/// The web host's spring evaluator: one engine, sampled at commits.
#[derive(Debug, Default)]
pub struct Springs {
    engine: Engine,
    holds: BTreeMap<u64, HoldToken>,
    #[cfg(test)]
    frame_compilations: usize,
    /// Compare fixed-size curve identity before compiling browser keyframes.
    playing: BTreeMap<(u64, Property), (SpringDescriptor, f64)>,
    /// Boxes with authored height and transition rows. A computed transition can
    /// first select a spring in the same commit that changes the height.
    height_transitions: BTreeMap<NodeKey, Option<Value>>,
    /// One explicit trial owner, bound to its arena generation. Unsupported
    /// styles retire its slot; a later eligible style can adopt it again.
    height_owner: Option<(NodeKey, ViewId)>,
    /// A hold or its return curve retires with the drag registration.
    dragged_height: bool,
    /// Springs lowered held, by their curve (the descriptor without its
    /// start, which moves with the clock while held): lowered once.
    held: BTreeMap<(u64, Property), SpringDescriptor>,
    /// Springs the browser presented, at their ready time: the started op's
    /// `at`, so the page's pin writes the start the browser chose.
    presented: BTreeMap<(u64, Property), f64>,
}

impl Springs {
    /// Empty, at clock zero. The browser runs every CSS animation from the
    /// page's `@keyframes` (LLP 1055 D7), so the engine only tracks them.
    pub fn new() -> Springs {
        let mut springs = Springs::default();
        springs.engine.set_lowered(true);
        springs
    }

    fn retire_height(&mut self, key: NodeKey, view: ViewId, out: &mut Vec<Lowered>) {
        let node = motion_node(key);
        let removed = self.engine.remove_property(node, Property::Height);
        self.playing.remove(&(node, Property::Height));
        self.held.remove(&(node, Property::Height));
        self.presented.remove(&(node, Property::Height));
        if removed {
            out.push(Lowered::Retire {
                view,
                property: Property::Height,
            });
        }
    }

    fn tracks_height(kernel: &Kernel, key: NodeKey) -> bool {
        kernel.node_by_key(key).is_some_and(|node| {
            let rows = node.style.mask.union(node.style.written);
            rows.has(StyleId::Height) && rows.has(StyleId::Transition)
        })
    }

    fn sync_height(&mut self, kernel: &Kernel, key: NodeKey, view: ViewId, out: &mut Vec<Lowered>) {
        let node = motion_node(key);
        let value = kernel.height_target(key);
        let previous = self.height_transitions.insert(key, value).flatten();
        let covers = kernel
            .node(view)
            .is_some_and(|n| n.style.transition.matching(Property::Height).is_some());
        let target = self.engine.target(node, Property::Height);
        if !covers || value.is_none() {
            if target.is_some() {
                self.retire_height(key, view, out);
            }
            return;
        }
        // A newly covering transition needs the box's last numeric value as its first observation.
        let start = previous.filter(|_| target.is_none());
        for value in start
            .into_iter()
            .chain(value.filter(|v| Some(*v) != target))
        {
            let applied = self.engine.observe(Change {
                node,
                property: Property::Height,
                value,
                velocity: None,
            });
            debug_assert!(applied.is_ok(), "validated pixel height");
        }
    }

    fn reconcile_height(&mut self, kernel: &Kernel, out: &mut Vec<Lowered>) {
        let Some((key, view)) = self.height_owner else {
            return;
        };
        let sync = kernel.height_motion_sync(key);
        if let Some(previous) = self.height_transitions.get_mut(&key) {
            *previous = sync.changes.first().map(|change| change.value);
        }
        if self.dragged_height
            && !self.engine.is_held(motion_node(key), Property::Height)
            && sync
                .changes
                .first()
                .is_some_and(|c| self.engine.target(c.node, c.property) != Some(c.value))
        {
            self.dragged_height = false;
        }
        if !sync.retired.is_empty() {
            self.retire_height(key, view, out);
        }
        let applied = sync.apply(&mut self.engine);
        debug_assert!(applied.is_ok(), "height target is validated kernel input");
        if kernel.node_by_key(key).is_none() {
            self.height_owner = None;
        }
    }

    fn view_of(&self, kernel: &Kernel, node: u64) -> Option<ViewId> {
        let key = NodeKey {
            index: node as u32,
            generation: (node >> 32) as u32,
        };
        kernel.node_by_key(key).map(|n| n.id)
    }
}

impl Motion for Springs {
    fn now(&self) -> f64 {
        self.engine.now()
    }

    fn settle_time(&self) -> Option<f64> {
        self.engine.settle_time()
    }

    fn linked_engine(&self) -> Option<&Engine> {
        Some(&self.engine)
    }

    #[cfg(test)]
    fn frame_compilations(&self) -> usize {
        self.frame_compilations
    }

    /// Number of property springs retained for the current mounted tree.
    fn playing_count(&self) -> usize {
        self.playing.len()
    }

    fn height_owner(&self) -> Option<(NodeKey, ViewId)> {
        self.height_owner
    }

    fn set_height_owner(
        &mut self,
        kernel: &Kernel,
        view: Option<ViewId>,
    ) -> Result<Vec<Lowered>, &'static str> {
        // Refuse before retiring the old owner or touching its clock/hold.
        let next = view
            .map(|view| {
                let node = kernel.node(view).ok_or("unknown height owner")?;
                // Registration is generational intent across temporary hidden
                // or unsupported styles; repeating it is still idempotent.
                if self.height_owner == Some((node.key, view)) {
                    return Ok::<_, &'static str>((node.key, view));
                }
                kernel
                    .height_target(node.key)
                    .ok_or("height owner is not an attached numeric box")?;
                Ok((node.key, view))
            })
            .transpose()?;
        if self.height_owner == next {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        if let Some((key, view)) = self.height_owner.take() {
            let ordinary = self.height_transitions.contains_key(&key)
                && kernel
                    .node(view)
                    .is_some_and(|n| n.style.transition.matching(Property::Height).is_some());
            if !ordinary || self.dragged_height {
                self.retire_height(key, view, &mut out);
                if ordinary {
                    self.sync_height(kernel, key, view, &mut out);
                }
            }
        }
        self.dragged_height = false;
        self.height_owner = next;
        self.reconcile_height(kernel, &mut out);
        self.holds.retain(|_, token| self.engine.has_hold(*token));
        Ok(out)
    }

    fn hold_velocity(&self, serial: u64, now: f64) -> Option<Value> {
        self.engine.hold_velocity(self.token(serial)?, now)
    }

    fn track_hold(&mut self, serial: u64, now: f64, shown: Value) -> bool {
        self.token(serial)
            .is_some_and(|token| self.engine.track_hold(token, now, shown))
    }

    fn token(&self, serial: u64) -> Option<HoldToken> {
        self.holds
            .get(&serial)
            .copied()
            .filter(|t| self.engine.has_hold(*t))
    }

    fn begin_hold(
        &mut self,
        kernel: &Kernel,
        view: ViewId,
        property: Property,
        presented: Value,
        now: f64,
    ) -> Result<Option<HoldStart>, EngineError> {
        let Some(node) = kernel.node(view) else {
            return Ok(None);
        };
        if property == Property::Height && self.height_owner != Some((node.key, view)) {
            return Ok(None);
        }
        if self.engine.value(motion_node(node.key), property).is_none() {
            return Ok(None);
        }
        validate_height_position(property, presented)?;
        let Some(start) =
            self.engine
                .begin_hold(motion_node(node.key), property, now, Some(presented))?
        else {
            return Ok(None);
        };
        self.holds.retain(|_, token| self.engine.has_hold(*token));
        self.holds.insert(start.token.serial(), start.token);
        self.dragged_height |= property == Property::Height;
        // Even a curve crossing its target must be cancelled on takeover; a
        // same-clock release with new velocity must not hit the old dedup key.
        self.playing.remove(&(start.token.node(), property));
        Ok(Some(start))
    }

    fn update_hold(&mut self, serial: u64, value: Value, now: f64) -> Result<bool, EngineError> {
        let Some(token) = self.token(serial) else {
            return Ok(false);
        };
        validate_height_position(token.property(), value)?;
        self.engine.update_hold(token, now, value)
    }

    fn begin_transform_hold(
        &mut self,
        node: u64,
        values: [Value; 2],
        now: f64,
    ) -> Result<Option<TransformHold>, EngineError> {
        let Some(pair) = self.engine.begin_transform_hold(node, now, Some(values))? else {
            return Ok(None);
        };
        self.holds.retain(|_, token| self.engine.has_hold(*token));
        for start in [pair.translate(), pair.scale()] {
            self.holds.insert(start.token.serial(), start.token);
            self.playing.remove(&(node, start.token.property()));
        }
        Ok(Some(pair))
    }

    fn update_transform_hold(
        &mut self,
        pair: TransformHold,
        values: [Value; 2],
        now: f64,
    ) -> Result<bool, EngineError> {
        self.engine.update_transform_hold(pair, now, values)
    }

    fn synchronize_transform(
        &mut self,
        kernel: &Kernel,
        target: NodeKey,
        now: f64,
    ) -> Vec<Lowered> {
        let out = self.synchronize(kernel, &[], now);
        if let Some(node) = kernel.node_by_key(target) {
            let key = motion_node(target);
            let sync = MotionSync {
                transitions: vec![(key, transitions(node.style))],
                changes: targets(node.style)
                    .into_iter()
                    .filter(|(property, _)| {
                        matches!(property, Property::Translate | Property::Scale)
                    })
                    .map(|(property, value)| Change {
                        node: key,
                        property,
                        value,
                        velocity: None,
                    })
                    .collect(),
                ..Default::default()
            };
            let applied = sync.apply(&mut self.engine);
            debug_assert!(applied.is_ok(), "validated transform authoring");
        }
        out
    }

    fn end_hold(&mut self, serial: u64, end: HoldEnd, now: f64) -> Result<bool, EngineError> {
        let Some(token) = self.token(serial) else {
            return Ok(false);
        };
        let accepted = self.engine.end_hold(token, now, end)?;
        if accepted {
            self.holds.remove(&serial);
        }
        Ok(accepted)
    }

    /// Tell the engine about nodes that exist before any commit it saw —
    /// the tree at boot. Their values are taken as-is (there is no
    /// before-change style, so nothing transitions).
    fn adopt(&mut self, kernel: &Kernel, views: &[ViewId]) {
        let mut sync = MotionSync::default();
        for id in views {
            let Some(node) = kernel.node(*id) else {
                continue;
            };
            // Every row a commit would sync, the `animation` row too: a boot
            // node's animation starts at the boot, not at its first update
            // (which would find no animation and start one then).
            kernel.motion_sync_node(node.key, &mut sync);
            if Self::tracks_height(kernel, node.key) {
                let value = kernel.height_target(node.key);
                self.height_transitions.insert(node.key, value);
                if let Some(value) =
                    value.filter(|_| node.style.transition.matching(Property::Height).is_some())
                {
                    sync.changes.push(Change {
                        node: motion_node(node.key),
                        property: Property::Height,
                        value,
                        velocity: None,
                    });
                }
            }
        }
        let applied = sync.apply(&mut self.engine);
        debug_assert!(applied.is_ok(), "kernel rows are always valid engine input");
        let _ = self.engine.frame();
    }

    /// Synchronize an accepted receipt while existing holds still own presentation.
    /// Retirement ops are returned, but dirty frames remain for one common lowering.
    fn synchronize(
        &mut self,
        kernel: &Kernel,
        receipts: &[CommitReceipt],
        now: f64,
    ) -> Vec<Lowered> {
        let now = now.max(self.engine.now());
        let seek = self.engine.advance(now);
        debug_assert!(seek.is_ok(), "the clock never runs backwards here");
        // @ref LLP 1057.003 D4 — the browser gives a consumer a fresh CSS
        // animation for changes the kernel records on other nodes (an
        // ancestor's `display`, a move) as well as for its own rows and its
        // name's resolution, so while any node bears a timeline every commit
        // is followed; pages without one never are.
        let mut timelines = !receipts.is_empty() && kernel.has_timelines();
        let mut out = Vec::new();
        let mut touched = BTreeSet::new();
        for receipt in receipts {
            for key in receipt.destroyed.iter().chain(&receipt.renewed) {
                self.height_transitions.remove(key);
            }
            for key in receipt_nodes(receipt) {
                if Self::tracks_height(kernel, key) {
                    self.height_transitions.entry(key).or_default();
                    touched.insert(key);
                } else if self.height_transitions.remove(&key).is_some() {
                    if let Some(node) = kernel.node_by_key(key) {
                        if self.height_owner != Some((key, node.id)) {
                            self.retire_height(key, node.id, &mut out);
                        }
                    }
                }
            }
            let sync = kernel.motion_sync(receipt);
            // Engine::remove also erases dirty entries, so frame() will never
            // mention these nodes again. Retire ownership directly, without
            // scanning springs belonging to other mounted rows.
            for node in &sync.removed {
                for property in Property::ALL {
                    self.playing.remove(&(*node, property));
                    self.held.remove(&(*node, property));
                    self.presented.remove(&(*node, property));
                }
            }
            // Before it applies: the last consumer, unbound here, was bound.
            timelines |= sync.timelines.iter().any(|(node, source, binding)| {
                source.is_some() || binding.is_some() || self.engine.timeline_bound(*node)
            });
            let applied = sync.apply(&mut self.engine);
            debug_assert!(applied.is_ok(), "kernel rows are always valid engine input");
        }
        if timelines {
            out.push(Lowered::Timelines);
        }
        // Hiding or moving an ancestor can change a height target without touching the box itself.
        if receipts.iter().any(|r| r.layout_invalidated) {
            touched.extend(self.height_transitions.keys());
        }
        for key in touched {
            if let Some(node) = kernel.node_by_key(key) {
                if self.height_owner != Some((key, node.id)) {
                    self.sync_height(kernel, key, node.id, &mut out);
                }
            }
        }
        self.reconcile_height(kernel, &mut out);
        self.holds.retain(|_, token| self.engine.has_hold(*token));
        out
    }

    /// Synchronize and lower changed properties once at a commit/input boundary.
    fn commit(&mut self, kernel: &Kernel, receipts: &[CommitReceipt], now: f64) -> Vec<Lowered> {
        let now = now.max(self.engine.now());
        let mut out = self.synchronize(kernel, receipts, now);
        out.extend(self.lower_current(kernel));
        out
    }

    fn set_start_on_frame(&mut self, on: bool) {
        let at = self.engine.now();
        let set = self.engine.set_start_on_frame(on, at);
        debug_assert!(set.is_ok(), "the engine's clock is finite");
    }

    fn set_presentable(&mut self, presentable: bool) {
        let set = self.engine.set_presentable(presentable);
        debug_assert!(set.is_ok(), "the engine's clock is finite");
        self.note_released();
    }

    fn present_curve(
        &mut self,
        kernel: &Kernel,
        view: ViewId,
        property: Property,
        at: f64,
    ) -> bool {
        let Some(node) = Self::node_of(kernel, view) else {
            return false;
        };
        if !at.is_finite()
            || !self
                .engine
                .present_curve(node, property, at)
                .unwrap_or(false)
        {
            return false;
        }
        self.presented.insert((node, property), at);
        true
    }

    fn release_overdue(&mut self, now: f64) {
        let released = self.engine.release_overdue(now.max(self.engine.now()));
        debug_assert!(released.is_ok(), "the engine's clock is finite");
        self.note_released();
    }

    // Stale photo cleanup may lower surviving old tokens at the existing clock,
    // but must neither seek time nor import unrelated targets/owner changes.
    fn lower_current(&mut self, kernel: &Kernel) -> Vec<Lowered> {
        // Only lowered springs wait for the browser's ready time; what else a
        // commit began, the browser starts itself (LLP 1003.001 D9).
        let released = self.engine.release_held_but_springs();
        debug_assert!(released.is_ok(), "the engine's clock is finite");
        let now = self.engine.now();
        let mut out = Vec::new();
        for p in self.engine.frame() {
            let key = (p.node, p.property);
            let Some(view) = self.view_of(kernel, p.node) else {
                self.playing.remove(&key);
                self.held.remove(&key);
                continue;
            };
            if self.engine.is_held(p.node, p.property) {
                self.playing.remove(&key);
                self.held.remove(&key);
                continue;
            }
            match self.engine.spring_descriptor(p.node, p.property) {
                Some(descriptor) if self.engine.curve_start(p.node, p.property).is_none() => {
                    // Held: lowered once, unpinned, for the browser to start.
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
                    out.push(Lowered::Held {
                        view,
                        property: p.property,
                        delay: (frames.start - now).max(0.0),
                        duration: frames.duration,
                        values: frames.values,
                        due,
                    });
                }
                Some(descriptor) => {
                    self.held.remove(&key);
                    if self.playing.get(&key).map(|(d, _)| d) == Some(&descriptor) {
                        continue;
                    }
                    #[cfg(test)]
                    {
                        self.frame_compilations += 1;
                    }
                    let frames = self
                        .engine
                        .spring_frames(p.node, p.property)
                        .expect("a running spring descriptor has frames");
                    self.playing
                        .insert(key, (descriptor, frames.start + frames.duration));
                    // Started at the browser's ready time: the op's `at` is
                    // that instant, so the page pins what it already chose.
                    let at = self.presented.remove(&key).unwrap_or(now);
                    out.push(Lowered::Start {
                        view,
                        property: p.property,
                        at,
                        delay: (frames.start - at).max(0.0),
                        duration: frames.duration,
                        values: frames.values,
                    });
                }
                None => {
                    // A cancelled spring can have the same target. Until its
                    // end (a held one's is unknown), its browser frames would
                    // keep overriding the style.
                    let held = self.held.remove(&key).is_some();
                    let playing = self.playing.remove(&key);
                    if held
                        || playing
                            .is_some_and(|(previous, end)| now < end || p.value != previous.target)
                    {
                        out.push(Lowered::Cancel {
                            view,
                            property: p.property,
                        });
                    }
                }
            }
        }
        out
    }
}

impl Springs {
    /// Springs lowered held that the engine released at their commit: their
    /// started op is pinned at the curve's own start, so the page shows the
    /// engine's timing, not the ready time the browser would choose.
    fn note_released(&mut self) {
        for key in self.held.keys() {
            if let Some(start) = self.engine.curve_start(key.0, key.1) {
                self.presented.insert(*key, start);
            }
        }
    }

    /// A node's motion key, from its live view.
    fn node_of(kernel: &Kernel, view: ViewId) -> Option<u64> {
        kernel.node(view).map(|n| motion_node(n.key))
    }
}

// Native projection uses f32 CSS lengths. Reject malformed external positions
// before advancing the clock; signed velocities/internal curve samples remain
// unrestricted, and the DOM alone clamps a negative displayed height to zero.
pub(super) fn validate_height_position(
    property: Property,
    value: Value,
) -> Result<(), EngineError> {
    if property != Property::Height {
        return Ok(());
    }
    if !value.x.is_finite() || !value.y.is_finite() {
        return Err(EngineError::NonFinite);
    }
    if value.y != 0.0 || value.x < 0.0 || value.x > f32::MAX as f64 {
        return Err(EngineError::InvalidValueShape);
    }
    Ok(())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use exact_runner::{DataError, DataSource, Event, Value as DataValue};

    struct NoData;
    impl DataSource for NoData {
        fn query(&mut self, name: &str, _: &[DataValue]) -> Result<DataValue, DataError> {
            Err(DataError::UnknownSource(name.into()))
        }
    }

    /// LLP 1003.001 D9, "The web": on the page's clock a spring a commit
    /// begins is lowered held (no `at`), the browser's ready time starts it
    /// and the batch pins it there; an easing the browser runs itself is
    /// never held; a hidden page and the deadline release at the commit.
    #[test]
    fn a_held_spring_starts_at_the_browsers_ready_time() {
        crate::link::link_for_tests();
        let source = "component App\n  state on = false\n  action toggle\n    on = not on\n  view\n    column\n      button press=toggle testId=\"toggle\"\n        text \"Toggle\"\n      text \"Spring\" testId=\"spring\" translate=(on ? \"100px 0px\" : \"0px 0px\") opacity=(on ? 0.5 : 1) transition=\"translate -exact-spring(180, 12, 1), opacity 300ms linear\"\n";
        let boot = || {
            let (mut host, _) = crate::Host::boot(
                &contract::compile(source).unwrap().encode(),
                NoData,
                Default::default(),
                "/",
            )
            .unwrap();
            host.set_start_on_frame(true);
            let kernel = host.runner().kernel();
            let id = |name| {
                kernel
                    .node_by_key(kernel.find_by_test_id(name)[0])
                    .unwrap()
                    .id
            };
            let (toggle, spring) = (id("toggle"), id("spring"));
            (host, toggle, spring)
        };
        let animate = |batch: &str| {
            batch
                .split("{\"op\":\"animate\"")
                .skip(1)
                .find(|op| op.contains("\"translate\""))
                .map(str::to_owned)
        };
        let (mut host, toggle, spring) = boot();
        let batch = host.dispatch_at(toggle, Event::Press, 100.0);
        let held = animate(&batch).expect("the spring is lowered");
        assert!(
            held.contains("\"held\":true") && !held.contains("\"at\""),
            "{held}"
        );
        assert!(held.contains("\"due\":2100"), "{held}");
        // Only the spring waits: the opacity easing is the browser's own.
        assert_eq!(host.springs().engine().held_since(), Some(0.1));
        let pinned = host
            .present_curve(spring, Property::Translate, 140.0, 150.0)
            .expect("it was held");
        let op = animate(&pinned).expect("the started op");
        assert!(op.contains("\"at\":140"), "pinned at the ready time: {op}");
        assert_eq!(host.springs().engine().held_since(), None);
        assert!(host
            .present_curve(spring, Property::Translate, 160.0, 160.0)
            .is_none());

        // A hidden page releases at the commit.
        let (mut host, toggle, _) = boot();
        host.dispatch_at(toggle, Event::Press, 100.0);
        let hidden = exact_runner::Page {
            hidden: true,
            ..host.runner().page()
        };
        let op = animate(&host.set_page(hidden)).expect("re-emitted");
        assert!(op.contains("\"at\":100") && !op.contains("held"), "{op}");
        host.dispatch_at(toggle, Event::Press, 200.0);
        assert_eq!(
            host.springs().engine().held_since(),
            None,
            "nothing held while hidden"
        );

        // The deadline releases at the commit.
        let (mut host, toggle, _) = boot();
        host.dispatch_at(toggle, Event::Press, 100.0);
        assert!(animate(&host.release_overdue(1000.0)).is_none());
        let op = animate(&host.release_overdue(2100.0)).expect("re-emitted at its deadline");
        assert!(op.contains("\"at\":100"), "{op}");
    }

    #[test]
    fn hold_moves_do_not_recompile_unchanged_springs() {
        crate::link::link_for_tests();
        let mut source = String::from("component App\n  state big = false\n  action toggle\n    big = not big\n  view\n    column\n      button press=toggle testId=\"toggle\"\n        text \"Toggle\"\n      text \"Held\" testId=\"held\" transition=\"translate -exact-spring(180, 12, 1)\"\n");
        for _ in 0..32 {
            source.push_str("      text \"Moving\" scale=(big ? 1.5 : 1) opacity=(big ? 0.5 : 1) transition=\"scale -exact-spring(180, 12, 1), opacity -exact-spring(180, 12, 1)\"\n");
        }
        let (mut host, _) = crate::Host::boot(
            &contract::compile(&source).unwrap().encode(),
            NoData,
            Default::default(),
            "/",
        )
        .unwrap();
        let id = |host: &crate::Host<NoData>, name| {
            let kernel = host.runner().kernel();
            kernel
                .node_by_key(kernel.find_by_test_id(name)[0])
                .unwrap()
                .id
        };
        let toggle = id(&host, "toggle");
        let row = id(&host, "held");
        host.dispatch_at(toggle, Event::Press, 0.0);
        assert_eq!(host.springs().frame_compilations(), 64);
        let (hold, _) = host
            .begin_hold(row, Property::Translate, Value::new(80.0, 0.0), 1.0)
            .unwrap()
            .unwrap();
        for step in 2..102 {
            let batch = host
                .update_hold(
                    hold.token.serial(),
                    Value::new(80.0 + f64::from(step), 0.0),
                    f64::from(step),
                )
                .unwrap()
                .unwrap();
            assert!(!batch.contains("\"op\":\"animate\""));
        }
        assert_eq!(
            host.springs().frame_compilations(),
            64,
            "100 input samples must not rebuild the 64 unrelated curves"
        );
        host.end_hold(
            hold.token.serial(),
            HoldEnd::Release {
                velocity: Value::new(-20.0, 0.0),
            },
            101.0,
        )
        .unwrap()
        .unwrap();
        assert_eq!(host.springs().frame_compilations(), 65);
        let (caught, _) = host
            .begin_hold(row, Property::Translate, Value::new(181.0, 0.0), 101.0)
            .unwrap()
            .unwrap();
        host.end_hold(
            caught.token.serial(),
            HoldEnd::Release {
                velocity: Value::new(30.0, 0.0),
            },
            101.0,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            host.springs().frame_compilations(),
            66,
            "same-clock rebegin/release with a new velocity compiles once"
        );
    }
}
