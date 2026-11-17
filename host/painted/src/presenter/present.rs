//! A display host's presents (LLP 1003.001 D9): each picture carries the
//! engine time it was produced at, and its present, heard from the display,
//! starts what it carried and sets the page's `presented` (LLP 1069.000 D2).
use super::*;

/// What a picture carries to its present: the host that painted it and the
/// engine time it was produced at. A present for another host (a reload,
/// an activated update) starts nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Carried {
    host: u64,
    at: f64,
}

impl<D: DataSource> Presenter<D> {
    /// Whether this presenter's pictures reach a display whose presents its
    /// host hears: the rule holds what a commit begins until then, while the
    /// live clock owns time. A display host and the Android reader say so;
    /// headless, the render host and a driver's launch do not.
    pub fn presents_pictures(&mut self, on: bool) {
        self.presents = on;
        // Before the live clock, the rule stays as the boot set it.
        if !on || self.live_clock.is_some() {
            self.dirty |= self.host.frame_clock(on);
        }
    }

    /// A picture is produced now: what its present will start.
    pub fn picture_carried(&mut self) -> Carried {
        Carried {
            host: self.hosts,
            at: self.host.picture_carried(),
        }
    }

    /// Whether a picture painted now is to have its present reported:
    /// something is held, or the page has not been presented.
    pub fn present_wanted(&self) -> bool {
        let engine = self.host.engine();
        !self.host.runner().presented().seen
            || engine.starts_on_frame() && engine.held_since().is_some()
    }

    /// The present, at `at_ms` on the loop's clock, of the picture that
    /// carried `carried`.
    pub fn picture_presented(&mut self, carried: Carried, at_ms: f64) -> Option<String> {
        if carried.host != self.hosts {
            return None;
        }
        let (paint, error) = self.host.picture_presented(carried.at, at_ms);
        self.dirty |= paint;
        error.or_else(|| self.after_commit())
    }

    /// `prefer presented`: `exactPage().presented` held at `value`, moving
    /// no animation (LLP 1069.000 D6).
    pub fn prefer_presented(&mut self, value: Option<bool>) -> Option<String> {
        self.host
            .prefer_presented(value)
            .or_else(|| self.after_commit())
    }

    /// Whether anything will present: the page is visible and not frozen,
    /// and the carrier presents this session's frames.
    pub(super) fn refresh_presentable(&mut self) {
        let page = self.host.runner().page();
        let presentable = !page.hidden && !page.frozen && !self.timers_hidden;
        self.dirty |= self.host.set_presentable(presentable);
    }

    /// Holds whose present has not come by the deadline start at their
    /// commit, with no display frame.
    pub(super) fn release_overdue(&mut self) {
        self.dirty |= self.host.release_overdue();
    }

    /// A driver took the clock: what waits starts now (D7), each frame is
    /// presented at its commit, so the page is presented now, and a runner
    /// booted from here on is too.
    pub(super) fn driven_presented(&mut self) {
        exact_runner::page::set_awaits_present(false);
        self.dirty |= self.host.frame_clock(false);
        if let Some(error) = self.host.page_presented() {
            self.host.log(error);
        }
    }
}

#[cfg(test)]
#[path = "present_tests.rs"]
mod tests;
