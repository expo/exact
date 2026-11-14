//! HTTP starts, cache retries and file publication on the machine (LLP 1162 D4, D8).
use super::*;

impl Machine {
    /// An HTTP job: its file sized or read, or its fetch started on the
    /// lane's transport.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn http(
        self: &Arc<Self>,
        lane: usize,
        run: Run,
        request: Request,
        forced: bool,
        sized: bool,
        abort: AbortController,
        retiring: AbortRegistration,
        scoped: Option<ibex2::grant::GrantSet>,
    ) {
        let refuse = |outcome| self.answer(lane, run, outcome);
        if abort.signal().aborted() {
            return refuse(failed(FailureKind::Aborted, "native request aborted"));
        }
        if let Some(why) = request.timeout_refusal().or(request.long_poll_refusal()) {
            return refuse(failed(FailureKind::Refused, why));
        }
        let transport_lane = find(&mut self.shared.state.lock().unwrap().running, run)
            .expect("starting run")
            .transport_lane;
        let Some((_, fetch)) = self.transports.lock().unwrap()[transport_lane].clone() else {
            return refuse(failed(FailureKind::Refused, self.unbound.clone()));
        };
        if request.body_to.is_some()
            && !self.shared.state.lock().unwrap().book.portable[transport_lane]
        {
            return refuse(failed(
                FailureKind::Unsupported,
                "exactBodyTo: not on the native DarwinLane (URLSession) yet",
            ));
        }
        // A source's narrower scope runs on the lane's transport too.
        let fetch = match scoped {
            Some(scope) => fetch.with_grants(scope),
            None => fetch,
        };
        let fetch = super::super::reach::gate_fetch(fetch, request.reach);
        let limit = super::super::body_to::limit(&request);
        if request.body_from.is_some() || request.body_to.is_some() {
            let file = FileJob {
                request,
                forced,
                limit,
                abort,
                retiring,
                fetch,
            };
            return if file.request.body_from.is_some() {
                self.file(lane, run, file, sized)
            } else {
                self.file_reply(lane, run, file)
            };
        }
        let deadline = request.timeout_ms.map(|ms| (ms, abort.clone()));
        self.fetch(
            lane, run, request, None, forced, abort, retiring, limit, fetch, deadline, None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fetch(
        self: &Arc<Self>,
        lane: usize,
        run: Run,
        request: Request,
        upload: Option<Arc<Upload>>,
        forced: bool,
        abort: AbortController,
        retiring: AbortRegistration,
        limit: usize,
        fetch: ibex2::host::Fetch,
        deadline: Option<(u32, AbortController)>,
        destination: Option<ibex2::stdlib::fetch_async::ResponseBody>,
    ) {
        let timeout = request.timeout_ms;
        let mut req = fetch_request(request, forced);
        req.response_body = destination;
        super::super::cache::observe(&self.shared, &mut req);
        req.max_body = Some(limit);
        // The platform's idle timeout follows the deadline a second later,
        // as on the workers.
        req.timeout = timeout.map(|ms| Duration::from_millis(u64::from(ms) + 1_000));
        let tracked = Arc::new(Tracked {
            machine: self.clone(),
            lane,
            run,
            deadline: Mutex::new(deadline),
        });
        let retry = super::super::cache::retry_request(&self.shared, run.0, &req).map(|request| {
            let (fetch, tracked, upload, signal) = (
                fetch.clone(),
                tracked.clone(),
                upload.clone(),
                abort.signal(),
            );
            let url = request.url.clone();
            let retry = Box::new(move |events: Arc<Events>| {
                fetch.start_async(
                    tracked,
                    request,
                    upload,
                    signal,
                    HeaderCaps {
                        bytes: MAX_HEADERS,
                        entries: 1024,
                    },
                    events,
                );
            }) as RetryStart;
            (url, retry)
        });
        let events = Arc::new_cyclic(|this| Events {
            machine: self.clone(),
            lane,
            run,
            limit,
            this: this.clone(),
            retry: Mutex::new(retry),
            _retiring: Mutex::new(Some(retiring)),
        });
        fetch.start_async(
            tracked,
            req,
            upload,
            abort.signal(),
            HeaderCaps {
                bytes: MAX_HEADERS,
                entries: 1024,
            },
            events,
        );
    }
}

/// Each hop uses its admitted transport and places (LLP 1162 D1).
/// Its first hop arms the deadline when the task is resumed.
struct Tracked {
    machine: Arc<Machine>,
    lane: usize,
    run: Run,
    deadline: Mutex<Option<(u32, AbortController)>>,
}

#[path = "metrics.rs"]
mod metrics_log;
#[path = "async_transport.rs"]
mod tracked_transport;

type RetryStart = Box<dyn FnOnce(Arc<Events>) + Send>;
type Retry = (String, RetryStart);

/// A fetch's events, to its lane's finisher.
struct Events {
    machine: Arc<Machine>,
    lane: usize,
    run: Run,
    /// The response ceiling, which names an over-limit failure.
    limit: usize,
    this: std::sync::Weak<Events>,
    retry: Mutex<Option<Retry>>,
    /// Retirement's abort registration, held for the fetch's life.
    _retiring: Mutex<Option<AbortRegistration>>,
}

impl FetchEvents for Events {
    fn outcome(&self, result: Result<Collected, HostError>) {
        let (retry, accepted) = {
            let state = &mut self.machine.shared.state.lock().unwrap();
            let open = find(&mut state.running, self.run)
                .is_some_and(|r| !r.terminal && r.claim == Claim::None);
            let retry = open
                && result.as_ref().is_ok_and(|r| {
                    super::super::cache::retry(state, self.run.0, r.status, r.redirected)
                });
            let accepted = !retry
                && if let Some(r) = find(&mut state.running, self.run)
                    .filter(|r| !r.terminal && r.claim == Claim::Publishing)
                {
                    r.claim = Claim::Answered;
                    true
                } else {
                    claim(state, self.run, Claim::Answered)
                };
            (retry, accepted)
        };
        if retry {
            let (url, retry) = self
                .retry
                .lock()
                .unwrap()
                .take()
                .expect("a cache conditional");
            let events = self.this.upgrade().expect("live fetch events");
            self.next_hop(&url, Box::new(move || retry(events)));
        } else if accepted {
            self.machine.post(
                self.lane,
                Event::Outcome(self.run, Done::Fetch(result, self.limit)),
            );
        }
    }
    fn metrics(&self, url: &str, metrics: HopMetrics) {
        metrics_log::note(
            "lane",
            url,
            metrics.protocol.as_deref(),
            metrics.reused,
            metrics.local.as_deref(),
            metrics.remote.as_deref(),
            metrics.queued_ms,
        );
    }
    fn next_hop(&self, url: &str, start: Box<dyn FnOnce() + Send>) {
        self.machine
            .post(self.lane, Event::Hop(self.run, url.to_owned(), start));
    }
}
