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
    held: VecDeque<Outcome>,
    bytes: usize,
}

impl<D: DataSource> Runner<D> {
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
        if behind.held.len() < MOST_HELD && behind.bytes + size(&outcome) <= MOST_HELD_BYTES {
            behind.bytes += size(&outcome);
            behind.held.push_back(outcome);
            return Break(Ok(None));
        }
        let message = format!(
            "the stream's mapper fell behind: {} messages ({} bytes) waited for its turns",
            behind.held.len(),
            behind.bytes
        );
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
        Break(self.release_failed(ticket, target, why))
    }

    fn turn_landed(
        &mut self,
        stream: u64,
        outcome: Outcome,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let behind = (self.mapper_turns.by_stream.get_mut(&stream)).expect("found by turn_stream");
        let (_, held) = behind.out.take().expect("found by turn_stream");
        let result = if !held {
            self.run_turn(stream, outcome)
        } else if let Some(next) = behind.held.pop_front() {
            behind.bytes -= size(&next);
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
        outcome: Outcome,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let was_poisoned = self.poisoned;
        let checkpoint = self.checkpoint(true);
        let pos =
            (self.pending.iter().position(|p| p.ticket == stream)).expect("found by turn_stream");
        let p = self.pending[pos].clone();
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
            return self.release_failed(stream, target, Failure::of(e));
        }
        result.map(Some)
    }
}

/// What a held outcome counts against `MOST_HELD_BYTES`.
fn size(outcome: &Outcome) -> usize {
    match outcome {
        Outcome::Message(m) => m.data.len() + m.binary.as_ref().map_or(0, Vec::len),
        _ => 0,
    }
}
