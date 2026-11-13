//! Request replies and their transactions.

use super::*;

impl<D: DataSource> Runner<D> {
    /// A long poll's reply, an event or not (LLP 1016.000 D8): from its first
    /// it is held, and each raises `heldReplies` for `clock held`.
    fn long_poll_replied(&mut self, pos: usize) {
        let p = &mut self.pending[pos];
        p.replied = true;
        let target = p.target;
        self.count_held_reply(target);
        self.sync_pending_flags();
    }

    fn count_held_reply(&mut self, target: Target) {
        let name = self.target_name(target);
        match self.held_replies.iter_mut().find(|(n, _)| *n == name) {
            Some((_, count)) => *count += 1,
            None => self.held_replies.push((name, 1)),
        }
    }

    /// Whether request `ticket` is still wanted. A host asks after each
    /// commit and lets go of the work for any it holds that isn't (LLP 1016
    /// D5): a superseded or forgotten reply would only be dropped here.
    pub fn holds(&self, ticket: u64) -> bool {
        self.is_background(ticket)
            || self.is_tool(ticket)
            || self.is_module_work(ticket)
            || self.is_mapper_turn(ticket)
            || self.pending.iter().any(|p| p.ticket == ticket)
    }

    /// The host brought back the outcome of request `ticket`: the source
    /// parses it, the resource takes its value or the mutation's slot its
    /// `some`, and everything downstream settles as after an action — one
    /// commit. A ticket no longer held (forgotten, D5) is dropped with a
    /// journal line and no commit.
    pub fn fulfill(
        &mut self,
        ticket: u64,
        outcome: Outcome,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        self.fulfill_measured(ticket, outcome, None)
    }

    /// Fulfill with execution and delivery wait measured by the executor,
    /// never by the seekable agent clock. Missing measurements stay unnamed.
    pub fn fulfill_measured(
        &mut self,
        ticket: u64,
        outcome: Outcome,
        timing: Option<crate::CompletionTiming>,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        // A let-go request's reply: it has ended, and an abort names it no more.
        self.retired_fetches.retain(|r| r.1 != ticket);
        let answering = std::mem::replace(&mut self.churn.answering, true);
        let out = self.arrived(|r| r.fulfill_now(ticket, outcome, timing));
        self.churn.answering = answering;
        out
    }

    pub(super) fn fulfill_now(
        &mut self,
        ticket: u64,
        outcome: Outcome,
        timing: Option<crate::CompletionTiming>,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        self.hear_aborted();
        if let Some(message) = self.failed_tickets.remove(&ticket) {
            return self.executor_failed(ticket, message);
        }
        if self.is_background(ticket) {
            // Its own path (LLP 1097 D5): no commit, no `then`, no update.
            self.background_landed(outcome);
            return Ok(None);
        }
        if self.is_tool(ticket) {
            // A handler's request (LLP 1133.016 D9): its turn resumes, no commit.
            self.tool_landed(ticket, outcome);
            return Ok(None);
        }
        if self.is_module_work(ticket) {
            // Module work's (LLP 1146 D2): its turn, then a commit of its own.
            return self.module_work_landed(ticket, outcome);
        }
        // A stream's mapper turn, and what lands behind one (`mapper_turns.rs`).
        let outcome = match self.behind_turns(ticket, outcome) {
            std::ops::ControlFlow::Continue(outcome) => outcome,
            std::ops::ControlFlow::Break(done) => return done,
        };
        let mut summary = outcome.summary();
        if let Some(timing) = timing {
            if timing.cached {
                summary.push_str("; HTTP cache");
            }
            exact_num::push_text!(&mut summary, "; wall {} ms", timing.elapsed_ms);
            if timing.delivery_wait_ms > 0 {
                exact_num::push_text!(
                    &mut summary,
                    "; delivery wait {} ms",
                    timing.delivery_wait_ms
                );
            }
        }
        let Some(pos) = self.pending.iter().position(|p| p.ticket == ticket) else {
            self.log(super::lines::dropped(ticket, &summary));
            // A module's reply carrying handles that is not delivered
            // disposes of them (LLP 1133.009 D6).
            self.dispose_dropped();
            return Ok(None);
        };
        let handles = self.landed_handles(ticket);
        self.landed_send(ticket);
        if let (Some(_), Outcome::Message(m)) = (self.pending[pos].stream, &outcome) {
            // A long poll's reply that is no event (LLP 1016.000 D8): it holds.
            if self.pending[pos].long_poll && m.event == crate::LONG_POLL_REPLY {
                self.long_poll_replied(pos);
                return Ok(None);
            }
            // A listener's word on its address alone (LLP 1133.005 D2).
            if self.listen_event(pos, m) {
                return Ok(None);
            }
            // An upload's start (LLP 1016.000 D10): noted; no mapper sees it.
            if self.pending[pos].upload && m.upload_started() {
                self.pending[pos].sent = true;
                return Ok(None);
            }
            return self.message(pos, outcome, &summary);
        }
        // Messages that waited together (LLP 1016.000 D4): one commit.
        if let (Some(_), Outcome::Messages(_)) = (self.pending[pos].stream, &outcome) {
            return self.message(pos, outcome, &summary);
        }
        // A re-ask's wait was never sent (`overrun.rs`): only its invocation's end fails it.
        if self.pending[pos].reask_at.is_some() {
            return self.reask_answered(ticket, outcome, &summary);
        }
        self.overrun.replied = false;
        let was_poisoned = self.poisoned;
        let checkpoint = self.checkpoint();
        let p = self.pending.remove(pos);
        self.sync_pending_flags();
        if p.held || p.long_poll {
            self.count_held_reply(p.target);
        }
        let what = super::lines::fulfilling(ticket, &self.target_name(p.target), &summary);
        let target = p.target;
        // An answer lands as a commit of its request's origin (LLP 1133.002
        // D12), with every cause the request has (LLP 1133.035 D16 item 6).
        let (origin, id) = (self.ticket_origin(ticket), self.ticket_id(ticket));
        let causes = self.ticket_causes(ticket);
        let result = self.as_invocation_caused(origin, id, causes, |r| r.fulfill_inner(p, outcome));
        self.conclude(checkpoint, &result, was_poisoned);
        // Its `then` is armed under the same origin, invocation and causes.
        self.as_invocation_caused(origin, id, causes, |r| r.arm_then(result.is_ok()));
        self.arm_next(result.is_ok());
        self.log_outcome(&what, &result, was_poisoned);
        if let (Some(handles), Err(_)) = (handles, &result) {
            self.dispose(handles, "its commit was refused");
        }
        if let Err(e) = &result {
            // A one-shot reply is spent whatever refused its commit: the host
            // delivers it once, so left in flight by the rollback it would be
            // pending forever (a mutation's write shown, a resource's
            // `pending(…)`, a clock waiting on it), whether the source failed
            // to take it, a refusal's shaped failure trapped, or a value it
            // took trapped in a derive. The failure is in the journal (above);
            // what the host needs now is the commit that takes the target out
            // of `pending`. Admission refusal alone cannot restart a source
            // whose earlier turns may have written.
            if self.holds(ticket) {
                // Its own turn over its budget: asked again (`overrun.rs`).
                if self.hold_overrun_reply(ticket, target, e) {
                    return Ok(None);
                }
                return self.release_failed_as_asked(
                    ticket,
                    target,
                    crate::failure::Failure::of(e),
                );
            }
        }
        result.map(Some)
    }

    /// One message of the open stream at `pos` (LLP 1016.000 D1): parsed
    /// like a single reply and committed as its own settlement, with the
    /// ticket kept open. A message the source cannot take ends the stream,
    /// as a failed reply ends a request.
    fn message(
        &mut self,
        pos: usize,
        outcome: Outcome,
        summary: &str,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let was_poisoned = self.poisoned;
        // An upload's bytes sent: it has reached the network (D10), which a
        // refused commit of this message does not undo.
        let upload = self.pending[pos].upload;
        self.pending[pos].sent |= upload;
        let checkpoint = self.checkpoint();
        if self.pending[pos].long_poll {
            self.long_poll_replied(pos);
        }
        let p = &mut self.pending[pos];
        if let Some(count) = p.stream.as_mut() {
            for m in outcome.messages() {
                count.coalesced += u64::from(m.coalesced);
                sockets::socket_message(count, m);
            }
        }
        let p = p.clone();
        self.note_child_outcome(p.ticket, &outcome);
        let (ticket, target, name) = (p.ticket, p.target, self.target_name(p.target));
        let what = super::lines::fulfilling(ticket, &name, summary);
        let before = self.sockets.len();
        crate::stream_timing::entered(ticket);
        // A frame lands as a commit of its stream's origin (LLP 1133.002
        // D12), with every cause its request has (LLP 1133.035 D16 item 6),
        // as a one-shot's reply does: a stream a tool asked answers as the
        // tool's, frame after frame.
        let (origin, id) = (self.ticket_origin(ticket), self.ticket_id(ticket));
        let causes = self.ticket_causes(ticket);
        let result = self.as_invocation_caused(origin, id, causes, |r| r.fulfill_inner(p, outcome));
        // What the mapper staged answering this frame (LLP 1016.000 D7):
        // addressed for the commit, or left staged by a frame that failed.
        // The commit's answers may have sent too (its amendment of
        // 2026-10-18); their batches are not the mapper's.
        let left = !self.store.take_sockets().is_empty();
        let mapper = |b: &crate::SocketSend| matches!(b.origin, crate::SocketOrigin::Mapper(_));
        let replied = left || self.sockets[before..].iter().any(mapper);
        self.conclude(checkpoint, &result, was_poisoned);
        crate::stream_timing::committed(result.is_ok());
        self.as_invocation_caused(origin, id, causes, |r| r.arm_then(result.is_ok()));
        self.arm_next(result.is_ok());
        self.log_outcome(&what, &result, was_poisoned);
        // Or by one whose answer reopened the stream, which sends none.
        if replied && (result.is_err() || left) {
            let why = (result.as_ref().err())
                .map_or("its answer reopened the stream".into(), |e| {
                    crate::failure::Failure::of(e).message
                });
            self.log(sockets::refused(&crate::SocketOrigin::Mapper(name), &why));
        }
        if let (Err(e), true) = (&result, self.holds(ticket)) {
            return self.release_failed_as_asked(ticket, target, crate::failure::Failure::of(e));
        }
        result.map(Some)
    }

    pub(super) fn fulfill_inner(
        &mut self,
        p: PendingReq,
        outcome: Outcome,
    ) -> Result<CommitReceipt, RunnerError> {
        if self.poisoned {
            return Err(RunnerError::Poisoned);
        }
        let name = self.target_name(p.target);
        let turn = std::mem::take(&mut self.mapper_turns.landing);
        let message = p.stream.is_some() && !outcome.messages().is_empty();
        let messages = outcome.messages().len().max(1) as u64;
        let failed_end = matches!(outcome, Outcome::Failed { .. });
        let ty = match p.target {
            Target::Resource(i) => self.resource_row(i).ty,
            Target::Mutation(m) | Target::Sent(m, _) => self.plan.mutations[m].ty,
        };
        self.store.take_topics();
        self.snapshot_sockets();
        // A live module source's answer may name when it goes stale (LLP
        // 1133.015 D7), whether Contract or data code reads it.
        let live = (self.plan.module_named(&p.source))
            .filter(|row| row.live && !row.send)
            .map(|row| self.key(row))
            .or_else(|| (p.keepable.as_ref()).and_then(|r| self.module_topic(&r.body)));
        let outcome = match (live, outcome) {
            (Some(key), Outcome::Response(mut response)) => {
                response.body = self.take_stale(&key, std::mem::take(&mut response.body));
                Outcome::Response(response)
            }
            (_, outcome) => outcome,
        };
        self.store.set_clock_ms(self.now_ms);
        let (reads_before, revision_before) = (self.store.reads(), self.store.revision());
        let draws_before = self.store.entropy_draws();
        let parsed = if Self::activity_send(&p.source) {
            Self::activity_reply(&self.plan, ty, outcome).map(Some)
        } else if self.plan.module_named(&p.source).is_some() {
            self.module_parse(p.target, ty, &p.source, outcome)
                .map(Some)
        } else if self.plan.fetch_named(&p.source).is_some() {
            self.fetch_parse(ty, &p.source, outcome).map(Some)
        } else {
            let parsed = match outcome {
                Outcome::Messages(batch) => self.parse_batch(&p, batch),
                outcome => self.parse_frame(&p, message, outcome),
            };
            let parsed = parsed.and_then(|answer| match answer {
                Some(answer) if p.target.mutation().is_some() => {
                    self.single_answer(p.target, Ok(answer)).map(Some)
                }
                answer => Ok(answer),
            });
            self.refuse_locked(parsed)
        };
        // What the turn of a reply to a request with effects wrote stands,
        // whatever the commit then does: it keeps what the server did, as a
        // browser's continuation keeps it outside every commit (LLP 1146 D2).
        if p.effects {
            self.module_work.standing = Some(self.store.checkpoint());
        }
        if self.store.revision() != revision_before {
            self.ask_failed_readers(Some(p.target));
        }
        // Only a socket's mapper answering a frame stages for this commit
        // (LLP 1016.000 D7); a turn of an answer sends whatever it answered
        // (the amendment of 2026-10-18).
        if !message {
            self.store.take_sockets();
        }
        self.stage_answer(p.target);
        self.hear_aborted();
        for line in self.data.take_logs() {
            self.log(line);
        }
        // What this reply's turn watched joins the resource's topics whatever
        // the reply says: a turn that yields another request (a long native
        // call) watched them as much as the one that answers.
        if let Target::Resource(i) = p.target {
            // A read in a later turn (after an `await`) makes a store reader
            // as one in the first does (LLP 1146 §2.3).
            if self.store.reads() > reads_before {
                let contents = self.note_reads(i, reads_before, draws_before);
                self.module_work.reply_reader = Some((i, contents));
            }
            // A module's source that data code read: the answer is asked
            // again when that source's answer changes (LLP 1133.002 D14).
            let module = (p.keepable.as_ref()).and_then(|r| self.module_topic(&r.body));
            for topic in self.store.take_topics().into_iter().chain(module) {
                if !self.resources[i].watching.contains(&topic) {
                    self.resources[i].watching.push(topic);
                }
            }
        }
        self.overrun.replied = matches!(parsed, Err(DataError::Overrun(_)));
        let parsed = parsed.map_err(|error| RunnerError::Data {
            resource: name.clone(),
            error,
        })?;
        let Some(parsed) = parsed else {
            if p.target.mutation().is_some()
                || !self
                    .pending
                    .iter()
                    .any(|q| q.target == p.target && q.ticket != p.ticket)
            {
                return Err(RunnerError::Data {
                    resource: name,
                    error: DataError::Unavailable(
                        "an answer without new work needs another resource request in flight"
                            .into(),
                    ),
                });
            }
            self.continue_answer(&p);
            return self.update();
        };
        let value = match parsed {
            Answer::Now(value) => value,
            // A mapper that answered with a promise: its next turn (`mapper_turns.rs`).
            // What it sent before its first `await` goes with this commit.
            Answer::Later(request) if (message || turn) && request.continuation.is_some() => {
                if let (true, Err(why)) = (message, self.stage_replies(&p)) {
                    let error = DataError::Unavailable(why);
                    return Err(RunnerError::Data {
                        resource: name,
                        error,
                    });
                }
                self.mapper_turn(p.ticket, request);
                return self.update();
            }
            Answer::Later(request) if message && !request.stream => {
                // A message commits as its own settlement; a request that is
                // not a stream would replace the stream's ticket with one
                // answer (LLP 1016.000 D1).
                return Err(RunnerError::Data {
                    resource: name,
                    error: DataError::Unavailable(
                        "a stream's message answers now, or reopens the stream".into(),
                    ),
                });
            }
            Answer::Later(request) => {
                self.log(super::lines::one_more(&name));
                let requests = if p.target.mutation().is_some() {
                    vec![request]
                } else {
                    self.requests_for(p.target, request)
                        .map_err(|error| RunnerError::Data {
                            resource: name.clone(),
                            error,
                        })?
                };
                let ticket = self.next_ticket;
                if message || !self.pending.iter().any(|q| q.target == p.target) {
                    self.enqueue_requests(
                        p.target,
                        p.source.clone(),
                        p.args.clone(),
                        requests,
                        false,
                    );
                } else {
                    for request in requests {
                        self.append_request(
                            p.target,
                            p.source.clone(),
                            p.args.clone(),
                            request,
                            false,
                        );
                    }
                }
                self.continue_answer(&p);
                if p.upload {
                    for next in self
                        .pending
                        .iter_mut()
                        .filter(|n| n.ticket >= ticket && n.target == p.target)
                    {
                        self.uploads
                            .continue_at(p.ticket, next, p.sent || !failed_end);
                    }
                }
                return self.update();
            }
        };
        if !message && !turn {
            self.forget_ticket(p.target);
            self.sync_pending_flags();
        }
        if !self.conforms(&value, ty) {
            return Err(self.shape(name, &value, ty));
        }
        // Its replies go out once this frame's commit stands (D7).
        if message {
            if let Err(why) = self.stage_replies(&p) {
                let error = DataError::Unavailable(why);
                return Err(RunnerError::Data {
                    resource: name,
                    error,
                });
            }
        }
        if message || turn {
            let pending = self.pending.iter_mut().find(|q| q.ticket == p.ticket);
            if let Some(count) = pending.and_then(|p| p.stream.as_mut()) {
                count.messages += messages;
            }
            self.sync_pending_flags();
        }
        match p.target {
            Target::Resource(i) => {
                let shown = self.resources[i].state.take();
                self.revalidated(i, shown.as_ref(), &value);
                self.resources[i].stale = false;
                self.resources[i].failed_args = None;
                self.keep_answer(i, &p.args, &value);
                // An upload's end is the answer it stands on at its arguments;
                // one that never reached the network is an ordinary answer (D10).
                if p.upload && !message && !turn {
                    let end = super::UploadEnd::Answered(value.clone());
                    self.upload_over(p.ticket, p.sent || !failed_end, end);
                }
                if p.ask_again {
                    self.log(super::lines::asked_again(p.ticket, &name));
                    self.force_refresh(i);
                }
                self.resources[i].state = Some(ResourceState {
                    inspection_generation: p.inspection_generation,
                    args: p.args,
                    value: crate::held::Held::new(value),
                    store_revision: self.store.revision(),
                    placeholder: false,
                    kept_seed: false,
                    answered_for: None,
                    asleep: false,
                    slept: false,
                    origin: p.origin,
                });
                self.inspected_success(i, p.inspection_generation);
            }
            Target::Mutation(m) | Target::Sent(m, _) => {
                let slot = self.mutation_slot(m)?;
                self.land_write(p.target, p.write, &value);
                // A parallel send's reply concluding after a later send's
                // leaves the slot and the failure (LLP 1161.000 D4).
                if self.conclude_send(p.target) {
                    self.failed_mut[m] = None;
                    self.slots[slot] = Value::some(value);
                }
                self.landed.push(m);
                // The reply landed: what the mutation changed is asked again
                // in this commit (LLP 1054.000.000 D1); its write shows until
                // that answer lands.
                for r in self.declared_refreshes(m) {
                    self.force_refresh(r);
                }
            }
        }
        self.router_change()
            .and_then(|_| self.settle(false))
            .and_then(|_| self.gate_step())?;
        self.update()
    }
    /// Messages that waited together, parsed in arrival order (LLP
    /// 1016.000 D4): the last one's answer is the batch's, and the values
    /// before it are the mapper's to fold. One answered otherwise than now
    /// stops the batch there: a mapper's next turn holds the rest behind
    /// it, in order, and a stream reopened lets them go with its old ticket.
    fn parse_batch(
        &mut self,
        p: &PendingReq,
        batch: Vec<crate::Message>,
    ) -> Result<Option<Answer>, DataError> {
        let mut rest = batch.into_iter();
        let mut last = None;
        while let Some(m) = rest.next() {
            match self.parse_frame(p, true, Outcome::Message(m))? {
                Some(Answer::Now(value)) => last = Some(Answer::Now(value)),
                Some(Answer::Later(request)) if request.continuation.is_some() => {
                    self.hold_behind_turn(p.ticket, rest.collect())
                        .map_err(DataError::Unavailable)?;
                    return Ok(Some(Answer::Later(request)));
                }
                other => return Ok(other),
            }
        }
        Ok(last)
    }

    fn continue_answer(&mut self, previous: &PendingReq) {
        let effects = previous.admitted_effects();
        for next in self
            .pending
            .iter_mut()
            .filter(|n| n.target == previous.target)
        {
            next.ask_again |= previous.ask_again;
            next.module_again = next.module_again.or(previous.module_again);
            next.origin = previous.origin;
            next.inspection_generation = previous.inspection_generation;
            next.write = previous.write;
            next.reasks = previous.reasks;
            next.effects |= next.continuation.is_some() && effects;
            next.replay_unsafe |= previous.replay_unsafe;
        }
    }
}
