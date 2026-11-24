//! Motion starts at the present of the first picture that shows it (LLP
//! 1003.001 D9), and the page's `presented` fact (LLP 1069.000 D2): a live
//! host with a display holds what a commit begins until the present of a
//! picture that carries it; where nothing will present, or the present does
//! not come, it releases the hold at its commit.
use super::*;

impl<D: DataSource> Host<D> {
    /// The rule on for a live host whose display presents its pictures, off
    /// for a deterministic driver, which starts what waits at its takeover
    /// (D7). `presents` is false for headless and the render host. Whether
    /// anything changed to paint.
    pub(crate) fn frame_clock(&mut self, live: bool) -> bool {
        if self.engine.starts_on_frame() == live {
            return false;
        }
        let at = (self.now_ms / 1000.0).max(self.engine.now());
        let started = self
            .engine
            .set_start_on_frame(live, at)
            .expect("finite host clock");
        self.motion_started(&started) | self.present()
    }

    /// A runner that awaits its page's first present is a display host's:
    /// the boot's own `animation`s wait for that present too.
    pub(crate) fn hold_from_boot(&mut self) {
        if !self.runner.presented().seen {
            let held = self.engine.set_start_on_frame(true, self.engine.now());
            debug_assert!(held.is_ok_and(|started| started.is_empty()));
        }
    }

    /// A picture is produced now: the engine time it carries, every hold
    /// that began at or before it being in that picture.
    pub(crate) fn picture_carried(&mut self) -> f64 {
        let carried = self.engine.sample_time();
        self.carried = Some(carried);
        carried
    }

    /// The next sampled visual change, in the host's milliseconds. A change
    /// due now needs display callbacks; a later change needs only a timer.
    pub fn motion_due_ms(&self) -> Option<f64> {
        if self.press_releases_next_frame() {
            Some(self.now_ms)
        } else {
            self.motion_next_ms()
                .into_iter()
                .chain(self.press_due())
                .chain(self.press_deadline())
                .reduce(f64::min)
        }
    }

    /// Whether a hold began after the latest picture produced, which carried
    /// `carried`: a picture is owed to carry it to a present.
    pub(crate) fn holds_uncarried(&self) -> bool {
        self.engine
            .held_latest()
            .is_some_and(|latest| self.carried.is_none_or(|c| latest > c))
    }

    /// The present at `at_ms` (the host's clock) of a picture that carried
    /// `carried`: what it carried starts there, and the page's first present
    /// sets `presented`. Returns whether anything changed to paint, and the
    /// fact's error.
    pub(crate) fn picture_presented(&mut self, carried: f64, at_ms: f64) -> (bool, Option<String>) {
        let mut paint = false;
        if self.engine.starts_on_frame() {
            let at = at_ms / 1000.0;
            paint = match self.engine.present_frame(carried, at) {
                Ok(started) => self.motion_started(&started),
                Err(e) => {
                    debug_assert!(false, "a present is finite: {e:?}");
                    false
                }
            };
            paint |= self.present();
        }
        (paint, self.page_presented())
    }

    /// The page's first picture was presented: `presented` from this commit,
    /// once a runner.
    pub(crate) fn page_presented(&mut self) -> Option<String> {
        if self.runner.presented().seen {
            return None;
        }
        let answer = self.runner.page_presented();
        self.told(answer, "page")
    }

    /// `prefer presented` (LLP 1069.000 D6): the fact held at `value`, or
    /// (`None`) the latch again. No animation moves.
    pub(crate) fn prefer_presented(&mut self, value: Option<bool>) -> Option<String> {
        let answer = self.runner.prefer_presented(value);
        self.told(answer, "page")
    }

    /// Whether anything will present what a commit begins: the page is
    /// visible and not frozen, and the host has a surface (D9, "The bound").
    /// Not, every hold is released at its own commit and none is made.
    pub(crate) fn set_presentable(&mut self, presentable: bool) -> bool {
        if self.engine.presentable() == presentable {
            return false;
        }
        let started = match self.engine.set_presentable(presentable) {
            Ok(started) => self.motion_started(&started),
            Err(e) => {
                debug_assert!(false, "a release is finite: {e:?}");
                false
            }
        };
        started | self.present()
    }

    /// Release, at its commit, what has waited past the deadline for a
    /// present that has not come (a lost callback, a refused present).
    pub(crate) fn release_overdue(&mut self) -> bool {
        if self
            .engine
            .held_deadline()
            .is_none_or(|due| due * 1000.0 > self.now_ms)
        {
            return false;
        }
        let started = match self.engine.release_overdue(self.now_ms / 1000.0) {
            Ok(started) => self.motion_started(&started),
            Err(e) => {
                debug_assert!(false, "a release is finite: {e:?}");
                false
            }
        };
        started | self.present()
    }

    /// The engine's next sampled change, in ms. While the rule holds curves
    /// that a picture already carries, no frame is wanted for them: the
    /// present starts them, and the deadline is the wake if it never comes.
    pub(crate) fn motion_next_ms(&self) -> Option<f64> {
        let next = if self.engine.starts_on_frame() && !self.holds_uncarried() {
            self.engine
                .next_change_unheld()
                .into_iter()
                .chain(self.engine.held_deadline())
                .reduce(f64::min)
        } else {
            self.engine.next_change()
        };
        next.map(|seconds| seconds * 1000.0)
    }

    /// Plays that started: a lowering executor (Android's layer tracks) takes
    /// their tracks again, in a picture owed for it. Whether any started.
    pub(crate) fn motion_started(&mut self, nodes: &[u64]) -> bool {
        if !nodes.is_empty() {
            self.lowered_epoch += 1;
            for node in nodes {
                self.lowered_changed.insert(*node, self.lowered_epoch);
            }
        }
        !nodes.is_empty()
    }
}
