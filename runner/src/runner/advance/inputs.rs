//! An input that arrives while work is due before it (the timers RFC, D2):
//! that work runs first, then the input, at its arrival. A change action
//! yields for the host to apply its receipt (`needs_apply`), so an input
//! that arrives behind one waits here, held, until an advance has run what
//! was due by its arrival; [`Runner::timer_due_ms`] counts it, so a
//! wall-clock host advances again at once.
use super::super::{
    Advanced, CommitReceipt, DataSource, Event, Runner, RunnerError, Timed, ViewId,
    TIMER_FIRE_LIMIT,
};

/// An input waiting on the work due by its arrival.
pub(super) struct Held {
    input: Input,
    at: f64,
}

enum Input {
    Event(ViewId, Event, Option<Box<dyn FnOnce(bool) + Send>>),
    Forward(ViewId),
}

impl<D: DataSource> Runner<D> {
    /// [`Runner::dispatch_at`]'s input, held until the work due by `now_ms`
    /// has run.
    pub(super) fn hold_event(&mut self, view: ViewId, event: Event, now_ms: f64) -> Advanced {
        self.hold_input(Input::Event(view, event, None), now_ms)
    }

    /// Dispatch as [`Runner::dispatch_at`], reporting whether this event committed its
    /// receipt, independently of timer or later host errors. A deferred event owns
    /// the callback until it runs; dropping the runner drops it without calling it.
    /// Hosts can keep imported resources here until the event publishes them.
    pub fn dispatch_at_with_receipt(
        &mut self,
        view: ViewId,
        event: Event,
        now_ms: f64,
        committed: impl FnOnce(bool) + Send + 'static,
    ) -> Advanced {
        self.hold_input(Input::Event(view, event, Some(Box::new(committed))), now_ms)
    }

    /// [`Runner::host_forward_at`]'s input, likewise.
    pub(super) fn hold_forward(&mut self, root: ViewId, now_ms: f64) -> Advanced {
        self.hold_input(Input::Forward(root), now_ms)
    }

    fn hold_input(&mut self, input: Input, at: f64) -> Advanced {
        self.clock.held.push_back(Held { input, at });
        let mut a = self.run_held(self.seek_cutoff(at), false, true);
        a.now_ms = self.now_ms;
        a
    }

    /// When the first held input arrived.
    pub(in crate::runner) fn held_input_due(&self) -> Option<f64> {
        self.clock.held.front().map(|h| h.at)
    }

    /// The held inputs that arrived by `cutoff`, in order, each after the
    /// work due by its arrival. A refusal of that work rides along and the
    /// work after it goes on, so the input still runs, at its arrival; a
    /// change action's yield or a stop on a request leaves it held, for the
    /// host to apply the receipts, or land the reply, and advance again.
    pub(super) fn run_held(&mut self, cutoff: f64, until_request: bool, timers: bool) -> Advanced {
        let mut a = Advanced {
            receipts: Vec::new(),
            now_ms: self.now_ms,
            error: None,
            stopped_on_request: false,
            needs_apply: false,
        };
        while let Some(at) = self.held_input_due().filter(|at| *at <= cutoff) {
            let due = self.seek_cutoff(at);
            for _ in 0..TIMER_FIRE_LIMIT {
                let step = self.advance_due(due, until_request, timers);
                a.receipts.extend(step.receipts);
                if step.needs_apply || step.stopped_on_request {
                    a.needs_apply = step.needs_apply;
                    a.stopped_on_request = step.stopped_on_request;
                    a.now_ms = self.now_ms;
                    return a;
                }
                let Some(error) = step.error else {
                    break;
                };
                let stops = self.poisoned
                    || matches!(error, RunnerError::TimerFireLimit { .. })
                    || a.receipts.len() >= TIMER_FIRE_LIMIT;
                a.error.get_or_insert(error);
                if stops {
                    break;
                }
            }
            let held = self.clock.held.pop_front().expect("a held input");
            let ran: Result<Option<CommitReceipt>, RunnerError> = match held.input {
                Input::Event(view, event, committed) => {
                    let result = self.dispatch(view, event);
                    if let Some(committed) = committed {
                        committed(result.is_ok());
                    }
                    result.map(Some)
                }
                Input::Forward(root) => self.host_forward(root),
            };
            match ran {
                Ok(receipt) => a.receipts.extend(receipt.map(|receipt| Timed {
                    at_ms: at.max(self.now_ms),
                    receipt,
                })),
                Err(e) => a.error = Some(e),
            }
        }
        a.now_ms = self.now_ms;
        a
    }
}
