//! Child turns and their request sets, scoped to their executor.
use super::*;

impl<J: DataSource, R: DataSource> Mixed<J, R> {
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
        fetch: Option<u64>,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Option<Answer>, DataError> {
        let child: &mut dyn DataSource = if rust {
            &mut self.rust
        } else {
            &mut self.javascript
        };
        match target {
            Some(target) => child.parse_fetch_for(target, fetch, store, source, args, outcome),
            None => child.parse(store, source, args, outcome).map(Some),
        }
    }

    pub(super) fn answer_with(
        &mut self,
        target: Option<Target>,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
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
        self.route_set(rust, target, answer)
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
        let fetch = self
            .stages
            .keys()
            .find(|(k, _)| *k == call_key)
            .and_then(|(_, fetch)| *fetch);
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
        let rust = self.owner(source)?;
        let grants = self.child_grants(rust);
        if let Some(set) = self.set_of[rust as usize] {
            let call_key = key(rust, target, source, args);
            let k = (call_key.clone(), fetch);
            let stage = self.stages.get_mut(&k).and_then(VecDeque::pop_front);
            if self.stages.get(&k).is_some_and(VecDeque::is_empty) {
                self.stages.remove(&k);
            }
            return match stage {
                Some(Stage::Turn) => {
                    // The reserved turn ended, however it ended.
                    self.sets[set].busy = false;
                    self.sets[set].running = None;
                    let answer = if self.child_placement(rust) == Placement::Worker {
                        store.with_grants(&grants, |store| {
                            self.child_parse_fetch(
                                rust, target, fetch, store, source, args, outcome,
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
                            let answer = self.route_set(rust, target, Answer::Later(request))?;
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
                                Stage::Turn
                            } else {
                                Stage::Yielded
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
                                        .push_back(Stage::Yielded);
                                }
                            }
                            Ok(Some(answer))
                        }
                        other => other,
                    }
                }
                Some(Stage::Yielded) => self
                    .record(
                        set,
                        call_key,
                        Recorded::Resume {
                            fetch,
                            rust,
                            target,
                            source: source.to_string(),
                            args: args.to_vec(),
                            outcome,
                        },
                    )
                    .map(Some),
                None if self.child_placement(rust) == Placement::Worker => Ok(None),
                None => Err(unavailable(format!(
                    "`{source}`: a reply for an answer not in flight"
                ))),
            };
        }
        let answer = store.with_grants(&grants, |store| {
            self.child_parse_fetch(rust, target, fetch, store, source, args, outcome)
        })?;
        answer.map(|a| self.route_set(rust, target, a)).transpose()
    }
}
