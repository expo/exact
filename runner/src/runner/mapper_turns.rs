//! A stream's mapper that answers a message with a promise (LLP 1016.000 D1,
//! D7; Charlie, 2026-11-10, issue 20261023-a-stream-mapper-cannot-yield).
//! The source parks the mapper's call and asks for its next turn as an
//! answer's continuation; the runner sends that request on the stream's
//! behalf, beside the stream's ticket rather than in place of it, and
//! commits the call's value as the stream's when it gives one. Each turn is
//! the source's own, with its own budget.
//!
//! What lands on the stream meanwhile, messages and its end, waits behind
//! the turn in order, so a later message's mapper call never overtakes an
//! earlier one's. Each is handed back through a request of its own
//! ([`HELD`]), one per landing, and commits as it would have on arrival.
use super::{CommitReceipt, DataSource, Runner, RunnerError};
use crate::failure::{Failure, FailureCode};
use crate::request::{Dispatch, Outcome, Request, RequestOut};
use std::collections::{BTreeMap, VecDeque};
use std::ops::ControlFlow::{self, Break, Continue};

/// The continuation token of a held message's request: reserved, as
/// [`crate::BACKGROUND`] is, and answered by the runner itself.
pub(super) const HELD: u64 = u64::MAX - 5;

/// The most messages, and bytes of them, that wait behind a stream's
/// mapper turns; one more ends the stream, as a failed reply does (D4: never
/// queue without limit). 64 MiB is the most one message may be
/// (`exactIndependentHttp.maxResponseBytes`).
const MOST_HELD: usize = 4096;
const MOST_HELD_BYTES: usize = 64 << 20;

#[derive(Default)]
pub(super) struct MapperTurns {
    by_stream: BTreeMap<u64, Behind>,
    /// The reply being committed is a mapper turn's: a continuation it asks
    /// for is the stream's next turn, not a new request for its target.
    pub(super) landing: bool,
}

#[derive(Default)]
struct Behind {
    /// The request out for the stream: its ticket, and whether it hands
    /// back a held outcome rather than resuming the mapper.
    out: Option<(u64, bool)>,
    /// The mapper request's fetch, distinct from the stream's.
    fetch: Option<u64>,
    held: VecDeque<Outcome>,
    /// The messages held, a batch counting each of its own, and their bytes.
    messages: usize,
    bytes: usize,
}

impl Behind {
    /// Whether `outcome` may wait behind the turn within both bounds; if
    /// not, why the stream ends.
    fn room(&self, outcome: &Outcome) -> Result<(), String> {
        let (n, bytes) = (outcome.messages().len().max(1), size(outcome));
        if self.messages + n <= MOST_HELD && self.bytes + bytes <= MOST_HELD_BYTES {
            return Ok(());
        }
        Err(format!(
            "the stream's mapper fell behind: {} messages ({} bytes) waited for its turns, and {} more arrived",
            self.messages, self.bytes, n
        ))
    }

    /// Hold `outcome` behind the turn, last or (`first`) ahead of the rest.
    fn hold(&mut self, outcome: Outcome, first: bool) {
        self.messages += outcome.messages().len().max(1);
        self.bytes += size(&outcome);
        if first {
            self.held.push_front(outcome);
        } else {
            self.held.push_back(outcome);
        }
    }

    fn next(&mut self) -> Option<Outcome> {
        let next = self.held.pop_front()?;
        self.messages -= next.messages().len().max(1);
        self.bytes -= size(&next);
        Some(next)
    }
}

impl<D: DataSource> Runner<D> {
    pub(super) fn fail_mapper_turn(&mut self, stream: u64, message: &str) {
        if let Some(behind) = self.mapper_turns.by_stream.remove(&stream) {
            if let Some((ticket, _)) = behind.out {
                if let Some(token) = self
                    .requests
                    .iter()
                    .find(|r| r.ticket == ticket)
                    .and_then(|r| r.request.continuation)
                {
                    self.data.discard(token);
                }
                self.explicit_aborts.push(ticket);
                self.failed_tickets.insert(ticket, message.into());
                if !self.module_aborts.contains(&ticket) {
                    self.module_aborts.push(ticket);
                }
            }
        }
    }

    /// The open stream whose turn request `ticket` is.
    fn turn_stream(&self, ticket: u64) -> Option<u64> {
        let (stream, _) = (self.mapper_turns.by_stream.iter())
            .find(|(_, b)| b.out.is_some_and(|(t, _)| t == ticket))?;
        (self.pending.iter())
            .any(|p| p.ticket == *stream)
            .then_some(*stream)
    }

    /// Whether `ticket` is a turn request of a stream still open.
    pub(super) fn is_mapper_turn(&self, ticket: u64) -> bool {
        self.turn_stream(ticket).is_some()
    }

    /// The turn requests out, by their stream's target: `clock settle`
    /// waits for them as for an answer's continuation.
    pub(super) fn mapper_turn_tickets(&self) -> Vec<(String, u64)> {
        (self.mapper_turns.by_stream.iter())
            .filter_map(|(stream, b)| {
                let p = self.pending.iter().find(|p| p.ticket == *stream)?;
                Some((self.target_name(p.target), b.out?.0))
            })
            .collect()
    }

    /// The work behind a held outcome's request: none, only the landing.
    pub(super) fn held_dispatch(token: u64) -> Option<Dispatch> {
        (token == HELD).then_some(Dispatch::Again)
    }

    /// Stream `stream`'s mapper asked for its next turn.
    pub(super) fn mapper_turn(&mut self, stream: u64, request: Request) {
        self.send_turn(stream, request, false);
    }

    fn send_turn(&mut self, stream: u64, request: Request, held: bool) {
        let Some(p) = self.pending.iter().find(|p| p.ticket == stream) else {
            return;
        };
        let name = self.target_name(p.target);
        let ticket = self.next_ticket;
        self.next_ticket += 1;
        self.log(super::lines::enqueued(ticket, &name, &request));
        let behind = self.mapper_turns.by_stream.entry(stream).or_default();
        behind.out = Some((ticket, held));
        behind.fetch = request.fetch;
        self.requests.push(RequestOut {
            class: crate::RequestClass::Ordered,
            ticket,
            target: name,
            request,
            forced: false,
        });
    }

    /// Before `fulfill_now` reads `ticket`'s outcome: a turn request's
    /// landing is the turn's (`Break`, its result), and an outcome for a
    /// stream with a turn out waits behind it. Otherwise the outcome comes
    /// back to be delivered.
    pub(super) fn behind_turns(
        &mut self,
        ticket: u64,
        outcome: Outcome,
    ) -> ControlFlow<Result<Option<CommitReceipt>, RunnerError>, Outcome> {
        // A stream let go takes what waited behind it, and its turn's
        // landing is dropped as any let-go request's reply is.
        if !self.mapper_turns.by_stream.is_empty() {
            let pending = &self.pending;
            (self.mapper_turns.by_stream).retain(|s, _| pending.iter().any(|p| p.ticket == *s));
        }
        if let Some(stream) = self.turn_stream(ticket) {
            return Break(self.turn_landed(stream, outcome));
        }
        let Some(behind) = self.mapper_turns.by_stream.get_mut(&ticket) else {
            return Continue(outcome);
        };
        if behind.out.is_none() {
            return Continue(outcome);
        }
        if let Err(message) = behind.room(&outcome) {
            return self.fell_behind(ticket, message, outcome);
        }
        behind.hold(outcome, false);
        Break(Ok(None))
    }

    /// Past the bound, the stream ends as a failed reply ends a request.
    fn fell_behind(
        &mut self,
        ticket: u64,
        message: String,
        outcome: Outcome,
    ) -> ControlFlow<Result<Option<CommitReceipt>, RunnerError>, Outcome> {
        self.mapper_turns.by_stream.remove(&ticket);
        let Some(target) = (self.pending.iter().find(|p| p.ticket == ticket)).map(|p| p.target)
        else {
            return Continue(outcome);
        };
        let why = Failure {
            code: FailureCode::Error,
            message,
            reason: String::new(),
        };
        Break(self.release_failed_as_asked(ticket, target, why))
    }

    /// The rest of a batch whose message the mapper answered with a
    /// promise: they wait behind its turn, ahead of what waited already,
    /// within the same bounds; past them, why the stream ends.
    pub(super) fn hold_behind_turn(
        &mut self,
        stream: u64,
        rest: Vec<crate::Message>,
    ) -> Result<(), String> {
        if rest.is_empty() {
            return Ok(());
        }
        let behind = self.mapper_turns.by_stream.entry(stream).or_default();
        let rest = Outcome::Messages(rest);
        behind.room(&rest)?;
        behind.hold(rest, true);
        Ok(())
    }

    fn turn_landed(
        &mut self,
        stream: u64,
        outcome: Outcome,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let behind = (self.mapper_turns.by_stream.get_mut(&stream)).expect("found by turn_stream");
        let (_, held) = behind.out.take().expect("found by turn_stream");
        let fetch = behind.fetch.take();
        let result = if !held {
            self.run_turn(stream, fetch, outcome)
        } else if let Some(next) = behind.next() {
            self.fulfill_now(stream, next, None)
        } else {
            Ok(None)
        };
        // The next held outcome goes back out once no turn is.
        let open = self.pending.iter().any(|p| p.ticket == stream);
        if let Some(b) = self.mapper_turns.by_stream.get(&stream) {
            if !open || b.out.is_none() && b.held.is_empty() {
                self.mapper_turns.by_stream.remove(&stream);
            } else if b.out.is_none() {
                self.send_turn(stream, Request::continuation(HELD), true);
            }
        }
        result
    }

    /// One turn of the mapper's call, committed as the stream's message is
    /// (`commit.rs`, `message`): its value is the stream's, a failure ends
    /// the stream, and a further turn goes out from `fulfill_inner`.
    fn run_turn(
        &mut self,
        stream: u64,
        fetch: Option<u64>,
        outcome: Outcome,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let was_poisoned = self.poisoned;
        let checkpoint = self.checkpoint();
        let pos =
            (self.pending.iter().position(|p| p.ticket == stream)).expect("found by turn_stream");
        let mut p = self.pending[pos].clone();
        p.fetch = fetch;
        let target = p.target;
        let what = super::lines::fulfilling(stream, &self.target_name(target), "mapper turn");
        let (origin, id) = (self.ticket_origin(stream), self.ticket_id(stream));
        let causes = self.ticket_causes(stream);
        self.mapper_turns.landing = true;
        let result = self.as_invocation_caused(origin, id, causes, |r| r.fulfill_inner(p, outcome));
        self.mapper_turns.landing = false;
        self.conclude(checkpoint, &result, was_poisoned);
        self.as_invocation_caused(origin, id, causes, |r| r.arm_then(result.is_ok()));
        self.arm_next(result.is_ok());
        self.log_outcome(&what, &result, was_poisoned);
        if let (Err(e), true) = (&result, self.holds(stream)) {
            return self.release_failed_as_asked(stream, target, Failure::of(e));
        }
        result.map(Some)
    }
}

/// What a held outcome counts against `MOST_HELD_BYTES`.
fn size(outcome: &Outcome) -> usize {
    match outcome {
        Outcome::Message(m) => m.data.len() + m.binary.as_ref().map_or(0, Vec::len),
        Outcome::Messages(ms) => ms.iter().map(|m| m.data.len()).sum(),
        _ => 0,
    }
}
