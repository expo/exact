use super::wire::{check_table, WireCodec};
use super::{
    Cancel, Done, Job, JobError, Limits, Order, Queued, RemoteJob, Shared, State, WireError,
};
use crate::{perf::Stamp, HookTime, Hooks};
use std::cell::OnceCell;
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Identifies a queue within one GPU module instance.
pub type QueueId = u32;
/// Identifies one posting within the module instance. Pass both 32-bit words
/// across a JavaScript boundary.
pub type Attempt = u64;
/// Identifies a worker lifetime within one GPU module instance.
pub type WorkerId = u32;

/// The state of an installed transport's workers.
pub enum TransportState {
    /// No worker is ready and at least one can still start.
    Starting,
    /// The number of ready workers, including those holding batches. Must be positive.
    Ready(usize),
    /// This transport will never start another worker.
    Ended,
}

/// An answer held by the transport.
pub enum Outcome {
    /// The length of the tagged answer.
    Bytes(usize),
    /// A trap inside the job's run.
    Trapped,
}

/// What happened when the queue tried to copy an announced answer.
pub enum ResultRead {
    /// The full answer was copied and consumed.
    Copied,
    /// The transport no longer holds the answer.
    Gone,
    /// The transport discarded an answer it could not copy at the announced length.
    Unreadable,
}

/// An event polled by a queue during preparation.
pub enum TransportEvent {
    /// A job answered. The transport retains its bytes until read or forgotten.
    Arrived {
        /// The posting that answered.
        attempt: Attempt,
        /// How the posting ended.
        outcome: Outcome,
    },
    /// A worker was lost. Already answered attempts are omitted.
    Lost {
        /// The worker lifetime that ended.
        worker: WorkerId,
        /// The worker's unanswered attempts.
        attempts: Vec<Attempt>,
    },
    /// The lane ended. Only trap notices survive for delivery on a live take.
    Ended,
}

/// One encoded job in a batch.
pub struct Posting {
    /// The posting's unique attempt number.
    pub attempt: Attempt,
    /// The registered job name.
    pub name: &'static str,
    /// Encoded inputs.
    pub input: Vec<u8>,
}

/// Instance-wide transport gauges and totals.
#[derive(Clone, Copy, Debug, Default)]
pub struct TransportReport {
    /// Ready workers, including workers with a batch.
    pub workers_ready: usize,
    /// Largest worker linear memory observed, in bytes.
    pub worker_bytes: u64,
    /// Total milliseconds spent in message handlers. Must be finite.
    pub handler_ms: f64,
    /// Largest total number of answer bytes buffered at once.
    pub peak_buffered_bytes: u64,
}

/// Workers and answer buffers for one GPU module instance (LLP 1182.000 D9).
///
/// Rust owns jobs, priorities, generations and limits. The transport owns
/// worker startup (5 s timeout), batch progress (10 s since posting or the last
/// answer), replacement, loss counting, late messages and worker assignment.
/// A batch timeout, worker error, post that throws or rejected answer is a loss. End the
/// lane after three consecutive losses or a failed replacement. Only an
/// answer accepted by the queue resets the count. Report `Ended` when no worker
/// can still start.
///
/// Every accepted attempt still wanted by the queue ends exactly once in
/// `Arrived`, `Lost` or an `Ended` covering the outstanding attempts.
/// A trapped attempt arrives as `Trapped`. Close and replace its worker without
/// counting a loss. Report the rest of its unanswered batch in `Lost` so the
/// queue requeues those jobs. Ignore messages from old worker lifetimes and
/// answers for forgotten attempts. Forgotten attempts can occupy a worker
/// until its batch ends.
///
/// During a synchronous take, transport state changes only in calls from
/// queues. Apply asynchronous events between takes or within a snapshot call.
/// A live, advancing take snapshots readiness and events once at entry. Posting
/// at exit counts announced answer bytes still held, using a 64-bit aggregate.
/// Keep announced buffers readable until read, forgotten or the lane ends.
/// On end, drop all buffers immediately, including those announced to open
/// takes. Reads of those buffers return `Gone`. Keep trap notices for live
/// delivery even in an ended snapshot.
/// The agent's clock discards arrived results, including traps, and requeues
/// their jobs in submission order within priority.
pub trait Transport: Send + Sync + 'static {
    /// Request worker startup with the module's registered names.
    fn start(&self, names: &[&str]);
    /// Start a take, snapshot readiness and drain this queue's events in arrival
    /// order. Arrivals and losses are applied before an end.
    fn events(&self, queue: QueueId) -> (TransportState, Vec<TransportEvent>);
    /// Whether an idle worker can serve this asking queue. Among active queues,
    /// choose the one served least recently. A queue that has never posted comes first.
    fn can_post(&self, queue: QueueId) -> bool;
    /// Count workers still holding this queue's batches, including forgotten attempts.
    fn busy_workers(&self, queue: QueueId) -> usize;
    /// At take exit, record an eligible queue's ask with the transport's clock.
    /// It is active for 250 ms. Passing false withdraws the ask immediately.
    /// The queue asks only with wire jobs queued and room under both its
    /// running and completed byte limits, and while it can encode a job
    /// within the deadline or with its remaining late unit.
    fn set_asking(&self, queue: QueueId, asking: bool);
    /// Post a batch of 1–16 jobs if this queue can use the idle worker.
    /// Success updates when the queue was last served. Refusal accepts no attempts.
    fn post(&self, queue: QueueId, jobs: &[Posting]) -> Option<WorkerId>;
    /// Consume one answer. A gone or unreadable answer falls back to the
    /// retained job. A gone answer does not count as a transport loss.
    fn read_result(&self, queue: QueueId, attempt: Attempt, out: &mut [u8]) -> ResultRead;
    /// Reset the consecutive loss count after the queue accepts a result,
    /// a `JobError` or a `Trapped` notice.
    fn accept_answer(&self);
    /// Count a loss and stop posting to this worker immediately. Report its
    /// unanswered attempts at the next live snapshot. Keep other answers
    /// already received for this take. Ignore repeat rejections and
    /// rejections for worker lifetimes that ended.
    fn reject(&self, worker: WorkerId);
    /// Discard a single answer and ignore it if it arrives later.
    fn forget_attempt(&self, queue: QueueId, attempt: Attempt);
    /// Forget attempts and withdraw the queue's ask.
    fn forget(&self, queue: QueueId);
    /// Forget the queue and ignore all subsequent messages addressed to it.
    fn retire(&self, queue: QueueId);
    /// Read transport gauges for renderer diagnostics without changing transport state.
    fn report(&self) -> TransportReport;
}

thread_local! {
    static TRANSPORT: OnceCell<Arc<TransportHandle>> = const { OnceCell::new() };
}

pub(crate) fn install_transport<H: Hooks>(transport: impl Transport) -> Result<(), WireError> {
    let handle = Arc::new(TransportHandle::new(H::REMOTE, Arc::new(transport))?);
    TRANSPORT.with(|slot| slot.set(handle).map_err(|_| WireError::AlreadyInstalled))
}

pub(super) fn installed() -> Option<Arc<TransportHandle>> {
    TRANSPORT.with(|slot| slot.get().cloned())
}

pub(super) fn installed_report() -> Option<TransportReport> {
    TRANSPORT.with(|slot| slot.get().map(|handle| handle.transport.report()))
}

pub(super) struct TransportHandle {
    entries: &'static [RemoteJob],
    transport: Arc<dyn Transport>,
    started: AtomicBool,
    next_queue: AtomicU32,
    pub(super) next_attempt: AtomicU64,
}

impl TransportHandle {
    pub(super) fn new(
        entries: &'static [RemoteJob],
        transport: Arc<dyn Transport>,
    ) -> Result<Self, WireError> {
        check_table(entries)?;
        Ok(Self {
            entries,
            transport,
            started: AtomicBool::new(false),
            next_queue: AtomicU32::new(1),
            next_attempt: AtomicU64::new(1),
        })
    }

    pub(super) fn queue<J: Job>(self: &Arc<Self>) -> Option<RemoteQueue<J>> {
        let entry = self.entries.iter().find(|entry| entry.is::<J>())?;
        Some(RemoteQueue {
            handle: self.clone(),
            id: next_id(&self.next_queue),
            name: entry.name,
            out: BTreeMap::new(),
            arrived: BTreeMap::new(),
            ended: false,
            ready: false,
            seekable: false,
            metrics: Arc::default(),
        })
    }

    fn start(&self) {
        if !self.started.swap(true, Ordering::Relaxed) {
            let names: Vec<_> = self.entries.iter().map(|entry| entry.name).collect();
            self.transport.start(&names);
        }
    }
}

fn next_id(counter: &AtomicU32) -> u32 {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("remote job ID exhausted")
}

struct Out<J> {
    order: Order,
    queued: Queued<J>,
    worker: WorkerId,
}

struct Arrived<J> {
    posted: Out<J>,
    attempt: Attempt,
    outcome: Outcome,
}

impl Outcome {
    fn bytes(&self) -> u64 {
        match *self {
            Self::Bytes(bytes) => bytes as u64,
            Self::Trapped => 0,
        }
    }
}

pub(super) struct RemoteQueue<J> {
    handle: Arc<TransportHandle>,
    id: QueueId,
    name: &'static str,
    out: BTreeMap<Attempt, Out<J>>,
    arrived: BTreeMap<Order, Arrived<J>>,
    ended: bool,
    ready: bool,
    seekable: bool,
    pub metrics: Arc<Mutex<WireMetrics>>,
}

impl<J: Job> RemoteQueue<J> {
    pub(super) fn begin(&mut self, state: &mut State<J>, time: &HookTime) -> bool {
        if time.seekable {
            if !self.seekable {
                self.requeue(state, false);
                if !self.ended {
                    self.handle.transport.forget(self.id);
                }
                self.seekable = true;
            }
            return false;
        }
        if self.ended {
            return false;
        }
        self.seekable = false;
        let (readiness, events) = self.handle.transport.events(self.id);
        let mut ended = matches!(readiness, TransportState::Ended | TransportState::Ready(0));
        let completed_before = state.completed_bytes;
        for event in events {
            match event {
                TransportEvent::Arrived { attempt, outcome } => {
                    if let Some(posted) = self.out.remove(&attempt) {
                        state.completed_bytes += outcome.bytes();
                        self.arrived.insert(
                            posted.order,
                            Arrived {
                                posted,
                                attempt,
                                outcome,
                            },
                        );
                    }
                }
                TransportEvent::Lost { worker, attempts } => {
                    for attempt in attempts {
                        if self
                            .out
                            .get(&attempt)
                            .is_some_and(|posted| posted.worker == worker)
                        {
                            let posted = self.out.remove(&attempt).unwrap();
                            state.wire_queue.insert(posted.order, posted.queued);
                        }
                    }
                }
                TransportEvent::Ended => ended = true,
            }
        }
        if ended {
            self.requeue(state, true);
            if matches!(readiness, TransportState::Ready(0)) {
                self.handle.transport.forget(self.id);
            }
            self.ended = true;
            return false;
        }
        if state.completed_bytes != completed_before {
            state.record_retained_peak();
        }
        self.ready |= matches!(readiness, TransportState::Ready(_));
        self.ready
    }

    fn requeue(&mut self, state: &mut State<J>, keep_traps: bool) {
        for posted in std::mem::take(&mut self.out).into_values() {
            state.wire_queue.insert(posted.order, posted.queued);
        }
        for (order, arrival) in self.arrived.extract_if(.., |_, arrival| {
            !keep_traps || !matches!(arrival.outcome, Outcome::Trapped)
        }) {
            state.wire_queue.insert(order, arrival.posted.queued);
        }
        state.completed_bytes = 0;
    }

    pub(super) fn wanted_bytes(&self, order: Order) -> Option<usize> {
        if let Some(arrival) = self.arrived.get(&order) {
            return Some(arrival.posted.queued.bytes);
        }
        self.out
            .values()
            .find(|posted| posted.order == order)
            .map(|posted| posted.queued.bytes)
    }

    pub(super) fn out(&self) -> usize {
        self.out.len()
    }

    pub(super) fn pending(&self) -> usize {
        self.out.len() + self.arrived.len()
    }

    pub(super) fn has_arrivals(&self) -> bool {
        !self.arrived.is_empty()
    }

    pub(super) fn invalidate(
        &mut self,
        state: &mut State<J>,
        order: Order,
    ) -> (Option<Queued<J>>, bool) {
        let transport = &self.handle.transport;
        if let Some(arrival) = self.arrived.remove(&order) {
            transport.forget_attempt(self.id, arrival.attempt);
            state.completed_bytes -= arrival.outcome.bytes();
            state.queued_bytes -= arrival.posted.queued.bytes;
            return (Some(arrival.posted.queued), true);
        }
        let attempt = self
            .out
            .iter()
            .find(|(_, posted)| posted.order == order)
            .map(|(&id, _)| id);
        if let Some(attempt) = attempt {
            let posted = self.out.remove(&attempt).unwrap();
            transport.forget_attempt(self.id, attempt);
            state.queued_bytes -= posted.queued.bytes;
            return (Some(posted.queued), false);
        }
        (None, false)
    }

    pub(super) fn clear(&mut self) -> usize {
        let stale = self.arrived.len();
        self.out.clear();
        self.arrived.clear();
        self.handle.transport.forget(self.id);
        stale
    }

    pub(super) fn accepts_wire(&self) -> bool {
        !self.ended
    }

    pub(super) fn needs_start(&self) -> bool {
        self.accepts_wire() && !self.handle.started.load(Ordering::Relaxed)
    }

    pub(super) fn start(&self) {
        if !self.ended {
            self.handle.start();
        }
    }

    pub(super) fn withdraw_if_empty(&self, state: &State<J>) {
        if !self.ended && state.wire_queue.is_empty() {
            self.handle.transport.set_asking(self.id, false);
        }
    }

    pub(super) fn deliver(
        &mut self,
        shared: &Shared<J>,
        work: &mut WireFrame,
    ) -> Option<Done<J::Output>> {
        let mut state = shared.state.lock().unwrap();
        let local = if self.ended {
            state.first_queued().copied()
        } else {
            state.queue.first_key_value().map(|(&order, _)| order)
        };
        let arrival = self.arrived.first_key_value().map(|(&order, _)| order);
        let (queued, result, wire) = match (arrival, local) {
            (Some(order), _) if local.is_none_or(|local| order < local) => {
                drop(state);
                let result = self.read(shared.codec.as_ref().unwrap(), work, &self.arrived[&order]);
                let arrived = self.arrived.remove(&order).unwrap();
                let queued = arrived.posted.queued;
                let mut state = shared.state.lock().unwrap();
                state.completed_bytes -= arrived.outcome.bytes();
                state.latest.remove(&queued.ticket.key);
                state.queued_bytes -= queued.bytes;
                (queued, result, false)
            }
            (_, Some(order)) => {
                let (queued, wire) = state.remove_queued(order);
                state.latest.remove(&queued.ticket.key);
                drop(state);
                (queued, None, wire)
            }
            _ => return None,
        };
        Some(Done {
            key: queued.ticket.key,
            generation: queued.ticket.generation,
            result: result
                .unwrap_or_else(|| shared.run(queued.job, &Cancel::default(), Some(wire))),
        })
    }

    fn read(
        &self,
        codec: &WireCodec<J>,
        work: &mut WireFrame,
        arrival: &Arrived<J>,
    ) -> Option<Result<J::Output, JobError>> {
        let transport = &self.handle.transport;
        let Outcome::Bytes(len) = arrival.outcome else {
            transport.accept_answer();
            return Some(Err(JobError("job trapped".into())));
        };
        let stamp = Stamp::now();
        let mut bytes = Vec::new();
        let result = if bytes.try_reserve_exact(len).is_ok() {
            bytes.resize(len, 0);
            match transport.read_result(self.id, arrival.attempt, &mut bytes) {
                ResultRead::Copied => {
                    work.in_bytes += len as u64;
                    codec.read_answer(&bytes)
                }
                ResultRead::Gone => {
                    work.ms += stamp.elapsed();
                    return None;
                }
                ResultRead::Unreadable => None,
            }
        } else {
            transport.forget_attempt(self.id, arrival.attempt);
            None
        };
        work.ms += stamp.elapsed();
        if result.is_some() {
            transport.accept_answer();
        } else {
            transport.reject(arrival.posted.worker);
        }
        result
    }

    pub(super) fn post(
        &mut self,
        state: &mut State<J>,
        codec: &WireCodec<J>,
        limits: Limits,
        time: &HookTime,
        late_unit_spent: &mut bool,
        work: &mut WireFrame,
    ) {
        let transport = &self.handle.transport;
        let mut busy = transport.busy_workers(self.id);
        let past_deadline = || time.deadline.is_some_and(|deadline| deadline.passed());
        let mut asking = false;
        while self.can_ask(state, limits, busy) && !(*late_unit_spent && past_deadline()) {
            if !asking {
                transport.set_asking(self.id, true);
                asking = true;
            }
            if !transport.can_post(self.id) {
                return;
            }
            let mut orders = Vec::new();
            let mut batch = Vec::new();
            for (&order, queued) in state.wire_queue.iter().take(16) {
                if *late_unit_spent && past_deadline() {
                    break;
                }
                let stamp = Stamp::now();
                let mut input = Vec::new();
                (codec.encode)(&queued.job, &mut input);
                work.ms += stamp.elapsed();
                *late_unit_spent |= past_deadline();
                orders.push(order);
                batch.push(Posting {
                    attempt: self.handle.next_attempt.fetch_add(1, Ordering::Relaxed),
                    name: self.name,
                    input,
                });
            }
            if batch.is_empty() {
                break;
            }
            let stamp = Stamp::now();
            let worker = transport.post(self.id, &batch);
            work.ms += stamp.elapsed();
            let Some(worker) = worker else {
                break;
            };
            for (order, posting) in orders.into_iter().zip(batch) {
                work.out_bytes += posting.input.len() as u64;
                let queued = state.wire_queue.remove(&order).unwrap();
                self.out.insert(
                    posting.attempt,
                    Out {
                        order,
                        queued,
                        worker,
                    },
                );
            }
            busy += 1;
        }
        transport.set_asking(self.id, false);
    }

    fn can_ask(&self, state: &State<J>, limits: Limits, busy: usize) -> bool {
        !state.wire_queue.is_empty()
            && busy < limits.running
            && (!self.has_arrivals() || state.completed_bytes < limits.completed_bytes as u64)
    }
}

impl<J> Drop for RemoteQueue<J> {
    fn drop(&mut self) {
        self.handle.transport.retire(self.id);
    }
}

#[derive(Default)]
pub(super) struct WireFrame {
    ms: f64,
    out_bytes: u64,
    in_bytes: u64,
}

#[derive(Default)]
pub(super) struct WireMetrics {
    frames: VecDeque<(u64, f64)>,
    out_bytes: u64,
    in_bytes: u64,
}

impl WireMetrics {
    pub(super) fn record(&mut self, serial: u64, work: WireFrame) {
        const SAMPLES: usize = 16384;
        if let Some((_, ms)) = self.frames.back_mut().filter(|(frame, _)| *frame == serial) {
            *ms += work.ms;
        } else {
            if self.frames.len() == SAMPLES {
                self.frames.pop_front();
            }
            self.frames.push_back((serial, work.ms));
        }
        self.out_bytes += work.out_bytes;
        self.in_bytes += work.in_bytes;
    }

    pub(super) fn append(out: &mut String, sources: &[Arc<Mutex<Self>>], report: TransportReport) {
        let TransportReport {
            workers_ready,
            worker_bytes,
            handler_ms,
            peak_buffered_bytes,
        } = report;
        let mut times = BTreeMap::<u64, f64>::new();
        let (mut out_bytes, mut in_bytes) = (0, 0);
        for source in sources {
            let source = source.lock().unwrap();
            out_bytes += source.out_bytes;
            in_bytes += source.in_bytes;
            for &(serial, ms) in &source.frames {
                *times.entry(serial).or_default() += ms;
            }
        }
        let mut times: Vec<_> = times.into_values().collect();
        times.sort_unstable_by(f64::total_cmp);
        let percentile = |percent: usize| {
            if times.is_empty() {
                return "null".to_owned();
            }
            times[(times.len() * percent).div_ceil(100) - 1].to_string()
        };
        write!(
            out,
            r#","wire":{{"ms":{{"p50":{},"p95":{}}},"outBytes":{out_bytes},"inBytes":{in_bytes},"workersReady":{workers_ready},"workerBytes":{worker_bytes},"handlerMs":{handler_ms},"peakBufferedBytes":{peak_buffered_bytes}}}"#,
            percentile(50),
            percentile(95)
        )
        .unwrap();
    }
}
