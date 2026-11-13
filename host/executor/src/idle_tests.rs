//! An idle executor costs nothing: no worker thread before the first
//! request (none of the independent lane's before the first of its own),
//! and no worker notified by the host's per-commit calls while none
//! has anything to do.
use super::*;
use std::sync::mpsc::channel;
use std::time::Duration;

struct NoTransport;
impl ibex2::stdlib::fetch::Transport for NoTransport {
    fn open(
        &self,
        _: &ibex2::stdlib::fetch::Request,
        _: &ibex2::stdlib::abort::AbortSignal,
    ) -> Result<ibex2::stdlib::fetch::StreamingResponse, ibex2::boundary::HostError> {
        panic!("a request without grants must not reach the transport");
    }
}

fn core() -> (Core, std::sync::mpsc::Receiver<()>) {
    let (wake, woke) = channel();
    // Exercise the pool even when EXACT_HTTP_ASYNC selects the async core.
    let core = Core::with_owners_on(
        (0..WORKERS).map(|_| None).collect(),
        "",
        Box::new(move || {
            let _ = wake.send(());
        }),
        Arc::new(|| ibex2::host::Host::with_transport(Box::new(NoTransport))),
    );
    (core, woke)
}

fn next(core: &Core, woke: &std::sync::mpsc::Receiver<()>) -> (u64, Outcome) {
    next_within(core, woke, Duration::from_secs(30))
}

fn next_within(
    core: &Core,
    woke: &std::sync::mpsc::Receiver<()>,
    bound: Duration,
) -> (u64, Outcome) {
    let deadline = std::time::Instant::now() + bound;
    loop {
        core.begin_pump();
        if let Some((ticket, outcome, _)) = core.drain().pop() {
            return (ticket, outcome);
        }
        woke.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .expect("the executor never completed the request");
    }
}

#[test]
fn an_independent_test_can_admit_work_while_another_reserves_every_worker() {
    let (reserved, ready) = channel();
    let (release, released) = channel();
    let holder = std::thread::spawn(move || {
        let full = Core::with_owners_on(
            (0..MAX_WORKERS).map(|_| None).collect(),
            "",
            Box::new(|| {}),
            Arc::new(ibex2::host::Host::new),
        );
        assert!(!full.disabled, "the fixture must reserve every worker");
        reserved.send(()).unwrap();
        let _ = released.recv_timeout(Duration::from_secs(60));
        drop(full);
    });
    ready.recv_timeout(Duration::from_secs(30)).unwrap();
    let script = Arc::new(async_tests::Script::default());
    let (core, woke) = async_tests::core(&script, "net.fetch https://example.test");
    let admitted = core.run(tests::job(1, Request::get("https://example.test/")), None);
    release.send(()).unwrap();
    holder.join().unwrap();
    admitted.expect("another test's reservations must not refuse this request");
    assert_eq!(script.wait(1), 1, "the request must reach the transport");
    script.answer("https://example.test/", b"done");
    assert!(matches!(
        tests::collect(&core, &woke, 1)[0].1,
        Outcome::Response(Response { status: 200, .. })
    ));
    let state = core.shared.state.lock().unwrap();
    let (state, _) = core
        .shared
        .ready
        .wait_timeout_while(state, Duration::from_secs(30), |s| s.bytes != [0, 0])
        .unwrap();
    assert_eq!(state.bytes, [0, 0]);
}

#[test]
fn cores_in_one_test_share_the_worker_limit_and_return_slots_to_their_owner() {
    let budget = WorkerBudget::Thread.live();
    let full = Core::with_owners_on(
        (0..MAX_WORKERS).map(|_| None).collect(),
        "",
        Box::new(|| {}),
        Arc::new(ibex2::host::Host::new),
    );
    assert!(!full.disabled);
    let (refused, _) = core();
    assert_eq!(
        refused.run(tests::job(1, Request::continuation(1)), None),
        Err("native executor worker limit reached")
    );
    assert_eq!(budget.load(Ordering::Acquire), MAX_WORKERS);
    // Releasing on another thread must credit the creating test's budget.
    std::thread::spawn(move || drop(full)).join().unwrap();
    assert_eq!(budget.load(Ordering::Acquire), 0);
    let (replacement, woke) = core();
    replacement
        .run(
            tests::job(2, Request::continuation(2)),
            Some(Box::new(|| Outcome::Storage(vec![7]))),
        )
        .unwrap();
    assert!(matches!(
        &tests::collect(&replacement, &woke, 1)[0],
        (2, Outcome::Storage(bytes)) if bytes == &[7]
    ));
}

#[test]
fn workers_start_with_the_first_request() {
    let (core, woke) = core();
    check_worker_start(&core, &woke, Duration::from_secs(30));
}

#[test]
fn an_ungranted_independent_request_needs_no_platform_host() {
    let made = Arc::new(AtomicUsize::new(0));
    let hosts = made.clone();
    let (wake, woke) = channel();
    let core = Core::with_owners_lanes(
        (0..WORKERS).map(|_| None).collect(),
        "",
        Box::new(move || {
            let _ = wake.send(());
        }),
        Arc::new(move || {
            hosts.fetch_add(1, Ordering::Relaxed);
            ibex2::host::Host::with_transport(Box::new(NoTransport))
        }),
        asynchronous().then(asynch_lanes),
    );
    check_worker_start(&core, &woke, Duration::from_secs(30));
    assert_eq!(made.load(Ordering::Relaxed), 0);
}

#[test]
#[ignore = "async lane: checks the platform core's independent completion within five seconds"]
fn a_platform_host_completes_an_ungranted_independent_request() {
    // Empty grants must refuse without constructing a host. An unrelated
    // grant keeps real platform initialization covered on the same path.
    for grants in ["", "net.fetch https://elsewhere.example.test"] {
        let (wake, woke) = channel();
        let core = Core::with_owners(
            (0..WORKERS).map(|_| None).collect(),
            grants,
            Box::new(move || {
                let _ = wake.send(());
            }),
        );
        check_worker_start(&core, &woke, Duration::from_secs(5));
    }
}

fn check_worker_start(core: &Core, woke: &std::sync::mpsc::Receiver<()>, bound: Duration) {
    assert!(core.idle.lock().unwrap().is_some(), "started at boot");
    // The host's per-commit and per-pump calls start nothing.
    core.forget(|_| true);
    core.begin_pump();
    assert!(core.drain().is_empty());
    assert!(core.idle.lock().unwrap().is_some());
    core.run(
        RequestOut {
            class: exact_runner::RequestClass::Ordered,
            ticket: 1,
            target: "test".into(),
            request: Request::continuation(1),
            forced: false,
        },
        Some(Box::new(|| Outcome::Storage(vec![7]))),
    )
    .unwrap();
    // The worker core leaves its independent lane idle until its own request;
    // the asynchronous core starts its base set together.
    let waiting = core.idle.lock().unwrap().as_ref().map(Vec::len);
    assert_eq!(waiting, core.machine.is_none().then_some(INDEPENDENT));
    assert!(matches!(
        next_within(core, woke, bound),
        (1, Outcome::Storage(bytes)) if bytes == [7]
    ));
    core.run(
        RequestOut {
            class: exact_runner::RequestClass::Ordered,
            ticket: 2,
            target: "test".into(),
            request: Request::get("https://example.test/").independent_http(4096),
            forced: false,
        },
        None,
    )
    .unwrap();
    assert!(core.idle.lock().unwrap().is_none());
    assert!(matches!(
        next_within(core, woke, bound),
        (
            2,
            Outcome::Failed {
                kind: FailureKind::Refused,
                ..
            }
        )
    ));
}

#[test]
fn an_idle_pool_is_not_woken() {
    let (core, woke) = core();
    core.run(
        RequestOut {
            class: exact_runner::RequestClass::Ordered,
            ticket: 1,
            target: "test".into(),
            request: Request::continuation(1),
            forced: false,
        },
        Some(Box::new(|| Outcome::Storage(vec![]))),
    )
    .unwrap();
    assert!(matches!(next(&core, &woke), (1, Outcome::Storage(_))));
    // Count notification calls: wait returns can be delayed or spurious.
    let before = core.shared.worker_notifications.load(Ordering::Relaxed);
    assert!(before > 0, "the first request must notify workers");
    for _ in 0..200 {
        // What a host does after every commit and at every pump.
        core.forget(|_| true);
        core.begin_pump();
        assert!(core.drain().is_empty());
        core.notify();
    }
    assert_eq!(
        core.shared.worker_notifications.load(Ordering::Relaxed),
        before,
        "idle workers were notified"
    );
}
