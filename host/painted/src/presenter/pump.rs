//! The executor's replies into the runner, each a commit (LLP 1016 D2).
use super::*;
use crate::executor::pump::Pump;
use std::time::Instant;

#[cfg(all(test, unix))]
#[path = "abort_tests.rs"]
mod abort_tests;

#[cfg(test)]
#[path = "held_work_tests.rs"]
mod held_work_tests;

impl<D: DataSource> Presenter<D> {
    /// The executor's replies into the runner (LLP 1016 D2), each a
    /// commit: the display loop calls this when the executor's fd is
    /// readable, the agent when it waits. A source may also wake it to
    /// release held work without a reply.
    pub fn pump(&mut self, now_ms: f64) -> Option<String> {
        self.pump_with_clock(now_ms, Instant::now)
    }

    fn pump_with_clock(&mut self, now_ms: f64, clock: impl Fn() -> Instant) -> Option<String> {
        let performance_error = self.poll_canvas_performance();
        let region_error = self.poll_content_region().or(performance_error);
        self.executor.begin_pump();
        self.finish_portal_picks();
        self.finish_file_imports();
        // Waiting admission can refuse without a completion to commit (LLP 1162 D1).
        if !self.host.has_ordered_request_refusals() {
            for (ticket, reason) in self.executor.resume_ordered() {
                self.host.refuse_request(ticket, reason, true);
                self.executor.notify();
            }
        }
        let region_error = region_error.or(self.dispatch_authored_scroll());
        // An abort a source heard off this thread (a worker's let-go).
        self.host.runner_mut().collect_aborts();
        let mut pump = Pump::new(clock);
        let executor = &self.executor;
        let turn = &mut self.refusal_turn;
        let mut next = |runner: &mut exact_runner::Runner<D>| {
            pump.ready(
                runner,
                turn,
                || executor.ordered_idle(),
                || executor.drain(),
            )
        };
        let first = next(self.host.runner_mut());
        if first.is_none() && !self.host.has_announced() {
            // A cancel with no answer to give (its request was retired) still goes.
            let aborted = self.host.runner_mut().take_explicit_aborts();
            self.executor.cancel(&aborted);
            self.release_held_work();
            let refined = self.refine_collections();
            return region_error
                .or(refined)
                .or(self.refresh_transform_geometry())
                .or(self.deliver_scroll_ends());
        }
        let mut first = first;
        let e = self
            .host
            .fulfill_ready(|runner| first.take().or_else(|| next(runner)), now_ms);
        if self.host.has_request_refusals(self.executor.ordered_idle()) {
            self.executor.notify();
        }
        let after = self.after_commit();
        let ends = self.deliver_scroll_ends();
        region_error.or(e).or(after).or(ends)
    }

    pub(super) fn release_held_work(&mut self) {
        for (token, dispatch) in self.host.release_work() {
            if let Some(r) = self.parked.remove(&token) {
                self.run_dispatch(r, dispatch);
            }
        }
        // A request the runner let go (newer arguments, a refresh, a target
        // answered now) is never released; one it keeps for its effects is.
        let runner = self.host.runner();
        self.parked.retain(|_, r| runner.holds(r.ticket));
    }
}

#[cfg(test)]
#[path = "pump_tests.rs"]
mod tests;
