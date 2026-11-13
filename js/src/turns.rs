//! Liveness: a waiting answer is asked again when its chain moves (LLP 1162 D3);
//! a call let go mid-turn runs its steps to the end (ledger F12).
use super::{secrets, Key, Module, TIMER, WAITING, YIELDED};
use exact_runner::{Dispatch, InFlight, Outcome, Request};
use std::collections::{HashMap, HashSet};

impl Module {
    /// A waiting answer's work: nothing to run, only an answer to ask again
    /// (`Dispatch::Again`, LLP 1041 §8.4 amended 2026-10-09). A host
    /// settles it in its ordered place without executor work.
    pub(crate) fn ask_again() -> Dispatch {
        Dispatch::Again
    }

    /// Whether `token` is an answer parked on `scheduler.yield()` or on its
    /// own timer: its work is only to be asked again (a timer's, once the
    /// runner's clock reaches it; the runner keeps it back until then).
    pub(crate) fn asked_again(&self, token: u64) -> bool {
        self.parked
            .iter()
            .any(|(_, p)| p.call == token && matches!(p.ticket, YIELDED | TIMER))
    }

    /// Whether anything in the module may yet settle a waiting answer.
    /// The module's storage queued or in flight counts, an answer's or the
    /// background's (LLP 1097 D2: an answer may await background work).
    pub(crate) fn outstanding(&self) -> bool {
        !self.streams.is_empty()
            || self.host.module_outstanding > 0
            || self.background.state.queued + self.background.state.in_flight > 0
            || self
                .parked
                .iter()
                .any(|(_, p)| p.ticket != WAITING || p.ready || p.due.is_some())
    }

    pub(crate) fn refresh_waiting(&mut self) {
        let waiting: Vec<_> = self
            .parked
            .iter()
            .filter(|(_, p)| p.ticket == WAITING)
            .map(|(_, p)| p.call)
            .collect();
        if waiting.is_empty() {
            return;
        }
        let ids = serde_json::to_string(&waiting).expect("call ids");
        let ready = self.engine.as_mut().and_then(|engine| {
            let text = engine.call("__exact_ready", [&ids, "", ""]).ok()?;
            serde_json::from_str::<HashSet<u64>>(&text).ok()
        });
        self.readied(ready.as_ref());
    }

    fn readied(&mut self, ready: Option<&HashSet<u64>>) {
        for (_, p) in &mut self.parked {
            if p.ticket == WAITING {
                // A failed readiness query must not strand an answer (LLP 1162 D3).
                p.ready = ready.is_none_or(|ids| ids.contains(&p.call));
            }
        }
    }

    pub(crate) fn release_ready(&mut self, now: f64) -> Vec<(u64, Dispatch)> {
        let mut released = self.host.secrets.release();
        for token in std::mem::take(&mut self.waiters) {
            match self.wake(token, now) {
                Some(work) => released.push((token, work)),
                None if self.parked.iter().any(|(_, p)| p.call == token) => {
                    self.waiters.push(token)
                }
                None => {}
            }
        }
        released
    }

    /// With nothing outstanding, a last settle refuses an answer still pending on nothing.
    pub(crate) fn wake(&mut self, token: u64, now: f64) -> Option<Dispatch> {
        let outstanding = self.outstanding();
        let (_, parked) = self
            .parked
            .iter_mut()
            .find(|(_, p)| p.call == token && p.ticket == WAITING)?;
        if parked.ready || parked.due.is_some_and(|due| due as f64 <= now) || !outstanding {
            parked.last = !outstanding;
            return Some(Module::ask_again());
        }
        None
    }

    /// The answer awaiting request `f`, a fetch of its own, has a timer of
    /// its own due before the fetch's reply (`fetch_timer`): the fetch runs
    /// on as module work (`__exact_adopt`), its reply delivered under its
    /// ticket in a module turn, and the answer parks on its timer, asked
    /// again at once. On the main thread only: a worker runs one turn to its
    /// end.
    pub(crate) fn timer_first(
        &mut self,
        f: &InFlight<'_>,
        fetch: Option<u64>,
    ) -> Option<(u64, Request)> {
        if !self.main_thread {
            return None;
        }
        let key = Module::key(Some(f.target), f.source, f.args);
        let at = self.parked.iter().position(|(k, p)| {
            *k == key
                && fetch.is_none_or(|fetch| p.ticket == fetch)
                && !matches!(p.ticket, 0 | WAITING | YIELDED | TIMER | secrets::TICKET)
        })?;
        let (call, ticket) = (self.parked[at].1.call, self.parked[at].1.ticket);
        let adopted = self.engine.as_mut()?.call(
            "__exact_adopt",
            [&call.to_string(), &ticket.to_string(), ""],
        );
        if adopted.ok().as_deref() != Some("adopted") {
            return None;
        }
        self.work.keep_ticket(ticket);
        let parked = &mut self.parked[at].1;
        parked.ticket = TIMER;
        parked.effects = false;
        Some((ticket, Request::continuation(call)))
    }

    /// Parked calls let go in the prelude too, with the fetches they wait on.
    /// One let go between storage steps cannot be stopped: the steps it began
    /// are running, and the app's promise chain (a serialized database, say)
    /// goes on behind them. A browser runs that turn to its end, and so does
    /// this, now, its answer discarded: otherwise the next answer chained
    /// behind it waits on steps nobody delivers and is refused as pending on
    /// nothing (ledger F12: a pre-mutation refresh the runner discarded).
    pub(crate) fn forget_calls(&mut self, calls: Vec<u64>) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        let (mut owed, mut rejected) = (false, false);
        for call in &calls {
            self.host.secrets.cancel_wait(*call);
            // Its waiter, if one is still running, now gives up without
            // taking any compression's right (LLP 1069.002 A1.5).
            if let Some(retired) = self.retired.remove(call) {
                retired.store(true, std::sync::atomic::Ordering::Release);
            }
        }
        // No answer's turn: a live answer's step that would run here waits
        // for its own (LLP 1146 D2).
        let _ = engine.call("__exact_turn", ["", "", ""]);
        for call in calls {
            if let Ok(said) = engine.call("__exact_forget", [&call.to_string(), "", ""]) {
                owed |= said.starts_with("storage");
                rejected |= said.ends_with("rejected");
            }
        }
        // The rejected fetches' continuations run now, their answers
        // discarded, before any answer is current: what they set (a busy
        // flag) is cleared, never stranded, and never lands on a live
        // answer's store. Between answers, as a let-go's steps run: storage
        // they start is the background's, not refused as at bake.
        if rejected {
            self.host.between_answers = true;
            // Steps a let-go answer had waiting on module work run here too.
            let _ = engine.call("__exact_module_discarded", ["", "", ""]);
            // An interrupt (or a job that throws) ends the job it stopped;
            // the jobs queued behind it still run here, not in the next
            // answer's drain. Each try pops at least the job it stopped; the
            // bound only ends an endless chain of failing jobs.
            let mut drained = engine.drain();
            for _ in 0..1024 {
                if drained.is_ok() {
                    break;
                }
                self.watch.take();
                drained = engine.drain();
            }
            self.host.between_answers = false;
            if drained.is_err() {
                self.watch.take();
            }
            // Storage a continuation started is the background's: arm it.
            self.refresh_background();
        }
        if owed {
            self.finish_let_go();
        }
    }

    /// Drop targeted calls the runner no longer has in flight. Storage a
    /// call already issued still runs ([`Module::forget_calls`]). LLP 1097
    /// deletes the answer-to-answer deferral, so a call that had not started
    /// is not replayed later in a cloned store.
    pub(crate) fn forget_in_flight(&mut self, in_flight: &[InFlight<'_>]) {
        let keep: HashMap<Key, Option<u64>> = in_flight
            .iter()
            .map(|f| {
                (
                    Module::key(Some(f.target), f.source, f.args),
                    f.continuation,
                )
            })
            .collect();
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.parked)
            .into_iter()
            .partition(|(key, parked)| {
                key.0.is_some()
                    && !matches!(keep.get(key), Some(None))
                    && keep.get(key) != Some(&Some(parked.call))
            });
        self.parked = kept;
        let (ended, open): (Vec<_>, Vec<_>) = std::mem::take(&mut self.streams)
            .into_iter()
            .partition(|(key, _)| key.0.is_some() && !keep.contains_key(key));
        self.streams = open;
        self.work.flight = keep;
        let mut calls = self.set_aside(gone);
        calls.extend(ended.into_iter().map(|(_, parked)| parked.call));
        self.forget_calls(calls);
    }

    /// Deliver the storage steps of calls let go mid-turn until none is left.
    /// There is no store: a let-go answer's `store.set` does not land on the
    /// live answer's store, and storage is not refused as at bake
    /// (`HostState::between_answers`).
    pub(crate) fn finish_let_go(&mut self) {
        let (Some(session), Some(engine)) = (self.storage.as_ref(), self.engine.as_mut()) else {
            return;
        };
        // Each step the chain begins after the first must reach storage too:
        // a refusal there ends the chain with its write unmade (splitter
        // rough 7). `between_answers` is that window: no store, and storage
        // is not refused as at bake.
        self.host.between_answers = true;
        let mut delivered = false;
        while engine
            .call("__exact_let_go", ["", "", ""])
            .is_ok_and(|r| r == "storage")
        {
            if let Outcome::Failed { message, .. } = session.continuation()() {
                // A compression so failed is settled and what was queued
                // behind it issued (LLP 1069.002 A1.5): a progress, and the
                // chain's other storage is delivered on.
                if engine
                    .call("__exact_let_go", ["failed", &message, ""])
                    .is_ok_and(|r| r == "settled")
                    && engine.drain().is_ok()
                {
                    delivered = true;
                    continue;
                }
                break;
            }
            if engine.deliver_storage_one().is_err() || engine.drain().is_err() {
                break;
            }
            delivered = true;
        }
        self.host.between_answers = false;
        if !delivered {
            return;
        }
        self.refresh_background();
    }
}

#[cfg(all(test, exact_js_engine))]
#[path = "turns_tests.rs"]
mod tests;
