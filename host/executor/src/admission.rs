//! Admission and the bounded ordered fence (LLP 1041 §8.4, LLP 1162 D1).
use super::*;

/// What a core's workers count against: `MAX_WORKERS` live at once,
/// retired ones included until they exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerBudget {
    /// The process's one bound, which every core of every host in the
    /// process shares.
    Process,
    /// A bound of the thread that starts the core, for tests that run side
    /// by side and must not refuse each other's cores.
    Thread,
}

impl WorkerBudget {
    /// The count of live workers this budget bounds.
    pub(super) fn live(self) -> Arc<AtomicUsize> {
        match self {
            // Cores created by one test share a bound. Slots retain that budget
            // when their workers outlive the test thread or retire elsewhere.
            WorkerBudget::Thread => {
                thread_local! {
                    static BUDGET: Arc<AtomicUsize> = Arc::default();
                }
                BUDGET.with(Arc::clone)
            }
            WorkerBudget::Process => {
                static BUDGET: std::sync::LazyLock<Arc<AtomicUsize>> =
                    std::sync::LazyLock::new(Arc::default);
                Arc::clone(&BUDGET)
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamKind {
    Network,
    Child,
}

impl StreamKind {
    pub(super) fn of(request: &Request) -> Option<Self> {
        request.stream.then(|| {
            if request.is_process() {
                Self::Child
            } else {
                Self::Network
            }
        })
    }
}

#[derive(Default)]
pub(super) struct StreamCounts {
    pub(super) network: usize,
    children: usize,
}

impl StreamCounts {
    pub(super) fn len(&self) -> usize {
        self.network + self.children
    }

    fn counter(&mut self, kind: StreamKind) -> &mut usize {
        match kind {
            StreamKind::Network => &mut self.network,
            StreamKind::Child => &mut self.children,
        }
    }

    pub(super) fn take(&mut self, kind: Option<StreamKind>) {
        if let Some(kind) = kind {
            *self.counter(kind) += 1;
        }
    }

    pub(super) fn release(&mut self, kind: Option<StreamKind>) {
        if let Some(kind) = kind {
            *self.counter(kind) -= 1;
        }
    }

    fn admit(&self, kind: StreamKind) -> Result<(), String> {
        match kind {
            StreamKind::Network => budget("network streams", self.network, 1, STREAMS)
                .map_err(|why| format!("{why}. Long polls, event feeds, WebSockets, listeners and running uploads each use a network slot")),
            StreamKind::Child => budget("streaming child processes", self.children, 1, STREAMING_CHILDREN),
        }
    }
}

pub(super) fn budget_message(name: &str, used: usize, additional: usize, maximum: usize) -> String {
    let requested = used.saturating_add(additional);
    format!("{requested} {name} requested ({used} in use, maximum {maximum})")
}

pub(super) fn budget(
    name: &str,
    used: usize,
    additional: usize,
    maximum: usize,
) -> Result<(), String> {
    if additional > maximum.saturating_sub(used) {
        Err(budget_message(name, used, additional, maximum))
    } else {
        Ok(())
    }
}

pub(super) fn lane_bytes(lane: usize) -> &'static str {
    if lane == 0 {
        "bytes for ordered requests"
    } else {
        "bytes for independent requests"
    }
}

pub(super) fn response_room(lane: usize, limit: usize) -> Result<(), String> {
    let maximum = BYTES[lane];
    if limit > maximum {
        return Err(format!(
            "This request needs {limit} bytes (maximum {maximum} {})",
            lane_bytes(lane)
        ));
    }
    Ok(())
}

pub(super) fn stream_room(state: &State, lane: usize, limit: usize) -> Result<(), String> {
    if lane == 1 {
        let streams = state
            .running
            .iter()
            .filter(|run| run.lane == lane && run.stream.is_some())
            .map(|run| run.charge)
            .sum();
        let maximum = BYTES[lane];
        if limit > maximum.saturating_sub(streams) {
            return Err(format!("This response needs {limit} bytes and open streams hold {streams} bytes (maximum {maximum} bytes for independent requests)"));
        }
    }
    Ok(())
}

impl Core {
    /// Admit `r`, hold it behind the ordered barrier, or refuse it, handing
    /// back its work for the caller to destroy.
    pub(super) fn admit(
        &self,
        r: RequestOut,
        work: Option<OwnedWork>,
        forgotten: bool,
        waiting: bool,
        cache_store_withheld: Option<bool>,
    ) -> Result<(), (String, Option<OwnedWork>)> {
        let ordered = r.request.is_ordered();
        let overlap = reads::overlaps(&r, reads::opaque(&work));
        let first_admission = cache_store_withheld.is_none();
        let preloads =
            (first_admission && r.request.prefetch).then(|| self.preloads.lock().unwrap());
        let mut state = self.shared.state.lock().unwrap();
        state.dispatched = true;
        // A deadline on work that cannot take one (a stream but an upload, a continuation, native work), or a
        // long poll that is no stream's HTTP GET (LLP 1016.000 D8), is refused here, before any path.
        let checked = (r.request.prefetch && r.class != RequestClass::Read)
            .then_some("exactPrefetch requires a plain HTTP read")
            .or(super::outcome::request_refusal(&r.request))
            .or(r.request.prefetch_refusal())
            .or(r.request.preload_refusal())
            .or(r.request.long_poll_refusal())
            .or(r.request.timeout_refusal())
            .or(r.request.protocols_refusal())
            .or(r.request.body_from_refusal())
            .or(r.request.body_to_refusal())
            .or(r.request.upload_progress_refusal())
            .or(r.request.stream_open_refusal())
            .map_or_else(
                || {
                    reads::reservation(
                        &r.request,
                        (overlap && r.class == RequestClass::Read)
                            || (r.class == RequestClass::Local && r.request.is_local_work()),
                    )
                },
                |why| Err(why.into()),
            )
            .and_then(|charge| state.book.reserve_inline_copy(&r.request, charge));
        let lane = checked.as_ref().map_or(0, |(lane, _, _)| *lane);
        if let Some(why) = &self.worker_refusal {
            return Err((why.clone(), work));
        }
        if !self.start_workers(&mut state, lane) || (overlap && !self.start_workers(&mut state, 1))
        {
            return Err(("native executor could not start its workers".into(), work));
        }
        if state.retired {
            return Err(("native executor retired".into(), work));
        }
        let cache_store_withheld = cache_store_withheld.unwrap_or_else(|| {
            cache::asked(&mut state, &r, reads::opaque(&work));
            false
        });
        let effect_read = reads::effect_read(&r, reads::opaque(&work));
        let waits_for_effect =
            (effect_read || waiting) && preload::effect_count(&state) >= COUNTS[0];
        if ordered && (state.ordered_barrier || !state.fenced.is_empty() || waits_for_effect) {
            // Later requests, GETs included, wait behind a held read so
            // admission preserves reply order (LLP 1162 D1).
            let held = state
                .fenced
                .iter()
                .filter(|f| matches!(f, Fenced::Held(..)))
                .count();
            let place = match checked {
                Err(why) => Err(why),
                Ok((_, charge, _)) => budget(
                    "ordered requests waiting behind earlier work",
                    held,
                    1,
                    ORDERED_READS,
                )
                .and_then(|()| {
                    budget(
                        "ordered request bytes waiting behind earlier work",
                        state.fenced_bytes,
                        charge,
                        ORDERED_WAITING_BYTES,
                    )
                })
                .map(|()| charge),
            };
            match place {
                Ok(charge) => {
                    state.fenced_bytes += charge;
                    state.fenced.push_back(Fenced::Held(
                        Box::new(r),
                        work,
                        charge,
                        forgotten,
                        cache_store_withheld,
                    ));
                }
                Err(why) => {
                    state.fenced.push_back(Fenced::Refused(r.ticket, why));
                    self.discard(&mut state, work);
                }
            }
            return Ok(());
        }
        // An upload waits for its start holding no count, bytes or place of
        // its lane, so a queue of them refuses no other request; it takes
        // them as it starts (`stream::start`).
        let waits = r.request.stream && r.request.upload_progress;
        // A process one-shot runs on a thread of its own with room of its
        // own, so a slow one leaves the lane's requests theirs (LLP 1133.007 D3).
        let one_shot = r.request.is_process() && !r.request.stream;
        let slot = StreamKind::of(&r.request).filter(|_| !waits);
        let admitted = (|| {
            let (lane, charge, limit) = checked?;
            if !r.request.stream {
                stream_room(&state, lane, limit)?;
            }
            let charge = if r.request.stream {
                if let Some(kind) = slot {
                    state.streams.admit(kind)?;
                }
                limit
            } else if one_shot {
                0
            } else if lane == 1 {
                budget(
                    "independent requests",
                    state.counts[1] - state.streams.len(),
                    1,
                    COUNTS[1],
                )?;
                charge
            } else {
                // Reads share the ordered backlog. Writes and opaque work also
                // count against the smaller effects limit until they settle.
                if !reads::safe_read(&r.request, reads::opaque(&work), r.class) {
                    budget(
                        "ordered request slots",
                        preload::effect_count(&state),
                        1,
                        COUNTS[0],
                    )
                    .map_err(|why| format!("{why}. An effect needs a free ordered slot"))?;
                }
                budget(
                    "ordered requests",
                    state.counts[0] - state.agains,
                    1,
                    ORDERED_READS,
                )?;
                charge
            };
            if waits {
                stream::check_upload_room(&self.shared, r.request.body.len())?;
            } else {
                // A parked upload keeps the bytes it will need when it resumes.
                let parked = state.book.parked(lane, &state.jobs[lane]);
                budget(
                    lane_bytes(lane),
                    state.bytes[lane] + parked,
                    charge,
                    BYTES[lane],
                )?;
            }
            if lane == 0 {
                // Count every waiting request, including writes. Otherwise
                // 128 large reads could fill the lane before its first job
                // can reserve the response bytes it needs to start.
                let waiting = state.jobs[0].iter().map(|job| job.waiting_charge).sum();
                budget(
                    "queued ordered request bytes",
                    waiting,
                    charge,
                    ORDERED_WAITING_BYTES,
                )?;
            }
            if one_shot {
                process::admit_one_shot(&self.shared, r.ticket, limit)?;
            }
            Ok((lane, charge, limit))
        })();
        let (lane, charge, limit) = match admitted {
            Ok(value) => value,
            Err(reason) => {
                // Failure parsing can mutate Store too. Refusals live on
                // runner tickets, but fence later ordered work here until
                // the host has settled/forgotten all those tickets.
                if ordered {
                    state.ordered_barrier = true;
                }
                return Err((reason, work));
            }
        };
        // Only a request that waits in its lane's queue has one: a stream or
        // a one-shot starts on a thread of its own at once.
        let queues = !r.request.stream && !one_shot;
        let queued_deadline = if lane == 1 && queues && !reads::opaque(&work) {
            match r
                .request
                .timeout_ms
                .map(|ms| match &self.machine {
                    Some(machine) => Ok(forget::QueuedDeadline::on_machine(
                        machine.timer.clone(),
                        lane,
                        r.ticket,
                        ms,
                    )),
                    None => forget::QueuedDeadline::arm(&self.shared, lane, r.ticket, ms),
                })
                .transpose()
            {
                Ok(deadline) => deadline,
                Err(()) => return Err(("native queued deadline could not be armed".into(), work)),
            }
        } else {
            None
        };
        if ordered {
            let kind = reads::ordered_kind(&r, reads::opaque(&work));
            if !forgotten {
                state
                    .ordered
                    .push_back(reads::OrderedEntry::new(r.ticket, kind));
            } else if matches!(
                kind,
                reads::OrderedKind::Barrier | reads::OrderedKind::Local
            ) {
                state.ordered.push_back(reads::OrderedEntry {
                    ticket: r.ticket,
                    kind,
                    forgotten: true,
                });
            }
        }
        if !waits && !one_shot {
            state.counts[lane] += 1;
            state.bytes[lane] += charge;
        }
        if r.request.stream || r.request.is_process() {
            // Never queued behind held replies: a stream opens on its own
            // thread with its own transport, and reads there (LLP 1067 D3).
            // An upload takes its place and thread once its budget has room.
            // A process request runs on a thread of its own, and only a
            // streaming child holds its own kind of slot (LLP 1133.007 D3).
            let upload = r.request.upload_progress;
            let stream = slot.is_some();
            let exchange = if r.request.is_process() {
                !stream
            } else {
                stream && r.request.listen.is_none() && r.request.long_poll.is_none()
            };
            state.streams.take(slot);
            let abort = AbortController::new();
            state.running.push(Running {
                started: std::time::Instant::now(),
                ticket: r.ticket,
                lane,
                transport_lane: lane,
                charge: if waits || one_shot { 0 } else { charge },
                limit,
                abort: Some(abort.clone()),
                exchange: exchange.then(|| abort.clone()),
                forgotten,
                stream: slot,
                waiting: waits || one_shot,
                read: false,
                overlap: false,
                class: r.class,
                terminal: false,
                live: 0,
                drained: false,
                blocking: false,
                generation: 0,
                claim: asynch::Claim::None,
                cached: false,
                cache_exchange: None,
                cache_replacing: cache::replacing(
                    &r.request,
                    r.class,
                    r.forced,
                    reads::opaque(&work),
                ),
                refresh: None,
                cache_barrier: cache::barrier(&r.request, r.class, reads::opaque(&work)),
                invalidates: cache::invalidates(&r.request, reads::opaque(&work)),
            });
            rouse(&self.shared, &state);
            drop(state);
            let (shared, grants) = (self.shared.clone(), self.grants.clone());
            let (ticket, request, forced) = (r.ticket, r.request, r.forced);
            let host = self.stream_host.clone();
            if upload {
                stream::queue_upload(&shared, ticket, request, forced, grants, host, abort);
                return Ok(());
            }
            let spawned = std::thread::Builder::new()
                .name(format!("exact-stream-{ticket}"))
                .spawn({
                    let shared = shared.clone();
                    move || match request.is_process() {
                        true => process::run(&shared, ticket, request, &grants, &*host, abort),
                        false => {
                            stream::run(&shared, ticket, request, forced, &grants, &*host, abort)
                        }
                    }
                });
            if let Err(e) = spawned {
                process::release_one_shot(&shared, ticket);
                complete(&shared, ticket, failed(FailureKind::Network, e.to_string()));
            }
            return Ok(());
        }
        if let Some(cache) = state.cache.as_mut() {
            preload::refresh(cache, &r);
        }
        let job = Job {
            ticket: r.ticket,
            request: r.request,
            forced: r.forced,
            work,
            waiting_charge: charge,
            waited_for_preload: false,
            charge,
            limit,
            forgotten,
            queued_deadline,
            overlap,
            class: r.class,
            cache_store_withheld,
        };
        // A fenced read sends itself when admitted again (LLP 1162.000 D5).
        if let Some(mut preloads) = preloads {
            self.prefetch(&mut state, &mut preloads, &job);
        }
        state.jobs[lane].push_back(job);
        cache::serve_ready(&mut state);
        #[cfg(test)]
        self.shared
            .worker_notifications
            .fetch_add(1, Ordering::Relaxed);
        self.shared.ready.notify_all();
        Ok(())
    }

    /// Lift the barrier once the runner retains no ordered refusal, and
    /// admit the requests held behind it, in order. One past a limit is
    /// refused, raising the barrier again over the rest: its ticket and
    /// reason are returned for the host to record (`refuse_request`).
    pub fn resume_ordered(&self) -> Vec<(u64, String)> {
        let (fenced, automatic) = {
            let mut state = self.shared.state.lock().unwrap();
            let automatic = !state.ordered_barrier;
            state.ordered_barrier = false;
            state.fenced_bytes = 0;
            (std::mem::take(&mut state.fenced), automatic)
        };
        let mut refused = Vec::new();
        for place in fenced {
            match place {
                Fenced::Held(r, work, _, forgotten, withheld) => {
                    let ticket = r.ticket;
                    // Behind a refusal made just now, it is held again.
                    if let Err((reason, work)) =
                        self.admit(*r, work, forgotten, automatic, Some(withheld))
                    {
                        refused.push((ticket, reason));
                        let mut state = self.shared.state.lock().unwrap();
                        self.discard(&mut state, work);
                    }
                }
                Fenced::Refused(ticket, reason) => {
                    let mut state = self.shared.state.lock().unwrap();
                    if state.ordered_barrier || !state.fenced.is_empty() {
                        state.fenced.push_back(Fenced::Refused(ticket, reason));
                    } else {
                        state.ordered_barrier = true;
                        refused.push((ticket, reason));
                    }
                }
            }
        }
        // The re-asks that waited enter after the requests held before them.
        let mut state = self.shared.state.lock().unwrap();
        place_waiting(&mut state);
        if has_ready(&state) {
            wake(&mut state);
        }
        refused
    }

    /// Waiting reads need no refusal lifted by the host (LLP 1162 D1).
    pub(super) fn resume_waiting(&self) {
        {
            let state = self.shared.state.lock().unwrap();
            if state.ordered_barrier || state.fenced.is_empty() {
                return;
            }
        }
        let refused = self.resume_ordered();
        if !refused.is_empty() {
            let mut state = self.shared.state.lock().unwrap();
            for (ticket, reason) in refused.into_iter().rev() {
                state.fenced.push_front(Fenced::Refused(ticket, reason));
            }
            state.ordered_barrier = true;
            wake(&mut state);
        }
    }
}
