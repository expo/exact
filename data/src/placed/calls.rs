//! The calls a worker records and the replies it parses.
use super::*;

impl<D: DataSource + 'static> Placed<D> {
    pub(super) fn record(&mut self, k: Key, recorded: Recorded) -> Result<Answer, DataError> {
        let token = self.token()?;
        let fetch = match &recorded {
            Recorded::Resume { fetch, .. } => *fetch,
            _ => None,
        };
        let instance = match &recorded {
            Recorded::Answer { instance, .. } | Recorded::Resume { instance, .. } => {
                instance.clone()
            }
        };
        self.recorded.insert(token, recorded);
        self.stages
            .entry((k, fetch))
            .or_default()
            .push_back(Stage::Turn(token));
        let mut request = Request::continuation::<crate::host_work::WorkerTurn>(token);
        request.fetch = fetch;
        request.module = instance;
        Ok(Answer::Later(request))
    }

    pub(super) fn answer_with(
        &mut self,
        target: Option<Target>,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        if self.inline() {
            return crate::answer(self.here()?, target, store, source, args);
        }
        self.sync_generation();
        self.record(
            key(target, source, args),
            Recorded::Answer {
                instance: self.generation.clone(),
                target,
                source: source.to_string(),
                args: args.to_vec(),
            },
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
        if self.inline() {
            return crate::parse(self.here()?, target, store, source, args, outcome);
        }
        let call_key = key(target, source, args);
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
        let call_key = key(target, source, args);
        let k = (call_key.clone(), fetch);
        let stage = self.stages.get_mut(&k).and_then(|stages| {
            let pos = match continuation {
                Some(token) => stages.iter().position(|s| match s {
                    Stage::Turn(t) => *t == token,
                    Stage::Yielded { outer, .. } => *outer == Some(token),
                })?,
                None => 0,
            };
            stages.remove(pos)
        });
        if self.stages.get(&k).is_some_and(VecDeque::is_empty) {
            self.stages.remove(&k);
        }
        if let Some(Stage::Turn(token)) = &stage {
            self.turn_ended(*token);
        }
        match stage {
            Some(Stage::Turn(_)) => {
                let mut more = Vec::new();
                let answer = envelope::apply_optional(outcome, store, &mut self.logs, &mut more)?;
                let Some(Answer::Later(request)) = answer else {
                    return Ok(answer);
                };
                let request = self.yield_request(call_key.clone(), args, request)?;
                if let Some(target) = target {
                    let more = more
                        .into_iter()
                        .map(|r| self.yield_request(call_key.clone(), args, r))
                        .collect::<Result<Vec<_>, _>>()?;
                    if !more.is_empty() {
                        self.more.insert(target, more);
                    }
                }
                Ok(Some(Answer::Later(request)))
            }
            Some(Stage::Yielded { child, .. }) => {
                if let Some(fetch) = fetch {
                    self.sent.remove(&(call_key.clone(), fetch));
                }
                self.record(
                    call_key,
                    Recorded::Resume {
                        instance: self.generation.clone(),
                        fetch,
                        continuation: child,
                        target,
                        source: source.to_string(),
                        args: args.to_vec(),
                        outcome,
                    },
                )
                .map(Some)
            }
            None => Ok(None),
        }
    }

    pub(super) fn yield_request(
        &mut self,
        k: Key,
        args: &[Value],
        mut request: Request,
    ) -> Result<Request, DataError> {
        if let Some(fetch) = request.fetch.filter(|fetch| {
            request.has_effects() && self.inner.as_ref().is_some_and(|s| s.keeps_sent(*fetch))
        }) {
            self.sent.insert((k.clone(), fetch));
        }
        let child = request.continuation;
        if let Some(child) = child {
            if self
                .inner
                .as_ref()
                .is_some_and(|s| s.host_continuation(child))
            {
                let token = self.token()?;
                self.hosted.insert(token, child);
                request.continuation = Some(token);
            }
        }
        if let Some(due) = request.due_ms {
            let resume = Recorded::Resume {
                instance: self.generation.clone(),
                fetch: request.fetch,
                continuation: request.continuation,
                target: k.0,
                source: k.1.clone(),
                args: args.to_vec(),
                outcome: Dispatch::again_outcome(),
            };
            return self.record(k, resume).map(|answer| match answer {
                Answer::Later(request) => request.due(due),
                _ => unreachable!("a recorded turn"),
            });
        }
        self.stages
            .entry((k, request.fetch))
            .or_default()
            .push_back(Stage::Yielded {
                outer: request.continuation,
                child,
            });
        Ok(request)
    }
}
