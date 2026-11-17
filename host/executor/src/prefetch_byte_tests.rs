use super::*;

#[test]
fn a_prefetch_counts_every_earlier_queued_reads_reply_reservation() {
    let mut failures = Vec::new();
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke, _) = runtime(&server, asynchronous, false);
        let mut releases = Vec::new();
        for ticket in 1..=INDEPENDENT as u64 {
            let path = format!("/{ticket}");
            releases.push(held_plan(&server, &path, b"earlier"));
            core.run(job(ticket, server.request(&path)), None).unwrap();
        }
        server.seen(INDEPENDENT);
        let retained = {
            let mut state = core.shared.state.lock().unwrap();
            let retained = BYTES[0] - state.bytes[0] - (92 << 20);
            state.bytes[0] += retained;
            retained
        };
        server.plan("/a", Plan::new(200, &[], b"a"));
        server.plan("/b", Plan::new(200, &[], b"b"));
        core.run(job(20, server.request("/a")), None).unwrap();
        core.run(job(21, server.request("/b").prefetch()), None)
            .unwrap();
        let held = {
            let mut state = core.shared.state.lock().unwrap();
            assert!(state.jobs[0].iter().any(|j| j.ticket == 20));
            let held = state.cache.as_ref().unwrap().preloads.len();
            state.bytes[0] -= retained;
            core.shared.ready.notify_all();
            held
        };
        for release in releases {
            release.send(()).unwrap();
        }
        for ticket in 1..=INDEPENDENT as u64 {
            expect(&core, &woke, ticket, b"earlier", false);
        }
        expect(&core, &woke, 20, b"a", false);
        let (ticket, outcome, timing) = collect(&core, &woke);
        assert_eq!(ticket, 21);
        assert!(matches!(outcome, Outcome::Response(r) if r.body == b"b"));
        if held != 0 || timing.cached {
            failures.push(asynchronous);
        }
        assert_eq!(server.count(), INDEPENDENT + 2);
    }
    assert!(
        failures.is_empty(),
        "preload passed queued read: {failures:?}"
    );
}

#[test]
fn every_waiter_keeps_room_after_its_hold_ends_before_later_cache_answers() {
    let mut failures = Vec::new();
    let body = vec![1; cache::MAX_ENTRY - 2048];
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            for held_answers in [false, true] {
                for ending in ["failure", "drop", "deadline"] {
                    let server = Server::new();
                    let (core, woke, clock) = runtime(&server, asynchronous, true);
                    server.plan(
                        "/cached",
                        Plan::new(200, &[("Cache-Control", "max-age=60")], &body),
                    );
                    let control = core.preloads.lock().unwrap().as_ref().unwrap().control();
                    let mut later = server.request("/cached");
                    if held_answers {
                        later.cache = exact_runner::CacheMode::NoStore;
                        let mut def = definition(&server, "/cached");
                        def.request = later.clone();
                        control.extend(vec![def], &core.grants, Arc::new(|_, _| {}));
                        ready(&core, 1);
                    } else {
                        assert!(!read(&core, &woke, &server, 1000, "/cached").1);
                    }
                    let (release, held) = channel();
                    let mut plan = Plan::new(200, &[], b"unused");
                    plan.held = Some(held);
                    plan.disconnect = ending == "failure";
                    server.plan("/query", plan);
                    let mut request = server.request("/query").timeout(5000);
                    request.method = "POST".into();
                    if prefetch {
                        core.run(job(1, request.clone().prefetch()), None).unwrap();
                    } else {
                        let mut def = definition(&server, "/query");
                        def.request = request.clone();
                        control.extend(vec![def], &core.grants, Arc::new(|_, _| {}));
                        core.run(job(1, request.clone()), None).unwrap();
                    }
                    server.seen(2);
                    core.run(job(2, request), None).unwrap();
                    for ticket in 3..=128 {
                        core.run(job(ticket, later.clone()), None).unwrap();
                    }
                    assert!(core.drain().is_empty());
                    let first = held_plan(&server, "/query", b"first");
                    let second = held_plan(&server, "/query", b"second");
                    match ending {
                        "drop" => {
                            control.cancel_names(&[if prefetch { "" } else { "query" }.into()])
                        }
                        "deadline" => {
                            *clock.lock().unwrap() += Duration::from_secs(6);
                            control.test_tick();
                        }
                        _ => {}
                    }
                    release.send(()).unwrap();
                    server.seen(3);
                    first.send(()).unwrap();
                    wait_state(&core, |s| s.completed[0].iter().any(|d| d.ticket == 1));
                    let reserved = {
                        let state = core.shared.state.lock().unwrap();
                        let reservation = state.jobs[0]
                            .iter()
                            .find(|j| j.ticket == 2)
                            .map(|j| (j.charge, j.limit))
                            .or_else(|| {
                                state
                                    .running
                                    .iter()
                                    .find(|r| r.ticket == 2)
                                    .map(|r| (r.charge, r.limit))
                            });
                        let mut later_ready = 0;
                        for done in state.completed[0].iter().filter(|d| d.ticket >= 3) {
                            assert!(done.cached);
                            assert!(
                                matches!(&done.outcome, Outcome::Response(r) if r.body == body)
                            );
                            later_ready += 1;
                        }
                        assert!(later_ready > 0);
                        reservation.is_some_and(|(charge, limit)| charge == limit)
                    };
                    if !reserved {
                        failures.push((asynchronous, prefetch, held_answers, ending));
                    }
                    expect(&core, &woke, 1, b"first", false);
                    assert!(core.drain().is_empty());
                    core.forget(|ticket| ticket <= 2);
                    second.send(()).unwrap();
                    expect(&core, &woke, 2, b"second", false);
                    wait_state(&core, |s| s.bytes == [0, 0]);
                    assert_eq!(server.count(), 4);
                }
            }
        }
    }
    assert!(failures.is_empty(), "fallback lost its room: {failures:?}");
}

#[test]
fn a_prefetch_waiting_for_response_bytes_sends_itself_when_room_returns() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke, _) = runtime(&server, asynchronous, false);
        let retained = BYTES[0] - (1 << 20);
        core.shared.state.lock().unwrap().bytes[0] = retained;
        server.plan("/next", Plan::new(200, &[], b"caller"));
        core.run(job(1, server.request("/next").prefetch()), None)
            .unwrap();
        let holds = {
            let mut state = core.shared.state.lock().unwrap();
            let holds = state.cache.as_ref().unwrap().preloads.len();
            state.bytes[0] -= retained;
            core.shared.ready.notify_all();
            holds
        };
        expect(&core, &woke, 1, b"caller", false);
        assert_eq!(holds, 0);
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn a_prefetch_does_not_pass_an_earlier_read_short_of_response_bytes() {
    for asynchronous in [false, true] {
        for waiting in [false, true] {
            let server = Server::new();
            let release = held_plan(&server, "/ahead", b"launch");
            let mut def = definition(&server, "/ahead");
            def.request.method = "POST".into();
            def.request.body = vec![b'x'; MAX_BODY_BYTES];
            let ahead = def.request.clone();
            let (set, _clock) = fresh_clock(&server, if waiting { vec![def] } else { Vec::new() });
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            let request = server.request("/next").prefetch();
            let (_, _, next_limit) = reads::reservation(&request, true).unwrap();
            let (_, ahead_charge, _) = reads::reservation(&ahead, true).unwrap();
            let retained = BYTES[0] - (next_limit + ahead_charge);
            core.shared.state.lock().unwrap().bytes[0] = retained;
            core.run(job(1, ahead), None).unwrap();
            if !asynchronous && !waiting {
                server.seen(1);
            }
            server.plan("/next", Plan::new(200, &[], b"caller"));
            core.run(job(2, request), None).unwrap();
            let holds = {
                let mut state = core.shared.state.lock().unwrap();
                let holds = state.cache.as_ref().unwrap().preloads.len();
                state.bytes[0] -= retained;
                core.shared.ready.notify_all();
                holds
            };
            release.send(()).unwrap();
            expect(&core, &woke, 1, b"launch", waiting);
            expect(&core, &woke, 2, b"caller", false);
            assert_eq!(holds, usize::from(waiting));
            assert_eq!(server.count(), 2);
        }
    }
}
