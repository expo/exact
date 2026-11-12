//! A settle's requests: handed to the executor, and an app's aborts answered.
use super::{lift, run, Dispatch, Executor, FailureKind, Held, Outcome, Settled};
use exact_runner::{DataSource, Runner, RunnerError};
use std::time::Instant;

pub(super) fn hand_out<D: DataSource>(
    runner: &mut Runner<D>,
    executor: &Executor,
    held: &mut Held,
) {
    executor.forget(|ticket| runner.holds(ticket));
    // Of the uploads it let go, which had begun to go out (LLP 1016.000 D10).
    let started = executor.let_go_started(|ticket| runner.holds(ticket));
    runner.uploads_started(&started);
    lift(executor, held);
    for r in runner.take_requests() {
        // Device capabilities: the environment has none; nor may a long poll run on other work (LLP 1016.000 D8).
        if r.request.surface.is_some()
            || r.request.storage.is_some()
            || r.request.long_poll_refusal().is_some()
        {
            held.refused.insert(r.ticket);
            continue;
        }
        let dispatch = match r.request.continuation {
            Some(token) => runner.dispatch_work(token),
            None if r.request.is_native() => runner.native_work(&r.request),
            None => Dispatch::Missing,
        };
        run(executor, held, r, dispatch);
    }
    for (token, dispatch) in runner.release_work() {
        if let Some(r) = held.parked.remove(&token) {
            run(executor, held, r, dispatch);
        }
    }
    // Again for what this batch put behind a refusal with nothing earlier
    // in flight: no completion would come to lift it.
    lift(executor, held);
}

/// A fetch the app aborted: its answer goes on now, and its request is
/// cancelled whatever its method, as on a host. Whether any answer went on.
pub(super) fn settle_aborts<D: DataSource>(
    runner: &mut Runner<D>,
    executor: &Executor,
) -> Result<bool, RunnerError> {
    runner.collect_aborts();
    let mut aborted = false;
    while let Some((ticket, outcome)) = runner.take_aborted_answer() {
        runner.fulfill_measured(ticket, outcome, None)?;
        aborted = true;
    }
    executor.cancel(&runner.take_explicit_aborts());
    Ok(aborted)
}

/// Run `runner`'s requests through `executor` until none is in flight but
/// the ones the environment refused, or until `deadline`. Each reply is a
/// commit, as on a host; a failed request is the source's data to shape
/// (LLP 1016 D4). An error returned by fulfillment ends the render.
pub fn settle<D: DataSource>(
    runner: &mut Runner<D>,
    executor: &Executor,
    deadline: Instant,
) -> Result<Settled, RunnerError> {
    settle_with_clock(runner, executor, deadline, Instant::now)
}

fn settle_with_clock<D: DataSource>(
    runner: &mut Runner<D>,
    executor: &Executor,
    deadline: Instant,
    clock: impl Fn() -> Instant,
) -> Result<Settled, RunnerError> {
    let mut held = Held::default();
    loop {
        hand_out(runner, executor, &mut held);
        if settle_aborts(runner, executor)? {
            continue;
        }
        // Check before taking the next reply: an open stream may always have
        // one waiting.
        if runner
            .pending()
            .iter()
            // A re-ask's wait is the runner's, never the executor's: no timer
            // fires here, and the checkpoint lists it for the client to ask.
            // A stream that has answered is open, not in flight: the page
            // shows its message, and the checkpoint lists it for the client
            // to subscribe again. A mutation's stream ends with the render.
            .all(|(_, t)| {
                held.refused.contains(t)
                    || held.busy.contains(t)
                    || runner.is_reask_wait(*t)
                    || runner.is_open_stream(*t)
            })
        {
            return Ok(if held.busy.is_empty() {
                Settled::Complete
            } else {
                Settled::Busy
            });
        }
        executor.begin_pump();
        let mut pump = super::pump::Pump::new(&clock);
        let mut taken = false;
        while let Some((ticket, outcome, timing)) = pump.take(|| executor.drain().pop()) {
            taken = true;
            if matches!(
                outcome,
                Outcome::Failed {
                    kind: FailureKind::Refused,
                    ..
                }
            ) {
                held.refused.insert(ticket);
            } else {
                runner.fulfill_measured(ticket, outcome, timing)?;
            }
            if Instant::now() >= deadline {
                return Ok(Settled::Deadline);
            }
        }
        if taken {
            continue;
        }
        if Instant::now() >= deadline {
            return Ok(Settled::Deadline);
        }
        executor.wait(deadline);
    }
}

#[cfg(test)]
#[path = "pump_tests.rs"]
mod tests;
