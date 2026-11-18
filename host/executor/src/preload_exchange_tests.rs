use super::*;
use exact_runner::{CacheMode, Redirect};
use std::sync::mpsc::Receiver;

fn wait(core: &Core, until: impl Fn(&State) -> bool) {
    let end = Instant::now() + HANG;
    let mut state = core.shared.state.lock().unwrap();
    while !until(&state) {
        assert!(Instant::now() < end, "executor condition did not hold");
        state = core
            .shared
            .ready
            .wait_timeout(state, Duration::from_millis(50))
            .unwrap()
            .0;
    }
}

fn start(core: &Core, server: &Server, request: Request, prefetch: bool) {
    if prefetch {
        core.run(job(1, request.prefetch()), None).unwrap();
    } else {
        core.preloads
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .control()
            .extend(
                vec![Definition {
                    name: "held".into(),
                    request,
                    deadline_ms: None,
                    secrets: Vec::new(),
                }],
                &format!("net.fetch http://{}", server.address),
                Arc::new(|_, _| {}),
            );
    }
}

fn runtime(server: &Server, asynchronous: bool) -> (Core, Receiver<()>) {
    let (core, woke) = server.core(asynchronous, true);
    adopt(&core, launch(server, Vec::new()));
    (core, woke)
}

#[test]
fn a_redirected_hold_removes_the_original_urls_stale_http_entry() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            let server = Server::new();
            let (core, woke) = runtime(&server, asynchronous);
            let control = core.preloads.lock().unwrap().as_ref().unwrap().control();
            let clock = Arc::new(Mutex::new(Instant::now()));
            let now = clock.clone();
            control.test_clock(Arc::new(move || *now.lock().unwrap()));
            server.plan(
                "/moved",
                Plan::new(200, &[("Cache-Control", "max-age=0")], b"old"),
            );
            assert!(!read(&core, &woke, &server, 10, "/moved").1);
            server.plan("/moved", Plan::new(302, &[("Location", "/new")], b""));
            server.plan(
                "/new",
                Plan::new(200, &[("Cache-Control", "max-age=60")], b"new"),
            );
            start(&core, &server, server.request("/moved"), prefetch);
            ready(&core, 1);
            if prefetch {
                let (_, outcome, timing) = collect(&core, &woke);
                assert!(timing.cached);
                assert!(matches!(outcome, Outcome::Response(r) if r.body == b"new"));
            }
            *clock.lock().unwrap() += HOLD;
            control.test_tick();
            core.run(
                job(2, server.request("/moved").cache(CacheMode::OnlyIfCached)),
                None,
            )
            .unwrap();
            let (ticket, outcome, timing) = collect(&core, &woke);
            assert_eq!(ticket, 2);
            assert!(!timing.cached);
            assert!(matches!(
                outcome,
                Outcome::Failed {
                    kind: FailureKind::Network,
                    ..
                }
            ));
            assert_eq!(server.count(), 3);
            server.plan("/moved", Plan::new(200, &[], b"after"));
            let (reply, cached) = ask(
                &core,
                &woke,
                job(3, server.request("/moved").cache(CacheMode::ForceCache)),
            );
            assert!(!cached);
            assert_eq!(reply.body, b"after");
            assert_eq!(server.count(), 4);
        }
    }
}

fn post_pair(
    core: &Core,
    server: &Server,
    prefetch: bool,
    failure: bool,
    first_timeout: Option<u32>,
) -> ([Request; 2], [Sender<()>; 2]) {
    let requests = std::array::from_fn(|i| {
        let mut request = server.request("/query");
        request.method = "POST".into();
        request.body = vec![i as u8];
        if i == 0 {
            request.timeout_ms = first_timeout;
        }
        request
    });
    let releases = std::array::from_fn(|i| {
        let (release, held) = channel();
        let mut plan = Plan::new(200, &[], &[i as u8]);
        plan.held = Some(held);
        plan.disconnect = failure && i == 0;
        server.plan("/query", plan);
        if prefetch {
            core.run(job(i as u64 + 1, requests[i].clone().prefetch()), None)
                .unwrap();
        } else {
            core.preloads
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .control()
                .extend(
                    vec![Definition {
                        name: format!("query{i}"),
                        request: requests[i].clone(),
                        deadline_ms: None,
                        secrets: Vec::new(),
                    }],
                    &core.grants,
                    Arc::new(|_, _| {}),
                );
        }
        server.seen(i + 1);
        release
    });
    assert!(core
        .shared
        .state
        .lock()
        .unwrap()
        .cache
        .as_ref()
        .unwrap()
        .preloads
        .iter()
        .all(|p| matches!(p.reply, ReplyState::Pending { .. })));
    (requests, releases)
}

#[test]
fn two_post_holds_keep_both_bodies_in_either_completion_order() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            for reverse in [false, true] {
                let server = Server::new();
                let (core, woke) = runtime(&server, asynchronous);
                let (requests, releases) = post_pair(&core, &server, prefetch, false, None);
                for i in if reverse { [1, 0] } else { [0, 1] } {
                    releases[i].send(()).unwrap();
                    wait(&core, |s| {
                        !matches!(
                            s.cache.as_ref().unwrap().preloads[i].reply,
                            ReplyState::Pending { .. }
                        )
                    });
                    assert!(core
                        .shared
                        .state
                        .lock()
                        .unwrap()
                        .cache
                        .as_ref()
                        .unwrap()
                        .preloads
                        .iter()
                        .all(|p| !matches!(p.reply, ReplyState::Gone)));
                }
                ready(&core, 2);
                if prefetch {
                    for expected in 1..=2 {
                        let (ticket, outcome, timing) = collect(&core, &woke);
                        assert_eq!(ticket, expected);
                        assert!(timing.cached);
                        assert!(
                            matches!(outcome, Outcome::Response(r) if r.body == [ticket as u8 - 1])
                        );
                    }
                }
                for (i, request) in requests.into_iter().enumerate() {
                    let (reply, cached) = ask(&core, &woke, job(10 + i as u64, request));
                    assert!(cached);
                    assert_eq!(reply.body, [i as u8]);
                }
                assert_eq!(server.count(), 2);
            }
        }
    }
}

#[test]
fn a_failed_or_cancelled_post_hold_leaves_the_other_body_held_and_its_reader_sends_once() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            for reverse in [false, true] {
                for cancelled in [true, false] {
                    let server = Server::new();
                    let (core, woke) = runtime(&server, asynchronous);
                    let control = core.preloads.lock().unwrap().as_ref().unwrap().control();
                    let clock = Arc::new(Mutex::new(Instant::now()));
                    let now = clock.clone();
                    control.test_clock(Arc::new(move || *now.lock().unwrap()));
                    let (requests, releases) = post_pair(
                        &core,
                        &server,
                        prefetch,
                        !cancelled,
                        cancelled.then_some(1000),
                    );
                    let (fallback, held) = channel();
                    let mut plan = Plan::new(200, &[], b"fallback");
                    // Its ordinary head may drop the surviving hold after its readers take it (D1).
                    plan.held = Some(held);
                    server.plan("/query", plan);
                    server.plan("/query", Plan::new(200, &[], b"unexpected"));
                    if !prefetch {
                        for (i, request) in requests.iter().enumerate() {
                            core.run(job(i as u64 + 1, request.clone()), None).unwrap();
                        }
                    }
                    if cancelled {
                        *clock.lock().unwrap() += Duration::from_secs(2);
                        control.test_tick();
                        assert!(!matches!(
                            core.shared
                                .state
                                .lock()
                                .unwrap()
                                .cache
                                .as_ref()
                                .unwrap()
                                .preloads[1]
                                .reply,
                            ReplyState::Gone
                        ));
                    }
                    for i in if reverse { [1, 0] } else { [0, 1] } {
                        releases[i].send(()).unwrap();
                        wait(&core, |s| {
                            !matches!(
                                s.cache.as_ref().unwrap().preloads[i].reply,
                                ReplyState::Pending { .. }
                            )
                        });
                        assert!(!matches!(
                            core.shared
                                .state
                                .lock()
                                .unwrap()
                                .cache
                                .as_ref()
                                .unwrap()
                                .preloads[1]
                                .reply,
                            ReplyState::Gone
                        ));
                    }
                    ready(&core, 1);
                    server.seen(3);
                    core.run(job(10, requests[1].clone()), None).unwrap();
                    wait(&core, |s| s.completed[0].iter().any(|d| d.ticket == 10));
                    fallback.send(()).unwrap();
                    for expected in [1, 2, 10] {
                        let (ticket, outcome, timing) = collect(&core, &woke);
                        assert_eq!(ticket, expected);
                        assert_eq!(timing.cached, ticket != 1);
                        let body = if ticket == 1 {
                            b"fallback".as_slice()
                        } else {
                            &[1]
                        };
                        assert!(matches!(outcome, Outcome::Response(r) if r.body == body));
                    }
                    assert_eq!(server.count(), 3);
                }
            }
        }
    }
}

#[test]
fn ordinary_unsafe_reads_and_sends_still_drop_both_post_holds() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            for pending in [false, true] {
                for send in [false, true] {
                    let server = Server::new();
                    let (core, woke) = runtime(&server, asynchronous);
                    let (_, releases) = post_pair(&core, &server, prefetch, false, None);
                    if prefetch {
                        core.cancel(&[1, 2]);
                    }
                    if !pending {
                        for release in &releases {
                            release.send(()).unwrap();
                        }
                        ready(&core, 2);
                    }
                    let mut request = server.request("/query");
                    request.method = "POST".into();
                    request.body = b"ordinary".to_vec();
                    let mut request = job(10, request);
                    if send {
                        request.class = RequestClass::Ordered;
                    }
                    server.plan("/query", Plan::new(200, &[], b"ordinary"));
                    assert!(!ask(&core, &woke, request).1);
                    assert!(core
                        .shared
                        .state
                        .lock()
                        .unwrap()
                        .cache
                        .as_ref()
                        .unwrap()
                        .preloads
                        .iter()
                        .all(|p| matches!(p.reply, ReplyState::Gone)));
                    if pending {
                        for release in releases {
                            release.send(()).unwrap();
                        }
                    }
                    assert_eq!(server.count(), 3);
                }
            }
        }
    }
}

#[test]
fn different_bodies_bypass_the_http_copy_despite_different_redirect_or_cache_policies() {
    for asynchronous in [false, true] {
        for cache_policy in [false, true] {
            let server = Server::new();
            let (core, woke) = runtime(&server, asynchronous);
            server.plan(
                "/item",
                Plan::new(200, &[("Cache-Control", "max-age=60")], b"empty"),
            );
            let holding = server.request("/item").cache(if cache_policy {
                CacheMode::NoCache
            } else {
                CacheMode::Default
            });
            start(&core, &server, holding, false);
            ready(&core, 1);
            let mut request = server.request("/item");
            request.body = b"different".to_vec();
            if !cache_policy {
                request.redirect = Redirect::Manual;
            }
            server.plan("/item", Plan::new(200, &[], b"different"));
            core.run(job(2, request), None).unwrap();
            let (ticket, outcome, timing) = collect(&core, &woke);
            assert_eq!(ticket, 2);
            assert!(!timing.cached);
            // URLSession refuses a GET that has a body, as a browser's fetch does.
            if cfg!(target_vendor = "apple") {
                assert!(matches!(outcome, Outcome::Failed { .. }), "{outcome:?}");
            } else {
                assert!(
                    matches!(&outcome, Outcome::Response(r) if r.body == b"different"),
                    "{outcome:?}"
                );
                assert_eq!(server.count(), 2);
            }
        }
    }
}

#[test]
fn a_refresh_drops_a_no_store_hold_in_every_refresh_mode() {
    for asynchronous in [false, true] {
        for pending in [false, true] {
            for mode in [CacheMode::Reload, CacheMode::NoCache, CacheMode::Default] {
                let server = Server::new();
                let (core, woke) = server.core(asynchronous, true);
                let (set, _clock) = fresh_clock(&server, Vec::new());
                adopt(&core, set);
                let old = held_plan(&server, "/a", b"old");
                let request = server.request("/a").cache(CacheMode::NoStore);
                start(&core, &server, request.clone(), true);
                server.seen(1);
                core.forget(|_| false);
                if !pending {
                    old.send(()).unwrap();
                    ready(&core, 1);
                }
                {
                    let state = core.shared.state.lock().unwrap();
                    assert!(matches!(
                        (&state.cache.as_ref().unwrap().preloads[0].reply, pending),
                        (ReplyState::Pending { .. }, true) | (ReplyState::Ready(_), false)
                    ));
                }
                server.plan("/a", Plan::new(200, &[], b"new"));
                let mut refresh = job(2, server.request("/a").cache(mode));
                refresh.forced = mode == CacheMode::Default;
                let (reply, cached) = ask(&core, &woke, refresh);
                assert!(!cached);
                assert_eq!(reply.body, b"new");
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
                if pending {
                    old.send(()).unwrap();
                }
                server.plan("/a", Plan::new(200, &[], b"new"));
                let (reply, cached) = ask(&core, &woke, job(3, request));
                assert!(!cached);
                assert_eq!(reply.body, b"new");
                assert_eq!(server.count(), 3);
            }
        }
    }
}

#[test]
fn a_prefetch_with_a_body_over_the_preload_bound_sends_only_its_caller() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        let release = held_plan(&server, "/query", b"caller");
        let mut request = server.request("/query").prefetch();
        request.method = "POST".into();
        request.body = vec![b'x'; MAX_BODY_BYTES + 1];
        core.run(job(1, request), None).unwrap();
        server.seen(1);
        {
            let state = core.shared.state.lock().unwrap();
            assert!(state.cache.as_ref().unwrap().preloads.is_empty());
            assert!(state.running.iter().any(|r| r.ticket == 1));
        }
        assert!(core.preloads.lock().unwrap().is_none());
        release.send(()).unwrap();
        let (ticket, outcome, timing) = collect(&core, &woke);
        assert_eq!(ticket, 1);
        assert!(!timing.cached);
        assert!(matches!(outcome, Outcome::Response(r) if r.body == b"caller"));
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn refreshing_a_prefetch_leaves_http_entries_to_the_cache_rules() {
    for asynchronous in [false, true] {
        for no_store in [true, false] {
            let server = Server::new();
            let (core, woke) = runtime(&server, asynchronous);
            let mode = if no_store {
                CacheMode::NoStore
            } else {
                CacheMode::Default
            };
            if no_store {
                server.plan(
                    "/a",
                    Plan::new(200, &[("Cache-Control", "max-age=60")], b"ordinary"),
                );
                assert!(!read(&core, &woke, &server, 10, "/a").1);
            }
            server.plan(
                "/a",
                Plan::new(200, &[("Cache-Control", "max-age=60")], b"prefetched"),
            );
            start(&core, &server, server.request("/a").cache(mode), true);
            assert!(collect(&core, &woke).2.cached);
            let (release, held) = channel();
            let mut plan = Plan::new(200, &[("Cache-Control", "max-age=60")], b"refreshed");
            plan.held = Some(held);
            server.plan("/a", plan);
            let mut refresh = job(2, server.request("/a").cache(mode));
            refresh.forced = true;
            core.run(refresh, None).unwrap();
            server.seen(if no_store { 3 } else { 2 });
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
            if no_store {
                release.send(()).unwrap();
                let (ticket, outcome, timing) = collect(&core, &woke);
                assert_eq!(ticket, 2);
                assert!(!timing.cached);
                assert!(matches!(outcome, Outcome::Response(r) if r.body == b"refreshed"));
                server.plan("/a", Plan::new(200, &[], b"unexpected"));
                let (reply, cached) = read(&core, &woke, &server, 3, "/a");
                assert!(cached);
                assert_eq!(reply.body, b"ordinary");
            } else {
                server.plan(
                    "/a",
                    Plan::new(200, &[("Cache-Control", "no-store")], b"after"),
                );
                core.run(job(3, server.request("/a")), None).unwrap();
                wait(&core, |s| s.completed[0].iter().any(|d| d.ticket == 3));
                release.send(()).unwrap();
                for expected in 2..=3 {
                    let (ticket, outcome, timing) = collect(&core, &woke);
                    assert_eq!(ticket, expected);
                    assert!(!timing.cached);
                    let body = if ticket == 2 {
                        b"refreshed".as_slice()
                    } else {
                        b"after"
                    };
                    assert!(matches!(outcome, Outcome::Response(r) if r.body == body));
                }
                let (reply, cached) = read(&core, &woke, &server, 4, "/a");
                assert!(cached);
                assert_eq!(reply.body, b"refreshed");
            }
            assert_eq!(server.count(), 3);
        }
    }
}

#[test]
fn taking_an_unsafe_hold_preserves_an_entry_stored_after_its_exchange() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            let server = Server::new();
            let (core, woke) = runtime(&server, asynchronous);
            let mut request = server.request("/query");
            request.method = "POST".into();
            server.plan("/query", Plan::new(200, &[], b"held"));
            start(&core, &server, request.clone(), prefetch);
            ready(&core, 1);
            if prefetch {
                assert!(collect(&core, &woke).2.cached);
            }
            server.plan(
                "/query",
                Plan::new(200, &[("Cache-Control", "max-age=60")], b"later"),
            );
            assert!(!read(&core, &woke, &server, 10, "/query").1);
            assert!(ask(&core, &woke, job(11, request)).1);
            let (reply, cached) = read(&core, &woke, &server, 12, "/query");
            assert!(cached);
            assert_eq!(reply.body, b"later");
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn a_prefetch_cache_hit_wakes_after_releasing_the_preloads_lock() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = runtime(&server, asynchronous);
        let core = Arc::new(core);
        server.plan(
            "/cached",
            Plan::new(200, &[("Cache-Control", "max-age=60")], b"cached"),
        );
        assert!(!read(&core, &woke, &server, 1, "/cached").1);
        let checked = Arc::downgrade(&core);
        let (send, observed) = channel();
        {
            let mut state = core.shared.state.lock().unwrap();
            let wake = state.wake.take().unwrap();
            state.wake = Some(Box::new(move || {
                let released = checked.upgrade().unwrap().preloads.try_lock().is_ok();
                send.send(released).unwrap();
                wake();
            }));
        }
        core.begin_pump();
        let (reply, cached) = ask(&core, &woke, job(2, server.request("/cached").prefetch()));
        assert!(cached);
        assert_eq!(reply.body, b"cached");
        assert!(observed.recv_timeout(HANG).unwrap());
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn unsafe_holds_invalidate_http_entries_at_each_head_and_on_failure_without_dropping_holds() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            for failure in [false, true] {
                let server = Server::new();
                let (core, woke) = runtime(&server, asynchronous);
                for (ticket, path) in [
                    (10, "/cart"),
                    (11, "/target"),
                    (12, "/related"),
                    (13, "/final-related"),
                ] {
                    server.plan(
                        path,
                        Plan::new(200, &[("Cache-Control", "max-age=60")], b"old"),
                    );
                    assert!(!read(&core, &woke, &server, ticket, path).1);
                }
                let (release, held) = channel();
                let mut reply = Plan::new(
                    307,
                    &[("Location", "/target"), ("Content-Location", "/related")],
                    b"",
                );
                reply.disconnect = failure;
                reply.held = Some(held);
                server.plan("/cart", reply);
                server.plan(
                    "/target",
                    Plan::new(200, &[("Content-Location", "/final-related")], b"written"),
                );
                let mut request = server.request("/cart");
                request.method = "POST".into();
                start(&core, &server, request.clone(), prefetch);
                server.seen(5);
                if failure && prefetch {
                    core.cancel(&[1]);
                }
                release.send(()).unwrap();
                if failure {
                    wait(&core, |s| {
                        matches!(
                            s.cache.as_ref().unwrap().preloads[0].reply,
                            ReplyState::Gone
                        )
                    });
                } else {
                    ready(&core, 1);
                    if !prefetch {
                        core.run(job(1, request), None).unwrap();
                    }
                    let (_, outcome, timing) = collect(&core, &woke);
                    assert!(timing.cached, "the unsafe exchange keeps its own hold");
                    assert!(matches!(outcome, Outcome::Response(r) if r.body == b"written"));
                }
                for (ticket, path) in [
                    (20, "/cart"),
                    (21, "/target"),
                    (22, "/related"),
                    (23, "/final-related"),
                ] {
                    let invalidated = !failure || path == "/cart";
                    if invalidated {
                        server.plan(path, Plan::new(200, &[], b"new"));
                    }
                    let (reply, cached) = read(&core, &woke, &server, ticket, path);
                    assert_eq!(cached, !invalidated, "{path}, failure={failure}");
                    assert_eq!(
                        reply.body,
                        if invalidated {
                            b"new".as_slice()
                        } else {
                            b"old"
                        }
                    );
                }
            }
        }
        for failure in [false, true] {
            let server = Server::new();
            server.plan(
                "/cart",
                Plan::new(200, &[("Cache-Control", "max-age=60")], b"old"),
            );
            let set = launch(&server, vec![definition(&server, "/cart")]);
            set.control().wait_ready(1);
            let mut reply = Plan::new(200, &[], b"written");
            reply.disconnect = failure;
            server.plan("/cart", reply);
            let mut def = definition(&server, "/cart");
            def.name = "write".into();
            def.request.method = "POST".into();
            let request = def.request.clone();
            set.control().extend(
                vec![def],
                &format!("net.fetch http://{}", server.address),
                Arc::new(|_, _| {}),
            );
            complete_set(&set);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            if !failure {
                assert!(ask(&core, &woke, job(1, request)).1);
            }
            server.plan("/cart", Plan::new(200, &[], b"new"));
            let mut request = server.request("/cart");
            request.redirect = Redirect::Manual;
            let (reply, cached) = ask(&core, &woke, job(2, request));
            assert!(!cached);
            assert_eq!(reply.body, b"new");
            server.plan("/cart", Plan::new(200, &[], b"unexpected"));
            let (reply, cached) = read(&core, &woke, &server, 3, "/cart");
            assert!(cached);
            assert_eq!(reply.body, b"old");
            assert_eq!(server.count(), 3);
        }
    }
}

#[test]
fn held_redirect_policies_match_for_equal_and_named_reads() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            for named in [false, true] {
                for policy in [Redirect::Manual, Redirect::Error] {
                    let server = Server::new();
                    let (core, woke) = runtime(&server, asynchronous);
                    server.plan(
                        "/start",
                        Plan::new(302, &[("Location", "/end")], b"redirect"),
                    );
                    server.plan("/end", Plan::new(200, &[], b"followed"));
                    start(&core, &server, server.request("/start"), prefetch);
                    ready(&core, 1);
                    if prefetch {
                        assert!(collect(&core, &woke).2.cached);
                    }
                    server.plan(
                        "/start",
                        Plan::new(302, &[("Location", "/end")], b"redirect"),
                    );
                    let mut request = server.request("/start");
                    request.redirect = policy;
                    if named && !prefetch {
                        request = request.preload("held").unwrap();
                    }
                    core.run(job(2, request), None).unwrap();
                    let (_, outcome, timing) = collect(&core, &woke);
                    assert!(!timing.cached);
                    match policy {
                        Redirect::Manual => {
                            assert!(matches!(outcome, Outcome::Response(r) if r.status == 302))
                        }
                        Redirect::Error => assert!(matches!(
                            outcome,
                            Outcome::Failed {
                                kind: FailureKind::Network,
                                ..
                            }
                        )),
                        _ => unreachable!(),
                    }
                    assert_eq!(server.count(), 3);
                }
            }
        }
    }
}

#[test]
fn a_holds_cache_mode_preserves_wire_headers_and_storage_policy() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            for named in [false, true] {
                for holding in [
                    CacheMode::NoStore,
                    CacheMode::Default,
                    CacheMode::Reload,
                    CacheMode::NoCache,
                ] {
                    if prefetch && matches!(holding, CacheMode::Reload | CacheMode::NoCache) {
                        continue;
                    }
                    let server = Server::new();
                    let (core, woke) = runtime(&server, asynchronous);
                    server.plan("/mode", Plan::new(200, &[], b"held"));
                    start(
                        &core,
                        &server,
                        server.request("/mode").cache(holding),
                        prefetch,
                    );
                    ready(&core, 1);
                    if prefetch {
                        assert!(collect(&core, &woke).2.cached);
                    }
                    let mode = if holding == CacheMode::Default {
                        CacheMode::NoStore
                    } else {
                        CacheMode::Default
                    };
                    let mut request = server.request("/mode").cache(mode);
                    if named && !prefetch {
                        request = request.preload("held").unwrap();
                    }
                    server.plan("/mode", Plan::new(200, &[], b"wire"));
                    let (reply, cached) = ask(&core, &woke, job(2, request));
                    assert!(!cached);
                    assert_eq!(reply.body, b"wire");
                    assert_eq!(server.count(), 2);
                }
            }
        }
    }
}

#[test]
fn closing_a_failed_coordinator_drops_complete_and_concurrent_holds_and_stays_closed() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan("/complete", Plan::new(200, &[], b"complete"));
        let set = Preloads::start(
            vec![definition(&server, "/complete")],
            &core.grants,
            ibex2::host::Host::new,
        );
        complete_set(&set);
        let control = set.control();
        adopt(&core, set);
        server.plan("/prefetched", Plan::new(200, &[], b"prefetched"));
        assert!(
            ask(
                &core,
                &woke,
                job(1, server.request("/prefetched").prefetch())
            )
            .1
        );
        let alive_server = Server::new();
        alive_server.plan("/alive", Plan::new(200, &[], b"alive"));
        let alive_set = launch(&alive_server, vec![definition(&alive_server, "/alive")]);
        complete_set(&alive_set);
        let (alive, alive_woke) = alive_server.core(asynchronous, true);
        adopt(&alive, alive_set);
        let (reached, paused) = channel();
        let (resume, resume_at_close) = channel();
        *control.destination.before_close.lock().unwrap() = Some((reached, resume_at_close));
        // The coordinator fails on its own thread, with a prefetch's start
        // queued behind the failure, and closes.
        let fail = Box::new(|| panic!("injected preload coordinator panic"));
        control.send.send(Message::Hop(fail)).unwrap();
        server.plan("/panic", Plan::new(200, &[], b"own"));
        core.run(job(2, server.request("/panic").prefetch()), None)
            .unwrap();
        paused.recv_timeout(HANG).unwrap();
        server.plan("/during", Plan::new(200, &[], b"own"));
        core.run(job(3, server.request("/during").prefetch()), None)
            .unwrap();
        resume.send(()).unwrap();
        exited(&control.destination);
        let (gone, bytes) = {
            let state = core.shared.state.lock().unwrap();
            let cache = state.cache.as_ref().unwrap();
            (
                cache
                    .preloads
                    .iter()
                    .all(|p| matches!(p.reply, ReplyState::Gone)),
                cache.bytes,
            )
        };
        assert!(gone);
        assert_eq!(bytes, 0);
        for ticket in 2..=3 {
            let (got, outcome, timing) = collect(&core, &woke);
            assert_eq!(got, ticket);
            assert!(!timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"own"));
        }
        let count = core
            .shared
            .state
            .lock()
            .unwrap()
            .cache
            .as_ref()
            .unwrap()
            .preloads
            .len();
        server.plan("/later", Plan::new(200, &[], b"own"));
        assert!(!ask(&core, &woke, job(4, server.request("/later").prefetch())).1);
        control.extend(
            vec![definition(&server, "/refused")],
            &core.grants,
            Arc::new(|_, _| {}),
        );
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
            count
        );
        assert!(core
            .take_preload_journal()
            .iter()
            .any(|f| f.name == "complete" && f.outcome == Event::Cancelled));
        assert!(read(&alive, &alive_woke, &alive_server, 1, "/alive").1);
        assert_eq!(server.count(), 5);
        assert_eq!(alive_server.count(), 1);
    }
}

#[test]
fn a_coordinator_panic_releases_pending_launch_and_prefetch_waiters() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            let server = Server::new();
            let (mut core, woke) = server.core(asynchronous, true);
            let (entered, reached) = channel();
            let (release, held) = channel();
            let held = Arc::new(Mutex::new(held));
            let factory = move || {
                entered.send(()).unwrap();
                held.lock().unwrap().recv_timeout(HANG).unwrap();
                panic!("injected preload factory panic")
            };
            if prefetch {
                *core.lane.get_mut().unwrap() = Lane::new(Arc::new(factory));
                core.run(job(1, server.request("/panic").prefetch()), None)
                    .unwrap();
            } else {
                let set = Preloads::start(
                    vec![definition(&server, "/panic"), definition(&server, "/also")],
                    &format!("net.fetch http://{}", server.address),
                    factory,
                );
                adopt(&core, set);
                core.run(job(1, server.request("/panic")), None).unwrap();
            }
            reached.recv_timeout(HANG).unwrap();
            let mut second = server.request("/also");
            second.prefetch = prefetch;
            core.run(job(2, second), None).unwrap();
            assert_eq!(core.shared.state.lock().unwrap().jobs[0].len(), 2);
            server.plan("/panic", Plan::new(200, &[], b"fallback"));
            server.plan("/also", Plan::new(200, &[], b"fallback"));
            release.send(()).unwrap();
            for ticket in 1..=2 {
                let (got, outcome, timing) = collect(&core, &woke);
                assert_eq!(got, ticket);
                assert!(!timing.cached);
                assert!(matches!(outcome, Outcome::Response(r) if r.body == b"fallback"));
            }
            server.plan("/later", Plan::new(200, &[], b"fallback"));
            assert!(!ask(&core, &woke, job(3, server.request("/later").prefetch())).1);
            assert_eq!(server.count(), 3);
        }
    }
}

#[test]
fn a_forgotten_unsafe_waiter_is_sent_after_failure_drop_deadline_refusal_or_worker_panic() {
    struct Refuse {
        entered: Sender<()>,
        release: Arc<Mutex<Receiver<()>>>,
        panic: bool,
    }
    impl ibex2::stdlib::fetch::Transport for Refuse {
        fn open(
            &self,
            request: &ibex2::stdlib::fetch::Request,
            _: &ibex2::stdlib::abort::AbortSignal,
        ) -> Result<ibex2::stdlib::fetch::StreamingResponse, HostError> {
            assert!(request.speculative);
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv_timeout(HANG).unwrap();
            assert!(!self.panic, "injected preload worker panic");
            Err(HostError::Failed("Low Data Mode".into()))
        }
    }
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            for ending in 0..5 {
                let server = Server::new();
                let (core, woke) = runtime(&server, asynchronous);
                let control = core.preloads.lock().unwrap().as_ref().unwrap().control();
                let clock = Arc::new(Mutex::new(Instant::now()));
                let now = clock.clone();
                control.test_clock(Arc::new(move || *now.lock().unwrap()));
                let (release, held) = channel();
                let (entered, reached) = channel();
                if ending >= 3 {
                    let held = Arc::new(Mutex::new(held));
                    control.startup.lock().unwrap().as_mut().unwrap().lane =
                        Lane::new(Arc::new(move || {
                            ibex2::host::Host::with_transport(Box::new(Refuse {
                                entered: entered.clone(),
                                release: held.clone(),
                                panic: ending == 4,
                            }))
                        }));
                } else {
                    let mut reply = Plan::new(200, &[], b"unused");
                    reply.held = Some(held);
                    reply.disconnect = ending == 0;
                    server.plan("/effect", reply);
                }
                let mut request = server.request("/effect").timeout(1000);
                request.method = "POST".into();
                start(&core, &server, request.clone(), prefetch);
                if ending >= 3 {
                    reached.recv_timeout(HANG).unwrap();
                } else {
                    server.seen(1);
                }
                if !prefetch {
                    core.run(job(1, request), None).unwrap();
                }
                core.forget(|_| false);
                server.plan("/effect", Plan::new(200, &[], b"fallback"));
                if ending == 1 {
                    control.cancel_names(&[if prefetch {
                        String::new()
                    } else {
                        "held".into()
                    }]);
                } else if ending == 2 {
                    *clock.lock().unwrap() += Duration::from_secs(2);
                    control.test_tick();
                }
                release.send(()).unwrap();
                server.seen(if ending >= 3 { 1 } else { 2 });
                wait(&core, |s| s.counts == [0, 0]);
                assert!(core.drain().is_empty());
                assert!(core.ordered_idle());
                assert!(woke.try_iter().count() > 0);
            }
        }
    }
}

#[test]
fn a_preloads_transport_idle_timeout_follows_its_exchange_deadline() {
    struct Record(Sender<Option<Duration>>);
    impl ibex2::stdlib::fetch::Transport for Record {
        fn open(
            &self,
            request: &ibex2::stdlib::fetch::Request,
            signal: &ibex2::stdlib::abort::AbortSignal,
        ) -> Result<ibex2::stdlib::fetch::StreamingResponse, HostError> {
            self.0.send(request.timeout).unwrap();
            Ok(ibex2::stdlib::fetch::Response {
                status: 200,
                status_text: String::new(),
                headers: ibex2::stdlib::fetch::Headers::new(),
                body: b"held".to_vec(),
                url: request.url.clone(),
                redirected: false,
            }
            .into_stream(request.body_limit(), signal.clone()))
        }
    }
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            let server = Server::new();
            let (mut core, woke) = server.core(asynchronous, true);
            let (send, recorded) = channel();
            let host = move || ibex2::host::Host::with_transport(Box::new(Record(send.clone())));
            let request = server.request("/deadline").timeout(120_000);
            if prefetch {
                *core.lane.get_mut().unwrap() = Lane::new(Arc::new(host));
                core.run(job(1, request.prefetch()), None).unwrap();
            } else {
                let mut def = definition(&server, "/deadline");
                def.request = request.clone();
                def.deadline_ms = Some(180_000);
                adopt(&core, Preloads::start(vec![def], &core.grants, host));
                core.run(job(1, request), None).unwrap();
            }
            assert!(collect(&core, &woke).2.cached);
            assert_eq!(
                recorded.recv_timeout(HANG).unwrap(),
                Some(Duration::from_secs(if prefetch { 121 } else { 181 }))
            );
        }
    }
}

#[test]
fn a_panicking_launch_observer_cannot_strand_a_hold_or_poison_its_executor() {
    for asynchronous in [false, true] {
        for phase in [0, 1, 2] {
            let server = Server::new();
            let (core, woke) = runtime(&server, asynchronous);
            let release = held_plan(&server, "/observed", b"held");
            let observer: Observer = Arc::new(move |_, observation| {
                if matches!(
                    (phase, observation),
                    (0, Observation::Complete)
                        | (1, Observation::Handed)
                        | (2, Observation::Taken(_))
                ) {
                    panic!("injected launch observer panic");
                }
            });
            core.preloads
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .control()
                .extend(
                    vec![definition(&server, "/observed")],
                    &core.grants,
                    observer,
                );
            core.run(job(1, server.request("/observed")), None).unwrap();
            release.send(()).unwrap();
            let (_, outcome, timing) = collect(&core, &woke);
            assert!(timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"held"));
            assert_eq!(server.count(), 1);
        }
    }
}

#[test]
fn cancelling_an_unsafe_hold_invalidates_before_its_failure_message_is_processed() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            let server = Server::new();
            let (core, woke) = runtime(&server, asynchronous);
            let control = core.preloads.lock().unwrap().as_ref().unwrap().control();
            let clock = Arc::new(Mutex::new(Instant::now()));
            let now = clock.clone();
            control.test_clock(Arc::new(move || *now.lock().unwrap()));
            let (entered, reached) = channel();
            let (release, held) = channel::<()>();
            server.plan(
                "/cart",
                Plan::new(200, &[("Cache-Control", "max-age=60")], b"old"),
            );
            assert!(!read(&core, &woke, &server, 10, "/cart").1);
            let exchange = held_plan(&server, "/cart", b"unused");
            let mut request = server.request("/cart").timeout(1000);
            request.method = "POST".into();
            start(&core, &server, request, prefetch);
            server.seen(2);
            if prefetch {
                core.cancel(&[1]);
            }
            // The preload thread is held, so the hold's failure waits unread.
            let stall = move || {
                entered.send(()).unwrap();
                held.recv_timeout(HANG).unwrap();
            };
            control.send.send(Message::Hop(Box::new(stall))).unwrap();
            reached.recv_timeout(HANG).unwrap();
            *clock.lock().unwrap() += Duration::from_secs(2);
            control.test_tick();
            server.plan("/cart", Plan::new(200, &[], b"new"));
            let (reply, cached) = read(&core, &woke, &server, 20, "/cart");
            release.send(()).unwrap();
            exchange.send(()).unwrap();
            assert!(!cached);
            assert_eq!(reply.body, b"new");
        }
    }
}
