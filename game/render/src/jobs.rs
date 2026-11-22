//! Bounded presentation jobs (LLP 1182.000 D9).
//! `Job::output_bytes` measures completed output held for installation.
//! `Full<J>` returns ownership of the job so the hook can retry submission.
//! Job hooks declare `ALLOWANCE > 0`.
//! `Hooks::jobs` exposes additive `JobsReport` counters.
//! Native workers use the platform's default priority because `std` exposes no priority control.

use crate::{FrameView, HookGpu, HookTime};
pub use remote::{
    Attempt, Outcome, Posting, QueueId, ResultRead, Transport, TransportEvent, TransportReport,
    TransportState, WorkerId,
};
use remote::{RemoteQueue, TransportHandle, WireFrame, WireMetrics};
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use wire::WireCodec;
pub use wire::{run_remote_job, RemoteJob, RunMark, WireError, WireJob};

/// Install the owner thread's transport before constructing any hooks.
///
/// This entry point is for generated GPU module code. Code holding the
/// transport can read its buffers, so the generated module installs it and
/// keeps it out of authored hooks. A second installation on the thread fails.
/// Queues created before installation retain their original lane policy.
#[doc(hidden)]
pub fn install_module_transport<H: crate::Hooks>(
    transport: impl Transport,
) -> Result<(), WireError> {
    remote::install_transport::<H>(transport)
}

#[cfg(not(target_arch = "wasm32"))]
mod pool;

mod remote;
mod wire;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod world_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod remote_tests;

/// Presentation work owned by a job. Its result depends only on the inputs moved into it.
pub trait Job: Send + 'static {
    /// Data delivered to the hook during preparation.
    type Output: Send + 'static;
    /// Bytes held while queued.
    fn bytes(&self) -> usize;
    /// Bytes retained by a completed output until taken.
    fn output_bytes(output: &Self::Output) -> usize;
    /// Compute a result, optionally checking for cooperative cancellation.
    fn run(self, cancel: &Cancel) -> Result<Self::Output, JobError>;
}

/// A cooperative cancellation signal for one job.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// Whether this job has been cancelled.
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn set(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// A job's own failure message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError(pub String);

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for JobError {}

/// The queued byte limit was reached. The original job is available for retry.
#[derive(Debug)]
pub struct Full<J>(pub J);

/// Queue, concurrency and installation limits for one job lane.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Input bytes, with room for one oversized job in an empty queue.
    pub queued_bytes: usize,
    /// Maximum native jobs or remote worker batches in flight, at least one.
    pub running: usize,
    /// Workers stop starting work once waiting results reach this many bytes.
    pub completed_bytes: usize,
    /// Maximum results per frame, raised to one when set to zero and bounded by the allowance.
    pub take_per_frame: usize,
}

/// A current result, including a failure returned by the job.
#[derive(Debug)]
pub struct Done<O> {
    /// The job's key.
    pub key: u64,
    /// The generation supplied at submission.
    pub generation: u64,
    /// Computed output or the job's error.
    pub result: Result<O, JobError>,
}

/// The executor selected for preparation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lane {
    /// Native workers.
    Pool,
    /// Web workers.
    Worker,
    /// The frame thread.
    #[default]
    Inline,
}

impl Lane {
    /// The diagnostics field value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pool => "pool",
            Self::Worker => "worker",
            Self::Inline => "inline",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Counts {
    done: u64,
    stale: u64,
    failed: u64,
}

/// Current queue gauges and cumulative result counts for diagnostics.
#[derive(Clone, Default)]
pub struct JobsReport {
    /// Jobs waiting to start.
    pub queued: usize,
    /// Input bytes held, including remote jobs awaiting delivery.
    pub queued_bytes: usize,
    /// Jobs out that the hook can still receive on the worker lane.
    /// On the native pool this also counts jobs winding down after cancellation.
    pub running: usize,
    /// Successful results taken.
    pub done: u64,
    /// Results discarded after a job was replaced, cancelled or cleared.
    /// On the worker lane this counts only results the queue saw in a snapshot.
    pub stale: u64,
    /// Failure results taken.
    pub failed: u64,
    /// The selected lane. A sum prefers worker, then pool, then inline.
    pub lane: Lane,
    sources: Vec<Arc<Mutex<Counts>>>,
    wire: Vec<Arc<Mutex<WireMetrics>>>,
}

impl std::fmt::Debug for JobsReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobsReport")
            .field("queued", &self.queued)
            .field("queued_bytes", &self.queued_bytes)
            .field("running", &self.running)
            .field("done", &self.done)
            .field("stale", &self.stale)
            .field("failed", &self.failed)
            .field("lane", &self.lane)
            .finish()
    }
}

impl std::ops::Add for JobsReport {
    type Output = Self;

    fn add(mut self, other: Self) -> Self {
        self.queued += other.queued;
        self.queued_bytes += other.queued_bytes;
        self.running += other.running;
        self.done += other.done;
        self.stale += other.stale;
        self.failed += other.failed;
        if other.lane == Lane::Worker || self.lane == Lane::Inline {
            self.lane = other.lane;
        }
        self.sources.extend(other.sources);
        self.wire.extend(other.wire);
        self
    }
}

impl JobsReport {
    pub(crate) fn reset_counters(&mut self) {
        for counts in &self.sources {
            *counts.lock().unwrap() = Counts::default();
        }
        for wire in &self.wire {
            *wire.lock().unwrap() = WireMetrics::default();
        }
        self.done = 0;
        self.stale = 0;
        self.failed = 0;
    }

    pub(crate) fn append_wire(&self, out: &mut String) {
        if self.wire.is_empty() {
            return;
        }
        if let Some(report) = remote::installed_report() {
            WireMetrics::append(out, &self.wire, report);
        }
    }
}

// Ascending serial breaks ties among jobs with the same priority.
type Order = (Reverse<i32>, u64);

struct Ticket {
    key: u64,
    generation: u64,
}

struct Queued<J> {
    ticket: Ticket,
    bytes: usize,
    job: J,
}

struct Completed<O> {
    done: Done<O>,
    bytes: usize,
}

struct State<J: Job> {
    latest: BTreeMap<u64, (u64, Order)>,
    queue: BTreeMap<Order, Queued<J>>,
    wire_queue: BTreeMap<Order, Queued<J>>,
    queued_bytes: usize,
    running: BTreeMap<Order, Cancel>,
    completed: BTreeMap<Order, Completed<J::Output>>,
    completed_bytes: u64,
    retained_peak: usize,
    serial: u64,
    lane: Lane,
    #[cfg(not(target_arch = "wasm32"))]
    closed: bool,
}

impl<J: Job> State<J> {
    fn retained_bytes(&self) -> usize {
        self.queued_bytes
            .saturating_add(usize::try_from(self.completed_bytes).unwrap_or(usize::MAX))
    }

    fn record_retained_peak(&mut self) {
        self.retained_peak = self.retained_peak.max(self.retained_bytes());
    }

    fn queued(&self) -> usize {
        self.queue.len() + self.wire_queue.len()
    }

    fn first_queued(&self) -> Option<&Order> {
        self.queue
            .first_key_value()
            .map(|(order, _)| order)
            .into_iter()
            .chain(self.wire_queue.first_key_value().map(|(order, _)| order))
            .min()
    }

    fn pop(&mut self) -> Option<(Order, Queued<J>, bool)> {
        let order = *self.first_queued()?;
        let (queued, wire) = self.remove_queued(order);
        Some((order, queued, wire))
    }

    fn remove_queued(&mut self, order: Order) -> (Queued<J>, bool) {
        let (queued, wire) = if let Some(queued) = self.queue.remove(&order) {
            (queued, false)
        } else {
            (self.wire_queue.remove(&order).unwrap(), true)
        };
        self.queued_bytes -= queued.bytes;
        (queued, wire)
    }

    fn cancel_running(&self) {
        for cancel in self.running.values() {
            cancel.set();
        }
    }

    fn invalidate(
        &mut self,
        key: u64,
        remote: Option<&mut RemoteQueue<J>>,
        counts: &Mutex<Counts>,
    ) -> Option<Queued<J>> {
        let (_, order) = self.latest.remove(&key)?;
        if let Some(cancel) = self.running.get(&order) {
            cancel.set();
        }
        if let Some(queued) = self
            .queue
            .remove(&order)
            .or_else(|| self.wire_queue.remove(&order))
        {
            self.queued_bytes -= queued.bytes;
            return Some(queued);
        }
        let (queued, stale) = remote?.invalidate(self, order);
        if stale {
            counts.lock().unwrap().stale += 1;
        }
        queued
    }
}

struct Shared<J: Job> {
    state: Mutex<State<J>>,
    codec: Option<WireCodec<J>>,
    counts: Arc<Mutex<Counts>>,
    limits: Limits,
    #[cfg(not(target_arch = "wasm32"))]
    changed: std::sync::Condvar,
}

impl<J: Job> Shared<J> {
    fn run(&self, job: J, cancel: &Cancel, _wire: Option<bool>) -> Result<J::Output, JobError> {
        #[cfg(debug_assertions)]
        if let Some(codec) = &self.codec {
            if _wire.unwrap_or_else(|| (codec.wire)(&job)) {
                return codec.roundtrip(job, cancel);
            }
        }
        job.run(cancel)
    }
}

/// An ordered presentation queue with a lazy native pool and a deterministic inline lane.
///
/// Dropping a native pool cancels and joins its workers. A running job should poll
/// its `Cancel` or finish quickly. Dropping a remote queue retires it and returns
/// immediately. The transport owns its workers.
pub struct Jobs<J: Job> {
    shared: Arc<Shared<J>>,
    remote: Option<Mutex<RemoteQueue<J>>>,
    prepare_serial: u64,
    yielded: usize,
    late_unit_spent: bool,
    #[cfg(not(target_arch = "wasm32"))]
    workers: Vec<std::thread::JoinHandle<()>>,
    #[cfg(not(target_arch = "wasm32"))]
    pool_unavailable: bool,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    fail_pool_start: bool,
}

impl<J: WireJob> Jobs<J> {
    /// Create a queue that can use the installed transport for registered jobs.
    pub fn remote(limits: Limits) -> Self {
        Self::remote_with(limits, remote::installed())
    }

    fn remote_with(limits: Limits, transport: Option<Arc<TransportHandle>>) -> Self {
        let remote = transport.as_ref().and_then(|handle| handle.queue::<J>());
        Self::with_codec(limits, Some(WireCodec::new()), remote, transport.is_some())
    }
}

impl<J: Job> Jobs<J> {
    /// Create a queue. Workers start when a live frame selects the pool and work is queued.
    pub fn new(limits: Limits) -> Self {
        Self::with_codec(limits, None, None, false)
    }

    fn with_codec(
        limits: Limits,
        codec: Option<WireCodec<J>>,
        remote: Option<RemoteQueue<J>>,
        _transport_installed: bool,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    latest: BTreeMap::new(),
                    queue: BTreeMap::new(),
                    wire_queue: BTreeMap::new(),
                    queued_bytes: 0,
                    running: BTreeMap::new(),
                    completed: BTreeMap::new(),
                    completed_bytes: 0,
                    retained_peak: 0,
                    serial: 0,
                    lane: Lane::Inline,
                    #[cfg(not(target_arch = "wasm32"))]
                    closed: false,
                }),
                codec,
                counts: Arc::default(),
                limits: Limits {
                    running: limits.running.max(1),
                    take_per_frame: limits.take_per_frame.max(1),
                    ..limits
                },
                #[cfg(not(target_arch = "wasm32"))]
                changed: std::sync::Condvar::new(),
            }),
            remote: remote.map(Mutex::new),
            prepare_serial: 0,
            yielded: 0,
            late_unit_spent: false,
            #[cfg(not(target_arch = "wasm32"))]
            workers: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            pool_unavailable: _transport_installed,
            #[cfg(all(test, not(target_arch = "wasm32")))]
            fail_pool_start: false,
        }
    }

    /// Queue a job, superseding the current work for its key on success.
    pub fn submit(
        &mut self,
        key: u64,
        generation: u64,
        priority: i32,
        job: J,
    ) -> Result<(), Full<J>> {
        let bytes = job.bytes();
        let mut state = self.shared.state.lock().unwrap();
        let mut remote = self.remote.as_mut().map(|remote| remote.get_mut().unwrap());
        let replacing = state.latest.get(&key).and_then(|&(_, order)| {
            state
                .queue
                .get(&order)
                .or_else(|| state.wire_queue.get(&order))
                .map(|queued| queued.bytes)
                .or_else(|| remote.as_ref()?.wanted_bytes(order))
        });
        let retained = state.queued_bytes - replacing.unwrap_or_default();
        let pending = state.queued() + remote.as_deref().map_or(0, RemoteQueue::pending);
        let empty = pending == usize::from(replacing.is_some());
        if !empty
            && (retained > self.shared.limits.queued_bytes
                || bytes > self.shared.limits.queued_bytes - retained)
        {
            return Err(Full(job));
        }
        let wire = remote
            .as_deref()
            .is_some_and(|remote| remote.accepts_wire() || cfg!(debug_assertions))
            && (self.shared.codec.as_ref().unwrap().wire)(&job);
        let replaced = state.invalidate(key, remote.as_deref_mut(), &self.shared.counts);
        let order = (Reverse(priority), state.serial);
        state.serial += 1;
        state.latest.insert(key, (generation, order));
        let queue = if wire {
            &mut state.wire_queue
        } else {
            &mut state.queue
        };
        queue.insert(
            order,
            Queued {
                ticket: Ticket { key, generation },
                bytes,
                job,
            },
        );
        state.queued_bytes += bytes;
        state.record_retained_peak();
        if let Some(remote) = remote {
            remote.withdraw_if_empty(&state);
        }
        drop(state);
        drop(replaced);
        #[cfg(not(target_arch = "wasm32"))]
        self.wake_pool();
        Ok(())
    }

    /// Invalidate the current job and signal cancellation to any native worker running it.
    pub fn cancel(&mut self, key: u64) {
        let mut state = self.shared.state.lock().unwrap();
        let mut remote = self.remote.as_mut().map(|remote| remote.get_mut().unwrap());
        let removed = state.invalidate(key, remote.as_deref_mut(), &self.shared.counts);
        if let Some(remote) = remote {
            remote.withdraw_if_empty(&state);
        }
        drop(state);
        drop(removed);
    }

    /// Invalidate all work, retaining cumulative diagnostics until `perf_reset`.
    pub fn clear(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.latest.clear();
        state.cancel_running();
        state.queued_bytes = 0;
        state.completed_bytes = 0;
        state.retained_peak = 0;
        let queued = std::mem::take(&mut state.queue);
        let wire = std::mem::take(&mut state.wire_queue);
        let completed = std::mem::take(&mut state.completed);
        let stale = completed.len()
            + self
                .remote
                .as_mut()
                .map_or(0, |remote| remote.get_mut().unwrap().clear());
        self.shared.counts.lock().unwrap().stale += stale as u64;
        drop(state);
        drop((queued, wire, completed));
    }

    /// Select the native lane for an advancing frame. Transport queues and held
    /// frames report the lane selected by the last take.
    pub fn lane(&mut self, time: &HookTime) -> Lane {
        if time.allowance == 0 || self.remote.is_some() {
            return self.shared.state.lock().unwrap().lane;
        }
        #[cfg(target_arch = "wasm32")]
        {
            Lane::Inline
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let lane = if time.seekable || self.pool_unavailable {
                Lane::Inline
            } else {
                Lane::Pool
            };
            self.shared.state.lock().unwrap().lane = lane;
            if lane == Lane::Pool {
                self.wake_pool();
            } else {
                self.shared.changed.notify_all();
            }
            self.shared.state.lock().unwrap().lane
        }
    }

    /// Take ready results by priority and submission order with the `HookGpu` from `prepare`.
    /// Takes from this queue share the hook's allowance. A late unit spent on
    /// a remote queue stays spent until the next prepare.
    /// `idleWithWork` counts live, advancing prepares whose first take finds
    /// a wire job queued and a ready worker with no batch, before posting.
    ///
    /// The iterator borrows the frame's GPU access and view. Keep GPU handles out of jobs.
    ///
    /// ```compile_fail
    /// use exact_game_render::{FrameView, HookGpu, Job, Jobs, Take};
    /// fn escape<'a, J: Job>(
    ///     jobs: &'a mut Jobs<J>, gpu: &HookGpu<'_>, view: &FrameView<'_>,
    /// ) -> Take<'a, J> {
    ///     jobs.take(gpu, view)
    /// }
    /// ```
    pub fn take<'a>(&'a mut self, _gpu: &'a HookGpu<'_>, view: &'a FrameView<'_>) -> Take<'a, J> {
        self.take_with_time(&view.time)
    }

    pub(crate) fn take_with_time(&mut self, time: &HookTime) -> Take<'_, J> {
        let mut start_workers = false;
        let mut transport_lane = None;
        if time.allowance == 0 {
            if self.remote.is_some() {
                transport_lane = Some(self.shared.state.lock().unwrap().lane);
            }
        } else {
            let first_take = self.prepare_serial != time.serial;
            if first_take {
                self.prepare_serial = time.serial;
                self.yielded = 0;
                self.late_unit_spent = false;
            }
            if let Some(remote) = &mut self.remote {
                let remote = remote.get_mut().unwrap();
                let mut state = self.shared.state.lock().unwrap();
                state.lane = if remote.begin(&mut state, time, first_take) {
                    Lane::Worker
                } else {
                    Lane::Inline
                };
                start_workers =
                    !time.seekable && remote.needs_start() && !state.wire_queue.is_empty();
                transport_lane = Some(state.lane);
            } else {
                let _lane = self.lane(time);
                #[cfg(not(target_arch = "wasm32"))]
                if _lane == Lane::Inline {
                    let state = self.shared.state.lock().unwrap();
                    drop(
                        self.shared
                            .changed
                            .wait_while(state, |state| !state.running.is_empty())
                            .unwrap(),
                    );
                }
            }
        };
        Take {
            limit: self
                .shared
                .limits
                .take_per_frame
                .min(time.allowance as usize),
            jobs: self,
            time: *time,
            transport_lane,
            start_workers,
            wire: WireFrame::default(),
            frame: PhantomData,
        }
    }

    /// Queued, running and completed jobs still held by the lane.
    pub fn pending(&self) -> usize {
        let state = self.shared.state.lock().unwrap();
        state.queued()
            + state.running.len()
            + state.completed.len()
            + self
                .remote
                .as_ref()
                .map_or(0, |remote| remote.lock().unwrap().pending())
    }

    /// Current queued and completed bytes, and their combined peak since the last sample.
    pub fn sample_retained_bytes(&mut self) -> (usize, usize) {
        let mut state = self.shared.state.lock().unwrap();
        let current = state.retained_bytes();
        let peak = std::mem::replace(&mut state.retained_peak, current);
        (current, peak)
    }

    /// Snapshot of the current gauges and cumulative result counts.
    pub fn report(&self) -> JobsReport {
        let state = self.shared.state.lock().unwrap();
        let Counts {
            done,
            stale,
            failed,
        } = *self.shared.counts.lock().unwrap();
        let remote = self.remote.as_ref().map(|remote| remote.lock().unwrap());
        JobsReport {
            queued: state.queued(),
            queued_bytes: state.queued_bytes,
            running: state.running.len() + remote.as_deref().map_or(0, RemoteQueue::out),
            done,
            stale,
            failed,
            lane: state.lane,
            sources: vec![self.shared.counts.clone()],
            wire: remote.iter().map(|remote| remote.metrics.clone()).collect(),
        }
    }
}

impl<J: Job> Drop for Jobs<J> {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.remote.is_some() {
                return;
            }
            let mut state = self.shared.state.lock().unwrap();
            state.closed = true;
            state.cancel_running();
            drop(state);
            self.shared.changed.notify_all();
            for worker in self.workers.drain(..) {
                let _ = worker.join();
            }
        }
    }
}

/// Results held for preparation. Inline work runs only as this iterator advances.
///
/// Drop the take before using the queue again. A worker take can post its batch
/// when it drops, so its borrow lasts until then.
pub struct Take<'a, J: Job> {
    jobs: &'a mut Jobs<J>,
    time: HookTime,
    limit: usize,
    transport_lane: Option<Lane>,
    start_workers: bool,
    wire: WireFrame,
    frame: PhantomData<(&'a HookGpu<'a>, &'a FrameView<'a>)>,
}

impl<J: Job> Take<'_, J> {
    /// The live native lane or the transport lane selected at the start of this take.
    pub fn lane(&self) -> Lane {
        self.transport_lane
            .unwrap_or_else(|| self.jobs.shared.state.lock().unwrap().lane)
    }

    /// Queue more work without opening another take for this frame.
    /// Inline jobs can run when a take with allowance advances. On the worker
    /// lane, jobs that take the wire can post when the take drops.
    pub fn submit(
        &mut self,
        key: u64,
        generation: u64,
        priority: i32,
        job: J,
    ) -> Result<(), Full<J>> {
        self.jobs.submit(key, generation, priority, job)?;
        if self.time.allowance > 0 && !self.time.seekable {
            self.start_workers |= self
                .jobs
                .remote
                .as_mut()
                .is_some_and(|remote| remote.get_mut().unwrap().needs_start())
                && !self.jobs.shared.state.lock().unwrap().wire_queue.is_empty();
        }
        Ok(())
    }

    /// Spend this queue's late unit on work the hook did in this prepare.
    /// Mark it on a worker take to stop delivery and encoding past the deadline
    /// for the rest of this prepare, even if the lane ends.
    pub fn mark_late_unit_spent(&mut self) {
        if self.time.allowance > 0 && self.transport_lane == Some(Lane::Worker) {
            self.jobs.late_unit_spent = true;
        }
    }

    /// Sample retained bytes before transferring a result into resident storage.
    pub fn sample_retained_bytes(&mut self) -> (usize, usize) {
        self.jobs.sample_retained_bytes()
    }

    fn past_deadline(&self) -> bool {
        let spent = self.jobs.late_unit_spent
            || (self.transport_lane != Some(Lane::Worker) && self.jobs.yielded > 0);
        spent && self.time.deadline.is_some_and(|deadline| deadline.passed())
    }

    fn result(&mut self) -> Option<Done<J::Output>> {
        if let Some(remote) = &mut self.jobs.remote {
            let remote = remote.get_mut().unwrap();
            if self.transport_lane == Some(Lane::Worker) || remote.has_arrivals() {
                return remote.deliver(&self.jobs.shared, &mut self.wire);
            }
        }
        let mut state = self.jobs.shared.state.lock().unwrap();
        while let Some((&order, _)) = state.completed.first_key_value() {
            if state.lane == Lane::Inline
                && state.first_queued().is_some_and(|&queued| queued < order)
            {
                break;
            }
            let completed = state.completed.pop_first().unwrap().1;
            state.completed_bytes -= completed.bytes as u64;
            #[cfg(not(target_arch = "wasm32"))]
            self.jobs.shared.changed.notify_all();
            let done = completed.done;
            if state.latest.get(&done.key) == Some(&(done.generation, order)) {
                state.latest.remove(&done.key);
                return Some(done);
            }
            self.jobs.shared.counts.lock().unwrap().stale += 1;
            drop(state);
            drop(done);
            if self.past_deadline() {
                return None;
            }
            state = self.jobs.shared.state.lock().unwrap();
        }
        if state.lane == Lane::Pool {
            return None;
        }
        let (_, Queued { ticket, job, .. }, wire) = state.pop()?;
        state.latest.remove(&ticket.key);
        drop(state);
        Some(Done {
            key: ticket.key,
            generation: ticket.generation,
            result: self.jobs.shared.run(
                job,
                &Cancel::default(),
                self.transport_lane.map(|_| wire),
            ),
        })
    }
}

impl<J: Job> Iterator for Take<'_, J> {
    type Item = Done<J::Output>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.jobs.yielded >= self.limit || self.past_deadline() {
            return None;
        }
        let Some(done) = self.result() else {
            self.limit = self.jobs.yielded;
            return None;
        };
        self.jobs.yielded += 1;
        if self.transport_lane.is_some()
            && self.time.deadline.is_some_and(|deadline| deadline.passed())
        {
            self.jobs.late_unit_spent = true;
        }
        let mut counts = self.jobs.shared.counts.lock().unwrap();
        if done.result.is_ok() {
            counts.done += 1;
        } else {
            counts.failed += 1;
        }
        Some(done)
    }
}

impl<J: Job> Drop for Take<'_, J> {
    fn drop(&mut self) {
        if self.time.allowance == 0 {
            return;
        }
        let Some(remote) = &mut self.jobs.remote else {
            return;
        };
        let remote = remote.get_mut().unwrap();
        if self.start_workers {
            remote.start();
        }
        let shared = &self.jobs.shared;
        let mut state = shared.state.lock().unwrap();
        if self.transport_lane == Some(Lane::Worker) {
            remote.post(
                &mut state,
                shared.codec.as_ref().unwrap(),
                shared.limits,
                &self.time,
                &mut self.jobs.late_unit_spent,
                &mut self.wire,
            );
            remote
                .metrics
                .lock()
                .unwrap()
                .record(self.time.serial, std::mem::take(&mut self.wire));
        }
    }
}
