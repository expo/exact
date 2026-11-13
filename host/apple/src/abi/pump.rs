use super::*;
use crate::executor::pump::Pump;
use std::time::Instant;

impl<D: DataSource> Bridge<D> {
    /// Fulfill ready replies as separate transactions, then lay out and
    /// present one batch within the wake's budget (LLP 1162 D3).
    pub fn pump(&mut self, now_ms: f64) -> u32 {
        self.pump_with_clock(now_ms, Instant::now)
    }

    fn pump_with_clock(&mut self, now_ms: f64, clock: impl Fn() -> Instant) -> u32 {
        if let Some(host) = self.host.as_mut() {
            if host
                .launch_store
                .as_ref()
                .is_some_and(|store| store.take_changed())
            {
                if let Some(executor) = self.executor.as_deref() {
                    executor.begin_pump();
                }
                let out = host.refresh_store_after(false);
                if let Some(executor) = self.executor.as_deref() {
                    executor.notify();
                }
                return self.emit(out);
            }
        }
        let out = match (self.host.as_mut(), self.executor.as_deref()) {
            (Some(host), Some(executor)) => {
                executor.begin_pump();
                host.runner_mut().collect_aborts();
                let mut pump = Pump::new(clock);
                let turn = &mut self.refusal_turn;
                let out = host.fulfill_ready(
                    |runner| {
                        pump.ready(
                            runner,
                            turn,
                            || executor.ordered_idle(),
                            || executor.drain(),
                        )
                    },
                    now_ms,
                );
                if host.has_request_refusals(executor.ordered_idle()) {
                    executor.notify();
                }
                out
            }
            (Some(host), None) => host.fulfill_all(vec![], now_ms),
            _ => not_booted(),
        };
        self.emit(out)
    }
}

#[cfg(test)]
#[path = "pump_tests.rs"]
mod tests;
