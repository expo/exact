use super::*;
use std::sync::mpsc::Receiver;

fn runtime(
    server: &Server,
    asynchronous: bool,
    adopted: bool,
) -> (Core, Receiver<()>, Arc<Mutex<Instant>>) {
    let (core, woke) = server.core(asynchronous, true);
    let (set, clock) = fresh_clock(server, Vec::new());
    if adopted {
        adopt(&core, set);
    } else {
        let now = clock.clone();
        core.shared
            .state
            .lock()
            .unwrap()
            .cache
            .as_mut()
            .unwrap()
            .clock = Arc::new(move || *now.lock().unwrap());
    }
    (core, woke, clock)
}
fn expect(core: &Core, woke: &Receiver<()>, ticket: u64, body: &[u8], cached: bool) {
    let (got, outcome, timing) = collect(core, woke);
    assert_eq!(got, ticket);
    assert_eq!(timing.cached, cached);
    assert!(matches!(outcome, Outcome::Response(r) if r.body == body));
}
fn gone(core: &Core) {
    let state = core.shared.state.lock().unwrap();
    let (state, timeout) = core
        .shared
        .ready
        .wait_timeout_while(state, HANG, |s| {
            s.cache
                .as_ref()
                .unwrap()
                .preloads
                .iter()
                .any(|p| !matches!(p.reply, ReplyState::Gone))
        })
        .unwrap();
    assert!(!timeout.timed_out());
    assert_eq!(state.cache.as_ref().unwrap().bytes, 0);
}
fn tick(core: &Core) {
    core.preloads
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send
        .send(Message::Tick)
        .unwrap();
}

#[test]
fn a_prefetch_holds_one_exchange_until_ten_seconds_after_completion_with_or_without_adoption() {
    for asynchronous in [false, true] {
        for adopted in [false, true] {
            let server = Server::new();
            let (core, woke, clock) = runtime(&server, asynchronous, adopted);
            let release = held_plan(&server, "/next", b"held");
            core.run(job(1, server.request("/next").prefetch()), None)
                .unwrap();
            server.seen(1);
            *clock.lock().unwrap() += Duration::from_secs(4);
            release.send(()).unwrap();
            expect(&core, &woke, 1, b"held", true);
            *clock.lock().unwrap() += Duration::from_secs(9);
            assert_eq!(read(&core, &woke, &server, 2, "/next").0.body, b"held");
            assert_eq!(server.count(), 1);
            *clock.lock().unwrap() += Duration::from_secs(1);
            server.plan("/next", Plan::new(200, &[], b"sent"));
            assert!(!read(&core, &woke, &server, 3, "/next").1);
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn equal_reads_and_prefetches_wait_for_the_same_preload_in_flight() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke, _) = runtime(&server, asynchronous, false);
        let release = held_plan(&server, "/next", b"held");
        for (ticket, prefetch) in [(1, true), (2, false), (3, true)] {
            let mut request = server.request("/next");
            request.prefetch = prefetch;
            core.run(job(ticket, request), None).unwrap();
        }
        server.seen(1);
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.cache.as_ref().unwrap().preloads.len(), 1);
            assert_eq!(state.jobs[0].len(), 3);
            assert!(state.running.is_empty());
        }
        release.send(()).unwrap();
        for ticket in 1..=3 {
            expect(&core, &woke, ticket, b"held", true);
        }
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn forgetting_or_aborting_the_caller_releases_it_and_leaves_the_preload() {
    for asynchronous in [false, true] {
        for abort in [false, true] {
            for method in ["GET", "POST"] {
                let server = Server::new();
                let (core, woke, _) = runtime(&server, asynchronous, false);
                let release = held_plan(&server, "/next", b"held");
                let mut request = server.request("/next").prefetch();
                request.method = method.into();
                core.run(job(1, request.clone()), None).unwrap();
                server.seen(1);
                if abort {
                    core.cancel(&[1]);
                } else {
                    core.forget(|_| false);
                }
                {
                    let state = core.shared.state.lock().unwrap();
                    assert_eq!(state.counts, [0, 0]);
                    assert_eq!(state.bytes, [0, 0]);
                    assert!(
                        state.ordered.is_empty()
                            && state.jobs[0].is_empty()
                            && state.running.is_empty()
                            && state.completed[0].is_empty()
                    );
                    assert!(matches!(
                        state.cache.as_ref().unwrap().preloads[0].reply,
                        ReplyState::Pending { .. }
                    ));
                }
                release.send(()).unwrap();
                ready(&core, 1);
                assert!(core.drain().is_empty());
                request.prefetch = false;
                assert!(ask(&core, &woke, job(2, request)).1);
                assert_eq!(server.count(), 1);
            }
        }
    }
}

#[test]
fn refreshing_drops_the_preload_and_waiters_fetch_in_their_delivery_order() {
    for asynchronous in [false, true] {
        for mode in 0..3 {
            let server = Server::new();
            let (core, woke, _) = runtime(&server, asynchronous, false);
            let old = held_plan(&server, "/next", b"old");
            core.run(job(20, server.request("/next").prefetch()), None)
                .unwrap();
            server.seen(1);
            let (first, held) = channel();
            let mut plan = Plan::new(200, &[], b"new");
            plan.held = Some(held);
            plan.cache_control = Some("");
            server.plan("/next", plan);
            let (second, held) = channel();
            let mut plan = Plan::new(200, &[], b"new");
            plan.held = Some(held);
            plan.cache_control = Some(if mode == 2 { "max-age=0" } else { "no-cache" });
            server.plan("/next", plan);
            let mut refresh = job(10, server.request("/next"));
            match mode {
                0 => refresh.forced = true,
                1 => refresh.request.cache = exact_runner::CacheMode::Reload,
                _ => refresh.request.cache = exact_runner::CacheMode::NoCache,
            }
            core.run(refresh, None).unwrap();
            server.seen(3);
            assert!(matches!(
                core.shared
                    .state
                    .lock()
                    .unwrap()
                    .cache
                    .as_ref()
                    .unwrap()
                    .preloads[0]
                    .reply,
                ReplyState::Gone
            ));
            second.send(()).unwrap();
            {
                let end = Instant::now() + HANG;
                let mut state = core.shared.state.lock().unwrap();
                // An out-of-order worker completion has no presenter wake (LLP 1162 D1).
                while !state.completed[0].iter().any(|d| d.ticket == 10) {
                    assert!(Instant::now() < end, "refresh did not complete");
                    state = core
                        .shared
                        .ready
                        .wait_timeout(state, Duration::from_millis(50))
                        .unwrap()
                        .0;
                }
            }
            assert!(core.drain().is_empty());
            first.send(()).unwrap();
            old.send(()).unwrap();
            expect(&core, &woke, 20, b"new", false);
            expect(&core, &woke, 10, b"new", false);
            gone(&core);
            assert_eq!(server.count(), 3);
        }
    }
}

#[test]
fn a_send_to_the_url_drops_a_prefetch_hold() {
    for asynchronous in [false, true] {
        for pending in [false, true] {
            let server = Server::new();
            let (core, woke, _) = runtime(&server, asynchronous, false);
            let release = held_plan(&server, "/next", b"old");
            core.run(job(1, server.request("/next").prefetch()), None)
                .unwrap();
            server.seen(1);
            if pending {
                core.forget(|_| false);
            } else {
                release.send(()).unwrap();
                expect(&core, &woke, 1, b"old", true);
            }
            server.plan("/next", Plan::new(200, &[], b"sent"));
            let mut send = job(2, server.request("/next"));
            send.class = RequestClass::Ordered;
            send.request.method = "POST".into();
            core.run(send, None).unwrap();
            expect(&core, &woke, 2, b"sent", false);
            gone(&core);
            if pending {
                release.send(()).unwrap();
            }
            server.plan("/next", Plan::new(200, &[], b"after"));
            assert!(!read(&core, &woke, &server, 3, "/next").1);
            assert_eq!(server.count(), 3);
        }
    }
}

#[test]
fn a_failed_or_oversized_preload_sends_the_waiting_read_and_holds_nothing() {
    for asynchronous in [false, true] {
        for oversized in [false, true] {
            let server = Server::new();
            let (core, woke, _) = runtime(&server, asynchronous, false);
            let (release, held) = channel();
            let body = if oversized {
                vec![0; cache::MAX_ENTRY + 1]
            } else {
                b"ignored".to_vec()
            };
            let mut plan = Plan::new(200, &[], &body);
            plan.disconnect = !oversized;
            plan.held = Some(held);
            server.plan("/next", plan);
            server.plan("/next", Plan::new(200, &[], b"fallback"));
            let mut request = server.request("/next").prefetch();
            request.method = "POST".into();
            core.run(job(1, request), None).unwrap();
            server.seen(1);
            release.send(()).unwrap();
            expect(&core, &woke, 1, b"fallback", false);
            gone(&core);
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn a_prefetch_redirect_stays_within_the_asking_sources_scope() {
    for asynchronous in [false, true] {
        let first = Server::new();
        let outside = Server::new();
        let grants = format!(
            "net.fetch http://{}\nnet.fetch http://{}",
            first.address, outside.address
        );
        let (core, woke) = first.core_grants(asynchronous, true, &grants);
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
        let target = outside.request("/private").url;
        let (release, held) = channel();
        let mut plan = Plan::new(302, &[("Location", &target)], b"");
        plan.held = Some(held);
        first.plan("/start", plan);
        first.plan("/start", Plan::new(302, &[("Location", &target)], b""));
        let mut request = first.request("/start").prefetch();
        request.grants = Some(format!("net.fetch http://{}", first.address));
        core.run(job(1, request), None).unwrap();
        first.seen(1);
        release.send(()).unwrap();
        let (_, outcome, timing) = collect(&core, &woke);
        assert!(!timing.cached);
        assert!(matches!(
            outcome,
            Outcome::Failed {
                kind: FailureKind::Refused,
                ..
            }
        ));
        gone(&core);
        assert_eq!(first.count(), 2);
        assert_eq!(outside.count(), 0);
    }
}

#[test]
fn another_reach_or_a_source_outside_a_visited_scope_cannot_take_the_prefetch() {
    for asynchronous in [false, true] {
        let first = Server::new();
        let middle = Server::new();
        let grants = format!(
            "net.fetch http://{}\nnet.fetch http://{}",
            first.address, middle.address
        );
        let (core, woke) = first.core_grants(asynchronous, true, &grants);
        let now = Instant::now();
        core.shared
            .state
            .lock()
            .unwrap()
            .cache
            .as_mut()
            .unwrap()
            .clock = Arc::new(move || now);
        let target = middle.request("/middle").url;
        first.plan("/start", Plan::new(302, &[("Location", &target)], b""));
        let target = first.request("/end").url;
        middle.plan("/middle", Plan::new(302, &[("Location", &target)], b""));
        let release = held_plan(&first, "/end", b"held");
        core.run(job(1, first.request("/start").prefetch()), None)
            .unwrap();
        first.seen(2);
        release.send(()).unwrap();
        expect(&core, &woke, 1, b"held", true);
        for (ticket, restricted) in [(2, false), (3, true)] {
            first.plan("/start", Plan::new(200, &[], b"own"));
            let mut request = first.request("/start");
            if restricted {
                request.grants = Some(format!("net.fetch http://{}", first.address));
            } else {
                request.reach.origin = exact_runner::CallOrigin::Tool;
            }
            assert_eq!(ask(&core, &woke, job(ticket, request)).0.body, b"own");
        }
        assert_eq!(first.count(), 4);
        assert!(read(&core, &woke, &first, 4, "/start").1);
    }
}

#[test]
fn eight_prefetches_evict_the_oldest_complete_or_leave_the_ninth_a_plain_read() {
    for asynchronous in [false, true] {
        for pending in [false, true] {
            let server = Server::new();
            let (core, woke, _) = runtime(&server, asynchronous, true);
            let mut releases = Vec::new();
            for ticket in 1..=8 {
                let path = format!("/{ticket}");
                releases.push(held_plan(&server, &path, b"held"));
                core.run(job(ticket, server.request(&path).prefetch()), None)
                    .unwrap();
                if !pending {
                    releases.pop().unwrap().send(()).unwrap();
                    expect(&core, &woke, ticket, b"held", true);
                }
            }
            server.seen(8);
            server.plan("/9", Plan::new(200, &[], b"ninth"));
            core.run(job(9, server.request("/9").prefetch()), None)
                .unwrap();
            server.seen(9);
            if pending {
                for release in releases {
                    release.send(()).unwrap();
                }
                for ticket in 1..=8 {
                    expect(&core, &woke, ticket, b"held", true);
                }
            }
            expect(&core, &woke, 9, b"ninth", !pending);
            assert_eq!(
                core.shared
                    .state
                    .lock()
                    .unwrap()
                    .cache
                    .as_ref()
                    .unwrap()
                    .preloads
                    .len(),
                8
            );
            if !pending {
                server.plan("/1", Plan::new(200, &[], b"evicted"));
                assert!(!read(&core, &woke, &server, 10, "/1").1);
            }
            assert!(read(&core, &woke, &server, 11, "/2").1);
            assert_eq!(server.count(), if pending { 9 } else { 10 });
        }
    }
}

#[test]
fn other_reaches_and_an_executor_without_a_cache_start_no_preload() {
    for asynchronous in [false, true] {
        for origin in [
            exact_runner::CallOrigin::App,
            exact_runner::CallOrigin::Tool,
            exact_runner::CallOrigin::Wake,
            exact_runner::CallOrigin::Device,
        ] {
            let server = Server::new();
            let cache = origin != exact_runner::CallOrigin::App;
            let (core, woke) = server.core(asynchronous, cache);
            if cache {
                let now = Instant::now();
                core.shared
                    .state
                    .lock()
                    .unwrap()
                    .cache
                    .as_mut()
                    .unwrap()
                    .clock = Arc::new(move || now);
            }
            let release = held_plan(&server, "/next", b"plain");
            let mut request = server.request("/next").prefetch();
            request.reach.origin = origin;
            core.run(job(1, request), None).unwrap();
            server.seen(1);
            release.send(()).unwrap();
            expect(&core, &woke, 1, b"plain", false);
            assert!(core.preloads.lock().unwrap().is_none());
            assert_eq!(server.count(), 1);
        }
    }
}

#[test]
fn an_idle_executor_expires_a_prefetch_without_a_host_pump() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke, clock) = runtime(&server, asynchronous, false);
        let release = held_plan(&server, "/next", b"held");
        core.run(job(1, server.request("/next").prefetch()), None)
            .unwrap();
        server.seen(1);
        release.send(()).unwrap();
        expect(&core, &woke, 1, b"held", true);
        assert!(core.ordered_idle());
        *clock.lock().unwrap() += HOLD;
        tick(&core);
        gone(&core);
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn a_speculative_refusal_leaves_the_callers_ordinary_fetch() {
    struct Constrained(Sender<bool>);
    impl ibex2::stdlib::fetch::Transport for Constrained {
        fn open(
            &self,
            request: &ibex2::stdlib::fetch::Request,
            signal: &ibex2::stdlib::abort::AbortSignal,
        ) -> Result<ibex2::stdlib::fetch::StreamingResponse, HostError> {
            self.0.send(request.speculative).unwrap();
            if request.speculative {
                return Err(HostError::Failed("Low Data Mode".into()));
            }
            ibex2::transport::RustlsHttpTransport::new().open(request, signal)
        }
    }
    for asynchronous in [false, true] {
        let server = Server::new();
        let grants = format!("net.fetch http://{}", server.address);
        let (sent, seen) = channel();
        let host = Arc::new(move || {
            ibex2::host::Host::with_transport(Box::new(Constrained(sent.clone())))
        });
        let owners = (0..WORKERS)
            .map(|_| Some(host().endow(ibex2::grant::GrantSet::parse(&grants).unwrap())))
            .collect();
        let (wake, woke) = channel();
        let core = Core::with_owners_lanes(
            owners,
            &grants,
            Box::new(move || {
                let _ = wake.send(());
            }),
            host,
            asynchronous.then(asynch_lanes),
        );
        core.enable_http_cache();
        let now = Instant::now();
        core.shared
            .state
            .lock()
            .unwrap()
            .cache
            .as_mut()
            .unwrap()
            .clock = Arc::new(move || now);
        let release = held_plan(&server, "/next", b"plain");
        core.run(job(1, server.request("/next").prefetch()), None)
            .unwrap();
        server.seen(1);
        release.send(()).unwrap();
        expect(&core, &woke, 1, b"plain", false);
        assert!(seen.recv_timeout(HANG).unwrap());
        assert!(!seen.recv_timeout(HANG).unwrap());
        assert!(seen.try_recv().is_err());
        gone(&core);
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn prefetches_have_eight_places_beside_eight_late_launch_preloads() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (set, _clock) = fresh_clock(&server, Vec::new());
        let control = set.control();
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        let mut releases = Vec::new();
        let mut definitions = Vec::new();
        for ticket in 1..=16 {
            let path = format!("/{ticket}");
            releases.push(held_plan(&server, &path, b"held"));
            if ticket <= 8 {
                core.run(job(ticket, server.request(&path).prefetch()), None)
                    .unwrap();
            } else {
                let mut def = definition(&server, &path);
                def.secrets.push("session".into());
                definitions.push(def);
            }
        }
        control.extend(
            definitions,
            &format!("net.fetch http://{}", server.address),
            Arc::new(|_, _| {}),
        );
        server.seen(16);
        {
            let state = core.shared.state.lock().unwrap();
            let holds = &state.cache.as_ref().unwrap().preloads;
            assert_eq!(holds.len(), 16);
            assert_eq!(holds.iter().filter(|p| p.prefetch).count(), 8);
        }
        for release in releases {
            release.send(()).unwrap();
        }
        for ticket in 1..=8 {
            expect(&core, &woke, ticket, b"held", true);
        }
        ready(&core, 16);
        assert!(read(&core, &woke, &server, 17, "/16").1);
        core.drop_preload_secret("session");
        server.plan("/16", Plan::new(200, &[], b"sent"));
        assert!(!read(&core, &woke, &server, 18, "/16").1);
        assert!(read(&core, &woke, &server, 19, "/1").1);
        assert_eq!(server.count(), 17);
    }
}

#[test]
fn a_prefetch_joins_an_equal_launch_preload_pending_or_complete() {
    for asynchronous in [false, true] {
        for pending in [false, true] {
            let server = Server::new();
            let release = held_plan(&server, "/next", b"launch");
            let (set, _clock) = fresh_clock(&server, vec![definition(&server, "/next")]);
            server.seen(1);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            if !pending {
                release.send(()).unwrap();
                ready(&core, 1);
            }
            core.run(job(1, server.request("/next").prefetch()), None)
                .unwrap();
            assert_eq!(
                core.shared
                    .state
                    .lock()
                    .unwrap()
                    .cache
                    .as_ref()
                    .unwrap()
                    .preloads
                    .len(),
                1
            );
            if pending {
                release.send(()).unwrap();
            }
            expect(&core, &woke, 1, b"launch", true);
            assert_eq!(server.count(), 1);
        }
    }
}

#[test]
fn prefetch_preloads_use_the_request_deadline_or_sixty_seconds() {
    for asynchronous in [false, true] {
        for timeout in [None, Some(1000)] {
            let server = Server::new();
            let (core, woke, clock) = runtime(&server, asynchronous, false);
            let release = held_plan(&server, "/next", b"late");
            let mut request = server.request("/next").prefetch();
            request.timeout_ms = timeout;
            core.run(job(1, request), None).unwrap();
            server.seen(1);
            let delay = Duration::from_millis(timeout.unwrap_or(60_000).into());
            {
                let state = core.shared.state.lock().unwrap();
                let ReplyState::Pending { due, .. } =
                    &state.cache.as_ref().unwrap().preloads[0].reply
                else {
                    panic!("pending preload");
                };
                assert_eq!(*due, *clock.lock().unwrap() + delay);
            }
            server.plan("/next", Plan::new(200, &[], b"fallback"));
            *clock.lock().unwrap() += delay;
            tick(&core);
            expect(&core, &woke, 1, b"fallback", false);
            release.send(()).unwrap();
            gone(&core);
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn prefetch_holds_share_the_cache_bytes_and_http_freshness_survives_expiry_and_slot_reuse() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke, clock) = runtime(&server, asynchronous, false);
        for ticket in 1..=8 {
            let path = format!("/{ticket}");
            let release = held_plan(&server, &path, &vec![0; 3 << 20]);
            core.run(job(ticket, server.request(&path).prefetch()), None)
                .unwrap();
            server.seen(ticket as usize);
            release.send(()).unwrap();
            let (_, outcome, timing) = collect(&core, &woke);
            assert!(timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body.len() == 3 << 20));
        }
        {
            let state = core.shared.state.lock().unwrap();
            let cache = state.cache.as_ref().unwrap();
            assert!(cache.bytes <= cache::CAPACITY);
            assert!(cache.preloads.iter().filter(|p| p.used().is_some()).count() < 8);
        }
        let (release, held) = channel();
        let mut fresh = Plan::new(200, &[("Cache-Control", "max-age=60")], b"fresh");
        fresh.held = Some(held);
        server.plan("/fresh", fresh);
        core.run(job(9, server.request("/fresh").prefetch()), None)
            .unwrap();
        server.seen(9);
        release.send(()).unwrap();
        expect(&core, &woke, 9, b"fresh", true);
        *clock.lock().unwrap() += HOLD;
        tick(&core);
        {
            let state = core.shared.state.lock().unwrap();
            let (state, timeout) = core
                .shared
                .ready
                .wait_timeout_while(state, HANG, |s| {
                    s.cache
                        .as_ref()
                        .unwrap()
                        .preloads
                        .iter()
                        .any(|p| !matches!(p.reply, ReplyState::Gone))
                })
                .unwrap();
            assert!(!timeout.timed_out());
            assert!(state.cache.as_ref().unwrap().bytes > 0);
        }
        assert!(read(&core, &woke, &server, 10, "/fresh").1);
        let release = held_plan(&server, "/replacement", b"replacement");
        core.run(job(12, server.request("/replacement").prefetch()), None)
            .unwrap();
        server.seen(10);
        release.send(()).unwrap();
        expect(&core, &woke, 12, b"replacement", true);
        let mut refresh = job(13, server.request("/replacement"));
        refresh.forced = true;
        server.plan("/replacement", Plan::new(200, &[], b"new"));
        ask(&core, &woke, refresh);
        assert!(read(&core, &woke, &server, 14, "/fresh").1);
        assert_eq!(server.count(), 11);
    }
}

#[test]
fn a_prefetch_behind_a_running_or_queued_send_sends_itself_and_holds_nothing() {
    for asynchronous in [false, true] {
        for class in [
            RequestClass::Ordered,
            RequestClass::Parallel(0),
            RequestClass::Read,
        ] {
            let server = Server::new();
            let (core, woke, _) = runtime(&server, asynchronous, false);
            let release = held_plan(&server, "/ahead", b"ahead");
            let mut ahead = job(1, server.request("/ahead"));
            ahead.class = class;
            if class != RequestClass::Read {
                ahead.request.method = "POST".into();
            }
            core.run(ahead, None).unwrap();
            server.seen(1);
            if class == RequestClass::Read {
                server.plan("/send", Plan::new(200, &[], b"send"));
                let mut send = job(2, server.request("/send"));
                send.class = RequestClass::Ordered;
                send.request.method = "POST".into();
                core.run(send, None).unwrap();
            }
            let caller = held_plan(&server, "/next", b"caller");
            core.run(job(3, server.request("/next").prefetch()), None)
                .unwrap();
            {
                let mut state = core.shared.state.lock().unwrap();
                assert!(state.cache.as_ref().unwrap().preloads.is_empty());
                assert!(state.jobs[0].iter().any(|j| j.ticket == 3));
                assert!(reads::next_job(&core.shared, &mut state, 1).is_none());
            }
            assert!(core.preloads.lock().unwrap().is_none());
            assert_eq!(server.count(), 1);
            release.send(()).unwrap();
            expect(&core, &woke, 1, b"ahead", false);
            let exchanges = if class == RequestClass::Read {
                expect(&core, &woke, 2, b"send", false);
                3
            } else {
                2
            };
            server.seen(exchanges);
            caller.send(()).unwrap();
            expect(&core, &woke, 3, b"caller", false);
            gone(&core);
            server.plan("/next", Plan::new(200, &[], b"later"));
            let (reply, cached) = read(&core, &woke, &server, 4, "/next");
            assert_eq!(reply.body, b"later");
            assert!(!cached);
            assert_eq!(server.count(), exchanges + 1);
        }
    }
}

#[test]
fn a_prefetch_behind_a_barrier_or_fence_never_starts_a_preload_on_readmission() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke, _) = runtime(&server, asynchronous, false);
        core.shared.state.lock().unwrap().ordered_barrier = true;
        let first = held_plan(&server, "/first", b"first");
        core.run(job(1, server.request("/first").prefetch()), None)
            .unwrap();
        core.shared.state.lock().unwrap().ordered_barrier = false;
        let second = held_plan(&server, "/second", b"second");
        core.run(job(2, server.request("/second").prefetch()), None)
            .unwrap();
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.fenced.len(), 2);
            assert!(state.jobs[0].is_empty());
            assert!(state.cache.as_ref().unwrap().preloads.is_empty());
        }
        assert_eq!(server.count(), 0);
        assert!(core.resume_ordered().is_empty());
        server.seen(2);
        assert!(core.preloads.lock().unwrap().is_none());
        first.send(()).unwrap();
        second.send(()).unwrap();
        expect(&core, &woke, 1, b"first", false);
        expect(&core, &woke, 2, b"second", false);
        gone(&core);
        server.plan("/first", Plan::new(200, &[], b"later"));
        assert!(!read(&core, &woke, &server, 3, "/first").1);
        assert_eq!(server.count(), 3);
    }
}

#[test]
fn forgetting_a_prefetch_queued_behind_a_send_leaves_no_exchange_or_hold() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke, _) = runtime(&server, asynchronous, false);
        let release = held_plan(&server, "/send", b"sent");
        let mut send = job(1, server.request("/send"));
        send.class = RequestClass::Ordered;
        send.request.method = "POST".into();
        core.run(send, None).unwrap();
        server.seen(1);
        core.run(job(2, server.request("/next").prefetch()), None)
            .unwrap();
        core.forget(|ticket| ticket != 2);
        {
            let state = core.shared.state.lock().unwrap();
            assert!(state.jobs[0].is_empty());
            assert!(state.cache.as_ref().unwrap().preloads.is_empty());
        }
        release.send(()).unwrap();
        expect(&core, &woke, 1, b"sent", false);
        assert_eq!(server.count(), 1);
        server.plan("/next", Plan::new(200, &[], b"later"));
        let (reply, cached) = read(&core, &woke, &server, 3, "/next");
        assert_eq!(reply.body, b"later");
        assert!(!cached);
        assert_eq!(server.count(), 2);
        assert!(core.ordered_idle());
        gone(&core);
    }
}

#[test]
fn a_send_asked_after_a_prefetch_to_another_url_leaves_its_hold_for_the_equal_read() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke, _) = runtime(&server, asynchronous, false);
        let release = held_plan(&server, "/next", b"held");
        core.run(job(1, server.request("/next").prefetch()), None)
            .unwrap();
        server.seen(1);
        let sent = held_plan(&server, "/send", b"sent");
        let mut send = job(2, server.request("/send"));
        send.class = RequestClass::Ordered;
        send.request.method = "POST".into();
        core.run(send, None).unwrap();
        core.run(job(3, server.request("/next")), None).unwrap();
        release.send(()).unwrap();
        expect(&core, &woke, 1, b"held", true);
        server.seen(2);
        sent.send(()).unwrap();
        expect(&core, &woke, 2, b"sent", false);
        expect(&core, &woke, 3, b"held", true);
        assert_eq!(server.count(), 2);
    }
}

#[test]
fn forced_and_replacing_prefetches_start_only_the_callers_exchange() {
    for asynchronous in [false, true] {
        for mode in [
            exact_runner::CacheMode::Default,
            exact_runner::CacheMode::Reload,
            exact_runner::CacheMode::NoCache,
        ] {
            for method in ["GET", "POST"] {
                let server = Server::new();
                let (core, woke, _) = runtime(&server, asynchronous, false);
                let release = held_plan(&server, "/next", b"caller");
                let mut caller = job(1, server.request("/next").prefetch().cache(mode));
                caller.request.method = method.into();
                caller.forced = mode == exact_runner::CacheMode::Default;
                core.run(caller, None).unwrap();
                server.seen(1);
                {
                    let state = core.shared.state.lock().unwrap();
                    assert!(state.cache.as_ref().unwrap().preloads.is_empty());
                    assert!(state.running.iter().any(|r| r.ticket == 1));
                }
                assert!(core.preloads.lock().unwrap().is_none());
                release.send(()).unwrap();
                expect(&core, &woke, 1, b"caller", false);
                gone(&core);
                assert_eq!(server.count(), 1);
            }
        }
    }
}

#[test]
fn a_full_read_lane_still_starts_a_prefetch_on_its_own_workers() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke, _) = runtime(&server, asynchronous, false);
        let mut releases = Vec::new();
        for ticket in 1..=INDEPENDENT as u64 + 1 {
            let path = format!("/{ticket}");
            releases.push(held_plan(&server, &path, b"read"));
            core.run(job(ticket, server.request(&path)), None).unwrap();
        }
        server.seen(INDEPENDENT);
        let release = held_plan(&server, "/next", b"held");
        core.run(job(20, server.request("/next").prefetch()), None)
            .unwrap();
        server.seen(INDEPENDENT + 1);
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.running.len(), INDEPENDENT);
            assert!(state.jobs[0]
                .iter()
                .any(|j| j.ticket == INDEPENDENT as u64 + 1));
            assert!(matches!(
                state.cache.as_ref().unwrap().preloads[0].reply,
                ReplyState::Pending { .. }
            ));
        }
        release.send(()).unwrap();
        ready(&core, 1);
        for release in releases {
            release.send(()).unwrap();
        }
        for ticket in 1..=INDEPENDENT as u64 + 1 {
            expect(&core, &woke, ticket, b"read", false);
        }
        expect(&core, &woke, 20, b"held", true);
        assert!(read(&core, &woke, &server, 21, "/next").1);
        assert_eq!(server.count(), INDEPENDENT + 2);
    }
}

#[test]
fn a_prefetch_holds_post_replies_and_errors_for_equal_reads_pending_or_complete() {
    for asynchronous in [false, true] {
        for method in ["GET", "POST"] {
            for pending in [false, true] {
                let server = Server::new();
                let (core, woke, _) = runtime(&server, asynchronous, false);
                let (release, held) = channel();
                let mut reply = Plan::new(503, &[], b"held error");
                reply.held = Some(held);
                server.plan("/next", reply);
                let mut request = server.request("/next").prefetch();
                request.method = method.into();
                core.run(job(1, request.clone()), None).unwrap();
                server.seen(1);
                request.prefetch = false;
                if pending {
                    core.run(job(2, request.clone()), None).unwrap();
                }
                release.send(()).unwrap();
                for ticket in 1..=2 {
                    if ticket == 2 && !pending {
                        core.run(job(ticket, request.clone()), None).unwrap();
                    }
                    let (got, outcome, timing) = collect(&core, &woke);
                    assert_eq!(got, ticket);
                    assert!(timing.cached);
                    assert!(
                        matches!(outcome, Outcome::Response(r) if r.status == 503 && r.body == b"held error")
                    );
                }
                ready(&core, 1);
                assert_eq!(server.count(), 1);
            }
        }
    }
}

#[test]
fn cacheable_prefetches_report_held_hits_and_keep_the_http_entry_after_the_hold() {
    use exact_runner::CacheMode;
    for asynchronous in [false, true] {
        for mode in [
            CacheMode::Default,
            CacheMode::NoStore,
            CacheMode::ForceCache,
            CacheMode::OnlyIfCached,
        ] {
            let server = Server::new();
            let (core, woke, clock) = runtime(&server, asynchronous, false);
            let (release, held) = channel();
            let mut reply = Plan::new(200, &[("Cache-Control", "max-age=60")], b"fresh");
            reply.held = Some(held);
            server.plan("/next", reply);
            core.run(job(1, server.request("/next").prefetch().cache(mode)), None)
                .unwrap();
            server.seen(1);
            release.send(()).unwrap();
            expect(&core, &woke, 1, b"fresh", true);
            let (reply, cached) = read(&core, &woke, &server, 2, "/next");
            assert_eq!(reply.body, b"fresh");
            assert!(cached);
            *clock.lock().unwrap() += HOLD;
            tick(&core);
            {
                let mut state = core.shared.state.lock().unwrap();
                let cache = state.cache.as_mut().unwrap();
                expire(cache);
                assert!(cache
                    .preloads
                    .iter()
                    .all(|p| matches!(p.reply, ReplyState::Gone)));
                assert_eq!(cache.bytes > 0, mode != CacheMode::NoStore);
            }
            if mode == CacheMode::NoStore {
                server.plan("/next", Plan::new(200, &[], b"wire"));
                assert!(!read(&core, &woke, &server, 3, "/next").1);
            } else {
                let (reply, cached) = read(&core, &woke, &server, 3, "/next");
                assert_eq!(reply.body, b"fresh");
                assert!(cached);
            }
            assert_eq!(
                server.count(),
                if mode == CacheMode::NoStore { 2 } else { 1 }
            );
        }
    }
}
