//! Bounded presentation jobs (LLP 1182.000 D9).
//! `Job::output_bytes` measures completed output held for installation.
//! `Full<J>` returns ownership of the job so the hook can retry submission.
//! Job hooks declare `ALLOWANCE > 0`.
//! `Hooks::jobs` exposes additive `JobsReport` counters.
//! Native workers use the platform's default priority because `std` exposes no priority control.

use crate::{FrameView, HookGpu, HookTime};
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[cfg(not(target_arch = "wasm32"))]
mod pool;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod world_tests;

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
    /// Queued bytes, with room for one oversized job in an empty queue.
    pub queued_bytes: usize,
    /// Maximum jobs running concurrently, raised to one when set to zero.
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
    /// The frame thread.
    #[default]
    Inline,
}

impl Lane {
    /// The diagnostics field value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pool => "pool",
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
#[derive(Clone, Debug, Default)]
pub struct JobsReport {
    /// Jobs waiting to start.
    pub queued: usize,
    /// Input bytes held in the queue.
    pub queued_bytes: usize,
    /// Jobs currently running, including jobs winding down after cancellation.
    pub running: usize,
    /// Successful results taken.
    pub done: u64,
    /// Results discarded after a job was replaced, cancelled or cleared.
    pub stale: u64,
    /// Failure results taken.
    pub failed: u64,
    /// The selected lane. A mixed sum reports the pool if either uses it.
    pub lane: Lane,
    sources: Vec<Arc<Mutex<Counts>>>,
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
        if other.lane == Lane::Pool {
            self.lane = Lane::Pool;
        }
        self.sources.extend(other.sources);
        self
    }
}

impl JobsReport {
    pub(crate) fn reset_counters(&mut self) {
        for counts in &self.sources {
            *counts.lock().unwrap() = Counts::default();
        }
        self.done = 0;
        self.stale = 0;
        self.failed = 0;
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
    queued_bytes: usize,
    running: BTreeMap<Order, Cancel>,
    completed: BTreeMap<Order, Completed<J::Output>>,
    completed_bytes: usize,
    retained_peak: usize,
    serial: u64,
    lane: Lane,
    #[cfg(not(target_arch = "wasm32"))]
    closed: bool,
}

impl<J: Job> State<J> {
    fn retained_bytes(&self) -> usize {
        self.queued_bytes.saturating_add(self.completed_bytes)
    }

    fn record_retained_peak(&mut self) {
        self.retained_peak = self.retained_peak.max(self.retained_bytes());
    }

    fn pop(&mut self) -> Option<(Order, Queued<J>)> {
        let (order, queued) = self.queue.pop_first()?;
        self.queued_bytes -= queued.bytes;
        Some((order, queued))
    }

    fn cancel_running(&self) {
        for cancel in self.running.values() {
            cancel.set();
        }
    }

    fn invalidate(&mut self, key: u64) -> Option<Queued<J>> {
        let (_, order) = self.latest.remove(&key)?;
        if let Some(cancel) = self.running.get(&order) {
            cancel.set();
        }
        let queued = self.queue.remove(&order)?;
        self.queued_bytes -= queued.bytes;
        Some(queued)
    }
}

struct Shared<J: Job> {
    state: Mutex<State<J>>,
    counts: Arc<Mutex<Counts>>,
    limits: Limits,
    #[cfg(not(target_arch = "wasm32"))]
    changed: std::sync::Condvar,
}

/// An ordered presentation queue with a lazy native pool and a deterministic inline lane.
///
/// Dropping the queue cancels and joins its workers. A running job should poll its
/// `Cancel` or finish quickly, since drop waits for it to return.
pub struct Jobs<J: Job> {
    shared: Arc<Shared<J>>,
    prepare_serial: u64,
    yielded: usize,
    #[cfg(not(target_arch = "wasm32"))]
    workers: Vec<std::thread::JoinHandle<()>>,
    #[cfg(not(target_arch = "wasm32"))]
    pool_unavailable: bool,
}

impl<J: Job> Jobs<J> {
    /// Create a queue. Workers start when a live frame selects the pool and work is queued.
    pub fn new(limits: Limits) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    latest: BTreeMap::new(),
                    queue: BTreeMap::new(),
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
                counts: Arc::default(),
                limits: Limits {
                    running: limits.running.max(1),
                    take_per_frame: limits.take_per_frame.max(1),
                    ..limits
                },
                #[cfg(not(target_arch = "wasm32"))]
                changed: std::sync::Condvar::new(),
            }),
            prepare_serial: 0,
            yielded: 0,
            #[cfg(not(target_arch = "wasm32"))]
            workers: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            pool_unavailable: false,
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
        let replacing = state
            .latest
            .get(&key)
            .and_then(|(_, order)| state.queue.get(order));
        let retained = state.queued_bytes - replacing.map_or(0, |queued| queued.bytes);
        let empty = state.queue.len() == usize::from(replacing.is_some());
        if !empty
            && (retained > self.shared.limits.queued_bytes
                || bytes > self.shared.limits.queued_bytes - retained)
        {
            return Err(Full(job));
        }
        let replaced = state.invalidate(key);
        let order = (Reverse(priority), state.serial);
        state.serial += 1;
        state.latest.insert(key, (generation, order));
        state.queue.insert(
            order,
            Queued {
                ticket: Ticket { key, generation },
                bytes,
                job,
            },
        );
        state.queued_bytes += bytes;
        state.record_retained_peak();
        drop(state);
        drop(replaced);
        #[cfg(not(target_arch = "wasm32"))]
        self.wake_pool();
        Ok(())
    }

    /// Remove a queued job and cancel any running job for its key.
    pub fn cancel(&mut self, key: u64) {
        let removed = self.shared.state.lock().unwrap().invalidate(key);
        drop(removed);
    }

    /// Invalidate all work, retaining cumulative diagnostics until `perf_reset`.
    pub fn clear(&mut self) {
        self.discard_all();
    }

    fn discard_all(&self) {
        let mut state = self.shared.state.lock().unwrap();
        state.latest.clear();
        state.cancel_running();
        state.queued_bytes = 0;
        state.completed_bytes = 0;
        state.retained_peak = 0;
        let queued = std::mem::take(&mut state.queue);
        let completed = std::mem::take(&mut state.completed);
        self.shared.counts.lock().unwrap().stale += completed.len() as u64;
        drop(state);
        drop((queued, completed));
    }

    /// Select the lane for an advancing frame. A held frame reports the last lane.
    pub fn lane(&mut self, time: &HookTime) -> Lane {
        if time.allowance == 0 {
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
    /// Takes from this queue share the hook's allowance and may deliver the first result past the deadline.
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
        if time.allowance > 0 {
            if self.prepare_serial != time.serial {
                self.prepare_serial = time.serial;
                self.yielded = 0;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if self.lane(time) == Lane::Inline {
                let state = self.shared.state.lock().unwrap();
                drop(
                    self.shared
                        .changed
                        .wait_while(state, |state| !state.running.is_empty())
                        .unwrap(),
                );
            }
        }
        Take {
            limit: self
                .shared
                .limits
                .take_per_frame
                .min(time.allowance as usize),
            jobs: self,
            time: *time,
            frame: PhantomData,
        }
    }

    /// Queued, running and completed jobs still held by the lane.
    pub fn pending(&self) -> usize {
        let state = self.shared.state.lock().unwrap();
        state.queue.len() + state.running.len() + state.completed.len()
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
        JobsReport {
            queued: state.queue.len(),
            queued_bytes: state.queued_bytes,
            running: state.running.len(),
            done,
            stale,
            failed,
            lane: state.lane,
            sources: vec![self.shared.counts.clone()],
        }
    }
}

impl<J: Job> Drop for Jobs<J> {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
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
pub struct Take<'a, J: Job> {
    jobs: &'a mut Jobs<J>,
    time: HookTime,
    limit: usize,
    frame: PhantomData<(&'a HookGpu<'a>, &'a FrameView<'a>)>,
}

impl<J: Job> Take<'_, J> {
    /// The executor selected for this prepare.
    pub fn lane(&self) -> Lane {
        self.jobs.shared.state.lock().unwrap().lane
    }

    /// Queue more work without opening another take for this frame.
    /// Submit before asking for the next result to run it inline in this take.
    pub fn submit(
        &mut self,
        key: u64,
        generation: u64,
        priority: i32,
        job: J,
    ) -> Result<(), Full<J>> {
        self.jobs.submit(key, generation, priority, job)
    }

    /// Sample retained bytes before transferring a result into resident storage.
    pub fn sample_retained_bytes(&mut self) -> (usize, usize) {
        self.jobs.sample_retained_bytes()
    }

    fn past_deadline(&self) -> bool {
        self.jobs.yielded > 0 && self.time.deadline.is_some_and(|deadline| deadline.passed())
    }

    fn result(&self) -> Option<Done<J::Output>> {
        let mut state = self.jobs.shared.state.lock().unwrap();
        while let Some((&order, _)) = state.completed.first_key_value() {
            if state.lane == Lane::Inline
                && state
                    .queue
                    .first_key_value()
                    .is_some_and(|(&queued, _)| queued < order)
            {
                break;
            }
            let completed = state.completed.pop_first().unwrap().1;
            state.completed_bytes -= completed.bytes;
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
        let (_, Queued { ticket, job, .. }) = state.pop()?;
        state.latest.remove(&ticket.key);
        drop(state);
        Some(Done {
            key: ticket.key,
            generation: ticket.generation,
            result: job.run(&Cancel::default()),
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
        let mut counts = self.jobs.shared.counts.lock().unwrap();
        if done.result.is_ok() {
            counts.done += 1;
        } else {
            counts.failed += 1;
        }
        Some(done)
    }
}
