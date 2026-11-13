//! Resume and park a TypeScript answer on its host work.
use super::*;

impl Module {
    pub(super) fn settle_reply(
        engine: &mut Watched,
        call: &str,
        final_settle: &str,
    ) -> Result<String, String> {
        let text = engine.call("__exact_settle", [call, final_settle, ""]);
        // Reply cleanup and serialization can queue other answers' reactions (LLP 1162 D3).
        let drained = engine.drain();
        text.and_then(|text| drained.map(|()| text))
    }

    /// Continue an answer: fulfil its fetch, drain, settle. No store at
    /// bake, where only a timer continues one (`source.rs`, `query`).
    pub(super) fn resume(
        &mut self,
        store: Option<&mut Store>,
        target: Option<Target>,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        self.resume_fetch(store, target, None, source, args, outcome)?
            .ok_or_else(|| {
                DataError::Unavailable(format!("`{source}` still has fetches in flight"))
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn resume_fetch(
        &mut self,
        store: Option<&mut Store>,
        target: Option<Target>,
        fetch: Option<u64>,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Option<Answer>, DataError> {
        self.resume_request(store, target, fetch, None, source, args, outcome)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn resume_request(
        &mut self,
        mut store: Option<&mut Store>,
        target: Option<Target>,
        fetch: Option<u64>,
        continuation: Option<u64>,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Option<Answer>, DataError> {
        let key = Module::key(target, source, args);
        if let Some(error) = self.retired_reply() {
            return Err(error);
        }
        if self.engine.is_none() {
            return Err(DataError::Unavailable(
                "exact-js: the engine is not loaded".into(),
            ));
        }
        // A stream's mapper call parked on its key is resumed first: the
        // runner delivers no message while one is out (`mapper_turns.rs`).
        let turn = self.parked.iter().any(|(k, _)| *k == key);
        if let (true, false, Some(store)) = (
            self.streams.iter().any(|(k, _)| *k == key),
            turn,
            &mut store,
        ) {
            return self.message(store, source, key, outcome).map(Some);
        }
        let exact = continuation
            .and_then(|call| {
                self.parked
                    .iter()
                    .position(|(k, p)| *k == key && p.call == call)
            })
            .or_else(|| {
                fetch.and_then(|fetch| {
                    self.parked
                        .iter()
                        .position(|(k, p)| *k == key && p.ticket == fetch)
                })
            });
        let pos = exact
            .or_else(|| {
                if continuation.is_some() {
                    return None;
                }
                let stream = self.streams.iter().any(|(k, _)| *k == key);
                self.parked.iter().position(|(k, p)| {
                    *k == key
                        && (fetch.is_none() || stream)
                        && matches!(p.ticket, 0 | WAITING | YIELDED | TIMER | secrets::TICKET)
                })
            })
            .or_else(|| {
                (continuation.is_none() && fetch.is_none())
                    .then(|| self.parked.iter().position(|(k, _)| *k == key))
                    .flatten()
            });
        let Some(pos) = pos else {
            return Err(DataError::Unavailable(format!(
                "`{source}`: a reply for an answer not in flight"
            )));
        };
        let Parked {
            call, ticket, last, ..
        } = self.parked.remove(pos).1;
        self.retired.remove(&call);
        if matches!(ticket, WAITING | YIELDED | TIMER) {
            if let Outcome::Failed { message, .. } = &outcome {
                // The answer ends here: its call is unlinked from the
                // prelude's bookkeeping, so its pending fetches are dropped
                // with it (LLP 1041 §8.4, Q5). A continuation the shared
                // promise still runs is not cancelled by this.
                let message = message.clone();
                self.forget_calls(vec![call]);
                return Err(DataError::Unavailable(message));
            }
        }
        let outcome_text = outcome_to_json(&outcome).to_string();
        let now = store
            .as_deref()
            .map_or(String::new(), |s| s.clock_ms().to_string());
        self.host.store = store.as_mut().map(|s| &mut **s as *mut Store);
        self.host.answer_ops.clear();
        if let Some(store) = store.as_deref() {
            self.host.socket_buffered = socket::snapshot(&self.streams, store);
        }
        let started = Instant::now();
        let budget_ms = self.turn_budget_ms(started);
        // (its own time on this thread's CPU clock, a wall backstop beside it: turn.rs)
        let clock = TurnClock::start(&self.turn_time);
        let result: Result<Step, DataError> = (|| {
            let engine = self.engine.as_mut().expect("checked above");
            module_work::begin_turn(engine, call, source)?;
            if ticket == WAITING || ticket == TIMER {
                // A timer of its own due by now runs: for a wait on others'
                // work, any; for one on a timer, the one it waited for.
                let due = if ticket == TIMER { "due" } else { "" };
                engine
                    .call("__exact_timer", [&call.to_string(), &now, due])
                    .map_err(|e| DataError::Unavailable(format!("`{source}` threw: {e}")))?;
            } else if ticket == YIELDED {
                engine
                    .call("__exact_yielded", [&call.to_string(), "", ""])
                    .map_err(|e| DataError::Unavailable(format!("`{source}` threw: {e}")))?;
            } else if ticket == secrets::TICKET {
                let error = self.host.secrets.ready(call, store.as_deref_mut());
                engine
                    .call("__exact_secret_ready", [&call.to_string(), &error, ""])
                    .map_err(DataError::Unavailable)?;
            } else if ticket == 0 {
                if matches!(outcome, Outcome::Failed { .. }) {
                    engine
                        .call(
                            "__exact_storage_failed",
                            [&call.to_string(), &outcome_text, ""],
                        )
                        .map_err(DataError::Unavailable)?;
                } else {
                    engine
                        .deliver_storage_one()
                        .map_err(DataError::Unavailable)?;
                }
            } else {
                self.host.secrets.landed(ticket);
                engine
                    .call("__exact_fulfill", [&ticket.to_string(), &outcome_text, ""])
                    .map_err(|e| DataError::Unavailable(format!("`{source}` threw: {e}")))?;
            }
            engine
                .drain()
                .map_err(|e| DataError::Unavailable(format!("`{source}` threw: {e}")))?;
            let settle = if last { "final" } else { "" };
            let text = Module::settle_reply(engine, &call.to_string(), settle)
                .map_err(|e| DataError::Unavailable(format!("`{source}` threw: {e}")))?;
            let Some(sig) = self.sigs.get(source) else {
                engine.clear_reply();
                return Err(DataError::UnknownSource(source.to_string()));
            };
            let reply = Module::decode_reply(sig, engine, source, &text)?;
            Ok(Module::step(sig, engine, source, reply, self.host.baking))
        })();
        self.host.store = None;
        if let Some(error) = self.retire_oom() {
            return Err(error);
        }
        // What this turn of an answer sent (D7, the amendment of 2026-10-18).
        let answer_ops = std::mem::take(&mut self.host.answer_ops);
        if let (Some(store), false) = (store, answer_ops.is_empty()) {
            self.stage_answer(store, source, answer_ops);
        }
        let deadline = self.snapshot_deadline.is_some();
        let over = clock.judge(
            budget_ms,
            deadline,
            self.main_thread,
            source,
            &mut self.logs,
        );
        if result.is_err() || over.is_some() || matches!(&result, Ok(Step::Done(_))) {
            self.host.secrets.retire(call);
        }
        if result.is_err() || over.is_some() {
            self.engine.as_mut().expect("checked above").clear_reply();
            self.forget_calls(vec![call]);
        }
        if result.is_err() && self.watch.take() {
            self.finish_let_go();
            return Err(DataError::Unavailable(format!(
                "`{source}` was interrupted"
            )));
        }
        if let Some(why) = over {
            self.overruns += 1;
            self.finish_let_go();
            return Err(over_budget(source, &why));
        }
        let answer = match result {
            Err(e) => Err(e),
            Ok(Step::Done(r)) => {
                self.parked.retain(|(_, p)| p.call != call);
                r.map(|v| Some(Answer::Now(v)))
            }
            Ok(Step::Pending {
                call,
                ticket: WAITING,
                ..
            }) if self.parked.iter().any(|(_, p)| p.call == call) => Ok(None),
            Ok(Step::Pending { call, ticket, due }) => {
                let request = if matches!(ticket, 0 | WAITING | YIELDED | TIMER | secrets::TICKET) {
                    timer_due(Request::continuation(call), ticket, due)
                } else {
                    let Some(mut request) = self.take_request(ticket) else {
                        self.finish_let_go();
                        return Err(DataError::Unavailable(format!(
                            "`{source}` awaits a fetch it never made"
                        )));
                    };
                    request.fetch = Some(ticket);
                    fetch_timer(request, due)
                };
                self.park(key.clone(), call, ticket, &request)?;
                self.park_more(&key, call)?;
                Ok(Some(Answer::Later(request)))
            }
        };
        // No store: a let-go call's file writes land and its Store writes do not (LLP 1097).
        self.finish_let_go();
        self.after_oom(answer)
    }

    /// Park a call on the request it waits for. A stream's call is not
    /// resumed by its reply: each message is mapped (`__exact_message`).
    pub(super) fn park(
        &mut self,
        key: Key,
        call: u64,
        ticket: u64,
        request: &Request,
    ) -> Result<(), DataError> {
        if request.has_effects() {
            if let Err(error) = self.host.secrets.begin_effect(call) {
                self.forget_calls(vec![call]);
                return Err(DataError::Unavailable(error));
            }
        }
        self.host
            .secrets
            .sent(call, request.has_effects().then_some(ticket));
        let parked = Parked {
            call,
            ticket,
            more: None,
            effects: request.has_effects(),
            work_taken: false,
            ready: false,
            due: request.wake_ms.or(request.due_ms),
            last: false,
        };
        if request.stream {
            self.streams.push((key, parked));
        } else {
            self.parked.push((key, parked));
        }
        Ok(())
    }

    pub(super) fn park_more(&mut self, key: &Key, call: u64) -> Result<(), DataError> {
        let text = self
            .engine
            .as_mut()
            .expect("loaded")
            .call("__exact_more", [&call.to_string(), "", ""])
            .map_err(DataError::Unavailable)?;
        let more: Vec<Json> =
            serde_json::from_str(&text).map_err(|e| DataError::Unavailable(e.to_string()))?;
        for item in more {
            let ticket = item["ticket"]
                .as_u64()
                .ok_or_else(|| DataError::Unavailable("a request with no fetch ticket".into()))?;
            let mut request = self
                .take_request(ticket)
                .ok_or_else(|| DataError::Unavailable("a fetch it never made".into()))?;
            request.fetch = Some(ticket);
            let request = fetch_timer(request, item["timer"].as_u64());
            self.park(key.clone(), call, ticket, &request)?;
            self.parked.last_mut().expect("parked fetch").1.more = Some(request);
        }
        Ok(())
    }

    /// One message of the stream answer `key` began, or its end: the
    /// source's `exactStream` maps it to the answer (LLP 1016.000 D1). A
    /// mapper that returns a promise is settled as an answer is, and parks
    /// on the stream's key until it gives its value (Charlie, 2026-11-10).
    fn message(
        &mut self,
        store: &mut Store,
        source: &str,
        key: Key,
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let Some(at) = self.streams.iter().position(|(k, _)| *k == key) else {
            return Err(DataError::Unavailable(format!(
                "`{source}`: a message for a stream not open"
            )));
        };
        let call = self.streams[at].1.call;
        let ended = !matches!(outcome, Outcome::Message(_));
        let (outcome_text, binary) = wire::message_to_json(outcome);
        self.host.binary = binary;
        self.host.socket_buffered = socket::snapshot(&self.streams, store);
        self.host.socket_ops.clear();
        self.host.answer_ops.clear();
        self.host.store = Some(store as *mut Store);
        let started = Instant::now();
        let budget_ms = self.turn_budget_ms(started);
        // (its own time on this thread's CPU clock, a wall backstop beside it: turn.rs)
        let clock = TurnClock::start(&self.turn_time);
        let mut mapper = None;
        let result: Result<Step, DataError> = (|| {
            let engine = self.engine.as_mut().expect("checked by resume");
            let threw = |e| DataError::Unavailable(format!("`{source}` threw: {e}"));
            let text = engine
                .call("__exact_message", [&call.to_string(), &outcome_text, ""])
                .map_err(threw)?;
            let Some(sig) = self.sigs.get(source) else {
                engine.clear_reply();
                return Err(DataError::UnknownSource(source.to_string()));
            };
            let mut reply = Module::decode_reply(sig, engine, source, &text)?;
            if reply.fields.get("tag").and_then(Json::as_u64) == Some(3) {
                mapper = reply.fields.get("call").and_then(Json::as_u64);
                let id = mapper.unwrap_or_default().to_string();
                engine.drain().map_err(threw)?;
                let text = Module::settle_reply(engine, &id, "").map_err(threw)?;
                reply = Module::decode_reply(sig, engine, source, &text)?;
            }
            Ok(Module::step(sig, engine, source, reply, self.host.baking))
        })();
        self.host.store = None;
        if let Some(error) = self.retire_oom() {
            return Err(error);
        }
        self.host.binary = Vec::new();
        // The replies the mapper staged answering this frame (LLP 1016.000
        // D7), staged on the store however the call ended: the runner sends
        // them with this frame's commit, or journals them refused.
        // A socket no resource owns (a mutation's stream) takes none.
        let ops = std::mem::take(&mut self.host.socket_ops);
        let staged = self.stage_sockets(store, ops);
        if let Err(why) = &staged {
            (self.logs).push(format!(
                "mapper `{source}` failed: {why}; socket frames were refused"
            ));
        }
        let result = match staged {
            Err(why) if matches!(result, Ok(Step::Done(Ok(_)))) => Ok(Step::Done(Err(
                DataError::Failed(FailureCode::Error, why, String::new()),
            ))),
            _ => result,
        };
        if ended {
            self.streams.remove(at);
            self.forget_calls(vec![call]);
        }
        let deadline = self.snapshot_deadline.is_some();
        let over = clock.judge(
            budget_ms,
            deadline,
            self.main_thread,
            source,
            &mut self.logs,
        );
        if result.is_err() || over.is_some() {
            self.engine
                .as_mut()
                .expect("checked by resume")
                .clear_reply();
            self.forget_calls(mapper.into_iter().collect());
        }
        if let Some(why) = over {
            self.overruns += 1;
            return Err(DataError::Overrun(format!("`{source}` {why}")));
        }
        match result? {
            Step::Done(r) => r.map(Answer::Now),
            // Its later turns: a yield's, or a wait on another answer's work.
            Step::Pending { call, ticket, due }
                if matches!(ticket, WAITING | YIELDED | secrets::TICKET) =>
            {
                let request = timer_due(Request::continuation(call), ticket, due);
                self.park(key, call, ticket, &request)?;
                Ok(Answer::Later(request))
            }
            Step::Pending { call, .. } => {
                self.forget_calls(vec![call]);
                Err(DataError::Unavailable(format!(
                    "`{source}`: a stream's mapper awaits only promises and scheduler.yield()"
                )))
            }
        }
    }
}
