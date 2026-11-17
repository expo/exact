use super::*;
use exact_runner::CacheMode;

fn wait(core: &Core, ticket: u64) {
    let end = Instant::now() + HANG;
    let mut state = core.shared.state.lock().unwrap();
    while !state.completed[0].iter().any(|r| r.ticket == ticket) {
        assert!(Instant::now() < end, "ticket {ticket} did not complete");
        state = core
            .shared
            .ready
            .wait_timeout(state, Duration::from_millis(50))
            .unwrap()
            .0;
    }
}

#[test]
fn independent_requests_neither_refresh_complete_holds_nor_prevent_prefetch_holds() {
    for asynchronous in [false, true] {
        for mode in [CacheMode::Reload, CacheMode::NoCache, CacheMode::Default] {
            for complete in [false, true] {
                let server = Server::new();
                let request = server.request("/feed").header("X-Trace", "one");
                let definitions = if complete {
                    server.plan("/feed", Plan::new(200, &[], b"held"));
                    vec![Definition {
                        request: request.clone(),
                        ..definition(&server, "/feed")
                    }]
                } else {
                    Vec::new()
                };
                let (set, _clock) = fresh_clock(&server, definitions);
                complete_set(&set);
                let (core, woke) = server.core(asynchronous, true);
                adopt(&core, set);
                let release = held_plan(&server, "/feed", b"independent");
                let mut independent = job(1, request.clone().cache(mode).independent_http(1024));
                independent.forced = mode == CacheMode::Default;
                core.run(independent, None).unwrap();
                server.seen(if complete { 2 } else { 1 });
                {
                    let state = core.shared.state.lock().unwrap();
                    let holds = &state.cache.as_ref().unwrap().preloads;
                    assert_eq!(holds.len(), usize::from(complete));
                    if complete {
                        assert!(matches!(holds[0].reply, ReplyState::Ready(_)));
                    }
                }
                if !complete {
                    server.plan("/feed", Plan::new(200, &[], b"held"));
                    let (reply, cached) = ask(&core, &woke, job(2, request.clone().prefetch()));
                    assert!(cached);
                    assert_eq!(reply.body, b"held");
                    let state = core.shared.state.lock().unwrap();
                    let holds = &state.cache.as_ref().unwrap().preloads;
                    assert_eq!(holds.len(), 1);
                    assert!(holds[0].prefetch);
                    assert!(matches!(holds[0].reply, ReplyState::Ready(_)));
                }
                let (reply, cached) = ask(&core, &woke, job(3, request.clone()));
                assert!(cached);
                assert_eq!(reply.body, b"held");
                assert_eq!(server.count(), 2);
                release.send(()).unwrap();
                let (ticket, outcome, timing) = collect(&core, &woke);
                assert_eq!(ticket, 1);
                assert!(!timing.cached);
                assert!(matches!(outcome, Outcome::Response(r) if r.body == b"independent"));
                let (reply, cached) = ask(&core, &woke, job(4, request));
                assert!(cached);
                assert_eq!(reply.body, b"held");
                assert_eq!(server.count(), 2);
            }
        }
    }
}

#[test]
fn a_queued_or_running_refresh_leaves_a_prefetch_as_a_plain_read() {
    let mut failures = Vec::new();
    for asynchronous in [false, true] {
        for queued in [false, true] {
            for mode in [CacheMode::Reload, CacheMode::NoCache, CacheMode::Default] {
                let server = Server::new();
                let (core, woke) = server.core(asynchronous, true);
                let open = queued.then(|| {
                    let open = held_plan(&server, "/ordered", b"ordered");
                    let mut ordered = job(90, server.request("/ordered"));
                    ordered.class = RequestClass::Ordered;
                    core.run(ordered, None).unwrap();
                    server.seen(1);
                    open
                });
                let (finish, held) = channel();
                let mut new = Plan::new(200, &[("Cache-Control", "max-age=60")], b"new");
                new.cache_control = Some(if mode == CacheMode::NoCache {
                    "max-age=0"
                } else {
                    "no-cache"
                });
                new.held = Some(held);
                server.plan("/feed", new);
                server.plan("/feed", Plan::new(200, &[], b"old"));
                let mut refresh = job(20, server.request("/feed").cache(mode));
                refresh.forced = mode == CacheMode::Default;
                core.run(refresh, None).unwrap();
                if !queued {
                    server.seen(1);
                }
                core.run(job(10, server.request("/feed").prefetch()), None)
                    .unwrap();
                let no_hold = core
                    .shared
                    .state
                    .lock()
                    .unwrap()
                    .cache
                    .as_ref()
                    .unwrap()
                    .preloads
                    .is_empty();
                if let Some(open) = open {
                    open.send(()).unwrap();
                    assert_eq!(collect(&core, &woke).0, 90);
                }
                server.seen(if queued { 3 } else { 2 });
                wait(&core, 10);
                finish.send(()).unwrap();
                for (ticket, body) in [(20, b"new".as_slice()), (10, b"old")] {
                    let (got, outcome, _) = collect(&core, &woke);
                    assert_eq!(got, ticket);
                    assert!(matches!(outcome, Outcome::Response(r) if r.body == body));
                }
                let (reply, cached) = read(&core, &woke, &server, 30, "/feed");
                if !no_hold || !cached || reply.body != b"new" {
                    failures.push((asynchronous, queued, mode));
                }
                assert_eq!(server.count(), if queued { 3 } else { 2 });
                server.plan("/feed", Plan::new(200, &[], b"held"));
                core.run(
                    job(
                        40,
                        server.request("/feed").cache(CacheMode::NoStore).prefetch(),
                    ),
                    None,
                )
                .unwrap();
                let (_, outcome, timing) = collect(&core, &woke);
                assert!(timing.cached);
                assert!(matches!(outcome, Outcome::Response(r) if r.body == b"held"));
            }
        }
    }
    assert!(failures.is_empty(), "holds during refreshes: {failures:?}");
}

#[test]
fn a_launch_definition_handed_over_during_a_refresh_is_dropped_without_starting() {
    let mut failures = Vec::new();
    for asynchronous in [false, true] {
        for mode in [CacheMode::Reload, CacheMode::NoCache, CacheMode::Default] {
            for named in [false, true] {
                let server = Server::new();
                let (set, _clock) = fresh_clock(&server, Vec::new());
                let control = set.control();
                let (core, woke) = server.core(asynchronous, true);
                adopt(&core, set);
                let finish = held_plan(&server, "/feed", b"new");
                let mut refresh = job(1, server.request("/feed").cache(mode));
                refresh.forced = mode == CacheMode::Default;
                if named {
                    refresh.request = refresh
                        .request
                        .preload("feed")
                        .unwrap()
                        .header("X-Trace", "refresh");
                }
                core.run(refresh, None).unwrap();
                server.seen(1);
                server.plan("/feed", Plan::new(200, &[], b"old"));
                control.extend(
                    vec![definition(&server, "/feed")],
                    &core.grants,
                    Arc::new(|_, _| {}),
                );
                let dropped = control
                    .take_journal()
                    .iter()
                    .any(|f| f.name == "feed" && f.outcome == Event::DroppedByRefresh);
                if !dropped {
                    ready(&core, 1);
                }
                finish.send(()).unwrap();
                assert_eq!(collect(&core, &woke).0, 1);
                if !dropped || server.count() != 1 {
                    failures.push((asynchronous, mode, named));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "late launch holds started: {failures:?}"
    );
}

#[test]
fn late_holds_use_refresh_equality_for_bodies_redirects_cache_modes_and_enabled_names() {
    let mut failures = Vec::new();
    for asynchronous in [false, true] {
        for named in [false, true] {
            for selected in [false, true] {
                for difference in 0..8 {
                    let server = Server::new();
                    let (set, _clock) = fresh_clock(&server, Vec::new());
                    let control = set.control();
                    if selected {
                        control.select_update();
                    }
                    let (core, woke) = server.core(asynchronous, true);
                    adopt(&core, set);
                    let (finish, held) = channel();
                    let mut plan = Plan::new(503, &[], b"refresh");
                    plan.held = Some(held);
                    server.plan("/feed", plan);
                    let mut request = Request::post_json(&server.request("/feed").url, "query")
                        .header("X-Trace", "one")
                        .header("X-Other", "two");
                    if named {
                        request = request.preload("feed").unwrap();
                    }
                    let mut refresh = job(1, request.clone().cache(CacheMode::NoStore));
                    refresh.forced = true;
                    core.run(refresh, None).unwrap();
                    server.seen(1);
                    let mut candidate = request;
                    candidate.preload = None;
                    match difference {
                        0 => {}
                        1 => candidate.method = "PUT".into(),
                        2 => candidate.url.push_str("?other"),
                        3 => candidate.headers[0].1 = "other".into(),
                        4 => candidate.body.push(b'!'),
                        5 => candidate.redirect = exact_runner::Redirect::Manual,
                        6 => candidate.cache = CacheMode::NoStore,
                        _ => {
                            candidate.headers.reverse();
                            for (name, _) in &mut candidate.headers {
                                *name = name.to_ascii_lowercase();
                            }
                        }
                    }
                    let should_drop = matches!(difference, 0 | 6 | 7)
                        || (named && !selected && !matches!(difference, 1 | 2));
                    server.plan(
                        if difference == 2 {
                            "/feed?other"
                        } else {
                            "/feed"
                        },
                        Plan::new(503, &[], b"held"),
                    );
                    control.extend(
                        vec![Definition {
                            request: candidate,
                            ..definition(&server, "/feed")
                        }],
                        &core.grants,
                        Arc::new(|_, _| {}),
                    );
                    let dropped = control
                        .take_journal()
                        .iter()
                        .any(|f| f.outcome == Event::DroppedByRefresh);
                    if !dropped {
                        ready(&core, 1);
                    }
                    finish.send(()).unwrap();
                    assert_eq!(collect(&core, &woke).0, 1);
                    if dropped != should_drop {
                        failures.push((asynchronous, named, selected, difference));
                    }
                    assert_eq!(server.count(), if dropped { 1 } else { 2 });
                }
            }
        }
    }
    assert!(failures.is_empty(), "wrong refresh equality: {failures:?}");
}

#[test]
fn a_write_head_wakes_readers_of_pending_and_complete_holds_without_another_pump() {
    let mut failures = Vec::new();
    for asynchronous in [false, true] {
        for pending in [false, true] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            server.plan("/warm", Plan::new(200, &[], b"warm"));
            read(&core, &woke, &server, 1, "/warm");
            let request = server.request("/feed");
            let (_, charge, limit) = reads::reservation(&request, true).unwrap();
            let mut held = Held::new(definition(&server, "/feed"), Instant::now(), None, 1);
            let abort = match &held.reply {
                ReplyState::Pending { abort, .. } => abort.clone(),
                _ => unreachable!(),
            };
            if !pending {
                held.reply = ReplyState::Ready(Ready {
                    response: Response {
                        status: 200,
                        headers: Vec::new(),
                        body: b"held".to_vec(),
                    },
                    visited: vec![request.url.clone()],
                    completed: Instant::now(),
                    bytes: 0,
                    used: 1,
                });
            }
            // Isolate the head's wake from a preload coordinator or a transport callback.
            let notifications = {
                let mut state = core.shared.state.lock().unwrap();
                state.cache.as_mut().unwrap().preloads.push(held);
                state.jobs[0].push_back(Job {
                    ticket: 2,
                    request,
                    forced: false,
                    work: None,
                    waiting_charge: charge,
                    waited_for_preload: true,
                    charge,
                    limit,
                    forgotten: false,
                    queued_deadline: None,
                    overlap: true,
                    class: RequestClass::Read,
                    cache_store_withheld: false,
                });
                state.ordered.push_back(2);
                state.counts[0] += 1;
                state.bytes[0] += charge;
                assert!(waiting(&state, &state.jobs[0][0]));
                core.shared.worker_notifications.load(Ordering::Relaxed)
            };
            let release = held_plan(&server, "/feed", b"sent");
            let mut write = fetch_request(
                Request::post_json(&server.request("/feed").url, "write"),
                false,
            );
            cache::observe(&core.shared, &mut write);
            write.received.as_ref().unwrap().0(
                "POST",
                &write.url,
                200,
                &ibex2::stdlib::fetch::Headers::new(),
            );
            server.seen(2);
            if core.shared.worker_notifications.load(Ordering::Relaxed) == notifications {
                failures.push((asynchronous, pending));
            }
            release.send(()).unwrap();
            let (ticket, outcome, timing) = collect(&core, &woke);
            assert_eq!(ticket, 2);
            assert!(!timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"sent"));
            assert_eq!(abort.signal().aborted(), pending);
        }
    }
    assert!(
        failures.is_empty(),
        "head omitted the worker wake: {failures:?}"
    );
}
