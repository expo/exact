//! Child turns and their request sets, scoped to their executor.
use super::*;

impl<J: DataSource, R: DataSource> Mixed<J, R> {
    pub(super) fn sync_generations(&mut self) {
        for rust in [false, true] {
            let at = rust as usize;
            if self.generations[at]
                .as_ref()
                .and_then(|m| m.failure())
                .is_none()
            {
                continue;
            }
            self.continuations.retain(|_, (owner, _, _)| *owner != rust);
            self.recorded
                .retain(|_, (_, call)| recorded_key(call).0 != rust);
            self.stages.retain(|(key, _), _| key.0 != rust);
            for set in &mut self.sets {
                if set.running.as_ref().is_some_and(|r| r.key.0 == rust) {
                    set.running = None;
                    set.busy = false;
                }
                set.held.retain(|t| self.recorded.contains_key(t));
            }
            if !rust {
                self.deliveries.clear();
                self.delivery_waiters.clear();
                self.delivery_phases.clear();
                self.delivery_routes.clear();
            }
            let child: &dyn DataSource = if rust { &self.rust } else { &self.javascript };
            self.generations[at] = child.module_identity("").map(|m| m.instance());
        }
    }

    pub(super) fn child_answer(
        &mut self,
        rust: bool,
        target: Option<Target>,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        if rust {
            crate::answer(&mut self.rust, target, store, source, args)
        } else {
            crate::answer(&mut self.javascript, target, store, source, args)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn child_parse_fetch(
        &mut self,
        rust: bool,
        target: Option<Target>,
        request: (Option<u64>, Option<u64>),
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Option<Answer>, DataError> {
        let (fetch, continuation) = request;
        let child: &mut dyn DataSource = if rust {
            &mut self.rust
        } else {
            &mut self.javascript
        };
        match target {
            Some(target) => child.parse_fetch_for(target, fetch, store, source, args, outcome),
            None => child
                .parse_tool(fetch, continuation, store, source, args, outcome)
                .map(Some),
        }
    }

    pub(super) fn answer_with(
        &mut self,
        target: Option<Target>,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        self.sync_generations();
        let rust = self.owner(source)?;
        if let Some(set) = self.set_of[rust as usize] {
            return self.record(
                set,
                key(rust, target, source, args),
                Recorded::Answer {
                    rust,
                    target,
                    source: source.to_string(),
                    args: args.to_vec(),
                },
            );
        }
        let grants = self.child_grants(rust);
        let answer = store.with_grants(&grants, |store| {
            self.child_answer(rust, target, store, source, args)
        })?;
        self.route_set(
            rust,
            target,
            answer,
            target.is_none() && source.starts_with("tool:"),
        )
    }

    pub(super) fn parse_with(
        &mut self,
        target: Option<Target>,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let call_key = key(self.owner(source)?, target, source, args);
        // Without a fetch, only a single request can identify the reply (LLP 1162 D2).
        let mut requests = self.stages.keys().filter(|(k, _)| *k == call_key);
        let fetch = requests.next().and_then(|(_, fetch)| *fetch);
        if requests.next().is_some() {
            return Err(unavailable(format!(
                "`{source}`: more than one request is in flight; the reply must name its fetch"
            )));
        }
        self.parse_with_fetch(target, fetch, store, source, args, outcome)?
            .ok_or_else(|| unavailable("the answer still has requests in flight"))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn parse_with_fetch(
        &mut self,
        target: Option<Target>,
        fetch: Option<u64>,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Option<Answer>, DataError> {
        self.parse_with_request(target, (fetch, None), store, source, args, outcome)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn parse_with_request(
        &mut self,
        target: Option<Target>,
        request: (Option<u64>, Option<u64>),
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Option<Answer>, DataError> {
        let (fetch, continuation) = request;
        let rust = self.owner(source)?;
        let grants = self.child_grants(rust);
        if let Some(set) = self.set_of[rust as usize] {
            let call_key = key(rust, target, source, args);
            let k = (call_key.clone(), fetch);
            let stage = self.stages.get_mut(&k).and_then(|stages| {
                let pos = match continuation {
                    Some(token) => stages.iter().position(|s| match s {
                        Stage::Turn(t) => *t == token,
                        Stage::Yielded(t) => *t == Some(token),
                    })?,
                    None => 0,
                };
                stages.remove(pos)
            });
            if self.stages.get(&k).is_some_and(VecDeque::is_empty) {
                self.stages.remove(&k);
            }
            return match stage {
                Some(Stage::Turn(_)) => {
                    // The reserved turn ended, however it ended.
                    self.sets[set].busy = false;
                    let child = self.sets[set].running.take().and_then(|r| r.child);
                    let answer = if self.child_placement(rust) == Placement::Worker {
                        store.with_grants(&grants, |store| {
                            self.child_parse_fetch(
                                rust,
                                target,
                                (fetch, child),
                                store,
                                source,
                                args,
                                outcome,
                            )
                        })
                    } else {
                        let mut logs = Vec::new();
                        let answer = store.with_grants(&grants, |store| {
                            envelope::apply_optional(outcome, store, &mut logs, &mut Vec::new())
                        });
                        self.logs.extend(logs);
                        answer
                    };
                    match answer {
                        Ok(Some(Answer::Later(request))) => {
                            let worker_turn = request
                                .due_ms
                                .and(request.continuation)
                                .filter(|_| self.child_placement(rust) == Placement::Worker);
                            let answer = self.route_set(
                                rust,
                                target,
                                Answer::Later(request),
                                target.is_none() && source.starts_with("tool:"),
                            )?;
                            let stage = if let (Some(child), Answer::Later(request)) =
                                (worker_turn, &answer)
                            {
                                // A timer token names the worker's next turn, whose reply
                                // is an envelope. It reserves the set again at dispatch.
                                let token = request.continuation.expect("a routed worker turn");
                                self.continuations.remove(&token);
                                self.recorded.insert(
                                    token,
                                    (
                                        set,
                                        Recorded::WorkerTurn {
                                            key: call_key.clone(),
                                            child,
                                        },
                                    ),
                                );
                                Stage::Turn(token)
                            } else {
                                Stage::Yielded(match &answer {
                                    Answer::Later(r) => r.continuation,
                                    _ => None,
                                })
                            };
                            let next_fetch = match &answer {
                                Answer::Later(r) => r.fetch,
                                _ => None,
                            };
                            self.stages
                                .entry((call_key.clone(), next_fetch))
                                .or_default()
                                .push_back(stage);
                            if let Some(target) = target {
                                for fetch in self
                                    .more
                                    .get(&target)
                                    .into_iter()
                                    .flatten()
                                    .map(|r| r.fetch)
                                {
                                    self.stages
                                        .entry((call_key.clone(), fetch))
                                        .or_default()
                                        .push_back(Stage::Yielded(None));
                                }
                            }
                            Ok(Some(answer))
                        }
                        other => other,
                    }
                }
                Some(Stage::Yielded(token)) => {
                    let continuation = token.and_then(|token| {
                        self.continuations.remove(&token).map(|(_, child, _)| child)
                    });
                    self.record(
                        set,
                        call_key,
                        Recorded::Resume {
                            fetch,
                            continuation,
                            rust,
                            target,
                            source: source.to_string(),
                            args: args.to_vec(),
                            outcome,
                        },
                    )
                    .map(Some)
                }
                None if self.child_placement(rust) == Placement::Worker => Ok(None),
                None => Err(unavailable(format!(
                    "`{source}`: a reply for an answer not in flight"
                ))),
            };
        }
        let answer = store.with_grants(&grants, |store| {
            self.child_parse_fetch(rust, target, (fetch, None), store, source, args, outcome)
        })?;
        answer
            .map(|a| {
                self.route_set(
                    rust,
                    target,
                    a,
                    target.is_none() && source.starts_with("tool:"),
                )
            })
            .transpose()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Source {
        identity: exact_runner::ModuleIdentity,
        next: u64,
        discarded: Vec<u64>,
    }

    impl DataSource for Source {
        fn module_identity(&self, _: &str) -> Option<exact_runner::ModuleIdentity> {
            Some(self.identity.clone())
        }
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::str("done"))
        }

        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            self.next += 1;
            Ok(Answer::Later(Request::continuation(self.next)))
        }

        fn parse(
            &mut self,
            _: &mut Store,
            source: &str,
            _: &[Value],
            outcome: Outcome,
        ) -> Result<Answer, DataError> {
            if source == "feed" {
                if matches!(outcome, Outcome::Message(_)) {
                    self.next += 1;
                    return Ok(Answer::Later(Request::continuation(self.next)));
                }
                if matches!(outcome, Outcome::Storage(_)) {
                    return Ok(Answer::Later(Request::continuation(self.next)));
                }
            }
            Ok(Answer::Now(Value::str("done")))
        }

        fn continuation(&mut self, _: u64) -> Option<Box<dyn FnOnce() -> Outcome + Send>> {
            Some(Box::new(Dispatch::again_outcome))
        }

        fn parse_fetch_for(
            &mut self,
            _: Target,
            fetch: Option<u64>,
            store: &mut Store,
            source: &str,
            args: &[Value],
            outcome: Outcome,
        ) -> Result<Option<Answer>, DataError> {
            if fetch == Some(123) {
                return Ok(None);
            }
            self.parse(store, source, args, outcome).map(Some)
        }

        fn discard(&mut self, token: u64) {
            self.discarded.push(token);
        }
    }

    #[test]
    fn retirement_clears_tool_routes_without_a_reply() {
        let mut mixed = Mixed::new(Source::default(), Source::default(), &["tool:h"], &[]).unwrap();
        let mut store = Store::default();
        for _ in 0..32 {
            let Answer::Later(request) = mixed.answer(&mut store, "tool:h", &[]).unwrap() else {
                panic!("tool")
            };
            assert!(matches!(
                mixed.dispatch(request.continuation.unwrap(), &store),
                Dispatch::Run(_)
            ));
        }
        mixed.javascript.identity.retire("heap exhausted".into());
        mixed.take_aborted();
        assert!(mixed.continuations.is_empty());
        assert!(mixed.recorded.is_empty());
        assert!(mixed.stages.is_empty());
        assert!(mixed.deliveries.is_empty());
        assert!(mixed.delivery_phases.is_empty());
        assert!(mixed.delivery_routes.is_empty());
        assert!(mixed.delivery_waiters.is_empty());
    }

    #[test]
    fn untargeted_calls_live_until_completion_or_cancellation() {
        let mut mixed = Mixed::new(Source::default(), Source::default(), &["tool:h"], &[]).unwrap();
        let mut store = Store::default();
        for cancel in [false, true] {
            let Answer::Later(request) = mixed.answer(&mut store, "tool:h", &[]).unwrap() else {
                panic!("handler continuation")
            };
            let token = request.continuation.unwrap();
            assert!(matches!(mixed.dispatch(token, &store), Dispatch::Run(_)));
            mixed.forgotten(&store, &[]);
            assert_eq!(
                mixed.continuations.len(),
                1,
                "untargeted work is still live"
            );
            if cancel {
                mixed.discard(token);
                assert_eq!(mixed.javascript.discarded, [2]);
            } else {
                mixed
                    .parse_tool(
                        None,
                        Some(token),
                        &mut store,
                        "tool:h",
                        &[],
                        Dispatch::again_outcome(),
                    )
                    .unwrap();
            }
            assert!(
                mixed.continuations.is_empty(),
                "finished work leaves no route"
            );
        }
    }

    #[test]
    fn overlapping_untargeted_calls_keep_their_own_routes() {
        for cancel in [false, true] {
            let mut mixed =
                Mixed::new(Source::default(), Source::default(), &["tool:h"], &[]).unwrap();
            let mut store = Store::default();
            let mut tokens = Vec::new();
            for _ in 0..2 {
                let Answer::Later(request) = mixed.answer(&mut store, "tool:h", &[]).unwrap()
                else {
                    panic!("handler continuation")
                };
                tokens.push(request.continuation.unwrap());
            }
            mixed.forgotten(&store, &[]);
            for token in &tokens {
                assert!(
                    matches!(mixed.dispatch(*token, &store), Dispatch::Run(_)),
                    "a later call must not consume an earlier continuation"
                );
            }
            mixed
                .parse_tool(
                    None,
                    Some(tokens[0]),
                    &mut store,
                    "tool:h",
                    &[],
                    Dispatch::again_outcome(),
                )
                .unwrap();
            assert_eq!(
                mixed.continuations.len(),
                1,
                "the second call is still outstanding"
            );
            if cancel {
                mixed.discard(tokens[1]);
                assert_eq!(mixed.javascript.discarded, [2]);
            } else {
                mixed
                    .parse_tool(
                        None,
                        Some(tokens[1]),
                        &mut store,
                        "tool:h",
                        &[],
                        Dispatch::again_outcome(),
                    )
                    .unwrap();
            }
            assert!(mixed.continuations.is_empty());
        }
    }

    #[test]
    fn yielding_mapper_routes_end_with_each_turn() {
        let mut mixed = Mixed::new(Source::default(), Source::default(), &["feed"], &[]).unwrap();
        let mut store = Store::default();
        let target = Target::Resource(0);
        for _ in 0..32 {
            let mut answer = mixed
                .parse_for(
                    target,
                    &mut store,
                    "feed",
                    &[],
                    Outcome::Message(Default::default()),
                )
                .unwrap();
            for outcome in [Outcome::Storage(Vec::new()), Dispatch::again_outcome()] {
                let Answer::Later(request) = answer else {
                    panic!("yielding mapper")
                };
                assert_eq!(
                    mixed.continuations.len(),
                    1,
                    "only the outstanding turn needs a route"
                );
                assert!(matches!(
                    mixed.dispatch(request.continuation.unwrap(), &store),
                    Dispatch::Run(_)
                ));
                assert_eq!(
                    mixed.continuations.len(),
                    0,
                    "a mapper dispatch consumes its route"
                );
                answer = mixed
                    .parse_for(target, &mut store, "feed", &[], outcome)
                    .unwrap();
            }
            assert_eq!(answer, Answer::Now(Value::str("done")));
            assert!(
                mixed.continuations.is_empty(),
                "a completed mapper leaves no route"
            );
        }
    }

    #[test]
    fn another_fetch_keeps_an_outstanding_continuations_route() {
        for dispatched in [false, true] {
            let mut mixed =
                Mixed::new(Source::default(), Source::default(), &["feed"], &[]).unwrap();
            let mut store = Store::default();
            let target = Target::Resource(0);
            let Answer::Later(request) = mixed.answer_for(target, &mut store, "feed", &[]).unwrap()
            else {
                panic!("continuation")
            };
            let token = request.continuation.unwrap();
            if dispatched {
                assert!(matches!(mixed.dispatch(token, &store), Dispatch::Run(_)));
            }
            assert_eq!(
                mixed
                    .parse_fetch_for(
                        target,
                        Some(123),
                        &mut store,
                        "feed",
                        &[],
                        Dispatch::again_outcome(),
                    )
                    .unwrap(),
                None
            );
            assert_eq!(mixed.continuations.contains_key(&token), !dispatched);
            if !dispatched {
                assert!(matches!(mixed.dispatch(token, &store), Dispatch::Run(_)));
            }
            mixed
                .parse_for(target, &mut store, "feed", &[], Dispatch::again_outcome())
                .unwrap();
            assert!(mixed.continuations.is_empty());
        }
    }
    #[test]
    fn repeated_successful_tool_fetches_leave_no_bookkeeping() {
        struct Fetches(u64);
        impl DataSource for Fetches {
            fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
                unreachable!()
            }
            fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
                self.0 += 1;
                let mut r = Request::get("https://test.invalid/");
                r.fetch = Some(self.0);
                Ok(Answer::Later(r))
            }
            fn parse_tool(
                &mut self,
                fetch: Option<u64>,
                _: Option<u64>,
                _: &mut Store,
                _: &str,
                _: &[Value],
                _: Outcome,
            ) -> Result<Answer, DataError> {
                assert_eq!(fetch, Some(self.0));
                Ok(Answer::Now(Value::Unit))
            }
        }
        let mut mixed = Mixed::new(Fetches(0), Source::default(), &["tool:fetch"], &[]).unwrap();
        let mut store = Store::default();
        for _ in 0..32 {
            let Answer::Later(r) = mixed.answer(&mut store, "tool:fetch", &[]).unwrap() else {
                panic!("fetch")
            };
            mixed
                .parse_tool(
                    r.fetch,
                    r.continuation,
                    &mut store,
                    "tool:fetch",
                    &[],
                    Dispatch::again_outcome(),
                )
                .unwrap();
            assert!(mixed.continuations.is_empty());
            assert!(mixed.recorded.is_empty());
            assert!(mixed.stages.is_empty());
            assert!(mixed.more.is_empty());
        }
    }
}
