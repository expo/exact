use super::*;

fn wait_for_reply(core: &Core, ticket: u64) {
    let end = Instant::now() + HANG;
    let mut state = core.shared.state.lock().unwrap();
    while !state.completed[0].iter().any(|d| d.ticket == ticket) {
        assert!(Instant::now() < end, "ticket {ticket} did not complete");
        // An out-of-order completion has no presenter wake (LLP 1162 D1).
        state = core
            .shared
            .ready
            .wait_timeout(state, Duration::from_millis(50))
            .unwrap()
            .0;
    }
}

fn expect_reply(core: &Core, woke: &Receiver<()>, ticket: u64, body: &[u8]) {
    let (got, outcome, _) = collect(core, woke);
    assert_eq!(got, ticket);
    assert!(matches!(outcome, Outcome::Response(r) if r.body == body));
}

#[test]
fn a_waiting_cacheable_read_retains_only_a_flag_beside_its_reserved_request() {
    let mut wrong = Vec::new();
    for asynchronous in [false, true] {
        for fenced in [false, true] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            let (release, held) = channel();
            let mut hold = Plan::new(200, &[], b"lane");
            hold.held = Some(held);
            server.plan("/hold", hold);
            let mut hold = job(1, server.request("/hold"));
            hold.class = RequestClass::Ordered;
            core.run(hold, None).unwrap();
            server.seen(1);
            if fenced {
                core.shared.state.lock().unwrap().ordered_barrier = true;
            }
            let mut request = server.request("/feed?");
            request.url.push_str(&"x".repeat(2 << 20));
            let (_, charge, _) = reads::reservation(&request, true).unwrap();
            core.run(job(2, request), None).unwrap();
            {
                let state = core.shared.state.lock().unwrap();
                let cache_bytes = if fenced {
                    assert!(state.jobs[0].is_empty());
                    let Fenced::Held(r, _, retained, _, cache_store_withheld) =
                        state.fenced.front().unwrap()
                    else {
                        panic!("the read must wait behind the fence");
                    };
                    assert_eq!(r.ticket, 2);
                    assert_eq!(*retained, charge);
                    assert_eq!(state.fenced_bytes, charge);
                    std::mem::size_of_val(cache_store_withheld)
                } else {
                    let waiting = state.jobs[0].front().unwrap();
                    assert_eq!(waiting.ticket, 2);
                    assert_eq!(waiting.charge, charge);
                    assert_eq!(waiting.waiting_charge, charge);
                    std::mem::size_of_val(&waiting.cache_store_withheld)
                };
                if cache_bytes > std::mem::size_of::<bool>() {
                    wrong.push((asynchronous, fenced, cache_bytes));
                }
            }
            core.cancel(&[2]);
            if fenced {
                assert!(core.resume_ordered().is_empty());
            }
            release.send(()).unwrap();
            expect_reply(&core, &woke, 1, b"lane");
        }
    }
    assert!(
        wrong.is_empty(),
        "waiting cache state owns more than a flag: {wrong:?}"
    );
}

#[test]
fn a_refresh_prevents_an_earlier_running_read_from_storing() {
    let mut wrong = Vec::new();
    for asynchronous in [false, true] {
        for mode in [CacheMode::Default, CacheMode::Reload, CacheMode::NoCache] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            let (release, held) = channel();
            let mut old = Plan::new(200, FRESH, b"old");
            old.held = Some(held);
            server.plan("/feed", old);
            server.plan("/feed", Plan::new(200, FRESH, b"new"));
            core.run(job(20, server.request("/feed")), None).unwrap();
            server.seen(1);
            let mut refresh = job(10, server.request("/feed").cache(mode));
            refresh.forced = mode == CacheMode::Default;
            core.run(refresh, None).unwrap();
            wait_for_reply(&core, 10);
            assert!(core.drain().is_empty());
            release.send(()).unwrap();
            expect_reply(&core, &woke, 20, b"old");
            expect_reply(&core, &woke, 10, b"new");
            let (reply, cached) = read(&core, &woke, &server, 30, "/feed");
            if reply.body != b"new" {
                wrong.push((asynchronous, mode));
            }
            assert!(cached);
            assert_eq!(server.count(), 2);
        }
    }
    assert!(wrong.is_empty(), "older replies stored: {wrong:?}");
}

#[test]
fn a_refresh_prevents_an_earlier_queued_or_fenced_read_from_storing() {
    let mut wrong = Vec::new();
    for asynchronous in [false, true] {
        for mode in [CacheMode::Default, CacheMode::Reload, CacheMode::NoCache] {
            for fenced in [false, true] {
                let server = Server::new();
                let (core, woke) = server.core(asynchronous, true);
                let (open_lane, held) = channel();
                let mut hold = Plan::new(200, &[], b"lane");
                hold.held = Some(held);
                server.plan("/hold", hold);
                let mut hold = job(90, server.request("/hold"));
                hold.class = RequestClass::Ordered;
                core.run(hold, None).unwrap();
                server.seen(1);
                if fenced {
                    core.shared.state.lock().unwrap().ordered_barrier = true;
                }
                let (release, held) = channel();
                let mut old = Plan::new(200, FRESH, b"old");
                old.held = Some(held);
                old.cache_control = Some("");
                server.plan("/feed", old);
                let mut new = Plan::new(200, FRESH, b"new");
                new.cache_control = Some(if mode == CacheMode::NoCache {
                    "max-age=0"
                } else {
                    "no-cache"
                });
                server.plan("/feed", new);
                core.run(job(20, server.request("/feed")), None).unwrap();
                let mut refresh = job(10, server.request("/feed").cache(mode));
                refresh.forced = mode == CacheMode::Default;
                core.run(refresh, None).unwrap();
                {
                    let state = core.shared.state.lock().unwrap();
                    assert_eq!(state.jobs[0].len(), if fenced { 0 } else { 2 });
                    assert_eq!(state.fenced.len(), if fenced { 2 } else { 0 });
                }
                if fenced {
                    assert!(core.resume_ordered().is_empty());
                }
                open_lane.send(()).unwrap();
                expect_reply(&core, &woke, 90, b"lane");
                server.seen(3);
                wait_for_reply(&core, 10);
                release.send(()).unwrap();
                expect_reply(&core, &woke, 20, b"old");
                expect_reply(&core, &woke, 10, b"new");
                let (reply, cached) = read(&core, &woke, &server, 30, "/feed");
                if reply.body != b"new" {
                    wrong.push((asynchronous, mode, fenced));
                }
                assert!(cached);
                assert_eq!(server.count(), 3);
            }
        }
    }
    assert!(wrong.is_empty(), "older replies stored: {wrong:?}");
}

#[test]
fn an_earlier_304_answers_its_caller_without_renewing_over_a_refresh() {
    let mut wrong = Vec::new();
    for asynchronous in [false, true] {
        for mode in [CacheMode::Default, CacheMode::Reload, CacheMode::NoCache] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            server.plan("/feed", Plan::new(200, &[("ETag", "\"old\"")], b"old"));
            read(&core, &woke, &server, 1, "/feed");
            let (release, held) = channel();
            let mut old = Plan::new(304, FRESH, b"");
            old.held = Some(held);
            server.plan("/feed", old);
            server.plan("/feed", Plan::new(200, FRESH, b"new"));
            core.run(job(20, server.request("/feed")), None).unwrap();
            server.seen(2);
            assert_eq!(first(&server.headers(1), "if-none-match"), Some("\"old\""));
            let mut refresh = job(10, server.request("/feed").cache(mode));
            refresh.forced = mode == CacheMode::Default;
            core.run(refresh, None).unwrap();
            wait_for_reply(&core, 10);
            release.send(()).unwrap();
            let (ticket, outcome, timing) = collect(&core, &woke);
            assert_eq!(ticket, 20);
            assert!(timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.status == 200 && r.body == b"old"));
            expect_reply(&core, &woke, 10, b"new");
            let (reply, cached) = read(&core, &woke, &server, 30, "/feed");
            if reply.body != b"new" {
                wrong.push((asynchronous, mode));
            }
            assert!(cached);
            assert_eq!(server.count(), 3);
        }
    }
    assert!(wrong.is_empty(), "older validations stored: {wrong:?}");
}

#[test]
fn dropping_a_validations_previous_entry_releases_its_retired_bytes() {
    let mut wrong = Vec::new();
    for asynchronous in [false, true] {
        for status in [304, 503, 200] {
            for withheld in [false, true] {
                if status != 200 && !withheld {
                    continue;
                }
                let server = Server::new();
                let (core, woke) = server.core(asynchronous, true);
                server.plan("/feed", Plan::new(200, &[("ETag", "\"old\"")], b"old"));
                read(&core, &woke, &server, 1, "/feed");
                let (release, held) = channel();
                let redirected = status == 200;
                let mut old = if redirected {
                    Plan::new(302, &[("Location", "/other")], b"")
                } else {
                    Plan::new(status, FRESH, b"")
                };
                old.held = Some(held);
                server.plan("/feed", old);
                if redirected {
                    server.plan("/other", Plan::new(200, FRESH, b"other"));
                }
                core.run(job(20, server.request("/feed")), None).unwrap();
                server.seen(2);
                assert_eq!(first(&server.headers(1), "if-none-match"), Some("\"old\""));
                if withheld {
                    server.plan("/feed", Plan::new(200, FRESH, b"new"));
                    let mut refresh = job(10, server.request("/feed"));
                    refresh.forced = true;
                    core.run(refresh, None).unwrap();
                    wait_for_reply(&core, 10);
                    let state = core.shared.state.lock().unwrap();
                    assert_eq!(state.cache.as_ref().unwrap().retired.len(), 1);
                }
                release.send(()).unwrap();
                let (ticket, outcome, timing) = collect(&core, &woke);
                assert_eq!(ticket, 20);
                let Outcome::Response(reply) = outcome else {
                    panic!("{outcome:?}");
                };
                assert_eq!(reply.status, if status == 304 { 200 } else { status });
                if status == 304 {
                    assert!(timing.cached);
                    assert_eq!(reply.body, b"old");
                }
                if withheld {
                    expect_reply(&core, &woke, 10, b"new");
                }
                let state = core.shared.state.lock().unwrap();
                let cache = state.cache.as_ref().unwrap();
                let live_bytes: usize = cache.entries.iter().map(|entry| entry.bytes).sum();
                if cache.bytes != live_bytes || !cache.retired.is_empty() {
                    wrong.push((asynchronous, status, withheld, cache.bytes, live_bytes));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "retired entries still charged: {wrong:?}");
}

#[test]
fn a_redirected_304_releases_retired_bytes_before_its_unconditional_retry() {
    let mut wrong = Vec::new();
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan("/feed", Plan::new(200, &[("ETag", "\"old\"")], b"old"));
        read(&core, &woke, &server, 1, "/feed");
        let (release, held) = channel();
        let mut redirect = Plan::new(302, &[("Location", "/other")], b"");
        redirect.held = Some(held);
        server.plan("/feed", redirect);
        server.plan("/other", Plan::new(304, FRESH, b""));
        core.run(job(20, server.request("/feed")), None).unwrap();
        server.seen(2);
        server.plan("/feed", Plan::new(200, FRESH, b"new"));
        let mut refresh = job(10, server.request("/feed"));
        refresh.forced = true;
        core.run(refresh, None).unwrap();
        wait_for_reply(&core, 10);
        let (finish_retry, held) = channel();
        let mut retry = Plan::new(200, FRESH, b"retry");
        retry.held = Some(held);
        server.plan("/feed", retry);
        release.send(()).unwrap();
        server.seen(5);
        assert_eq!(first(&server.headers(4), "if-none-match"), None);
        {
            let state = core.shared.state.lock().unwrap();
            let cache = state.cache.as_ref().unwrap();
            let live_bytes: usize = cache.entries.iter().map(|entry| entry.bytes).sum();
            if cache.bytes != live_bytes || !cache.retired.is_empty() {
                wrong.push((asynchronous, cache.bytes, live_bytes));
            }
        }
        finish_retry.send(()).unwrap();
        expect_reply(&core, &woke, 20, b"retry");
        expect_reply(&core, &woke, 10, b"new");
        let (reply, cached) = read(&core, &woke, &server, 30, "/feed");
        assert!(cached);
        assert_eq!(reply.body, b"new");
    }
    assert!(
        wrong.is_empty(),
        "retry still charges retired entries: {wrong:?}"
    );
}

#[test]
fn a_read_asked_after_a_refresh_still_stores() {
    for asynchronous in [false, true] {
        for mode in [CacheMode::Default, CacheMode::Reload, CacheMode::NoCache] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            let (release_first, held) = channel();
            let mut first = Plan::new(200, FRESH, b"first");
            first.held = Some(held);
            server.plan("/feed", first);
            let (release_last, held) = channel();
            let mut last = Plan::new(200, FRESH, b"last");
            last.held = Some(held);
            server.plan("/feed", last);
            let mut first = job(20, server.request("/feed").cache(mode));
            first.forced = mode == CacheMode::Default;
            core.run(first, None).unwrap();
            server.seen(1);
            core.run(job(10, server.request("/feed")), None).unwrap();
            server.seen(2);
            release_first.send(()).unwrap();
            expect_reply(&core, &woke, 20, b"first");
            release_last.send(()).unwrap();
            expect_reply(&core, &woke, 10, b"last");
            let (reply, cached) = read(&core, &woke, &server, 30, "/feed");
            assert_eq!(reply.body, b"last");
            assert!(cached);
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn plain_overlapping_reads_store_whichever_reply_completes_last() {
    for asynchronous in [false, true] {
        for reverse in [false, true] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            let (release_first, held) = channel();
            let mut first = Plan::new(200, FRESH, b"first");
            first.held = Some(held);
            server.plan("/feed", first);
            let (release_second, held) = channel();
            let mut second = Plan::new(200, FRESH, b"second");
            second.held = Some(held);
            server.plan("/feed", second);
            core.run(job(20, server.request("/feed")), None).unwrap();
            server.seen(1);
            core.run(job(10, server.request("/feed")), None).unwrap();
            server.seen(2);
            if reverse {
                release_second.send(()).unwrap();
                wait_for_reply(&core, 10);
                release_first.send(()).unwrap();
                expect_reply(&core, &woke, 20, b"first");
            } else {
                release_first.send(()).unwrap();
                expect_reply(&core, &woke, 20, b"first");
                release_second.send(()).unwrap();
            }
            expect_reply(&core, &woke, 10, b"second");
            let (reply, cached) = read(&core, &woke, &server, 30, "/feed");
            assert_eq!(
                reply.body,
                if reverse {
                    b"first".as_slice()
                } else {
                    b"second"
                }
            );
            assert!(cached);
            assert_eq!(server.count(), 2);
        }
    }
}

fn queued(ticket: u64, request: Request, forced: bool) -> Job {
    let (_, charge, limit) = reads::reservation(&request, true).unwrap();
    Job {
        ticket,
        cache_store_withheld: false,
        request,
        forced,
        work: None,
        waiting_charge: charge,
        charge,
        limit,
        forgotten: false,
        queued_deadline: None,
        overlap: true,
        class: RequestClass::Read,
    }
}

#[test]
fn a_read_after_a_queued_or_running_refresh_cannot_hit_the_copy_being_replaced() {
    for mode in [CacheMode::Reload, CacheMode::NoCache, CacheMode::Default] {
        for running in [false, true] {
            let server = Server::new();
            let (core, woke) = server.core(false, true);
            server.plan("/a", Plan::new(200, FRESH, b"v1"));
            read(&core, &woke, &server, 1, "/a");
            let mut state = State {
                cache: core.shared.state.lock().unwrap().cache.take(),
                ..State::default()
            };
            for job in [
                queued(
                    20,
                    server.request("/a").cache(mode),
                    mode == CacheMode::Default,
                ),
                queued(10, server.request("/a"), false),
            ] {
                state.ordered.push_back(job.ticket);
                state.bytes[0] += job.charge;
                state.counts[0] += 1;
                state.jobs[0].push_back(job);
            }
            if running {
                reads::next_job(&core.shared, &mut state, 0).unwrap();
            }
            serve_ready(&mut state);
            assert!(
                state.completed[0].is_empty(),
                "mode {mode:?}, running {running}"
            );
            assert!(state.jobs[0].iter().any(|j| j.ticket == 10));
        }
    }
}

#[test]
fn a_worker_or_async_starter_treats_a_read_after_a_refresh_as_a_miss() {
    for mode in [CacheMode::Reload, CacheMode::NoCache, CacheMode::Default] {
        let server = Server::new();
        let (core, woke) = server.core(false, true);
        server.plan(
            "/a",
            Plan::new(
                200,
                &[("Cache-Control", "max-age=60"), ("ETag", "\"v1\"")],
                b"v1",
            ),
        );
        read(&core, &woke, &server, 1, "/a");
        {
            let mut state = core.shared.state.lock().unwrap();
            for job in [
                queued(
                    20,
                    server.request("/a").cache(mode),
                    mode == CacheMode::Default,
                ),
                queued(10, server.request("/a"), false),
            ] {
                state.ordered.push_back(job.ticket);
                state.bytes[0] += job.charge;
                state.counts[0] += 1;
                state.jobs[0].push_back(job);
            }
            for _ in 0..2 {
                reads::next_job(&core.shared, &mut state, 0).unwrap();
            }
        }
        let mut request = server.request("/a");
        let outcome = prepare(&core.shared, 10, &mut request, RequestClass::Read, false);
        let cached = core
            .shared
            .state
            .lock()
            .unwrap()
            .running
            .iter()
            .any(|r| r.ticket == 10 && r.cached);
        assert!(outcome.is_none(), "{mode:?}");
        assert!(!cached);
        assert_eq!(first(&request.headers, "if-none-match"), None);
    }
}

#[test]
fn a_later_read_uses_the_network_beside_a_refresh_and_never_returns_the_old_hit() {
    for asynchronous in [false, true] {
        for mode in [CacheMode::Reload, CacheMode::NoCache, CacheMode::Default] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            server.plan("/a", Plan::new(200, FRESH, b"v1"));
            read(&core, &woke, &server, 1, "/a");
            let (release, held) = channel();
            let mut refresh = Plan::new(200, FRESH, b"v2");
            refresh.held = Some(held);
            server.plan("/a", refresh);
            server.plan("/a", Plan::new(200, FRESH, b"v3"));
            let mut refresh = job(2, server.request("/a").cache(mode));
            refresh.forced = mode == CacheMode::Default;
            core.run(refresh, None).unwrap();
            server.seen(2);
            core.run(job(3, server.request("/a")), None).unwrap();
            let stale = core.shared.state.lock().unwrap().completed[0]
                .iter()
                .any(|d| d.ticket == 3 && d.cached);
            if !stale {
                server.seen(3);
            }
            release.send(()).unwrap();
            assert!(!stale, "asynchronous {asynchronous}, mode {mode:?}");
            for ticket in [2, 3] {
                let (got, outcome, _) = collect(&core, &woke);
                assert_eq!(got, ticket);
                let Outcome::Response(response) = outcome else {
                    panic!("{outcome:?}");
                };
                assert_ne!(response.body, b"v1");
            }
        }
    }
}

#[test]
fn equal_uncacheable_reads_have_their_own_exchanges_even_for_posts_and_errors() {
    for asynchronous in [false, true] {
        for method in ["GET", "POST"] {
            for inflight in [false, true] {
                let server = Server::new();
                let (core, woke) = server.core(asynchronous, true);
                let (release, held) = channel();
                let mut first = Plan::new(503, &[], b"first");
                first.held = Some(held);
                server.plan("/next", first);
                server.plan("/next", Plan::new(200, &[], b"second"));
                let mut request = server.request("/next");
                request.method = method.into();
                core.run(job(1, request.clone()), None).unwrap();
                server.seen(1);
                if inflight {
                    core.run(job(2, request.clone()), None).unwrap();
                    server.seen(2);
                }
                release.send(()).unwrap();
                let (ticket, outcome, timing) = collect(&core, &woke);
                assert_eq!(ticket, 1);
                assert!(!timing.cached);
                assert!(
                    matches!(outcome, Outcome::Response(r) if r.status == 503 && r.body == b"first")
                );
                if !inflight {
                    core.run(job(2, request), None).unwrap();
                    server.seen(2);
                }
                let (ticket, outcome, timing) = collect(&core, &woke);
                assert_eq!(ticket, 2);
                assert!(!timing.cached);
                assert!(
                    matches!(outcome, Outcome::Response(r) if r.status == 200 && r.body == b"second")
                );
                assert_eq!(server.count(), 2);
            }
        }
    }
}

#[test]
fn prefetch_is_a_plain_fetch_for_tools_wakes_and_devices() {
    for asynchronous in [false, true] {
        for origin in [
            exact_runner::CallOrigin::Tool,
            exact_runner::CallOrigin::Wake,
            exact_runner::CallOrigin::Device,
        ] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            for (index, prefetch) in [false, true].into_iter().enumerate() {
                server.plan("/next", Plan::new(503, &[], b"answer"));
                let mut request = server.request("/next");
                request.reach.origin = origin;
                request.prefetch = prefetch;
                let (reply, cached) = ask(&core, &woke, job(index as u64 + 1, request));
                assert_eq!(reply.status, 503);
                assert_eq!(reply.body, b"answer");
                assert!(!cached);
            }
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn prefetch_refuses_sends_streams_files_and_independent_requests() {
    for asynchronous in [false, true] {
        let server = Server::new();
        for kind in 0..6 {
            let (core, _) = server.core(asynchronous, true);
            let mut r = job(1, server.request("/refused").prefetch());
            match kind {
                0 => r.class = RequestClass::Ordered,
                1 => r.request.stream = true,
                2 => r.request = r.request.body_from("app:/data/file"),
                3 => r.request = r.request.body_to("app:/data/file"),
                4 => r.request = r.request.independent_http(1024),
                _ => r.class = RequestClass::Parallel(0),
            }
            assert_eq!(
                core.run(r, None),
                Err("exactPrefetch requires a plain HTTP read")
            );
        }
        assert_eq!(server.count(), 0);
    }
}
