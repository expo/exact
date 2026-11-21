use crate::hooks::{Deadline, HookTime};
use crate::jobs::{Cancel, Done, Job, JobError, Jobs, Lane, Limits};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

const LIMITS: Limits = Limits {
    queued_bytes: 8,
    running: 2,
    completed_bytes: 8,
    take_per_frame: 4,
};
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(super) struct Work {
    value: u64,
    bytes: usize,
    pub(super) fail: bool,
    gate: Option<(Sender<()>, Receiver<()>)>,
    pub(super) ignore_cancel: bool,
}

impl Work {
    pub(super) fn value(value: u64) -> Self {
        Self {
            value,
            bytes: 1,
            fail: false,
            gate: None,
            ignore_cancel: false,
        }
    }

    pub(super) fn gated(value: u64) -> (Self, Receiver<()>, Sender<()>) {
        let (started, start) = mpsc::channel();
        let (finish, finished) = mpsc::channel();
        (
            Self {
                gate: Some((started, finished)),
                ..Self::value(value)
            },
            start,
            finish,
        )
    }
}

impl Job for Work {
    type Output = u64;

    fn bytes(&self) -> usize {
        self.bytes
    }

    fn output_bytes(_: &u64) -> usize {
        8
    }

    fn run(self, cancel: &Cancel) -> Result<u64, JobError> {
        if let Some((started, finished)) = self.gate {
            started.send(()).unwrap();
            finished.recv_timeout(TIMEOUT).unwrap();
        }
        if (!self.ignore_cancel && cancel.is_set()) || self.fail {
            return Err(JobError("fixture failure".into()));
        }
        Ok(self.value)
    }
}

fn time(seekable: bool, allowance: u32) -> HookTime {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    HookTime {
        serial: SERIAL.fetch_add(1, Ordering::Relaxed),
        allowance,
        seekable,
        ..HookTime::default()
    }
}

fn values(jobs: &mut Jobs<Work>, clock: HookTime) -> Vec<u64> {
    jobs.take_with_time(&clock)
        .map(|done| done.result.unwrap())
        .collect()
}

#[test]
fn jobs_inline_is_ordered_and_observations_do_no_work() {
    fn drive(looks: usize) -> Vec<u64> {
        let mut jobs = Jobs::new(LIMITS);
        for (key, priority) in [(1, 0), (2, 5), (3, 5), (4, -1)] {
            jobs.submit(key, 0, priority, Work::value(key)).unwrap();
        }
        let mut got = values(&mut jobs, time(true, 1));
        for _ in 0..looks {
            assert!(values(&mut jobs, time(true, 0)).is_empty());
            assert_eq!(jobs.pending(), 3);
        }
        got.extend(values(&mut jobs, time(true, 8)));
        assert_eq!(jobs.report().lane, Lane::Inline);
        got
    }
    assert_eq!(drive(20), drive(0));
    assert_eq!(drive(0), [2, 3, 1, 4]);
}

#[test]
fn jobs_deadline_and_limit_are_checked_between_results() {
    let mut jobs = Jobs::new(Limits {
        take_per_frame: 2,
        ..LIMITS
    });
    for key in 1..=4 {
        jobs.submit(key, 0, 0, Work::value(key)).unwrap();
    }
    let expired = Deadline::new(crate::perf::Stamp::now(), -1.);
    assert_eq!(
        values(
            &mut jobs,
            HookTime {
                deadline: Some(expired),
                ..time(true, 4)
            }
        ),
        [1]
    );
    assert_eq!(values(&mut jobs, time(true, 4)), [2, 3]);
    assert_eq!(values(&mut jobs, time(true, 4)), [4]);
}

#[test]
fn jobs_queue_bound_replacement_cancel_and_failure() {
    let mut jobs = Jobs::new(LIMITS);
    let large = Work {
        bytes: 9,
        ..Work::value(1)
    };
    jobs.submit(1, 1, 0, large).unwrap();
    let full = jobs.submit(2, 1, 0, Work::value(2)).unwrap_err();
    assert_eq!(full.0.value, 2);
    jobs.submit(1, 2, 0, Work::value(10)).unwrap();
    jobs.submit(2, 1, 0, full.0).unwrap();
    jobs.cancel(2);
    jobs.submit(
        3,
        1,
        1,
        Work {
            fail: true,
            ..Work::value(3)
        },
    )
    .unwrap();
    let got: Vec<Done<u64>> = jobs.take_with_time(&time(true, 4)).collect();
    assert_eq!(got.len(), 2);
    assert_eq!(
        (got[0].key, &got[0].result),
        (3, &Err(JobError("fixture failure".into())))
    );
    assert_eq!(
        (got[1].key, got[1].generation, &got[1].result),
        (1, 2, &Ok(10))
    );
    let report = jobs.report();
    assert_eq!((report.done, report.stale, report.failed), (1, 0, 1));
    assert_eq!(jobs.pending(), 0);
}

#[test]
fn jobs_first_inline_take_waits_for_running_work() {
    let mut jobs = Jobs::new(Limits {
        running: 1,
        ..LIMITS
    });
    let (first, started, finish) = Work::gated(1);
    jobs.take_with_time(&time(false, 1));
    jobs.submit(1, 0, 0, first).unwrap();
    started.recv_timeout(TIMEOUT).unwrap();
    jobs.submit(2, 0, 0, Work::value(2)).unwrap();
    let shared = jobs.shared.clone();
    let release = std::thread::spawn(move || {
        let state = shared.state.lock().unwrap();
        let (state, _) = shared
            .changed
            .wait_timeout_while(state, TIMEOUT, |state| state.lane != Lane::Inline)
            .unwrap();
        drop(state);
        finish.send(()).unwrap();
    });
    let got = values(&mut jobs, time(true, 4));
    release.join().unwrap();
    assert_eq!(got, [1, 2]);
}

#[test]
fn jobs_invalidated_while_running_are_discarded_without_a_take() {
    for clear in [true, false] {
        let mut jobs = Jobs::new(LIMITS);
        jobs.take_with_time(&time(false, 1));
        let (mut job, started, finish) = Work::gated(1);
        job.ignore_cancel = true;
        jobs.submit(1, 0, 0, job).unwrap();
        started.recv_timeout(TIMEOUT).unwrap();
        if clear {
            jobs.clear();
        } else {
            jobs.cancel(1);
        }
        finish.send(()).unwrap();
        jobs.wait_for_workers();
        assert_eq!((jobs.pending(), jobs.report().stale), (0, 1));
    }
}

#[test]
fn jobs_replace_and_clear_invalidate_running_work_with_the_same_generation() {
    let mut jobs = Jobs::new(Limits {
        running: 1,
        completed_bytes: 64,
        ..LIMITS
    });
    jobs.take_with_time(&time(false, 1));
    for clear in [false, true] {
        let (mut first, started, finish) = Work::gated(1);
        first.ignore_cancel = true;
        jobs.submit(7, 1, 0, first).unwrap();
        started.recv_timeout(TIMEOUT).unwrap();
        if clear {
            jobs.clear();
        }
        jobs.submit(7, 1, 0, Work::value(2)).unwrap();
        finish.send(()).unwrap();
        jobs.wait_for_completions(1);
        assert_eq!(values(&mut jobs, time(false, 4)), [2]);
    }
    assert_eq!((jobs.report().done, jobs.report().stale), (2, 2));

    jobs.submit(8, 1, 0, Work::value(3)).unwrap();
    jobs.wait_for_completions(1);
    jobs.clear();
    assert_eq!((jobs.report().done, jobs.report().stale), (2, 3));
}

#[test]
fn jobs_late_frames_skip_stale_results_and_take_one_current_result() {
    let mut jobs = Jobs::new(Limits {
        completed_bytes: 64,
        ..LIMITS
    });
    for frame in 0..3 {
        for key in 1..=4 {
            jobs.submit(key, frame, 0, Work::value(key)).unwrap();
        }
        jobs.take_with_time(&time(false, 1));
        jobs.wait_for_completions(4);
        jobs.cancel(1);
        jobs.cancel(2);
        assert_eq!(
            values(
                &mut jobs,
                HookTime {
                    deadline: Some(Deadline::new(crate::perf::Stamp::now(), -1.)),
                    ..time(false, 4)
                }
            ),
            [3]
        );
        assert_eq!(jobs.report().stale, (frame + 1) * 2);
        assert_eq!(values(&mut jobs, time(false, 4)), [4]);
    }
}

#[test]
fn jobs_zero_limits_make_progress() {
    let mut jobs = Jobs::new(Limits {
        running: 0,
        take_per_frame: 0,
        ..LIMITS
    });
    jobs.submit(1, 0, 0, Work::value(1)).unwrap();
    jobs.take_with_time(&time(false, 1));
    assert_eq!(jobs.workers.len(), 1);
    jobs.wait_for_completions(1);
    assert_eq!(values(&mut jobs, time(false, 4)), [1]);
}

#[test]
fn jobs_failed_pool_start_stays_inline() {
    let mut jobs = Jobs::new(LIMITS);
    let mut take = jobs.take_with_time(&time(false, 4));
    assert_eq!(take.lane(), Lane::Pool);
    take.jobs.fail_pool_start = true;
    take.submit(1, 0, 0, Work::value(1)).unwrap();
    assert_eq!(take.lane(), Lane::Inline);
    assert_eq!(take.next().unwrap().result, Ok(1));
    drop(take);
    assert!(values(&mut jobs, time(false, 4)).is_empty());
    assert_eq!(jobs.report().lane, Lane::Inline);
    assert!(jobs.workers.is_empty());
}

#[test]
fn jobs_takes_share_the_frame_count_and_deadline() {
    let mut jobs = Jobs::new(LIMITS);
    for key in 1..=5 {
        jobs.submit(key, 0, 0, Work::value(key)).unwrap();
    }
    let clock = time(true, 2);
    assert_eq!(jobs.take_with_time(&clock).next().unwrap().result, Ok(1));
    assert_eq!(values(&mut jobs, clock), [2]);
    assert!(values(&mut jobs, clock).is_empty());

    let late = HookTime {
        deadline: Some(Deadline::new(crate::perf::Stamp::now(), -1.)),
        ..time(true, 2)
    };
    assert_eq!(values(&mut jobs, late), [3]);
    assert!(values(&mut jobs, late).is_empty());
    assert_eq!(values(&mut jobs, time(true, 2)), [4, 5]);
}

#[test]
fn jobs_live_pool_keeps_working_after_a_held_take() {
    let mut jobs = Jobs::new(Limits {
        running: 1,
        completed_bytes: 64,
        ..LIMITS
    });
    let (first, started, finish) = Work::gated(1);
    let observed = time(true, 0);
    jobs.take_with_time(&time(false, 1));
    jobs.submit(1, 0, 0, first).unwrap();
    started.recv_timeout(TIMEOUT).unwrap();
    jobs.submit(2, 0, 0, Work::value(2)).unwrap();
    let (taken, take_finished) = mpsc::channel();
    let release = std::thread::spawn(move || {
        let returned = take_finished.recv_timeout(TIMEOUT / 2).is_ok();
        finish.send(()).unwrap();
        returned
    });
    let lane = jobs.lane(&observed);
    let got = values(&mut jobs, observed);
    taken.send(()).ok();
    assert!(
        release.join().unwrap(),
        "held take waited for the running job"
    );
    assert_eq!(lane, Lane::Pool);
    assert!(got.is_empty());
    assert_eq!(jobs.report().lane, Lane::Pool);
    jobs.wait_for_completions(2);
}

#[test]
fn jobs_worker_unwind_publishes_failure_and_releases_the_running_slot() {
    #[derive(Debug)]
    struct Panic(bool);

    impl Job for Panic {
        type Output = bool;

        fn bytes(&self) -> usize {
            1
        }
        fn output_bytes(output: &bool) -> usize {
            assert!(!output, "output_bytes panicked");
            1
        }
        fn run(self, _: &Cancel) -> Result<bool, JobError> {
            assert!(self.0, "run panicked");
            Ok(self.0)
        }
    }

    for panic_in_size in [false, true] {
        let mut jobs = Jobs::new(LIMITS);
        jobs.submit(1, 7, 0, Panic(panic_in_size)).unwrap();
        jobs.take_with_time(&time(false, 1));
        jobs.wait_for_completions(1);
        let done = jobs.take_with_time(&time(true, 1)).next().unwrap();
        assert_eq!((done.key, done.generation), (1, 7));
        assert_eq!(done.result, Err(JobError("job panicked".into())));
        assert_eq!((jobs.pending(), jobs.report().failed), (0, 1));
    }
}

#[test]
fn jobs_drop_cancels_and_joins_running_work() {
    #[derive(Debug)]
    struct OnCancel(Sender<()>, Arc<AtomicBool>);

    impl Job for OnCancel {
        type Output = ();

        fn bytes(&self) -> usize {
            0
        }
        fn output_bytes(_: &()) -> usize {
            0
        }
        fn run(self, cancel: &Cancel) -> Result<(), JobError> {
            self.0.send(()).unwrap();
            let limit = std::time::Instant::now() + TIMEOUT;
            while !cancel.is_set() {
                assert!(
                    std::time::Instant::now() < limit,
                    "drop did not cancel work"
                );
                std::thread::yield_now();
            }
            self.1.store(true, Ordering::Release);
            Ok(())
        }
    }

    let (started, start) = mpsc::channel();
    let finished = Arc::new(AtomicBool::new(false));
    let mut jobs = Jobs::new(LIMITS);
    jobs.submit(1, 0, 0, OnCancel(started, finished.clone()))
        .unwrap();
    jobs.take_with_time(&time(false, 1));
    start.recv_timeout(TIMEOUT).unwrap();
    let shared = jobs.shared.clone();
    drop(jobs);
    assert_eq!(
        Arc::strong_count(&shared),
        1,
        "drop returned before the worker exited"
    );
    assert!(
        finished.load(Ordering::Acquire),
        "drop returned before run finished"
    );
}

#[test]
fn jobs_pool_throttles_completed_bytes() {
    let mut jobs = Jobs::new(Limits {
        running: 1,
        ..LIMITS
    });
    jobs.take_with_time(&time(false, 1));
    assert!(jobs.workers.is_empty());
    let (first, started, finish) = Work::gated(1);
    jobs.submit(1, 0, 0, first).unwrap();
    started.recv_timeout(TIMEOUT).unwrap();
    let (second, started, release) = Work::gated(2);
    jobs.submit(2, 0, 0, second).unwrap();
    finish.send(()).unwrap();
    jobs.wait_for_workers();
    assert_eq!((jobs.report().running, jobs.report().queued), (0, 1));
    assert_eq!(values(&mut jobs, time(false, 1)), [1]);
    started.recv_timeout(TIMEOUT).unwrap();
    release.send(()).unwrap();
}

#[test]
fn jobs_pool_backlog_stays_bounded_under_replacement() {
    let mut jobs = Jobs::new(Limits {
        running: 1,
        ..LIMITS
    });
    jobs.take_with_time(&time(false, 1));
    let (first, started, finish) = Work::gated(0);
    jobs.submit(0, 0, 0, first).unwrap();
    started.recv_timeout(TIMEOUT).unwrap();
    for generation in 1..100 {
        for key in 0..4 {
            jobs.submit(key, generation, 0, Work::value(10 * generation + key))
                .unwrap();
        }
        assert!(jobs.pending() <= 5);
    }
    finish.send(()).unwrap();
    assert_eq!(values(&mut jobs, time(true, 4)), [990, 991, 992, 993]);
    assert_eq!(jobs.report().stale, 1);
    assert_eq!(jobs.pending(), 0);
}

#[test]
fn jobs_ready_pool_results_are_ordered_with_the_first_submission_still_running() {
    let mut jobs = Jobs::new(Limits {
        completed_bytes: 64,
        ..LIMITS
    });
    let (first, started, finish) = Work::gated(1);
    jobs.submit(1, 0, 99, first).unwrap();
    for _ in 0..2 {
        let shared = jobs.shared.clone();
        jobs.workers
            .push(std::thread::spawn(move || super::pool::work(&shared)));
    }
    jobs.take_with_time(&time(false, 1));
    started.recv_timeout(TIMEOUT).unwrap();
    for (key, priority) in [(2, -3), (3, 10), (4, 10)] {
        jobs.submit(key, 0, priority, Work::value(key)).unwrap();
    }
    jobs.wait_for_completions(3);
    assert_eq!(values(&mut jobs, time(false, 4)), [3, 4, 2]);
    finish.send(()).unwrap();
    jobs.wait_for_completions(1);
    assert_eq!(values(&mut jobs, time(false, 4)), [1]);
}

#[test]
fn jobs_state_adds_and_resets_queues_without_a_device() {
    use crate::{Hooks, WorldSurface};

    struct Reports([Jobs<Work>; 2]);
    impl Default for Reports {
        fn default() -> Self {
            let build = || {
                let mut jobs = Jobs::new(LIMITS);
                jobs.submit(1, 0, 0, Work::value(1)).unwrap();
                assert_eq!(values(&mut jobs, time(true, 1)), [1]);
                jobs.clear();
                assert_eq!(jobs.report().done, 1);
                jobs.submit(2, 0, 0, Work::value(2)).unwrap();
                jobs
            };
            Self([build(), build()])
        }
    }
    impl Hooks for Reports {
        fn jobs(&self) -> Option<crate::JobsReport> {
            Some(self.0[0].report() + self.0[1].report())
        }
    }
    let mut canvas = WorldSurface::<crate::test_game::Fixture, (), false, Reports>::default();
    use exact_gpu::Surface;
    canvas.bind(&[], None).unwrap();
    let state = canvas.agent(r#"{"op":"state"}"#).unwrap();
    assert!(state.contains(r#""jobs":{"queued":2,"queuedBytes":2,"running":0,"done":2,"stale":0,"failed":0,"lane":"inline"}"#));
    let state = canvas.agent(r#"{"op":"state","perf_reset":true}"#).unwrap();
    assert!(state.contains(r#""jobs":{"queued":2,"queuedBytes":2,"running":0,"done":0,"stale":0,"failed":0,"lane":"inline"}"#));
    assert_eq!(canvas.render_hooks().0[0].report().done, 0);
    assert_eq!(canvas.render_hooks().0[1].report().stale, 0);
}

// Work owns a Receiver, which is Send and not Sync.
const _: fn() = || {
    fn check<T: Send + Sync>() {}
    check::<Jobs<Work>>();
};

#[test]
fn jobs_late_install_preserves_first_delivery_on_pool_and_inline() {
    for (seekable, lane) in [(false, Lane::Pool), (true, Lane::Inline)] {
        let mut jobs = Jobs::new(Limits {
            completed_bytes: 64,
            ..LIMITS
        });
        for key in 1..=2 {
            jobs.submit(key, 0, 0, Work::value(key)).unwrap();
        }
        if !seekable {
            drop(jobs.take_with_time(&time(false, 4)));
            jobs.wait_for_completions(2);
        }
        let frame = HookTime {
            deadline: Some(Deadline::new(crate::perf::Stamp::now(), -1.)),
            ..time(seekable, 4)
        };
        let mut take = jobs.take_with_time(&frame);
        assert_eq!(take.lane(), lane);
        take.mark_late_unit_spent();
        assert_eq!(take.next().map(|done| done.result), Some(Ok(1)));
        assert!(take.next().is_none());
        drop(take);
        assert_eq!(values(&mut jobs, time(seekable, 4)), [2]);
    }
}
