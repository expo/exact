//! A smooth scroll the presenter animates: an app's smooth
//! `scrollIntoView("element-id")` (LLP 1070.000 §14, LLP 1115.000 D8.4), each
//! scroller it moves, and a virtualized list's smooth correction (§11: a
//! smooth keyed request, a `scroll-behavior: smooth` list following its end).
//! Each glides as UIKit's `setContentOffset(_:animated:)` and AppKit's
//! clip-view animator do, one 0.3 s CSS `ease-in-out` from where it was.
//! `behavior="instant"`, reduced motion and a clock the display does not own
//! (the agent's, a headless run's) land at once. The glide owns the offset
//! until it lands: the end-follow moves nothing meanwhile (LLP 1070.000 §15),
//! and its scroller's `scrollend` comes once, where it rests. Any other move
//! of the port (the reader's, a `scrollTop` write, another request, an
//! anchor's correction) stops it where it is.
use super::*;

#[cfg(test)]
#[path = "glide_tests.rs"]
mod glide_tests;

/// A glide's length, ms: the iOS host's smooth correction (`OffsetDriver`).
const GLIDE_MS: f64 = 300.;

#[derive(Clone, Copy, Debug)]
pub(super) struct Glide {
    from: (f32, f32),
    to: (f32, f32),
    start: f64,
    /// Where the last step put the port: a port found elsewhere was moved
    /// by something else, which ends the glide.
    last: (f32, f32),
}

#[derive(Default)]
pub(super) struct Glides {
    /// By scroller, the page as [`PAGE`].
    running: BTreeMap<ViewId, Glide>,
}

impl Glides {
    /// Whether one runs on `id`.
    pub(super) fn has(&self, id: ViewId) -> bool {
        self.running.contains_key(&id)
    }
    /// Where the one on `id` is headed.
    pub(super) fn target(&self, id: ViewId) -> Option<(f32, f32)> {
        self.running.get(&id).map(|g| g.to)
    }
    /// `id` glides from `from` to `to`, starting at `now`, replacing (and
    /// so retargeting from where it is) a glide it had.
    pub(super) fn begin(&mut self, id: ViewId, from: (f32, f32), to: (f32, f32), now: f64) {
        let last = from;
        self.running.insert(
            id,
            Glide {
                from,
                to,
                start: now,
                last,
            },
        );
    }
    /// Stop the one on `id` where its port is.
    pub(super) fn stop(&mut self, id: ViewId) {
        self.running.remove(&id);
    }
    /// Something else moved the port of the one on `id`, now at `at`.
    pub(super) fn interrupted(&self, id: ViewId, at: (f32, f32)) -> bool {
        self.running.get(&id).is_some_and(|g| g.last != at)
    }
}

/// CSS `ease-in-out`, `cubic-bezier(0.42, 0, 0.58, 1)`, at progress `x`.
pub(super) fn ease(x: f64) -> f64 {
    let bez = |a: f64, b: f64, s: f64| {
        3. * a * s * (1. - s) * (1. - s) + 3. * b * s * s * (1. - s) + s * s * s
    };
    let (mut lo, mut hi) = (0., 1.);
    for _ in 0..32 {
        let m = (lo + hi) / 2.;
        if bez(0.42, 0.58, m) < x {
            lo = m;
        } else {
            hi = m;
        }
    }
    bez(0., 1., (lo + hi) / 2.)
}

impl<D: DataSource> Presenter<D> {
    /// Whether a scroll with `behavior` glides here: the platform's
    /// animated scroll unless it says `instant`, the reader asked for
    /// reduced motion, or the display does not own the clock.
    pub(super) fn reveal_glides(&self, behavior: Option<&str>) -> bool {
        behavior != Some("instant")
            && self.live_clock.is_some()
            && !self.host.runner().viewport().preferences.reduced_motion
    }

    /// Whether a glide owns `id`'s offset.
    pub(crate) fn gliding(&self, id: ViewId) -> bool {
        self.glides.has(id)
    }

    /// Any glide runs: the display wants frames.
    pub(crate) fn glides_moving(&self) -> bool {
        !self.glides.running.is_empty()
    }

    /// `id` glides from `from` to `to`, replacing a glide it had.
    pub(super) fn begin_glide(&mut self, id: ViewId, from: (f32, f32), to: (f32, f32)) {
        let now = self.host.now();
        self.glides.begin(id, from, to, now);
    }

    /// Stop the glide on `id` where its port is.
    pub(super) fn stop_glide(&mut self, id: ViewId) {
        self.glides.stop(id);
    }

    /// Each glide's frame at `now`: the port moves along its ease, and one
    /// at its end lands. One whose port something else moved is over. An
    /// ordinary scroller moves as a reader's wheel moves it (its `scroll`,
    /// its list's window); a virtualized list's port only, its reports
    /// giving the glide's destination until it lands (LLP 1070.000 §11).
    pub(crate) fn step_glides(&mut self, now: f64) {
        if self.glides.running.is_empty() {
            return;
        }
        let glides: Vec<_> = self.glides.running.iter().map(|(k, g)| (*k, *g)).collect();
        let mut landed_list = false;
        for (id, g) in glides {
            let at = if id == PAGE {
                self.page
            } else {
                self.scroll_of(id)
            };
            if at != g.last || (id != PAGE && self.host.kernel().node(id).is_none()) {
                self.glides.running.remove(&id);
                // A list reports where its port is, no longer animating.
                landed_list |= id != PAGE && self.host.collection(id).is_some();
                continue;
            }
            let t = ((now - g.start) / GLIDE_MS).clamp(0., 1.);
            let k = ease(t) as f32;
            let next = if t >= 1. {
                g.to
            } else {
                (
                    g.from.0 + (g.to.0 - g.from.0) * k,
                    g.from.1 + (g.to.1 - g.from.1) * k,
                )
            };
            let list = id != PAGE && self.host.collection(id).is_some();
            if t >= 1. {
                self.glides.running.remove(&id);
                landed_list |= list;
            } else if let Some(g) = self.glides.running.get_mut(&id) {
                g.last = next;
            }
            if next == at {
                continue;
            }
            self.dirty = true;
            if id == PAGE {
                self.page = next;
            } else if list {
                self.scroll.insert(id, next);
                self.collection_glided(id);
            } else {
                self.scroll.insert(id, next);
                self.scroll_end_moved(id, scroll_end::Completion::Glide);
                self.collection_scrolled_or_deferred(id, at);
            }
        }
        // A list that landed reports where it is, no longer animating.
        if landed_list {
            self.queue_collections();
        }
        if let Some(error) = self.refresh_transform_geometry() {
            self.host.log(error);
        }
    }
}
