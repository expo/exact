use super::*;

const HANG: Duration = Duration::from_secs(5);
const RETAINED: usize = 120 << 20;

fn core(asynchronous: bool) -> (Core, Receiver<()>) {
    let (wake, woke) = channel();
    let core = Core::with_owners_lanes(
        (0..WORKERS).map(|_| None).collect(),
        "",
        Box::new(move || {
            let _ = wake.send(());
        }),
        Arc::new(ibex2::host::Host::new),
        asynchronous.then(asynch_lanes),
    );
    (core, woke)
}

fn hand(core: &Core, ticket: u64, handed: &Sender<(u64, Reply)>) {
    let handed = handed.clone();
    core.run_owned(
        job(ticket, Request::continuation(ticket)),
        Some(OwnedWork::Later(Box::new(move |reply| {
            handed.send((ticket, reply)).unwrap();
        }))),
    )
    .unwrap();
}

fn wait(core: &Core, until: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + HANG;
    let mut state = core.shared.state.lock().unwrap();
    while !until(&state) {
        if Instant::now() >= deadline {
            let snapshot = (
                state.bytes,
                state.counts,
                state.running.len(),
                state.jobs[0].len(),
            );
            drop(state);
            panic!("executor condition did not hold: {snapshot:?}");
        }
        state = core
            .shared
            .ready
            .wait_timeout(state, Duration::from_millis(50))
            .unwrap()
            .0;
    }
}

#[test]
fn handed_off_outcomes_stay_within_the_ordered_byte_budget() {
    for asynchronous in [false, true] {
        let (core, woke) = core(asynchronous);
        let (handed, replies) = channel();
        for ticket in 1..=5 {
            hand(&core, ticket, &handed);
        }
        let (ticket, first) = replies.recv_timeout(HANG).unwrap();
        assert_eq!(ticket, 1);
        for ticket in 2..=4 {
            let (seen, reply) = replies.recv_timeout(HANG).unwrap();
            assert_eq!(seen, ticket);
            reply.send(Outcome::Storage(Vec::with_capacity(RETAINED)));
        }
        wait(&core, |s| s.completed[0].len() == 3);
        assert!(core.drain().is_empty(), "later outcomes wait for the first");
        first.send(Outcome::Storage(Vec::with_capacity(RETAINED)));
        wait(&core, |s| s.completed[0].len() == 4);
        // Finish any fifth handoff that started without room for its outcome.
        let started = core
            .shared
            .state
            .lock()
            .unwrap()
            .running
            .iter()
            .any(|r| r.ticket == 5);
        if started {
            let (ticket, reply) = replies.recv_timeout(HANG).unwrap();
            assert_eq!(ticket, 5);
            reply.send(Outcome::Storage(Vec::with_capacity(RETAINED)));
            wait(&core, |s| s.completed[0].len() == 5);
        }
        let (bytes, queued) = {
            let state = core.shared.state.lock().unwrap();
            (state.bytes[0], state.jobs[0].front().map(|job| job.ticket))
        };
        assert!(
            bytes <= BYTES[0],
            "{bytes} retained bytes exceed {}",
            BYTES[0]
        );
        assert_eq!(queued, Some(5));
        accounted(&core);
        core.begin_pump();
        assert_eq!(core.drain()[0].0, 1);
        let (ticket, reply) = replies.recv_timeout(HANG).unwrap();
        assert_eq!(ticket, 5);
        reply.send(Outcome::Storage(Vec::with_capacity(RETAINED)));
        assert_eq!(
            collect(&core, &woke, 4)
                .into_iter()
                .map(|d| d.0)
                .collect::<Vec<_>>(),
            [2, 3, 4, 5]
        );
        wait(&core, empty);
        accounted(&core);
    }
}

fn empty(state: &State) -> bool {
    state.counts == [0, 0]
        && state.bytes == [0, 0]
        && state.running.is_empty()
        && state.jobs.iter().all(VecDeque::is_empty)
        && state.completed.iter().all(VecDeque::is_empty)
}

fn accounted(core: &Core) {
    let (bytes, held) = {
        let state = core.shared.state.lock().unwrap();
        let held = std::array::from_fn::<_, 2, _>(|lane| {
            state.jobs[lane].iter().map(|j| j.charge).sum::<usize>()
                + state
                    .running
                    .iter()
                    .filter(|r| r.lane == lane)
                    .map(|r| r.charge)
                    .sum::<usize>()
                + state.completed[lane].iter().map(|d| d.bytes).sum::<usize>()
        });
        (state.bytes, held)
    };
    assert_eq!(bytes, held);
    assert!(bytes[0] <= BYTES[0]);
    assert!(bytes[1] <= BYTES[1]);
}

#[test]
fn a_handoff_reserves_its_outcome_record_and_refuses_an_oversized_outcome() {
    for asynchronous in [false, true] {
        let (core, woke) = core(asynchronous);
        let (handed, replies) = channel();
        for ticket in 1..=2 {
            hand(&core, ticket, &handed);
            let (seen, reply) = replies.recv_timeout(HANG).unwrap();
            assert_eq!(seen, ticket);
            let (charge, limit) = {
                let state = core.shared.state.lock().unwrap();
                let run = state.running.iter().find(|r| r.ticket == ticket).unwrap();
                (run.charge, run.limit)
            };
            assert_eq!(charge, limit);
            accounted(&core);
            let capacity = limit - std::mem::size_of::<Completed>();
            reply.send(Outcome::Storage(Vec::with_capacity(
                capacity + usize::from(ticket == 2),
            )));
            wait(&core, |s| s.running.is_empty());
            accounted(&core);
            let done = collect(&core, &woke, 1).pop().unwrap();
            if ticket == 1 {
                assert_eq!(retained(&done.1), capacity);
            } else {
                assert!(matches!(
                    done.1,
                    Outcome::Failed {
                        kind: FailureKind::Refused,
                        ..
                    }
                ));
            }
            wait(&core, empty);
        }
    }
}

#[test]
fn forgotten_and_cancelled_handoffs_hold_their_bytes_until_the_reply_ends() {
    for asynchronous in [false, true] {
        let (core, woke) = core(asynchronous);
        let (handed, replies) = channel();
        for ticket in 1..=5 {
            hand(&core, ticket, &handed);
        }
        let (_, first) = replies.recv_timeout(HANG).unwrap();
        let (_, second) = replies.recv_timeout(HANG).unwrap();
        let (_, third) = replies.recv_timeout(HANG).unwrap();
        let before = core.shared.state.lock().unwrap().bytes;
        core.forget(|ticket| ticket != 2);
        core.cancel(&[3]);
        assert_eq!(core.shared.state.lock().unwrap().bytes, before);
        accounted(&core);
        second.send(Outcome::Storage(Vec::with_capacity(RETAINED)));
        let (ticket, fourth) = replies.recv_timeout(HANG).unwrap();
        assert_eq!(ticket, 4);
        drop(third);
        let (ticket, fifth) = replies.recv_timeout(HANG).unwrap();
        assert_eq!(ticket, 5);
        for reply in [first, fourth, fifth] {
            reply.send(Outcome::Storage(Vec::with_capacity(RETAINED)));
        }
        wait(&core, |s| s.running.is_empty());
        accounted(&core);
        core.forget(|ticket| ticket != 4);
        assert_eq!(
            collect(&core, &woke, 2)
                .into_iter()
                .map(|d| d.0)
                .collect::<Vec<_>>(),
            [1, 5]
        );
        wait(&core, empty);
        accounted(&core);
    }
}

#[test]
fn a_failed_or_abandoned_handoff_releases_bytes_for_queued_work() {
    for asynchronous in [false, true] {
        let (core, woke) = core(asynchronous);
        let (handed, replies) = channel();
        for ticket in 1..=5 {
            hand(&core, ticket, &handed);
        }
        let (_, first) = replies.recv_timeout(HANG).unwrap();
        let (_, second) = replies.recv_timeout(HANG).unwrap();
        let (_, third) = replies.recv_timeout(HANG).unwrap();
        first.send(failed(FailureKind::Network, "module failed"));
        let (ticket, fourth) = replies.recv_timeout(HANG).unwrap();
        assert_eq!(ticket, 4);
        drop(second);
        let (ticket, fifth) = replies.recv_timeout(HANG).unwrap();
        assert_eq!(ticket, 5);
        for reply in [third, fourth, fifth] {
            reply.send(Outcome::Storage(Vec::with_capacity(RETAINED)));
        }
        wait(&core, |s| s.running.is_empty());
        accounted(&core);
        let done = collect(&core, &woke, 5);
        assert_eq!(
            done.iter().map(|d| d.0).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        assert!(matches!(
            done[0].1,
            Outcome::Failed {
                kind: FailureKind::Network,
                ..
            }
        ));
        assert!(matches!(
            done[1].1,
            Outcome::Failed {
                kind: FailureKind::Aborted,
                ..
            }
        ));
        wait(&core, empty);
        accounted(&core);
    }
}

#[test]
fn retirement_discards_queued_work_and_releases_late_handoff_replies() {
    for asynchronous in [false, true] {
        let (mut core, _) = core(asynchronous);
        let (handed, replies) = channel();
        hand(&core, 1, &handed);
        let (_, first) = replies.recv_timeout(HANG).unwrap();
        first.send(Outcome::Storage(Vec::with_capacity(MAX_BODY * 2)));
        wait(&core, |s| s.running.is_empty());
        hand(&core, 2, &handed);
        hand(&core, 3, &handed);
        let (_, second) = replies.recv_timeout(HANG).unwrap();
        let (_, third) = replies.recv_timeout(HANG).unwrap();
        for ticket in 4..=6 {
            hand(&core, ticket, &handed);
        }
        let held = core
            .shared
            .state
            .lock()
            .unwrap()
            .running
            .iter()
            .map(|r| r.charge)
            .sum::<usize>();
        core.retire();
        wait(&core, |s| s.jobs.iter().all(VecDeque::is_empty));
        if asynchronous {
            wait(&core, empty);
        } else {
            let bytes = core.shared.state.lock().unwrap().bytes;
            assert_eq!(bytes, [held, 0]);
        }
        accounted(&core);
        second.send(Outcome::Storage(Vec::with_capacity(RETAINED)));
        drop(third);
        wait(&core, empty);
        accounted(&core);
        assert!(core.drain().is_empty());
        assert!(
            replies.try_recv().is_err(),
            "retired queued work was not handed off"
        );
    }
}

#[test]
fn an_ordered_job_waits_for_retained_bytes_instead_of_refusing() {
    let (core, fixture, woke) = setup();
    // Four undrained outcomes retain 480 MiB of capacity (never touched).
    for ticket in 1..=4 {
        core.run(
            job(ticket, Request::continuation(ticket)),
            Some(Box::new(|| Outcome::Storage(Vec::with_capacity(120 << 20)))),
        )
        .unwrap();
    }
    wait(&core, |s| s.completed[0].len() == 4);
    // Admitted on its request buffers; its 128 MiB ceiling doesn't fit yet.
    core.run(job(5, Request::get("https://example.test/read")), None)
        .unwrap();
    {
        let mut state = core.shared.state.lock().unwrap();
        assert_eq!(state.jobs[0].front().map(|job| job.ticket), Some(5));
        assert!(reads::next_job(&core.shared, &mut state, 0).is_none());
    }
    assert!(fixture.state.lock().unwrap().2.is_empty());
    core.begin_pump();
    assert_eq!(core.drain()[0].0, 1);
    let rest = collect(&core, &woke, 4);
    assert_eq!(
        rest.into_iter().map(|v| v.0).collect::<Vec<_>>(),
        [2, 3, 4, 5]
    );
    assert_eq!(
        fixture.state.lock().unwrap().2,
        ["https://example.test/read"]
    );
    until_settled(&core);
}
