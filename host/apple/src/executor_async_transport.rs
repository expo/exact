//! Transport admission and physical-retirement accounting for one Exact lane.
//! @ref LLP 1120#d1-ibex2-an-asynchronous-transport-and-a-fetch-state-machine — the runner retains scheduling.
use super::*;

impl AsyncTransport for Tracked {
    fn start(&self, request: HopRequest, sink: Box<dyn Sink>) -> Result<Exchange, HostError> {
        let origin = origin(&request.url);
        let sends = request.body.is_some();
        let transport_lane = {
            let mut state = self.machine.shared.state.lock().unwrap();
            if state.retired
                || !find(&mut state.running, self.run)
                    .is_some_and(|r| !r.terminal && r.claim == Claim::None)
            {
                return Err(ibex2::stdlib::fetch_async::aborted());
            }
            let (transport_lane, held_origin) = state
                .book
                .reserved
                .remove(&self.run)
                .expect("a hop is admitted before transport preparation");
            debug_assert_eq!(held_origin, origin);
            find(&mut state.running, self.run).expect("open run").live += 1;
            if let Some(u) = state.book.uploads.get_mut(&self.run).filter(|_| sends) {
                u.hops += 1;
            }
            transport_lane
        };
        let sink = Box::new(TrackedSink {
            inner: sink,
            machine: self.machine.clone(),
            lane: self.lane,
            run: self.run,
            origin,
            transport_lane,
            ended: false,
            sends,
            redirect: false,
            file: self
                .machine
                .shared
                .state
                .lock()
                .unwrap()
                .book
                .downloads
                .contains(&self.run),
        });
        // The sink owns the physical hold before calling opaque transport
        // code, including refusal or unwind before an exchange is returned.
        let inner = self.machine.transports.lock().unwrap()[transport_lane]
            .as_ref()
            .map(|(transport, _)| transport.clone())
            .ok_or_else(ibex2::stdlib::fetch_async::aborted)?;
        let exchange = Arc::new(inner.start(request, sink)?);
        let first = self.deadline.lock().unwrap().take();
        let (machine, lane, run) = (self.machine.clone(), self.lane, self.run);
        let activate = exchange.clone();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let activated_cancelled = cancelled.clone();
        Ok(Exchange::new(
            move || {
                // Upload sizing and parking precede preparation. The one
                // deadline starts only when its first accepted hop activates.
                if !activated_cancelled.load(std::sync::atomic::Ordering::Acquire) {
                    if let Some((ms, abort)) = first {
                        machine.timer.arm(ms, lane, run, abort);
                    }
                }
                if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    activate.activate();
                })) {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        activate.cancel();
                    }));
                    std::panic::resume_unwind(panic);
                }
            },
            move || {
                cancelled.store(true, std::sync::atomic::Ordering::Release);
                exchange.cancel();
            },
        ))
    }
}

struct TrackedSink {
    inner: Box<dyn Sink>,
    machine: Arc<Machine>,
    lane: usize,
    run: Run,
    origin: String,
    transport_lane: usize,
    ended: bool,
    /// It sends the run's upload.
    sends: bool,
    file: bool,
    redirect: bool,
}

impl Sink for TrackedSink {
    fn head(&mut self, head: Head) -> Flow {
        let flow = self.inner.head(head);
        self.redirect = flow == Flow::Skip;
        flow
    }
    fn data(&mut self, bytes: &[u8]) -> Flow {
        self.inner.data(bytes)
    }
    fn end(&mut self, result: Result<(), HostError>) {
        // A file destination publishes or cleans up during `end`; its places
        // outlive that work (LLP 1162 D1). Other hops release before a redirect.
        if !self.file {
            self.release();
        }
        self.inner.end(result);
        #[cfg(test)]
        if self.file {
            self.machine.body_to_phase(
                if self.redirect {
                    body_to::Phase::RedirectEnded
                } else {
                    body_to::Phase::ReplyEnded
                },
                self.run,
            );
        }
        self.release();
    }
    fn metrics(&mut self, metrics: HopMetrics) {
        if let Some(protocol) = &metrics.protocol {
            self.machine.post(
                self.lane,
                Event::Protocol(self.transport_lane, self.origin.clone(), protocol.clone()),
            );
        }
        self.inner.metrics(metrics);
    }
    fn observed(&mut self, status: u16, headers: &ibex2::stdlib::fetch::Headers) {
        self.inner.observed(status, headers);
    }
}

impl TrackedSink {
    fn release(&mut self) {
        if !self.ended {
            self.ended = true;
            self.machine.post(
                self.lane,
                Event::Ended(
                    self.run,
                    Some((self.transport_lane, self.origin.clone())),
                    self.sends,
                    !self.redirect,
                ),
            );
        }
    }
}

impl Drop for TrackedSink {
    fn drop(&mut self) {
        self.release();
    }
}
