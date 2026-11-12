use super::*;

fn queued(ticket: u64, request: Request, forced: bool) -> Job {
    let (_, charge, limit) = reads::reservation(&request, true).unwrap();
    Job {
        ticket,
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
