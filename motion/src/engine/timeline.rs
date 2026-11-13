//! Drag, scroll and view timelines in the engine (LLP 1057.003 D2, D4, D5).
//!
//! A timeline's position is a drag source node's presented `translate` on
//! one axis, or a scroller's offset on one axis as its host presents it
//! ([`Engine::set_scroll_offset`]). A consumer's `animation`s are bound to
//! what its timeline resolved to, over a range of positions. The host
//! resolves names as CSS scopes them (the kernel's `timeline-scope` lookup)
//! and places a scroll or view timeline's range from layout, so the engine
//! hears a node and two positions, never a name. A bound consumer's plays
//! are held (as `animation-play-state: paused` holds them) at the local
//! time the position gives, re-sought whenever a frame is taken or the
//! scroller moves, so a held value, a release spring or a scroll moves the
//! consumer in the frame it moves the source. A sampling host paints it; a
//! lowering host samples the node (its compositor's time is the clock's) or
//! seeks its own executor's copy, as the web's glue does.
//!
//! What a name that finds no single source shows is Chrome's (Scroll-driven
//! Animations 1 §4.2, Web Animations 2's timeline switch):
//! - [`NamedTimeline::Inactive`]: a `timeline-scope` with no declaring
//!   descendant, or several. The animations' current time is unresolved, so
//!   they have no effect: the property's own value shows, whatever the fill.
//! - [`NamedTimeline::Missing`]: no timeline of that name in scope. The
//!   animation has no timeline and keeps the time it has: where its source
//!   left it, still out of effect after an inactive one, 0 if it is new.

use super::Engine;
use crate::property::Property;

/// What a consumer's timeline name resolved to (LLP 1057.003 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedTimeline {
    /// The drag timeline this node's presented translate drives.
    Source(u64),
    /// A scroll or view timeline: this scroller's offset on `x` or `y`.
    Scroll {
        /// The scroller's node.
        scroller: u64,
        /// Whether it reads the horizontal offset.
        x: bool,
    },
    /// An inactive timeline: its animations have no effect.
    Inactive,
    /// No timeline: its animations keep the time they have.
    Missing,
}

/// The timelines, in declaration order. A page has a handful; a scan is
/// cheaper, in code and time, than a map.
#[derive(Debug, Default)]
pub(super) struct Timelines {
    /// Each drag source node, and whether it reads the `x` axis.
    sources: Vec<(u64, bool)>,
    /// Each consumer node, what its name resolved to, and its range in points.
    bound: Vec<(u64, NamedTimeline, [f64; 2])>,
    /// Each scroller a host has given an offset, and that offset.
    scrolls: Vec<(u64, [f64; 2])>,
}

impl Timelines {
    /// Whether `node` is a timeline's source or follows one.
    pub(super) fn names(&self, node: u64) -> bool {
        self.sources.iter().any(|s| s.0 == node)
            || self.bound.iter().any(|b| b.0 == node)
            || self.scrolls.iter().any(|s| s.0 == node)
    }
}

impl Engine {
    /// Declare (or with `None`, retract) the timeline `node`'s presented
    /// translate drives, and whether it reads the `x` axis.
    pub fn set_drag_timeline(&mut self, node: u64, x: Option<bool>) {
        (self.links.set_drag_timeline)(self, node, x)
    }

    pub(super) fn set_drag_timeline_full(&mut self, node: u64, x: Option<bool>) {
        let at = self.timelines.sources.iter().position(|s| s.0 == node);
        match (x, at) {
            (Some(x), Some(i)) => self.timelines.sources[i].1 = x,
            (Some(x), None) => self.timelines.sources.push((node, x)),
            (None, Some(i)) => {
                self.timelines.sources.remove(i);
            }
            (None, None) => return,
        }
        for i in 0..self.timelines.bound.len() {
            if self.timelines.bound[i].1 == NamedTimeline::Source(node) {
                self.seek_timeline(i);
            }
        }
    }

    /// `scroller` now stands at `(x, y)`, its scroll offset in points as
    /// its host presents it (overscroll included): the scroll and view
    /// timelines it drives seek their consumers at once. A scroller never
    /// given one stands at its start.
    pub fn set_scroll_offset(&mut self, scroller: u64, x: f64, y: f64) {
        (self.links.set_scroll_offset)(self, scroller, x, y)
    }

    pub(super) fn set_scroll_offset_full(&mut self, scroller: u64, x: f64, y: f64) {
        if !(x.is_finite() && y.is_finite()) {
            return;
        }
        match self.timelines.scrolls.iter_mut().find(|s| s.0 == scroller) {
            Some(s) if s.1 == [x, y] => return,
            Some(s) => s.1 = [x, y],
            None => self.timelines.scrolls.push((scroller, [x, y])),
        }
        for i in 0..self.timelines.bound.len() {
            if matches!(self.timelines.bound[i].1, NamedTimeline::Scroll { scroller: s, .. } if s == scroller)
            {
                self.seek_timeline(i);
            }
        }
    }

    /// Bind (or with `None`, unbind) `node`'s animations to the timeline its
    /// name resolved to, over `range`: at `range[0]` they are at their
    /// start, at `range[1]` at their end.
    pub fn set_animation_timeline(
        &mut self,
        node: u64,
        binding: Option<(NamedTimeline, [f64; 2])>,
    ) {
        (self.links.set_animation_timeline)(self, node, binding)
    }

    pub(super) fn set_animation_timeline_full(
        &mut self,
        node: u64,
        binding: Option<(NamedTimeline, [f64; 2])>,
    ) {
        let at = self.timelines.bound.iter().position(|b| b.0 == node);
        match (binding, at) {
            (Some((timeline, range)), Some(i)) => {
                self.timelines.bound[i] = (node, timeline, range);
                self.seek_timeline(i);
            }
            (Some((timeline, range)), None) => {
                self.timelines.bound.push((node, timeline, range));
                self.seek_timeline(self.timelines.bound.len() - 1);
            }
            (None, Some(i)) => {
                self.timelines.bound.remove(i);
                // Back on the clock from where the timeline left it; from
                // an inactive one, from its start.
                // With the first-frame rule on, that resume waits for the
                // frame (LLP 1003.001 D3); a clock's join below decides for
                // its own.
                let now = self.sample_time();
                let pending = self.start_on_frame.then_some(now);
                for play in self.animations.get_mut(&node).into_iter().flatten() {
                    if !play.animation.paused {
                        if let Some(held) = play.hold.take() {
                            play.start = now - if held.is_nan() { 0.0 } else { held };
                            play.pending = pending;
                        }
                    }
                }
                // Binding took the node off the clock's list; a live play
                // goes back on it, waiting for the frame or not.
                self.schedule_animations(node);
                // Onto a clock timeline: in its phase (LLP 1055.002).
                self.join_clock_node(node);
            }
            (None, None) => {}
        }
    }

    /// Whether `node`'s animations follow a timeline instead of the clock.
    pub fn timeline_bound(&self, node: u64) -> bool {
        self.timelines.bound.iter().any(|b| b.0 == node)
    }

    pub(super) fn forget_timelines(&mut self, node: u64) {
        self.timelines.sources.retain(|s| s.0 != node);
        self.timelines.bound.retain(|b| b.0 != node);
        self.timelines.scrolls.retain(|s| s.0 != node);
    }

    /// Hold every bound consumer's plays at the time its timeline gives.
    pub(super) fn seek_timelines(&mut self) {
        (self.links.seek_timelines)(self)
    }

    pub(super) fn seek_timelines_full(&mut self) {
        for i in 0..self.timelines.bound.len() {
            self.seek_timeline(i);
        }
    }

    /// Re-seek a bound node whose plays were just set.
    pub(super) fn seek_bound(&mut self, node: u64) {
        if let Some(i) = self.timelines.bound.iter().position(|b| b.0 == node) {
            self.seek_timeline(i);
        }
    }

    fn seek_timeline(&mut self, i: usize) {
        let (node, timeline, [a, b]) = self.timelines.bound[i];
        // The source's presented translate on its axis, as progress through
        // the range; `None` for no timeline, NaN for an inactive one. A
        // source the engine has not heard of yet is no timeline until it is.
        let progress = match timeline {
            NamedTimeline::Source(s) => self
                .timelines
                .sources
                .iter()
                .find(|x| x.0 == s)
                .and_then(|&(_, x)| Some((self.value(s, Property::Translate)?, x)))
                .map(|(v, x)| {
                    // Unclamped: outside the range the animation is before or
                    // after its interval, and its fill decides, as on a CSS
                    // scroll timeline.
                    let p = ((if x { v.x } else { v.y }) - a) / (b - a);
                    if p.is_finite() {
                        p
                    } else {
                        0.0
                    }
                }),
            // Unclamped, as a drag's: the rubber band past either end moves
            // the consumer on, and its fill decides outside the range. A
            // zero-length range has no progress, an inactive timeline's.
            NamedTimeline::Scroll { scroller, x } => {
                let at = (self.timelines.scrolls.iter())
                    .find(|s| s.0 == scroller)
                    .map_or(0.0, |s| s.1[usize::from(!x)]);
                Some(if a == b { f64::NAN } else { (at - a) / (b - a) })
            }
            NamedTimeline::Inactive => Some(f64::NAN),
            NamedTimeline::Missing => None,
        };
        let Some(plays) = self.animations.get_mut(&node) else {
            return;
        };
        let mut moved = false;
        for play in plays.iter_mut() {
            // The range spans the delay and the active interval together, as
            // Web Animations 2 converts an animation's times on a
            // progress-based timeline (Chrome agrees: the parity fixture's
            // `tl-delay`). An endless animation has no end to span and holds
            // its start, a declared deviation: CSS gives it no duration and
            // shows its end. An unresolved time stays unresolved.
            let end = play.animation.end_time();
            let local = match progress {
                Some(p) if p.is_nan() => p,
                Some(p) if end.is_finite() => p * end,
                Some(_) => play.animation.delay.max(0.0),
                None => play.hold.unwrap_or(0.0),
            };
            // The source owns its time: nothing waits for a frame.
            play.pending = None;
            moved |= play.hold.replace(local).map(f64::to_bits) != Some(local.to_bits());
        }
        self.animating.remove(&node);
        if !moved {
            return;
        }
        // A lowering host's executor seeks its own copy (the web's glue); only
        // what the engine samples is painted from here.
        let properties: Vec<Property> = self.animations[&node]
            .iter()
            .flat_map(|play| play.animation.keyframes.properties())
            .filter(|p| !self.lowered_for(node, *p))
            .collect();
        for property in properties {
            self.dirty.insert((node, property));
        }
    }
}
