use super::super::super::cache::tests::{ask, collect, job, read, Plan, Server, HANG};
use super::*;
use std::sync::mpsc::{channel, Sender};

fn held_plan(server: &Server, path: &str) -> Sender<()> {
    let (release, held) = channel();
    let mut plan = Plan::new(503, &[], b"prefetched");
    plan.held = Some(held);
    server.plan(path, plan);
    release
}
fn clock(core: &Core) -> Arc<Mutex<Instant>> {
    let clock = Arc::new(Mutex::new(Instant::now()));
    let now = clock.clone();
    core.shared
        .state
        .lock()
        .unwrap()
        .cache
        .as_mut()
        .unwrap()
        .clock = Arc::new(move || *now.lock().unwrap());
    clock
}

#[test]
fn prefetch_returns_any_status_and_equal_reads_take_it_until_ten_seconds_from_completion() {
    for asynchronous in [false, true] {
        for method in ["GET", "POST"] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            let clock = clock(&core);
            let release = held_plan(&server, "/next");
            let mut request = server.request("/next");
            request.method = method.into();
            if method == "POST" {
                request.body = b"query".to_vec();
            }
            core.run(job(1, request.clone().prefetch()), None).unwrap();
            server.seen(1);
            *clock.lock().unwrap() += Duration::from_secs(4);
            release.send(()).unwrap();
            let (_, outcome, timing) = collect(&core, &woke);
            assert!(!timing.cached);
            assert!(
                matches!(outcome, Outcome::Response(r) if r.status == 503 && r.body == b"prefetched")
            );
            for (ticket, seconds) in [(2, 0), (3, 9)] {
                *clock.lock().unwrap() += Duration::from_secs(seconds);
                let (reply, cached) = ask(&core, &woke, job(ticket, request.clone()));
                assert!(cached);
                assert_eq!(reply.body, b"prefetched");
                assert_eq!(server.count(), 1);
            }
            *clock.lock().unwrap() += Duration::from_secs(1);
            server.plan("/next", Plan::new(200, &[], b"sent"));
            let (reply, cached) = ask(&core, &woke, job(4, request));
            assert!(!cached);
            assert_eq!(reply.body, b"sent");
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn an_inflight_prefetch_has_one_exchange_for_a_later_read_and_a_second_prefetch() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let release = held_plan(&server, "/next");
        let (core, woke) = server.core(asynchronous, true);
        core.run(job(1, server.request("/next").prefetch()), None)
            .unwrap();
        server.seen(1);
        core.run(job(2, server.request("/next")), None).unwrap();
        core.run(job(3, server.request("/next").prefetch()), None)
            .unwrap();
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.jobs[0].len(), 2);
            assert_eq!(state.running.len(), 1);
            assert!(state.completed[0].is_empty());
        }
        release.send(()).unwrap();
        for expected in 1..=3 {
            let (ticket, outcome, timing) = collect(&core, &woke);
            assert_eq!(ticket, expected);
            assert_eq!(timing.cached, ticket != 1);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"prefetched"));
        }
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn a_forgotten_prefetch_waiter_releases_its_reservation_when_another_prefetch_answers() {
    forgotten_waiter(false);
}

#[test]
fn a_forgotten_prefetch_waiter_releases_its_reservation_when_a_launch_preload_answers() {
    forgotten_waiter(true);
}

fn forgotten_waiter(launch: bool) {
    for asynchronous in [false, true] {
        let server = Server::new();
        let release = held_plan(&server, "/next");
        let (core, woke) = server.core(asynchronous, true);
        if launch {
            let set = Preloads::start(
                vec![Definition {
                    name: "next".into(),
                    request: server.request("/next"),
                    deadline_ms: None,
                    secrets: Vec::new(),
                }],
                &format!("net.fetch http://{}", server.address),
                ibex2::host::Host::new,
            );
            assert!(core.adopt_preloads(set).is_ok());
        }
        let _clock = clock(&core);
        let bytes = core.shared.state.lock().unwrap().bytes;
        if !launch {
            core.run(job(1, server.request("/next").prefetch()), None)
                .unwrap();
        }
        server.seen(1);
        core.run(job(2, server.request("/next").prefetch()), None)
            .unwrap();
        assert!(core.shared.state.lock().unwrap().jobs[0]
            .iter()
            .any(|j| j.ticket == 2));
        core.forget(|ticket| ticket == 1);
        release.send(()).unwrap();
        if !launch {
            assert_eq!(collect(&core, &woke).0, 1);
        }
        let (reply, cached) = read(&core, &woke, &server, 3, "/next");
        assert!(cached);
        assert_eq!(reply.body, b"prefetched");
        let state = core.shared.state.lock().unwrap();
        let (state, timeout) = core
            .shared
            .ready
            .wait_timeout_while(state, HANG, |s| !s.running.is_empty())
            .unwrap();
        assert!(!timeout.timed_out(), "exchange did not end");
        assert_eq!(state.bytes, bytes);
        drop(state);
        assert!(core.ordered_idle(), "asynchronous={asynchronous}");
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn forgetting_a_prefetch_keeps_it_and_an_explicit_abort_after_forgetting_cancels_it() {
    for asynchronous in [false, true] {
        for aborted in [false, true] {
            let server = Server::new();
            let release = held_plan(&server, "/next");
            let (core, woke) = server.core(asynchronous, true);
            core.run(job(1, server.request("/next").prefetch()), None)
                .unwrap();
            server.seen(1);
            core.forget(|_| false);
            if aborted {
                core.cancel(&[1]);
            }
            server.plan("/next", Plan::new(200, &[], b"sent"));
            core.run(job(2, server.request("/next")), None).unwrap();
            release.send(()).unwrap();
            let (ticket, outcome, timing) = collect(&core, &woke);
            assert_eq!(ticket, 2);
            let Outcome::Response(reply) = outcome else {
                panic!("{outcome:?}")
            };
            let cached = timing.cached;
            assert_eq!(cached, !aborted);
            assert_eq!(
                reply.body,
                if aborted {
                    b"sent".as_slice()
                } else {
                    b"prefetched"
                }
            );
            assert_eq!(server.count(), if aborted { 2 } else { 1 });
            if !aborted {
                assert!(read(&core, &woke, &server, 3, "/next").1);
            }
        }
    }
}

#[test]
fn a_send_drops_a_prefetch_at_its_head_or_on_failure() {
    for asynchronous in [false, true] {
        for failure in [false, true] {
            let server = Server::new();
            server.plan("/next", Plan::new(200, &[], b"prefetched"));
            let (core, woke) = server.core(asynchronous, true);
            ask(&core, &woke, job(1, server.request("/next").prefetch()));
            let (release, after) = channel();
            let mut plan = Plan::new(200, &[], &[b'.'; 2048]);
            plan.disconnect = failure;
            if !failure {
                plan.declares = Some(4096);
                plan.after = Some(after);
            }
            server.plan("/next", plan);
            let mut send = job(2, server.request("/next"));
            send.class = RequestClass::Ordered;
            send.request.method = "POST".into();
            core.run(send, None).unwrap();
            if !failure {
                let state = core.shared.state.lock().unwrap();
                let (state, timeout) = core
                    .shared
                    .ready
                    .wait_timeout_while(state, HANG, |s| {
                        s.cache.as_ref().unwrap().prefetches[0].used().is_some()
                    })
                    .unwrap();
                assert!(!timeout.timed_out());
                assert!(state.running.iter().any(|r| !r.terminal));
                drop(state);
                release.send(()).unwrap();
            }
            assert!(matches!(collect(&core, &woke).1, Outcome::Failed { .. }));
            assert!(core
                .shared
                .state
                .lock()
                .unwrap()
                .cache
                .as_ref()
                .unwrap()
                .prefetches[0]
                .used()
                .is_none());
            server.plan("/next", Plan::new(200, &[], b"sent"));
            assert!(!read(&core, &woke, &server, 3, "/next").1);
        }
    }
}

#[test]
fn prefetch_refuses_sends_streams_files_and_independent_requests() {
    for asynchronous in [false, true] {
        let server = Server::new();
        for kind in 0..5 {
            let (core, _) = server.core(asynchronous, true);
            let mut r = job(1, server.request("/refused").prefetch());
            match kind {
                0 => r.class = RequestClass::Ordered,
                1 => {
                    r.request.stream = true;
                }
                2 => {
                    r.request.method = "POST".into();
                    r.request = r.request.body_from("app:/data/file");
                }
                3 => r.request = r.request.body_to("app:/data/file"),
                _ => {
                    r.request.http = HttpScheduling::Independent {
                        max_response_bytes: 1024,
                    }
                }
            }
            assert_eq!(
                core.run(r, None),
                Err(if kind == 0 {
                    "exactPrefetch requires the app's own plain HTTP read"
                } else {
                    "exactPrefetch requires a plain HTTP read"
                })
            );
        }
        assert_eq!(server.count(), 0);
    }
}

#[test]
fn the_ninth_complete_prefetch_evicts_the_oldest_and_an_oversized_reply_still_returns() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        let clock = clock(&core);
        let held = vec![b'x'; (2 << 20) - (8 << 10)];
        for i in 0..=MAX_PREFETCHES {
            let path = format!("/{i}");
            server.plan(&path, Plan::new(200, &[], &held));
            ask(
                &core,
                &woke,
                job(i as u64 + 1, server.request(&path).prefetch()),
            );
            *clock.lock().unwrap() += Duration::from_millis(1);
            if i == MAX_PREFETCHES - 1 {
                assert!(read(&core, &woke, &server, 100, "/0").1);
            }
        }
        server.plan("/0", Plan::new(200, &[], b"sent"));
        assert!(!read(&core, &woke, &server, 101, "/0").1);
        for i in 1..=MAX_PREFETCHES {
            assert!(read(&core, &woke, &server, 102 + i as u64, &format!("/{i}")).1);
        }
        let body = vec![b'x'; cache::MAX_ENTRY + 1];
        server.plan("/large", Plan::new(200, &[], &body));
        let (response, _) = ask(&core, &woke, job(200, server.request("/large").prefetch()));
        assert_eq!(response.body, body);
        server.plan("/large", Plan::new(200, &[], b"sent"));
        assert!(!read(&core, &woke, &server, 201, "/large").1);
        let state = core.shared.state.lock().unwrap();
        assert!(state.cache.as_ref().unwrap().bytes <= cache::CAPACITY);
    }
}

#[test]
fn a_failed_prefetch_returns_its_failure_and_sends_the_waiting_read() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (release, held) = channel();
        let mut plan = Plan::new(200, &[], b"");
        plan.disconnect = true;
        plan.held = Some(held);
        server.plan("/query", plan);
        let (core, woke) = server.core(asynchronous, true);
        let mut request = server.request("/query");
        request.method = "POST".into();
        core.run(job(1, request.clone().prefetch()), None).unwrap();
        server.seen(1);
        core.run(job(2, request), None).unwrap();
        server.plan("/query", Plan::new(200, &[], b"sent"));
        release.send(()).unwrap();
        let (ticket, outcome, _) = collect(&core, &woke);
        assert_eq!(ticket, 1);
        assert!(matches!(
            outcome,
            Outcome::Failed {
                kind: FailureKind::Network,
                ..
            }
        ));
        assert!(matches!(collect(&core, &woke).1, Outcome::Response(r) if r.body == b"sent"));
        assert_eq!(server.count(), 2);
    }
}

#[test]
fn a_prefetch_waits_behind_a_send_and_survives_forgetting_before_it_starts() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let write = held_plan(&server, "/write");
        let prefetch = held_plan(&server, "/next");
        let (core, woke) = server.core(asynchronous, true);
        let mut send = job(1, server.request("/write"));
        send.class = RequestClass::Ordered;
        core.run(send, None).unwrap();
        server.seen(1);
        core.run(job(2, server.request("/next").prefetch()), None)
            .unwrap();
        core.forget(|ticket| ticket == 1);
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.jobs[0].len(), 1);
            assert_eq!(server.count(), 1);
        }
        write.send(()).unwrap();
        assert_eq!(collect(&core, &woke).0, 1);
        server.seen(2);
        core.run(job(3, server.request("/next")), None).unwrap();
        prefetch.send(()).unwrap();
        let (ticket, _, timing) = collect(&core, &woke);
        assert_eq!(ticket, 3);
        assert!(timing.cached);
        assert_eq!(server.count(), 2);
    }
}

#[test]
fn prefetch_holds_share_the_http_caches_byte_budget() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        let _clock = clock(&core);
        let body = vec![b'x'; 3 << 20];
        server.plan(
            "/cache",
            Plan::new(200, &[("Cache-Control", "max-age=60")], &body),
        );
        read(&core, &woke, &server, 1, "/cache");
        for ticket in 2..=7 {
            let path = format!("/{ticket}");
            server.plan(&path, Plan::new(503, &[], &body));
            ask(&core, &woke, job(ticket, server.request(&path).prefetch()));
            let state = core.shared.state.lock().unwrap();
            assert!(state.cache.as_ref().unwrap().bytes <= cache::CAPACITY);
        }
        server.plan("/2", Plan::new(200, &[], b"evicted"));
        assert!(!read(&core, &woke, &server, 8, "/2").1);
        assert!(read(&core, &woke, &server, 9, "/7").1);
    }
}

#[test]
fn a_prefetched_redirect_reply_requires_every_visited_url_in_the_readers_scope() {
    for asynchronous in [false, true] {
        let first = Server::new();
        let middle = Server::new();
        let hop = middle.request("/hop").url;
        let end = first.request("/final").url;
        first.plan("/start", Plan::new(302, &[("Location", &hop)], b""));
        middle.plan("/hop", Plan::new(302, &[("Location", &end)], b""));
        first.plan("/final", Plan::new(503, &[], b"held"));
        let grants = format!(
            "net.fetch http://{}\nnet.fetch http://{}",
            first.address, middle.address
        );
        let (core, woke) = first.core_grants(asynchronous, true, &grants);
        ask(&core, &woke, job(1, first.request("/start").prefetch()));
        first.plan("/start", Plan::new(302, &[("Location", &hop)], b""));
        let mut scoped = first.request("/start");
        scoped.grants = Some(format!("net.fetch http://{}", first.address));
        core.run(job(2, scoped), None).unwrap();
        let (_, outcome, timing) = collect(&core, &woke);
        assert!(!timing.cached);
        assert!(matches!(
            outcome,
            Outcome::Failed {
                kind: FailureKind::Refused,
                ..
            }
        ));
        assert_eq!(first.count(), 3);
        assert_eq!(middle.count(), 1);
        let (reply, cached) = read(&core, &woke, &first, 3, "/start");
        assert!(cached);
        assert_eq!(reply.body, b"held");
        assert_eq!(first.count(), 3);
    }
}

#[test]
fn a_driver_failure_answers_a_prefetch_before_the_held_reply_or_transport() {
    for asynchronous in [false, true] {
        let server = Server::new();
        server.plan("/next", Plan::new(503, &[], b"held"));
        let (core, woke) = server.core(asynchronous, true);
        ask(&core, &woke, job(1, server.request("/next").prefetch()));
        core.run(
            job(2, server.request("/next").prefetch()),
            Some(Box::new(|| failed(FailureKind::Network, "driver fault"))),
        )
        .unwrap();
        let (_, outcome, timing) = collect(&core, &woke);
        assert!(!timing.cached);
        assert!(
            matches!(outcome, Outcome::Failed { kind: FailureKind::Network, message } if message == "driver fault")
        );
        assert!(read(&core, &woke, &server, 3, "/next").1);
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn prefetch_refuses_tool_wake_and_device_reaches_without_holding_a_reply() {
    for asynchronous in [false, true] {
        for origin in [
            exact_runner::CallOrigin::Tool,
            exact_runner::CallOrigin::Wake,
            exact_runner::CallOrigin::Device,
        ] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            let mut request = server.request("/next").prefetch();
            request.reach.origin = origin;
            let result = core.run(job(1, request), None);
            assert_eq!(
                result,
                Err("exactPrefetch requires the app's own plain HTTP read")
            );
            assert!(core
                .shared
                .state
                .lock()
                .unwrap()
                .cache
                .as_ref()
                .unwrap()
                .prefetches
                .is_empty());
            server.plan("/next", Plan::new(200, &[], b"sent"));
            assert!(core.resume_ordered().is_empty());
            assert!(!read(&core, &woke, &server, 2, "/next").1);
            assert_eq!(server.count(), 1);
        }
    }
}

#[test]
fn a_forgotten_prefetch_survives_pending_definitions_but_its_abort_removes_it() {
    for asynchronous in [false, true] {
        for function in [false, true] {
            for aborted in [false, true] {
                let server = Server::new();
                let set = Preloads::start(Vec::new(), "", ibex2::host::Host::new);
                let control = set.control();
                let now = Instant::now();
                control.test_clock(Arc::new(move || now));
                if function {
                    control.launch_pending(Some(Duration::from_secs(5)));
                } else {
                    control.definitions_pending(true);
                }
                let (core, woke) = server.core(asynchronous, true);
                assert!(core.adopt_preloads(set).is_ok());
                let release = held_plan(&server, "/next");
                core.run(job(1, server.request("/next").prefetch()), None)
                    .unwrap();
                core.forget(|_| false);
                let (queued, running) = {
                    let state = core.shared.state.lock().unwrap();
                    (state.jobs[0].len(), state.running.len())
                };
                assert_eq!(queued, 1);
                assert_eq!(running, 0);
                if aborted {
                    core.cancel(&[1]);
                }
                control.launch_pending(None);
                control.definitions_pending(false);
                if !aborted {
                    server.seen(1);
                }
                core.run(job(2, server.request("/next")), None).unwrap();
                release.send(()).unwrap();
                let (ticket, reply, timing) = collect(&core, &woke);
                assert_eq!(ticket, 2);
                assert_eq!(timing.cached, !aborted);
                assert!(matches!(reply, Outcome::Response(r) if r.body == b"prefetched"));
                assert_eq!(server.count(), 1);
            }
        }
    }
}

#[test]
fn a_refresh_drops_an_equal_prefetch_ready_or_inflight() {
    for asynchronous in [false, true] {
        for pending in [false, true] {
            for mode in 0..3 {
                let server = Server::new();
                let release = held_plan(&server, "/next");
                let (core, woke) = server.core(asynchronous, true);
                core.run(job(1, server.request("/next").prefetch()), None)
                    .unwrap();
                server.seen(1);
                if !pending {
                    release.send(()).unwrap();
                    collect(&core, &woke);
                }
                server.plan("/next", Plan::new(200, &[], b"new"));
                let mut refresh = job(2, server.request("/next").preload("feed").unwrap());
                match mode {
                    0 => refresh.forced = true,
                    1 => refresh.request.cache = exact_runner::CacheMode::Reload,
                    _ => refresh.request.cache = exact_runner::CacheMode::NoCache,
                }
                core.run(refresh, None).unwrap();
                if pending {
                    release.send(()).unwrap();
                    collect(&core, &woke);
                }
                assert!(
                    matches!(collect(&core, &woke).1, Outcome::Response(r) if r.body == b"new")
                );
                server.plan("/next", Plan::new(200, &[], b"sent"));
                let (reply, cached) = read(&core, &woke, &server, 3, "/next");
                assert!(!cached);
                assert_eq!(reply.body, b"sent");
                assert_eq!(server.count(), 3);
            }
        }
    }
}

#[test]
fn idle_prefetch_holds_release_their_bytes_when_the_timer_fires() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        let clock = clock(&core);
        let before = core
            .shared
            .state
            .lock()
            .unwrap()
            .cache
            .as_ref()
            .unwrap()
            .bytes;
        for ticket in 1..=8 {
            let path = format!("/{ticket}");
            server.plan(&path, Plan::new(503, &[], &[b'x'; 1024]));
            ask(&core, &woke, job(ticket, server.request(&path).prefetch()));
        }
        {
            let state = core.shared.state.lock().unwrap();
            assert!(state.jobs.iter().all(VecDeque::is_empty));
            let cache = state.cache.as_ref().unwrap();
            assert_eq!(
                cache
                    .prefetches
                    .iter()
                    .filter(|p| p.used().is_some())
                    .count(),
                8
            );
            assert!(cache.bytes > before + 8 * 1024);
        }
        *clock.lock().unwrap() += HOLD;
        if let Some(machine) = &core.machine {
            machine.timer.expire((0, 0));
        } else {
            core.shared.ready.notify_all();
        }
        let state = core.shared.state.lock().unwrap();
        let (state, timeout) = core
            .shared
            .ready
            .wait_timeout_while(state, HANG, |state| {
                state.cache.as_ref().unwrap().bytes != before
            })
            .unwrap();
        assert!(
            !timeout.timed_out(),
            "idle prefetch bytes did not expire: asynchronous={asynchronous}"
        );
        assert_eq!(state.cache.as_ref().unwrap().bytes, before);
        assert!(state
            .cache
            .as_ref()
            .unwrap()
            .prefetches
            .iter()
            .all(|p| matches!(p.reply, ReplyState::Gone)));
        assert_eq!(server.count(), 8);
    }
}
