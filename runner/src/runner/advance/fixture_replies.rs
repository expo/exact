//! A test response is admitted and read through the usual executor. Only its
//! delivery waits for the authored deadline. Replacing a rule cannot alter a
//! reply already selected for a ticket.

use super::super::*;
use crate::{CompletionTiming, FailureKind, Outcome};

type Reply = (Outcome, Option<CompletionTiming>);

pub(super) struct Delayed {
    ticket: u64,
    due: f64,
    timeout: Option<u32>,
    reply: Option<Reply>,
}

impl<D: DataSource> Runner<D> {
    /// Delay delivery of the fixture selected for this exchange. The caller
    /// supplies the request's timeout so an earlier deadline still wins.
    pub fn delay_fetch_fixture(
        &mut self,
        ticket: u64,
        asked_at: f64,
        delay_ms: u32,
        timeout_ms: Option<u32>,
    ) {
        if delay_ms == 0 || !asked_at.is_finite() || !self.holds(ticket) {
            return;
        }
        let timeout = timeout_ms.filter(|ms| *ms <= delay_ms);
        self.clock.fixtures.push(Delayed {
            ticket,
            due: asked_at + f64::from(timeout.unwrap_or(delay_ms)),
            timeout,
            reply: None,
        });
    }

    pub(in crate::runner) fn park_fixture_reply(
        &mut self,
        ticket: u64,
        outcome: Outcome,
        timing: Option<CompletionTiming>,
    ) -> Option<Reply> {
        let Some(i) = self.clock.fixtures.iter().position(|f| f.ticket == ticket) else {
            return Some((outcome, timing));
        };
        // An explicit abort, grant refusal or failed body read wins immediately.
        if !matches!(outcome, Outcome::Response(_)) {
            self.clock.fixtures.remove(i);
            return Some((outcome, timing));
        }
        let f = &mut self.clock.fixtures[i];
        let outcome = match f.timeout {
            Some(ms) => Outcome::Failed {
                kind: FailureKind::Timeout,
                message: format!("the request timed out after {ms} ms"),
            },
            None => outcome,
        };
        if f.due <= self.now_ms {
            self.clock.fixtures.remove(i);
            return Some((outcome, timing));
        }
        if f.reply.is_none() {
            f.reply = Some((outcome, timing));
        }
        None
    }

    pub(super) fn prune_fixture_replies(&mut self) {
        if !self.clock.fixtures.is_empty() {
            let mut fixtures = std::mem::take(&mut self.clock.fixtures);
            fixtures.retain(|f| self.holds(f.ticket));
            self.clock.fixtures = fixtures;
        }
    }

    pub(super) fn fixture_due(&self, ticket: Option<u64>) -> Option<f64> {
        self.clock
            .fixtures
            .iter()
            .filter(|f| {
                f.reply.is_some() && ticket.is_none_or(|t| t == f.ticket) && self.holds(f.ticket)
            })
            .map(|f| f.due)
            .reduce(f64::min)
    }

    pub(super) fn release_fixture_replies(
        &mut self,
        at: f64,
        receipts: &mut Vec<Timed>,
    ) -> Result<(), RunnerError> {
        while let Some((i, _)) = self
            .clock
            .fixtures
            .iter()
            .enumerate()
            .filter(|(_, f)| f.reply.is_some() && f.due <= at)
            .min_by(|(_, a), (_, b)| a.due.total_cmp(&b.due))
        {
            let f = self.clock.fixtures.remove(i);
            if !self.holds(f.ticket) {
                continue;
            }
            let (outcome, timing) = f.reply.unwrap();
            if let Some(receipt) = self.fulfill_measured(f.ticket, outcome, timing)? {
                receipts.push(Timed {
                    at_ms: self.now_ms,
                    receipt,
                });
            }
        }
        Ok(())
    }
}
