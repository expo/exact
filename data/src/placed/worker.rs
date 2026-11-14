//! Turns run and retire on the module's owner.
use super::*;

impl<D: DataSource + 'static> Placed<D> {
    pub(super) fn sync_generation(&mut self) {
        if self.generation.as_ref().and_then(|m| m.failure()).is_none() {
            return;
        }
        self.recorded.clear();
        self.hosted.clear();
        self.sent.clear();
        self.kept.clear();
        self.deliveries.clear();
        self.delivery_waiters.clear();
        self.stages.clear();
        self.more.clear();
        if let Some(owner) = &self.owner {
            owner.lock().unwrap().unqueued.clear();
        }
        self.generation = self.identity.as_ref().map(|m| m.instance());
    }

    /// Start the owner (LLP 1027.002 D2): it obtains its instance, then runs
    /// turns in order until this proxy retires it.
    pub(super) fn spawn_owner(&mut self, obtain: Obtain<D>) -> Result<(), DataError> {
        let (jobs, rx) = channel::<Job>();
        let grants = self.grants.clone();
        let storage_changed = self.storage_changed.clone();
        let aborted = self.aborted.clone();
        let abort_wake = self.abort_wake.clone();
        let identity = self.identity.clone();
        let retired = self.retired.clone();
        std::thread::Builder::new()
            .name("exact-owner".into())
            .spawn(move || {
                let mut source = obtain();
                for job in rx {
                    if retired.load(Ordering::Acquire) {
                        break;
                    }
                    let instance = identity.as_ref().map(|m| m.instance());
                    let _wake = RetirementWake {
                        instance,
                        wake: abort_wake.clone(),
                    };
                    let (instance, target, name, args, outcome, snapshot, clock, reply) = match job
                    {
                        Job::Keep {
                            calls,
                            snapshot,
                            refused,
                        } => {
                            if let Ok(source) = &mut source {
                                let decoded: Vec<_> = calls
                                    .iter()
                                    .filter_map(|((target, name, args), fetch)| {
                                        Some((
                                            *target,
                                            name.as_str(),
                                            decode_args(args).ok()?,
                                            *fetch,
                                        ))
                                    })
                                    .collect();
                                let view: Vec<_> = decoded
                                    .iter()
                                    .map(|(target, name, args, fetch)| InFlight {
                                        target: *target,
                                        source: name,
                                        args,
                                        fetch: *fetch,
                                        continuation: None,
                                    })
                                    .collect();
                                source.keep_sent(&Store::new(&grants, snapshot), &view, refused);
                            }
                            continue;
                        }
                        Job::KeptReply {
                            token,
                            outcome,
                            snapshot,
                            clock,
                            reply,
                        } => {
                            let mut local = Store::new(&grants, snapshot);
                            local.set_clock_ms(clock);
                            let result = match &mut source {
                                Ok(source) => source
                                    .module_landed(&mut local, token, outcome)
                                    .map(|()| Answer::Now(Value::Unit)),
                                Err(error) => Err(error.clone()),
                            };
                            if source.as_mut().is_ok_and(|s| s.take_storage_changed()) {
                                storage_changed.store(true, std::sync::atomic::Ordering::Release);
                            }
                            let logs = source.as_mut().map(|s| s.take_logs()).unwrap_or_default();
                            if let Ok(source) = source.as_mut() {
                                publish_aborts(source.take_aborted(), &aborted, &abort_wake);
                            }
                            reply.send(envelope::encode(result, &mut local, logs));
                            continue;
                        }
                        Job::Forgotten(slot) => {
                            let retirement = slot.lock().unwrap().take();
                            if let (Ok(source), Some((in_flight, snapshot))) =
                                (&mut source, retirement)
                            {
                                forget(source, &Store::new(&grants, snapshot), &in_flight);
                                // A let-go call's continuations run here, and
                                // may abort another call's fetch.
                                publish_aborts(source.take_aborted(), &aborted, &abort_wake);
                            }
                            continue;
                        }
                        Job::Answer {
                            instance,
                            target,
                            source,
                            args,
                            snapshot,
                            clock,
                            reply,
                        } => (instance, target, source, args, None, snapshot, clock, reply),
                        Job::Resume {
                            instance,
                            fetch,
                            continuation,
                            target,
                            source,
                            args,
                            outcome,
                            snapshot,
                            clock,
                            reply,
                        } => (
                            instance,
                            target,
                            source,
                            args,
                            Some((fetch, continuation, outcome)),
                            snapshot,
                            clock,
                            reply,
                        ),
                    };
                    let mut local = Store::new(&grants, snapshot);
                    local.set_clock_ms(clock);
                    let result = match &mut source {
                        Err(e) => Err(e.clone()),
                        Ok(source) => match instance.as_ref().and_then(|i| i.failure()) {
                            Some(message) => Err(unavailable(message)),
                            None => decode_args(&args).and_then(|args| {
                                turn(source, &mut local, target, &name, &args, outcome, &retired)
                            }),
                        },
                    };
                    if source.as_mut().is_ok_and(|s| s.take_storage_changed()) {
                        storage_changed.store(true, std::sync::atomic::Ordering::Release);
                    }
                    let logs = source.as_mut().map(|s| s.take_logs()).unwrap_or_default();
                    if let Ok(source) = source.as_mut() {
                        publish_aborts(source.take_aborted(), &aborted, &abort_wake);
                    }
                    let more = if matches!(result, Ok(Some(Answer::Later(_)))) {
                        target
                            .and_then(|target| source.as_mut().ok().map(|s| s.more_for(target)))
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    reply.send(envelope::encode_optional(result, more, &mut local, logs));
                }
                // Retired on its owner, after its last turn.
                drop(source);
            })
            .map_err(|e| unavailable(format!("the owner thread could not start: {e}")))?;
        self.owner = Some(Arc::new(Mutex::new(OwnerQueue {
            sender: jobs,
            pending: None,
            unqueued: HashMap::new(),
        })));
        Ok(())
    }
}

/// Tell the instance on its owner what is still in flight, without tokens:
/// the proxy's mean nothing there. An argument that doesn't cross leaves its
/// request out, which only lets its call go too.
pub(super) fn forget<D: DataSource>(source: &mut D, store: &Store, in_flight: &[Flight]) {
    let decoded: Vec<_> = in_flight
        .iter()
        .filter_map(|((target, name, args), fetch)| {
            Some((*target, name.as_str(), decode_args(args).ok()?, *fetch))
        })
        .collect();
    let view: Vec<InFlight<'_>> = decoded
        .iter()
        .map(|(target, source, args, fetch)| InFlight {
            target: *target,
            source,
            args,
            fetch: *fetch,
            continuation: None,
        })
        .collect();
    source.forgotten(store, &view);
}

#[allow(clippy::too_many_arguments)]
fn parse_fetch<D: DataSource>(
    source: &mut D,
    target: Option<Target>,
    request: (Option<u64>, Option<u64>),
    store: &mut Store,
    name: &str,
    args: &[Value],
    outcome: Outcome,
) -> Result<Option<Answer>, DataError> {
    let (fetch, continuation) = request;
    match target {
        Some(target) => source.parse_fetch_for(target, fetch, store, name, args, outcome),
        None => source
            .parse_tool(fetch, continuation, store, name, args, outcome)
            .map(Some),
    }
}

/// One turn (LLP 1027.002 D3, change 3): begin or resume; stay on the owner
/// through every wait the source runs itself (its storage, its own worker
/// closures); end at an answer, an error, or a request only the host can run
/// (`fetch`, the portable storage protocol) — a yield.
#[allow(clippy::too_many_arguments)]
fn turn<D: DataSource>(
    source: &mut D,
    local: &mut Store,
    target: Option<Target>,
    name: &str,
    args: &[Value],
    outcome: Option<(Option<u64>, Option<u64>, Outcome)>,
    retired: &AtomicBool,
) -> Result<Option<Answer>, DataError> {
    let mut result = match outcome {
        None => crate::answer(source, target, local, name, args).map(Some),
        Some((fetch, continuation, outcome)) => parse_fetch(
            source,
            target,
            (fetch, continuation),
            local,
            name,
            args,
            outcome,
        ),
    };
    loop {
        match result {
            // A long poll on the worker's own work (LLP 1016.000 D8): refused before that work runs.
            Ok(Some(Answer::Later(request))) if request.long_poll_refusal().is_some() => {
                if let Some(token) = request.continuation {
                    source.discard(token);
                }
                return Err(unavailable(request.long_poll_refusal().unwrap_or_default()));
            }
            // A Rust source's two-way socket (`Request::socket`, LLP 1016.003): its turns
            // are the main thread's, so it is refused rather than opened to only listen.
            // A TypeScript module's socket, whose request names its fetch, listens as
            // LLP 1016.000 D7 declares for a worker.
            Ok(Some(Answer::Later(request)))
                if request.socket
                    && request.fetch.is_none()
                    && request.listen.is_none()
                    && !request.is_process() =>
            {
                return Err(unavailable(
                    "Request::socket is unavailable in a worker-placed source; place it on main",
                ));
            }
            // A wait for the answer's own timer is the runner's to keep
            // until due (LLP 1027.000 D1): it yields, as a fetch does.
            Ok(Some(Answer::Later(request)))
                if request.continuation.is_some()
                    && request.due_ms.is_none()
                    && !source.host_continuation(request.continuation.expect("checked")) =>
            {
                let token = request.continuation.expect("checked");
                let Some(work) = source.continuation(token) else {
                    return Err(unavailable(format!(
                        "`{name}`: a continuation with no work on its owner"
                    )));
                };
                let outcome = work();
                if retired.load(Ordering::Acquire) {
                    return Err(unavailable("the module's worker was retired"));
                }
                result = parse_fetch(
                    source,
                    target,
                    (request.fetch, request.continuation),
                    local,
                    name,
                    args,
                    outcome,
                );
            }
            other => return other,
        }
    }
}
