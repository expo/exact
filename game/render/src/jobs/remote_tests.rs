use super::remote::TransportHandle;
use super::*;
use crate::hooks::Deadline;
use std::collections::BTreeSet;
use std::sync::atomic::AtomicU64;

#[path = "remote_policy_tests.rs"]
mod policy;
#[path = "remote_surface_tests.rs"]
mod surface;

const LIMITS: Limits = Limits {
    queued_bytes: 32,
    running: 1,
    completed_bytes: 64,
    take_per_frame: 8,
};

#[derive(Debug)]
struct Work {
    value: u8,
    bytes: u8,
    local: bool,
    required: bool,
    wire_calls: Option<Arc<AtomicU64>>,
}

impl Work {
    fn new(value: u8) -> Self {
        Self {
            value,
            bytes: 1,
            local: false,
            required: false,
            wire_calls: None,
        }
    }
}

impl Job for Work {
    type Output = u8;

    fn bytes(&self) -> usize {
        usize::from(self.bytes)
    }

    fn output_bytes(_: &u8) -> usize {
        1
    }

    fn run(self, _: &Cancel) -> Result<u8, JobError> {
        if self.required != (self.value == 255) {
            return Err(JobError("missing required field".into()));
        }
        Ok(self.value)
    }
}

impl WireJob for Work {
    const NAME: &'static str = "test.work";

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend([self.value, self.bytes]);
    }

    fn decode(bytes: &[u8]) -> Result<Self, JobError> {
        let &[value, count] = bytes else {
            return Err(JobError("bad input".into()));
        };
        Ok(Self {
            bytes: count,
            ..Self::new(value)
        })
    }

    fn encode_output(output: &u8, out: &mut Vec<u8>) {
        out.push(*output);
    }

    fn decode_output(bytes: &[u8]) -> Result<u8, JobError> {
        let &[value] = bytes else {
            return Err(JobError("bad output".into()));
        };
        Ok(value)
    }

    fn wire(&self) -> bool {
        if let Some(calls) = &self.wire_calls {
            calls.fetch_add(1, Ordering::Relaxed);
        }
        !self.local
    }
}

const ENTRIES: &[RemoteJob] = &[RemoteJob::of::<Work>()];

#[derive(Clone, Default)]
struct Script {
    state: Arc<Mutex<ScriptState>>,
    report_reads: Arc<AtomicU64>,
}

#[derive(Default)]
struct ScriptState {
    workers: BTreeSet<WorkerId>,
    next_worker: WorkerId,
    ended: bool,
    starts: usize,
    refuse_post: bool,
    ready_zero: bool,
    now_ms: u64,
    fail_replacement: bool,
    losses: usize,
    end_pending: bool,
    wanted: BTreeSet<Attempt>,
    events: BTreeMap<QueueId, Vec<TransportEvent>>,
    batches: BTreeMap<WorkerId, (QueueId, Vec<Attempt>)>,
    postings: Vec<(QueueId, Vec<Attempt>)>,
    inputs: BTreeMap<Attempt, [u8; 2]>,
    outputs: BTreeMap<Attempt, Vec<u8>>,
    announced: BTreeSet<Attempt>,
    rejected: BTreeSet<WorkerId>,
    worker_bytes: u64,
    handler_ms: f64,
    peak_buffered_bytes: u64,
    copies: Vec<Attempt>,
    reads: Vec<QueueId>,
    forgets: Vec<QueueId>,
    retired: Vec<QueueId>,
    asking: BTreeMap<QueueId, u64>,
    last_served: BTreeMap<QueueId, usize>,
}

impl ScriptState {
    fn add_worker(&mut self) {
        self.next_worker += 1;
        self.workers.insert(self.next_worker);
    }

    fn idle(&self) -> Option<WorkerId> {
        self.idle_workers().next()
    }

    fn idle_workers(&self) -> impl Iterator<Item = WorkerId> + '_ {
        self.workers
            .iter()
            .copied()
            .filter(|id| !self.batches.contains_key(id) && !self.rejected.contains(id))
    }

    fn answer(&mut self, attempt: Attempt, outcome: Outcome, bytes: Vec<u8>) {
        let worker = self.batches.iter_mut().find_map(|(worker, (_, batch))| {
            if self.rejected.contains(worker) {
                return None;
            }
            let index = batch.iter().position(|&id| id == attempt)?;
            batch.remove(index);
            Some((*worker, batch.is_empty()))
        });
        let Some((worker, empty)) = worker else {
            return;
        };
        let queue = self.batches[&worker].0;
        if empty {
            self.batches.remove(&worker);
        }
        let trapped = matches!(outcome, Outcome::Trapped);
        self.handler_ms += 0.25;
        if !trapped {
            self.worker_bytes = 65_536;
        }
        if self.wanted.remove(&attempt) {
            if !trapped {
                self.outputs.insert(attempt, bytes);
                let buffered = self.outputs.values().map(|bytes| bytes.len() as u64).sum();
                self.peak_buffered_bytes = self.peak_buffered_bytes.max(buffered);
            }
            self.events
                .entry(queue)
                .or_default()
                .push(TransportEvent::Arrived { attempt, outcome });
        }
        if trapped {
            self.close_worker(worker);
        }
    }

    fn lose(&mut self, worker: WorkerId) {
        if self.workers.contains(&worker) && !self.rejected.contains(&worker) {
            self.record_loss();
            self.close_worker(worker);
        }
    }

    fn record_loss(&mut self) {
        self.losses += 1;
        self.end_pending |= self.losses >= 3;
    }

    fn close_worker(&mut self, worker: WorkerId) {
        if !self.workers.remove(&worker) {
            return;
        }
        if let Some((queue, mut attempts)) = self.batches.remove(&worker) {
            attempts.retain(|attempt| self.wanted.remove(attempt));
            if !attempts.is_empty() {
                self.events
                    .entry(queue)
                    .or_default()
                    .push(TransportEvent::Lost { worker, attempts });
            }
        }
        if self.end_pending || self.fail_replacement {
            self.end();
        } else {
            self.add_worker();
        }
    }

    fn end(&mut self) {
        self.ended = true;
        for (queue, _) in &self.postings {
            if self.retired.contains(queue) {
                continue;
            }
            let events = self.events.entry(*queue).or_default();
            if !events
                .iter()
                .any(|event| matches!(event, TransportEvent::Ended))
            {
                events.push(TransportEvent::Ended);
            }
        }
        self.outputs.clear();
        self.announced.clear();
        self.workers.clear();
        self.batches.clear();
        self.wanted.clear();
        self.asking.clear();
        self.rejected.clear();
    }

    fn forget_attempt(&mut self, attempt: Attempt) {
        self.wanted.remove(&attempt);
        self.outputs.remove(&attempt);
        self.announced.remove(&attempt);
    }

    fn forget_queue(&mut self, queue: QueueId) {
        let ids: Vec<_> = self
            .postings
            .iter()
            .filter(|(owner, _)| *owner == queue)
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect();
        for attempt in ids {
            self.forget_attempt(attempt);
        }
        self.events.remove(&queue);
        self.asking.remove(&queue);
    }

    fn postable_worker(&self, queue: QueueId) -> Option<WorkerId> {
        if self.end_pending {
            return None;
        }
        let worker = self.idle()?;
        let (&winner, _) = self
            .asking
            .iter()
            .filter(|(_, asked)| self.now_ms - **asked < 250)
            .min_by_key(|(id, _)| (self.last_served.get(id), **id))?;
        (winner == queue).then_some(worker)
    }
}

impl Transport for Script {
    fn start(&self, names: &[&str]) {
        assert_eq!(names, [Work::NAME]);
        self.state.lock().unwrap().starts += 1;
    }

    fn events(&self, queue: QueueId) -> (TransportState, Vec<TransportEvent>) {
        let mut state = self.state.lock().unwrap();
        for worker in std::mem::take(&mut state.rejected) {
            state.close_worker(worker);
        }
        assert!(
            state.wanted.iter().all(|attempt| state
                .batches
                .values()
                .any(|(_, batch)| batch.contains(attempt))),
            "accepted attempt left its worker without a terminal event"
        );
        state.reads.push(queue);
        let events = state.events.remove(&queue).unwrap_or_default();
        let readiness = if state.ended {
            state.forget_queue(queue);
            TransportState::Ended
        } else if state.ready_zero {
            TransportState::Ready(0)
        } else if state.workers.is_empty() {
            TransportState::Starting
        } else {
            TransportState::Ready(state.workers.len())
        };
        for event in &events {
            if let TransportEvent::Arrived {
                attempt,
                outcome: Outcome::Bytes(_),
            } = event
            {
                if state.outputs.contains_key(attempt) {
                    state.announced.insert(*attempt);
                }
            }
        }
        (readiness, events)
    }

    fn can_post(&self, queue: QueueId) -> bool {
        self.state.lock().unwrap().postable_worker(queue).is_some()
    }

    fn busy_workers(&self, queue: QueueId) -> usize {
        self.state
            .lock()
            .unwrap()
            .batches
            .values()
            .filter(|(owner, _)| *owner == queue)
            .count()
    }

    fn set_asking(&self, queue: QueueId, asking: bool) {
        let mut state = self.state.lock().unwrap();
        if asking && !state.ended {
            let now_ms = state.now_ms;
            state.asking.insert(queue, now_ms);
        } else {
            state.asking.remove(&queue);
        }
    }

    fn post(&self, queue: QueueId, jobs: &[Posting]) -> Option<WorkerId> {
        let mut state = self.state.lock().unwrap();
        let worker = state.postable_worker(queue)?;
        if std::mem::take(&mut state.refuse_post) {
            state.lose(worker);
            return None;
        }
        let attempts: Vec<_> = jobs
            .iter()
            .map(|job| {
                assert_eq!(job.name, Work::NAME);
                assert!(state.wanted.insert(job.attempt));
                assert!(state
                    .inputs
                    .insert(job.attempt, job.input.as_slice().try_into().unwrap())
                    .is_none());
                job.attempt
            })
            .collect();
        assert!((1..=16).contains(&attempts.len()));
        state.batches.insert(worker, (queue, attempts.clone()));
        let posted = state.postings.len();
        state.last_served.insert(queue, posted);
        state.postings.push((queue, attempts));
        Some(worker)
    }

    fn read_result(&self, _: QueueId, attempt: Attempt, out: &mut [u8]) -> ResultRead {
        let mut state = self.state.lock().unwrap();
        let announced = state.announced.remove(&attempt);
        let Some(bytes) = state.outputs.remove(&attempt) else {
            return ResultRead::Gone;
        };
        assert!(announced);
        if bytes.len() != out.len() {
            return ResultRead::Unreadable;
        }
        out.copy_from_slice(&bytes);
        state.copies.push(attempt);
        ResultRead::Copied
    }

    fn accept_answer(&self) {
        self.state.lock().unwrap().losses = 0;
    }

    fn reject(&self, worker: WorkerId) {
        let mut state = self.state.lock().unwrap();
        if state.workers.contains(&worker) && state.rejected.insert(worker) {
            state.record_loss();
        }
    }

    fn forget_attempt(&self, queue: QueueId, attempt: Attempt) {
        let mut state = self.state.lock().unwrap();
        state.forget_attempt(attempt);
        if let Some(events) = state.events.get_mut(&queue) {
            events.retain(|event| !matches!(event, TransportEvent::Arrived { attempt: id, .. } if *id == attempt));
        }
    }

    fn forget(&self, queue: QueueId) {
        let mut state = self.state.lock().unwrap();
        state.forgets.push(queue);
        state.forget_queue(queue);
    }

    fn retire(&self, queue: QueueId) {
        let mut state = self.state.lock().unwrap();
        state.retired.push(queue);
        state.forget_queue(queue);
        state.last_served.remove(&queue);
    }

    fn report(&self) -> TransportReport {
        self.report_reads.fetch_add(1, Ordering::Relaxed);
        let state = self.state.lock().unwrap();
        TransportReport {
            workers_ready: state.workers.len(),
            worker_bytes: state.worker_bytes,
            handler_ms: state.handler_ms,
            peak_buffered_bytes: state.peak_buffered_bytes,
        }
    }
}

impl Script {
    fn finish_start(&self, outcomes: &[bool]) {
        let mut state = self.state.lock().unwrap();
        assert_eq!(state.starts, 1);
        for _ in outcomes.iter().filter(|&&ready| ready) {
            state.add_worker();
        }
        if state.workers.is_empty() {
            state.end();
        }
    }

    fn ready(&self, workers: usize) {
        let mut state = self.state.lock().unwrap();
        while state.workers.len() < workers {
            state.add_worker();
        }
    }

    fn posting(&self, index: usize) -> (QueueId, Vec<Attempt>) {
        let batch = self.state.lock().unwrap().postings.get(index).cloned();
        batch.expect("expected a posted batch")
    }

    fn posted_values(&self, ids: &[Attempt]) -> Vec<u8> {
        let state = self.state.lock().unwrap();
        ids.iter().map(|id| state.inputs[id][0]).collect()
    }

    fn lose_batch_with(&self, attempt: Attempt) {
        let mut state = self.state.lock().unwrap();
        let (&worker, _) = state
            .batches
            .iter()
            .find(|(_, (_, batch))| batch.contains(&attempt))
            .unwrap();
        state.lose(worker);
    }

    fn complete(&self, attempt: Attempt) {
        let mut state = self.state.lock().unwrap();
        let input = &state.inputs[&attempt];
        let output = run_remote_job(ENTRIES, Work::NAME, input, |_| {}).unwrap();
        state.answer(attempt, Outcome::Bytes(output.len()), output);
    }
}

fn setup(limits: Limits, ready: usize) -> (Jobs<Work>, Arc<Script>, Arc<TransportHandle>) {
    let script = Arc::new(Script::default());
    script.ready(ready);
    let handle = Arc::new(TransportHandle::new(ENTRIES, script.clone()).unwrap());
    (with_transport(limits, &handle), script, handle)
}

fn with_transport(limits: Limits, handle: &Arc<TransportHandle>) -> Jobs<Work> {
    Jobs::remote_with(limits, Some(handle.clone()))
}

fn time(seekable: bool, allowance: u32) -> HookTime {
    static SERIAL: AtomicU64 = AtomicU64::new(1);
    HookTime {
        serial: SERIAL.fetch_add(1, Ordering::Relaxed),
        seekable,
        allowance,
        ..HookTime::default()
    }
}

fn live() -> HookTime {
    time(false, 8)
}

fn late() -> HookTime {
    HookTime {
        deadline: Some(Deadline::new(crate::perf::Stamp::now(), -1.)),
        ..live()
    }
}

fn submit(jobs: &mut Jobs<Work>, value: u8, priority: i32) {
    jobs.submit(u64::from(value), 0, priority, Work::new(value))
        .unwrap();
}

fn results(jobs: &mut Jobs<Work>, clock: HookTime) -> Vec<u8> {
    jobs.take_with_time(&clock)
        .map(|done| done.result.unwrap())
        .collect()
}

#[test]
fn starts_and_posts_even_after_the_last_result() {
    let (mut jobs, script, _) = setup(LIMITS, 0);
    let mut take = jobs.take_with_time(&live());
    assert_eq!(take.lane(), Lane::Inline);
    assert!(take.next().is_none());
    take.submit(1, 0, 0, Work::new(1)).unwrap();
    assert_eq!(script.state.lock().unwrap().starts, 0);
    drop(take);
    assert_eq!(script.state.lock().unwrap().starts, 1);

    script.ready(1);
    let mut take = jobs.take_with_time(&live());
    assert_eq!(take.lane().as_str(), "worker");
    assert!(take.next().is_none());
    take.submit(2, 0, 10, Work::new(2)).unwrap();
    assert!(script.state.lock().unwrap().postings.is_empty());
    drop(take);
    let (_, ids) = script.posting(0);
    assert_eq!(script.posted_values(&ids), [2, 1]);
    for id in ids {
        script.complete(id);
    }

    let mut take = jobs.take_with_time(&live());
    assert_eq!(take.next().unwrap().result, Ok(2));
    assert_eq!(take.next().unwrap().result, Ok(1));
    assert!(take.next().is_none());
    take.submit(3, 0, 0, Work::new(3)).unwrap();
    take.submit(4, 0, 10, Work::new(4)).unwrap();
    drop(take);
    assert_eq!(script.posted_values(&script.posting(1).1), [4, 3]);
}

#[test]
fn clock_handoff_ignores_arrival_order_and_keeps_submit_credit() {
    for (arrivals, learn) in [
        (&[0, 1, 2][..], false),
        (&[2, 1, 0][..], true),
        (&[1][..], true),
        (&[][..], false),
    ] {
        let (mut jobs, script, _) = setup(
            Limits {
                queued_bytes: 3,
                ..LIMITS
            },
            1,
        );
        for (value, priority) in [(1, 0), (2, 2), (3, 2)] {
            submit(&mut jobs, value, priority);
        }
        drop(jobs.take_with_time(&live()));
        let (queue, ids) = script.posting(0);
        for &index in arrivals {
            script.complete(ids[index]);
        }
        if learn {
            drop(jobs.take_with_time(&live()));
        }
        let reads = script.state.lock().unwrap().reads.len();
        let mut parked = jobs.take_with_time(&time(true, 0));
        assert_eq!(parked.lane(), Lane::Worker);
        assert!(parked.next().is_none());
        drop(parked);
        assert!(script.state.lock().unwrap().forgets.is_empty());
        assert_eq!(script.state.lock().unwrap().reads.len(), reads);
        assert_eq!(jobs.pending(), 3);

        let mut take = jobs.take_with_time(&time(true, 1));
        assert_eq!(take.lane(), Lane::Inline);
        assert!(take.submit(4, 0, 0, Work::new(4)).is_err());
        assert_eq!(take.next().unwrap().result, Ok(2));
        take.submit(4, 0, 3, Work::new(4)).unwrap();
        assert!(take.next().is_none());
        drop(take);
        assert_eq!(results(&mut jobs, time(true, 8)), [4, 3, 1]);
        let state = script.state.lock().unwrap();
        assert_eq!(state.forgets, [queue]);
        assert!(state.copies.is_empty());
    }
}

#[test]
fn invalidation_and_refusal_cover_all_three_places() {
    let (mut jobs, script, _) = setup(
        Limits {
            queued_bytes: 4,
            ..LIMITS
        },
        1,
    );
    for value in 1..=3 {
        submit(&mut jobs, value, 0);
    }
    drop(jobs.take_with_time(&live()));
    let (_, ids) = script.posting(0);
    script.complete(ids[1]);
    drop(jobs.take_with_time(&live()));
    submit(&mut jobs, 4, 0);
    assert_eq!(jobs.pending(), 4);
    assert_eq!(jobs.report().queued_bytes, 4);
    let bigger = Work {
        bytes: 2,
        ..Work::new(12)
    };
    assert!(jobs.submit(2, 1, 0, bigger).is_err());
    assert_eq!(jobs.pending(), 4);
    assert!(script.state.lock().unwrap().outputs.contains_key(&ids[1]));
    jobs.submit(2, 1, 9, Work::new(22)).unwrap();
    jobs.submit(1, 1, 9, Work::new(11)).unwrap();
    script.complete(ids[0]);
    script.complete(ids[2]);
    assert_eq!(results(&mut jobs, live()), [3]);
    let (_, ids) = script.posting(1);
    for id in ids {
        script.complete(id);
    }
    assert_eq!(results(&mut jobs, live()), [22, 11, 4]);
    assert_eq!(jobs.pending(), 0);
    submit(&mut jobs, 5, 0);
    jobs.submit(
        5,
        1,
        0,
        Work {
            bytes: 9,
            ..Work::new(55)
        },
    )
    .unwrap();
    assert!(jobs.submit(6, 0, 0, Work::new(6)).is_err());
}

#[test]
fn loss_retries_unanswered_and_end_requeues_every_place_once() {
    let (mut jobs, script, _) = setup(LIMITS, 2);
    for value in 1..=3 {
        submit(&mut jobs, value, 0);
    }
    drop(jobs.take_with_time(&live()));
    let (_, ids) = script.posting(0);
    script.complete(ids[0]);
    script.lose_batch_with(ids[1]);
    assert_eq!(results(&mut jobs, live()), [1]);
    let (_, ids) = script.posting(1);
    assert_eq!(ids.len(), 2);
    for id in ids {
        script.complete(id);
    }
    assert_eq!(results(&mut jobs, live()), [2, 3]);
    assert_eq!(jobs.report().failed, 0);

    for value in 4..=6 {
        submit(&mut jobs, value, 0);
    }
    drop(jobs.take_with_time(&live()));
    let (_, ids) = script.posting(2);
    script.complete(ids[0]);
    drop(jobs.take_with_time(&live()));
    script.complete(ids[1]);
    submit(&mut jobs, 7, 0);
    {
        let mut state = script.state.lock().unwrap();
        assert_eq!(state.batches.values().next().unwrap().1, [ids[2]]);
        assert_eq!(state.outputs.keys().copied().collect::<Vec<_>>(), ids[..2]);
        for _ in 0..3 {
            let idle = state.idle().unwrap();
            state.lose(idle);
        }
    }
    assert_eq!(results(&mut jobs, live()), [4, 5, 6, 7]);
    assert_eq!(jobs.report().lane, Lane::Inline);
    assert_eq!((jobs.pending(), jobs.report().failed), (0, 0));
    assert!(results(&mut jobs, live()).is_empty());
}

#[test]
fn job_errors_and_traps_are_delivered_but_bad_wire_falls_back() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 0);
    submit(&mut jobs, 3, 0);
    drop(jobs.take_with_time(&live()));
    let (_, ids) = script.posting(0);
    script
        .state
        .lock()
        .unwrap()
        .answer(ids[0], Outcome::Trapped, Vec::new());
    let got: Vec<_> = jobs.take_with_time(&live()).collect();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].key, 1);
    assert_eq!(got[0].result, Err(JobError("job trapped".into())));
    let (_, retry) = script.posting(1);
    script.complete(retry[0]);
    assert_eq!(results(&mut jobs, live()), [3]);

    for value in [255, 2, 4] {
        submit(&mut jobs, value, 0);
    }
    drop(jobs.take_with_time(&live()));
    let (_, ids) = script.posting(2);
    script.complete(ids[0]);
    script
        .state
        .lock()
        .unwrap()
        .answer(ids[1], Outcome::Bytes(1), vec![0]);
    let got: Vec<_> = jobs.take_with_time(&live()).collect();
    assert_eq!(
        got.iter().map(|done| done.key).collect::<Vec<_>>(),
        [255, 2]
    );
    assert_eq!(
        got[0].result,
        Err(JobError("missing required field".into()))
    );
    assert_eq!(got[1].result, Ok(2));
    assert!(results(&mut jobs, live()).is_empty());
    let (_, retry) = script.posting(3);
    script.complete(retry[0]);
    assert_eq!(results(&mut jobs, live()), [4]);
    assert_eq!(jobs.report().failed, 2);
}

#[test]
fn start_that_stays_pending_then_ends_runs_inline() {
    let (mut jobs, script, _) = setup(LIMITS, 0);
    for value in [1, 2] {
        submit(&mut jobs, value, 0);
        assert_eq!(results(&mut jobs, live()), [value]);
    }
    script.state.lock().unwrap().end();
    submit(&mut jobs, 3, 0);
    assert_eq!(results(&mut jobs, live()), [3]);
    let state = script.state.lock().unwrap();
    assert_eq!(state.starts, 1);
    assert!(state.postings.is_empty());
}

#[test]
fn clear_and_drop_forget_late_events_without_reusing_ids() {
    let (mut jobs, script, handle) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 0);
    drop(jobs.take_with_time(&live()));
    let (queue, ids) = script.posting(0);
    jobs.clear();
    script.complete(ids[0]);
    submit(&mut jobs, 1, 0);
    drop(jobs.take_with_time(&live()));
    let (_, next) = script.posting(1);
    assert_ne!(ids[0], next[0]);
    script.state.lock().unwrap().events.insert(
        queue,
        vec![TransportEvent::Arrived {
            attempt: ids[0],
            outcome: Outcome::Bytes(2),
        }],
    );
    assert!(results(&mut jobs, live()).is_empty());
    drop(jobs);
    script.complete(next[0]);
    let mut jobs = with_transport(LIMITS, &handle);
    submit(&mut jobs, 2, 0);
    drop(jobs.take_with_time(&live()));
    let (other, ids) = script.posting(2);
    assert_ne!(queue, other);
    script.complete(ids[0]);
    assert_eq!(results(&mut jobs, live()), [2]);
    assert_eq!(script.state.lock().unwrap().retired, [queue]);
}

#[test]
fn a_local_job_runs_between_two_arrived_results() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 10);
    submit(&mut jobs, 3, -10);
    drop(jobs.take_with_time(&live()));
    let (_, ids) = script.posting(0);
    for id in ids.into_iter().rev() {
        script.complete(id);
    }
    jobs.submit(
        2,
        0,
        0,
        Work {
            local: true,
            ..Work::new(2)
        },
    )
    .unwrap();
    assert_eq!(results(&mut jobs, live()), [1, 2, 3]);
}

#[test]
fn queues_share_one_worker_fairly_and_keep_their_own_clock() {
    let (mut first, script, handle) = setup(LIMITS, 1);
    let mut second = with_transport(LIMITS, &handle);
    for value in 1..=4 {
        submit(&mut first, value, 0);
        submit(&mut second, value + 10, 0);
        drop(first.take_with_time(&late()));
        drop(second.take_with_time(&late()));
        let (_, ids) = script.posting(usize::from(value - 1));
        script.complete(ids[0]);
        let got = if value % 2 == 1 {
            results(&mut first, late())
        } else {
            results(&mut second, late())
        };
        assert_eq!(
            got,
            [value.div_ceil(2) + if value % 2 == 0 { 10 } else { 0 }]
        );
    }
    drop(first.take_with_time(&late()));
    drop(second.take_with_time(&late()));
    let (owner, ids) = script.posting(4);
    assert_eq!(owner, script.posting(0).0);
    script.complete(ids[0]);
    assert_eq!(results(&mut second, time(true, 8)), [13, 14]);
    assert_eq!(results(&mut first, late()), [3]);
}

#[test]
fn completed_limit_running_limit_and_batch_bound() {
    let (mut jobs, script, _) = setup(
        Limits {
            queued_bytes: 64,
            completed_bytes: 2,
            ..LIMITS
        },
        2,
    );
    for value in 1..=20 {
        submit(&mut jobs, value, 0);
    }
    drop(jobs.take_with_time(&live()));
    let (_, ids) = script.posting(0);
    assert_eq!(ids.len(), 16);
    for id in ids {
        script.complete(id);
    }
    drop(jobs.take_with_time(&live()));
    assert_eq!(script.state.lock().unwrap().postings.len(), 1);
    assert_eq!(results(&mut jobs, live()), [1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(results(&mut jobs, live()), [9, 10, 11, 12, 13, 14, 15, 16]);
    assert_eq!(script.posting(1).1.len(), 4);
}

#[test]
fn late_takes_share_one_unit_and_copy_arrivals_once() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 0);
    submit(&mut jobs, 2, 0);
    drop(jobs.take_with_time(&live()));
    let (_, ids) = script.posting(0);
    for &id in &ids {
        script.complete(id);
    }
    for value in 3..=5 {
        submit(&mut jobs, value, 0);
    }
    let frame = late();
    assert_eq!(results(&mut jobs, frame), [1]);
    let mut second = jobs.take_with_time(&frame);
    second
        .submit(
            6,
            0,
            20,
            Work {
                local: true,
                ..Work::new(6)
            },
        )
        .unwrap();
    assert!(second.next().is_none());
    drop(second);
    assert_eq!(script.state.lock().unwrap().postings.len(), 1);
    assert_eq!(script.state.lock().unwrap().copies, ids[..1]);
    jobs.cancel(6);
    assert_eq!(results(&mut jobs, late()), [2]);
    let frame = late();
    assert!(results(&mut jobs, frame).is_empty());
    assert!(results(&mut jobs, frame).is_empty());
    assert_eq!(script.posting(1).1.len(), 1);
    assert_eq!(script.state.lock().unwrap().postings.len(), 2);
    assert_eq!(script.state.lock().unwrap().copies, ids);
}

#[cfg(debug_assertions)]
#[test]
fn debug_roundtrip_exposes_a_dropped_field_inline_and_in_pool() {
    for seekable in [true, false] {
        let mut jobs = Jobs::remote(LIMITS);
        let work = Work {
            required: true,
            ..Work::new(255)
        };
        assert_eq!(
            Work {
                required: true,
                ..Work::new(255)
            }
            .run(&Cancel::default()),
            Ok(255)
        );
        jobs.submit(0, 0, 0, work).unwrap();
        if !seekable {
            drop(jobs.take_with_time(&live()));
            jobs.wait_for_completions(1);
        }
        let done = jobs.take_with_time(&time(seekable, 1)).next().unwrap();
        assert_eq!(done.result, Err(JobError("missing required field".into())));
    }
}

#[test]
fn wire_dispatch_marks_only_run_and_checks_registry() {
    let mut stages = Vec::new();
    assert_eq!(
        run_remote_job(ENTRIES, Work::NAME, &[7, 1], |stage| stages.push(stage)).unwrap(),
        [0, 7]
    );
    assert_eq!(stages, [RunMark::Begin, RunMark::End]);
    for (name, input) in [(Work::NAME, &[][..]), ("unknown", &[7, 1][..])] {
        stages.clear();
        assert!(run_remote_job(ENTRIES, name, input, |stage| stages.push(stage)).is_err());
        assert!(stages.is_empty());
    }
    const DUPLICATE: &[RemoteJob] = &[RemoteJob::of::<Work>(), RemoteJob::of::<Work>()];
    assert!(TransportHandle::new(DUPLICATE, Arc::new(Script::default())).is_err());
}

#[test]
fn installation_uses_type_identity_and_wire_metrics_reset() {
    std::thread::spawn(check_installation_and_wire_metrics)
        .join()
        .unwrap();
}

fn check_installation_and_wire_metrics() {
    use crate::{hooks::metrics::append_state, Hooks};

    #[derive(Default)]
    struct Registered {
        queue: Option<Jobs<Work>>,
    }
    impl Hooks for Registered {
        const REMOTE: &'static [RemoteJob] = ENTRIES;

        fn jobs(&self) -> Option<JobsReport> {
            self.queue.as_ref().map(Jobs::report)
        }
    }

    struct Unregistered;
    impl Job for Unregistered {
        type Output = ();

        fn bytes(&self) -> usize {
            0
        }
        fn output_bytes(_: &()) -> usize {
            0
        }
        fn run(self, _: &Cancel) -> Result<(), JobError> {
            Ok(())
        }
    }
    impl WireJob for Unregistered {
        const NAME: &'static str = Work::NAME;

        fn encode(&self, _: &mut Vec<u8>) {}
        fn decode(_: &[u8]) -> Result<Self, JobError> {
            Ok(Self)
        }
        fn encode_output(_: &(), _: &mut Vec<u8>) {}
        fn decode_output(_: &[u8]) -> Result<(), JobError> {
            Ok(())
        }
    }

    std::thread::spawn(|| {
        let mut early = Jobs::<Work>::remote(LIMITS);
        install_module_transport::<Registered>(Script::default()).unwrap();
        assert!(early.remote.is_none());
        assert_eq!(early.take_with_time(&live()).lane(), Lane::Pool);
        assert_eq!(
            install_module_transport::<Registered>(Script::default()),
            Err(WireError::AlreadyInstalled)
        );
        let mut unregistered = Jobs::<Unregistered>::remote(LIMITS);
        assert!(unregistered.remote.is_none());
        assert_eq!(unregistered.take_with_time(&live()).lane(), Lane::Inline);
        let mut native = Jobs::<Work>::new(LIMITS);
        assert_eq!(native.take_with_time(&live()).lane(), Lane::Pool);

        let mut registered = Jobs::<Work>::remote(LIMITS);
        assert!(registered.remote.is_some());
        submit(&mut registered, 1, 0);
        assert_eq!(results(&mut registered, live()), [1]);
    })
    .join()
    .unwrap();

    let script = Script::default();
    script.ready(1);
    install_module_transport::<Registered>(script.clone()).unwrap();
    let mut queue = Jobs::<Work>::remote(LIMITS);
    let frame = live();
    submit(&mut queue, 1, 0);
    drop(queue.take_with_time(&frame));
    let (_, ids) = script.posting(0);
    script.complete(ids[0]);
    assert_eq!(results(&mut queue, frame), [1]);
    let hooks = Registered { queue: Some(queue) };
    let diagnostics = |reset| {
        let mut out = String::from("{\"world\":true");
        append_state(None, &mut out, &hooks, None, reset);
        out.push('}');
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["renderHooks"]["jobs"].take()
    };
    let report = diagnostics(false);
    assert_eq!(report["lane"], "worker");
    assert_eq!(report["wire"]["outBytes"], 2);
    assert_eq!(report["wire"]["inBytes"], 2);
    assert_eq!(report["wire"]["workersReady"], 1);
    assert_eq!(report["wire"]["workerBytes"], 65_536);
    assert_eq!(report["wire"]["handlerMs"], 0.25);
    assert_eq!(report["wire"]["peakBufferedBytes"], 2);
    assert_eq!(report["wire"]["ms"]["p50"], report["wire"]["ms"]["p95"]);
    assert!(report["wire"]["ms"]["p95"].is_number());
    let reset = diagnostics(true);
    assert_eq!(reset["wire"]["outBytes"], 0);
    assert_eq!(reset["wire"]["inBytes"], 0);
    assert!(reset["wire"]["ms"]["p50"].is_null());
    assert_eq!(reset["done"], 0);
}

#[test]
fn a_refused_late_post_keeps_the_job_and_spends_the_unit() {
    let (mut jobs, script, _) = setup(LIMITS, 1);
    submit(&mut jobs, 1, 0);
    script.state.lock().unwrap().refuse_post = true;
    let frame = late();
    drop(jobs.take_with_time(&frame));
    drop(jobs.take_with_time(&frame));
    assert_eq!(jobs.pending(), 1);
    assert!(script.state.lock().unwrap().postings.is_empty());
    drop(jobs.take_with_time(&late()));
    let (_, ids) = script.posting(0);
    script.complete(ids[0]);
    assert_eq!(results(&mut jobs, live()), [1]);
}

#[test]
fn delivery_does_not_classify_any_of_the_wire_backlog() {
    let (mut jobs, script, _) = setup(
        Limits {
            queued_bytes: 50_001,
            ..LIMITS
        },
        1,
    );
    submit(&mut jobs, 1, 0);
    drop(jobs.take_with_time(&live()));
    script.complete(script.posting(0).1[0]);
    let wire_calls = Arc::new(AtomicU64::new(0));
    for key in 2..50_002 {
        let job = Work {
            wire_calls: Some(wire_calls.clone()),
            ..Work::new(2)
        };
        jobs.submit(key, 0, 0, job).unwrap();
    }
    wire_calls.store(0, Ordering::Relaxed);
    let mut take = jobs.take_with_time(&late());
    assert_eq!(take.next().unwrap().result, Ok(1));
    drop(take);
    let calls = wire_calls.load(Ordering::Relaxed);
    assert!(calls <= 2, "one delivery classified {calls} queued jobs");
    script.state.lock().unwrap().end();
    assert_eq!(results(&mut jobs, late()), [2]);
    wire_calls.store(0, Ordering::Relaxed);
    let job = Work {
        wire_calls: Some(wire_calls.clone()),
        ..Work::new(3)
    };
    jobs.submit(0, 0, 10, job).unwrap();
    assert_eq!(results(&mut jobs, late()), [3]);
    assert_eq!(
        wire_calls.load(Ordering::Relaxed),
        u64::from(cfg!(debug_assertions))
    );
}

#[test]
fn reporting_an_empty_remote_queue_does_not_read_transport_diagnostics() {
    std::thread::spawn(check_empty_remote_queue_diagnostics)
        .join()
        .unwrap();
}

fn check_empty_remote_queue_diagnostics() {
    #[derive(Default)]
    struct Registered;
    impl crate::Hooks for Registered {
        const REMOTE: &'static [RemoteJob] = ENTRIES;
    }

    let script = Script::default();
    script.ready(1);
    install_module_transport::<Registered>(script.clone()).unwrap();
    let jobs = Jobs::<Work>::remote(LIMITS);
    let before = format!("{:?}", jobs.report());
    script.ready(2);
    assert_eq!(format!("{:?}", jobs.report()), before);
    assert!(!before.contains("workers_ready"));
    assert!(!before.contains("wire"));
    assert!(script.state.lock().unwrap().reads.is_empty());
    assert_eq!(script.report_reads.load(Ordering::Relaxed), 0);
}
