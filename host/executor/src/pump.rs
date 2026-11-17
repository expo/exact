//! One wake's fulfillment budget (LLP 1162 D3, LLP 1041 D3).
use exact_runner::{CompletionTiming, DataSource, Outcome, Runner};
use std::time::{Duration, Instant};

/// A reply and the transport timing retained for its transaction.
pub type Completion = (u64, Outcome, Option<CompletionTiming>);

/// The count and observed fulfillment cost of one wake.
pub struct Pump<C> {
    clock: C,
    started: Option<Instant>,
    taken: usize,
}

impl<C: Fn() -> Instant> Pump<C> {
    /// Start a pump with a monotonic clock, held by deterministic tests.
    pub fn new(clock: C) -> Self {
        Self {
            clock,
            started: None,
            taken: 0,
        }
    }

    /// Take lazily: the preceding transaction finishes before the next
    /// budget check, and a shaping error leaves later replies queued.
    pub fn take<T>(&mut self, next: impl FnOnce() -> Option<T>) -> Option<T> {
        let now = (self.clock)();
        let started = *self.started.get_or_insert(now);
        if self.taken == 32 || now.saturating_duration_since(started) >= Duration::from_millis(4) {
            return None;
        }
        let outcome = next()?;
        self.taken += 1;
        Some(outcome)
    }

    /// Alternate refusal opportunities and the executor's next ready lane.
    pub fn ready<D: DataSource>(
        &mut self,
        runner: &mut Runner<D>,
        refusal_turn: &mut bool,
        idle: impl Fn() -> bool,
        drain: impl Fn() -> Vec<Completion>,
    ) -> Option<Completion> {
        self.take(|| {
            *refusal_turn = !*refusal_turn;
            let refusal = |runner: &mut Runner<D>| {
                runner
                    .take_request_refusal(idle())
                    .map(|(ticket, outcome)| (ticket, outcome, None))
            };
            if *refusal_turn {
                refusal(runner).or_else(|| drain().pop())
            } else {
                drain().pop().or_else(|| refusal(runner))
            }
        })
    }
}
