use super::super::cache::tests::{ask, collect, job, read, Plan, Server, HANG};
use super::*;
use std::sync::mpsc::{channel, Sender};

fn definition(server: &Server, path: &str) -> Definition {
    Definition {
        name: path.trim_start_matches('/').into(),
        request: server.request(path),
        deadline_ms: None,
        secrets: Vec::new(),
    }
}
fn launch(server: &Server, definitions: Vec<Definition>) -> Preloads {
    Preloads::start(
        definitions,
        &format!("net.fetch http://{}", server.address),
        ibex2::host::Host::new,
    )
}
fn adopt(core: &Core, set: Preloads) {
    assert!(core.adopt_preloads(set).is_ok());
}
fn held_plan(server: &Server, path: &str, body: &[u8]) -> Sender<()> {
    let (release, held) = channel();
    let mut plan = Plan::new(200, &[], body);
    plan.held = Some(held);
    server.plan(path, plan);
    release
}
fn ready(core: &Core, count: usize) {
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
                .filter(|p| p.used().is_some())
                .count()
                < count
        })
        .unwrap();
    drop(state);
    assert!(!timeout.timed_out(), "preload replies did not finish");
}
fn fresh_clock(server: &Server, definitions: Vec<Definition>) -> (Preloads, Arc<Mutex<Instant>>) {
    let clock = Arc::new(Mutex::new(Instant::now()));
    let now = clock.clone();
    (
        Preloads::start_clock(
            definitions,
            &format!("net.fetch http://{}", server.address),
            Arc::new(ibex2::host::Host::new),
            Arc::new(move || *now.lock().unwrap()),
        ),
        clock,
    )
}

#[test]
fn launch_reaches_the_server_before_an_executor_and_both_cores_take_it() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let release = held_plan(&server, "/early", b"early");
        let set = launch(&server, vec![definition(&server, "/early")]);
        server.seen(1);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        for ticket in 1..=2 {
            core.run(job(ticket, server.request("/early")), None)
                .unwrap();
        }
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(effect_count(&state), 0);
            assert!(state.running.is_empty());
            assert!(state.completed[0].is_empty());
        }
        release.send(()).unwrap();
        for expected in 1..=2 {
            let (ticket, outcome, timing) = collect(&core, &woke);
            assert_eq!(ticket, expected);
            assert!(timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"early"));
        }
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn a_tool_read_queued_before_its_preload_starts_is_sent_after_the_send() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let sender = Server::new();
        let grants = format!(
            "net.fetch http://{}\nnet.fetch http://{}",
            server.address, sender.address
        );
        let (set, _clock) = fresh_clock(&server, Vec::new());
        let control = set.control();
        let (core, woke) = server.core_grants(asynchronous, true, &grants);
        adopt(&core, set);
        let release = held_plan(&sender, "/send", b"send");
        let mut send = job(1, sender.request("/send"));
        send.class = RequestClass::Ordered;
        send.request.method = "POST".into();
        core.run(send, None).unwrap();
        sender.seen(1);
        let mut tool = job(2, server.request("/foreground"));
        tool.request.reach = exact_runner::local_network::Reach {
            origin: exact_runner::CallOrigin::Tool,
            substitute: Some(exact_runner::local_network::State::Unknown),
        };
        core.run(tool, None).unwrap();
        {
            let state = core.shared.state.lock().unwrap();
            assert!(state.cache.as_ref().unwrap().preloads.is_empty());
            assert!(state.jobs[0].iter().any(|j| j.ticket == 2));
        }
        server.plan("/foreground", Plan::new(200, &[], b"preload"));
        control.extend(
            vec![definition(&server, "/foreground")],
            &grants,
            Arc::new(|_, _| {}),
        );
        ready(&core, 1);
        assert_eq!(server.count(), 1);
        server.plan("/foreground", Plan::new(200, &[], b"sent"));
        release.send(()).unwrap();
        assert_eq!(collect(&core, &woke).0, 1);
        let (ticket, outcome, timing) = collect(&core, &woke);
        assert_eq!(ticket, 2);
        assert!(!timing.cached);
        assert!(matches!(outcome, Outcome::Response(r) if r.body == b"sent"));
        assert_eq!(server.count(), 2);
        let (reply, cached) = read(&core, &woke, &server, 3, "/foreground");
        assert!(cached);
        assert_eq!(reply.body, b"preload");
        assert_eq!(server.count(), 2);
    }
}

#[test]
fn tool_reads_bypass_complete_and_inflight_preloads_without_waiting() {
    for asynchronous in [false, true] {
        for pending in [false, true] {
            let server = Server::new();
            let release = held_plan(&server, "/held", b"preload");
            let (set, _clock) = fresh_clock(&server, vec![definition(&server, "/held")]);
            server.seen(1);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            if !pending {
                release.send(()).unwrap();
                ready(&core, 1);
            }
            server.plan("/held", Plan::new(200, &[], b"sent"));
            let mut tool = job(1, server.request("/held"));
            tool.request.reach.origin = exact_runner::CallOrigin::Tool;
            let (reply, cached) = ask(&core, &woke, tool);
            assert!(!cached);
            assert_eq!(reply.body, b"sent");
            assert_eq!(server.count(), 2);
            core.run(job(2, server.request("/held")), None).unwrap();
            if pending {
                {
                    let state = core.shared.state.lock().unwrap();
                    assert!(state.jobs[0].iter().any(|j| j.ticket == 2));
                    assert!(state.running.is_empty());
                }
                release.send(()).unwrap();
            }
            let (ticket, outcome, timing) = collect(&core, &woke);
            assert_eq!(ticket, 2);
            assert!(timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"preload"));
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn equality_folds_header_names_and_compares_every_body_byte_and_url_for_any_method() {
    for asynchronous in [false, true] {
        for method in ["GET", "POST"] {
            let server = Server::new();
            let status = if method == "GET" { 200 } else { 503 };
            server.plan(
                "/equal",
                Plan::new(status, &[("Cache-Control", "max-age=60")], b"held status"),
            );
            let mut def = definition(&server, "/equal");
            def.request.method = method.into();
            // URLSession refuses a GET that has a body, as a browser's fetch does.
            let bodied = method != "GET";
            if bodied {
                def.request.body = b"one".to_vec();
            }
            def.request.headers = vec![("X-A".into(), "1".into()), ("X-B".into(), "2".into())];
            let mut equal = def.request.clone();
            equal.headers = vec![("x-b".into(), "2".into()), ("x-a".into(), "1".into())];
            let set = launch(&server, vec![def]);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            ready(&core, 1);
            let (reply, cached) = ask(&core, &woke, job(1, equal.clone()));
            assert!(cached);
            assert_eq!(reply.status, status);
            for difference in 0..3 {
                if difference == 1 && !bodied {
                    continue;
                }
                let mut request = equal.clone();
                match difference {
                    0 => request.headers[0].1 = "different".into(),
                    1 => request.body.push(0),
                    _ => request.url.push_str("?different"),
                }
                let path = if difference == 2 {
                    "/equal?different"
                } else {
                    "/equal"
                };
                server.plan(path, Plan::new(200, &[], b"sent"));
                let (response, cached) = ask(&core, &woke, job(2, request));
                assert!(!cached);
                assert_eq!(response.body, b"sent");
            }
            assert_eq!(server.count(), if bodied { 4 } else { 3 });
        }
    }
}

#[test]
fn a_network_failure_or_a_held_clock_deadline_sends_the_waiting_read() {
    for asynchronous in [false, true] {
        for deadline in [false, true] {
            let server = Server::new();
            let (release, held) = channel();
            let mut plan = Plan::new(200, &[], b"ignored");
            plan.held = Some(held);
            plan.disconnect = !deadline;
            server.plan("/request", plan);
            let mut def = definition(&server, "/request");
            // POST's failed exchange is not retried by the platform client.
            def.request.method = "POST".into();
            def.deadline_ms = Some(1000);
            let request = def.request.clone();
            let (set, clock) = fresh_clock(&server, vec![def]);
            server.seen(1);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            core.run(job(1, request), None).unwrap();
            server.plan("/request", Plan::new(200, &[], b"fallback"));
            if deadline {
                *clock.lock().unwrap() += Duration::from_secs(2);
                core.preloads
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .send
                    .send(Message::Tick)
                    .unwrap();
            }
            release.send(()).unwrap();
            let (_, outcome, timing) = collect(&core, &woke);
            assert!(!timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"fallback"));
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn sends_forced_asks_streams_and_independent_requests_do_not_take_a_preload() {
    for asynchronous in [false, true] {
        // Each kind meets its own complete reply: a forced ask drops the one it equals.
        for kind in 0..6 {
            let server = Server::new();
            server.plan("/held", Plan::new(200, &[], b"held"));
            let mut def = definition(&server, "/held");
            let mut request = server.request("/held");
            if kind == 5 {
                // A file-bodied request is no GET; its preload is the equal POST.
                def.request.method = "POST".into();
                request.method = "POST".into();
            }
            let set = launch(&server, vec![def]);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            ready(&core, 1);
            server.plan("/held", Plan::new(200, &[], b"sent"));
            let mut r = job(1, request);
            match kind {
                0 => r.class = RequestClass::Ordered,
                1 => r.class = RequestClass::Parallel(1),
                2 => r.forced = true,
                3 => {
                    r.request.http = HttpScheduling::Independent {
                        max_response_bytes: 1024,
                    }
                }
                4 => {
                    r.request.stream = true;
                    r.request.http = HttpScheduling::Independent {
                        max_response_bytes: 1024,
                    };
                }
                // Its file is missing, so it ends before contact: still not the held reply.
                _ => r.request = r.request.body_from("app:/data/missing"),
            }
            if core.run(r, None).is_ok() {
                let (_, outcome, timing) = collect(&core, &woke);
                assert!(!timing.cached, "kind {kind}");
                assert!(
                    !matches!(&outcome, Outcome::Response(r) if r.body == b"held"),
                    "kind {kind}"
                );
            } else {
                assert_eq!(kind, 5);
            }
            assert_eq!(server.count(), if kind == 5 { 1 } else { 2 }, "kind {kind}");
        }
    }
}

#[test]
fn a_write_drops_a_preload_at_its_head_and_on_unanswered_failure() {
    for asynchronous in [false, true] {
        for failure in [false, true] {
            let server = Server::new();
            server.plan("/item", Plan::new(200, &[], b"held"));
            let set = launch(&server, vec![definition(&server, "/item")]);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            ready(&core, 1);
            let (release, after) = channel();
            let mut plan = Plan::new(200, &[], &[b'.'; 2048]);
            plan.disconnect = failure;
            if !failure {
                plan.declares = Some(4096);
                plan.after = Some(after);
            }
            server.plan("/item", plan);
            let mut request = server.request("/item");
            request.method = "POST".into();
            core.run(job(1, request), None).unwrap();
            let state = core.shared.state.lock().unwrap();
            let (state, timeout) = core
                .shared
                .ready
                .wait_timeout_while(state, HANG, |s| {
                    s.cache.as_ref().unwrap().preloads[0].used().is_some()
                })
                .unwrap();
            assert!(!timeout.timed_out());
            if !failure {
                assert!(state.running.iter().any(|r| !r.terminal));
            }
            drop(state);
            let _ = release.send(());
            assert!(matches!(collect(&core, &woke).1, Outcome::Failed { .. }));
            server.plan("/item", Plan::new(200, &[], b"after write"));
            assert!(!read(&core, &woke, &server, 2, "/item").1);
        }
    }
}

#[test]
fn a_secret_write_cancels_pending_uses_and_drops_complete_uses_only_of_that_secret() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let release = held_plan(&server, "/pending", b"discard");
        for path in ["/complete", "/other"] {
            server.plan(path, Plan::new(200, &[], b"held"));
        }
        let defs = ["/pending", "/complete", "/other"]
            .map(|path| {
                let mut def = definition(&server, path);
                def.secrets = vec![if path == "/other" { "other" } else { "session" }.into()];
                def
            })
            .into();
        let set = launch(&server, defs);
        server.seen(3);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        ready(&core, 2);
        core.drop_preload_secret("session");
        {
            let state = core.shared.state.lock().unwrap();
            let cache = state.cache.as_ref().unwrap();
            assert!(matches!(cache.preloads[0].reply, ReplyState::Gone));
            assert!(matches!(cache.preloads[1].reply, ReplyState::Gone));
            assert!(cache.preloads[2].used().is_some());
        }
        release.send(()).unwrap();
        for path in ["/pending", "/complete"] {
            server.plan(path, Plan::new(200, &[], b"sent"));
            assert!(!read(&core, &woke, &server, 1, path).1);
        }
        assert!(read(&core, &woke, &server, 2, "/other").1);
    }
}

#[test]
fn complete_replies_are_reusable_until_ten_seconds_after_completion() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let release = held_plan(&server, "/held", b"held");
        let (set, clock) = fresh_clock(&server, vec![definition(&server, "/held")]);
        server.seen(1);
        *clock.lock().unwrap() += Duration::from_secs(4);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        release.send(()).unwrap();
        ready(&core, 1);
        for seconds in [0, 9] {
            *clock.lock().unwrap() += Duration::from_secs(seconds);
            assert!(read(&core, &woke, &server, 1, "/held").1);
            assert!(read(&core, &woke, &server, 2, "/held").1);
        }
        *clock.lock().unwrap() += Duration::from_secs(1);
        server.plan("/held", Plan::new(200, &[], b"sent"));
        assert!(!read(&core, &woke, &server, 3, "/held").1);
        assert_eq!(server.count(), 2);
    }
}

fn complete_set(set: &Preloads) {
    let state = set.destination.state.lock().unwrap();
    let (state, timeout) = set.destination.changed.wait_timeout_while(state, HANG, |d| {
        matches!(d, Destination::Staging(c) if c.preloads.iter().any(|p| matches!(p.reply, ReplyState::Pending { .. })))
    }).unwrap();
    drop(state);
    assert!(!timeout.timed_out(), "launch work did not finish");
}

#[test]
fn only_the_first_eight_definitions_start_and_bodies_and_replies_have_bounds() {
    let server = Server::new();
    let mut definitions = Vec::new();
    let mut releases = Vec::new();
    for i in 0..9 {
        let path = format!("/{i}");
        releases.push(held_plan(&server, &path, b"held"));
        definitions.push(definition(&server, &path));
    }
    let set = launch(&server, definitions);
    server.seen(8);
    for release in releases {
        let _ = release.send(());
    }
    complete_set(&set);
    assert_eq!(server.count(), 8);
    let journal = set.take_journal();
    assert_eq!(
        journal.iter().filter(|f| f.outcome == Event::Sent).count(),
        8
    );
    drop(set);

    for (asynchronous, body) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut def = definition(&server, "/large");
        if body {
            def.request.body = vec![0; MAX_BODY_BYTES + 1];
        } else {
            server.plan(
                "/large",
                Plan::new(200, &[], &vec![0; cache::MAX_ENTRY + 1]),
            );
        }
        let set = launch(&server, vec![def]);
        complete_set(&set);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        server.plan("/large", Plan::new(200, &[], b"sent"));
        let (reply, cached) = read(&core, &woke, &server, 1, "/large");
        assert!(!cached);
        assert_eq!(reply.body, b"sent");
    }
    assert_eq!(server.count(), 14);
}

#[test]
fn grants_refuse_the_first_hop_and_redirects_and_the_journal_contains_only_origins() {
    let first = Server::new();
    let denied = Server::new();
    let target = denied.request("/private?secret").url;
    first.plan(
        "/redirect?private",
        Plan::new(302, &[("Location", &target)], b""),
    );
    let set = launch(
        &first,
        vec![
            definition(&denied, "/private?secret"),
            definition(&first, "/redirect?private"),
        ],
    );
    complete_set(&set);
    assert_eq!(denied.count(), 0);
    assert_eq!(first.count(), 1);
    let facts = set.take_journal();
    assert_eq!(
        facts
            .iter()
            .filter(|f| f.outcome == Event::RefusedGrant)
            .count(),
        2
    );
    for fact in facts {
        assert_eq!(url::Url::parse(&fact.origin).unwrap().path(), "/");
        assert!(!fact.origin.contains('?'));
    }
    assert_eq!(update(&set.destination, |cache| cache.bytes), Some(0));
}

#[test]
fn before_first_commit_takes_only_a_complete_equal_reply_and_never_waits() {
    for asynchronous in [false, true] {
        let server = Server::new();
        server.plan("/complete", Plan::new(404, &[], b"complete"));
        let release = held_plan(&server, "/pending", b"pending");
        let set = launch(
            &server,
            vec![
                definition(&server, "/complete"),
                definition(&server, "/pending"),
            ],
        );
        server.seen(2);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        ready(&core, 1);
        assert!(core
            .settle_preload(&job(1, server.request("/pending")))
            .is_none());
        let complete = job(2, server.request("/complete"));
        let (outcome, timing) = core.settle_preload(&complete).unwrap();
        assert!(timing.cached);
        assert!(matches!(outcome, Outcome::Response(r) if r.status == 404));
        assert!(core.ordered_idle());
        assert!(core
            .settle_preload(&job(3, server.request("/different")))
            .is_none());
        core.run(job(1, server.request("/pending")), None).unwrap();
        release.send(()).unwrap();
        assert!(collect(&core, &woke).2.cached);
        assert_eq!(server.count(), 2);
    }
}

fn exited(location: &Location) {
    let threads = location.threads.lock().unwrap();
    let (threads, timeout) = location
        .exited
        .wait_timeout_while(threads, HANG, |n| *n != 0)
        .unwrap();
    assert_eq!(*threads, 0);
    assert!(!timeout.timed_out());
}
#[test]
fn dropping_a_set_or_its_adopting_executor_retires_every_launch_thread() {
    for adopted in [None, Some(false), Some(true)] {
        let server = Server::new();
        let release = held_plan(&server, "/pending", b"cancelled");
        let set = launch(&server, vec![definition(&server, "/pending")]);
        server.seen(1);
        let location = set.destination.clone();
        if let Some(asynchronous) = adopted {
            let (core, _) = server.core(asynchronous, true);
            adopt(&core, set);
            let shared = core.shared.clone();
            drop(core);
            let state = shared.state.lock().unwrap();
            assert!(state.cache.is_none());
            assert_eq!(state.bytes, [0, 0]);
        } else {
            drop(set);
        }
        // The server holds its reply throughout cancellation and retirement.
        exited(&location);
        release.send(()).unwrap();
    }
}

#[test]
fn held_replies_keep_read_order_without_starting_a_worker() {
    for asynchronous in [false, true] {
        let server = Server::new();
        server.plan("/preload", Plan::new(200, &[], b"held"));
        let set = launch(&server, vec![definition(&server, "/preload")]);
        complete_set(&set);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        let mut releases = Vec::new();
        for ticket in 1..=6 {
            let path = format!("/read/{ticket}");
            releases.push(held_plan(&server, &path, b"read"));
            core.run(job(ticket, server.request(&path)), None).unwrap();
        }
        server.seen(7);
        core.run(job(7, server.request("/preload")), None).unwrap();
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.running.len(), 6);
            assert!(state.completed[0].iter().any(|c| c.ticket == 7 && c.cached));
        }
        assert!(core.drain().is_empty());
        assert_eq!(server.count(), 7, "the held reply starts no exchange");
        for release in releases {
            release.send(()).unwrap();
        }
        for ticket in 1..=7 {
            let (got, _, timing) = collect(&core, &woke);
            assert_eq!(got, ticket);
            assert_eq!(timing.cached, ticket == 7);
        }
        assert_eq!(server.count(), 7);
    }
}

#[test]
fn held_replies_share_the_cache_budget_and_its_eviction_and_store_http_fresh_replies() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let mut definitions = Vec::new();
        for i in 0..8 {
            let path = format!("/{i}");
            server.plan(&path, Plan::new(200, &[], &vec![0; 3 << 20]));
            definitions.push(definition(&server, &path));
        }
        let set = launch(&server, definitions);
        complete_set(&set);
        let (bytes, held) = update(&set.destination, |cache| {
            (
                cache.bytes,
                cache.preloads.iter().filter(|p| p.used().is_some()).count(),
            )
        })
        .unwrap();
        assert!(bytes <= cache::CAPACITY);
        assert!((1..8).contains(&held));
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        server.plan(
            "/http-cache",
            Plan::new(200, &[("Cache-Control", "max-age=60")], &vec![0; 3 << 20]),
        );
        assert!(!read(&core, &woke, &server, 1, "/http-cache").1);
        let (bytes, remaining) = {
            let state = core.shared.state.lock().unwrap();
            let cache = state.cache.as_ref().unwrap();
            (
                cache.bytes,
                cache.preloads.iter().filter(|p| p.used().is_some()).count(),
            )
        };
        assert!(bytes <= cache::CAPACITY);
        assert!(
            remaining < held,
            "an HTTP cache entry can evict a held reply"
        );
        drop(core);

        server.plan(
            "/fresh",
            Plan::new(200, &[("Cache-Control", "max-age=60")], b"fresh"),
        );
        let (set, clock) = fresh_clock(&server, vec![definition(&server, "/fresh")]);
        complete_set(&set);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        *clock.lock().unwrap() += HOLD;
        assert!(
            read(&core, &woke, &server, 1, "/fresh").1,
            "HTTP freshness survives the launch hold"
        );
        assert_eq!(server.count(), 10);
    }
}

#[test]
fn a_taking_source_checks_its_own_grants_and_adoption_is_before_dispatch() {
    for asynchronous in [false, true] {
        let server = Server::new();
        server.plan("/held", Plan::new(200, &[], b"held"));
        let set = launch(&server, vec![definition(&server, "/held")]);
        complete_set(&set);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        let mut scoped = server.request("/held");
        scoped.grants = Some("".into());
        core.run(job(1, scoped), None).unwrap();
        assert!(matches!(
            collect(&core, &woke).1,
            Outcome::Failed {
                kind: FailureKind::Refused,
                ..
            }
        ));
        assert_eq!(server.count(), 1);
        let set = launch(&server, Vec::new());
        assert!(core.adopt_preloads(set).is_err());
    }
}

#[test]
fn forgetting_a_post_waiter_releases_its_ticket_and_keeps_the_launch_exchange() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let release = held_plan(&server, "/post", b"held");
        let mut def = definition(&server, "/post");
        def.request.method = "POST".into();
        def.request.body = b"query".to_vec();
        let request = def.request.clone();
        let set = launch(&server, vec![def]);
        server.seen(1);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        core.run(job(1, request.clone()), None).unwrap();
        core.forget(|_| false);
        let (counts, bytes, jobs) = {
            let state = core.shared.state.lock().unwrap();
            (state.counts, state.bytes, state.jobs[0].len())
        };
        assert_eq!(counts, [0, 0]);
        assert_eq!(bytes, [0, 0]);
        assert_eq!(jobs, 0);
        release.send(()).unwrap();
        ready(&core, 1);
        assert!(ask(&core, &woke, job(2, request)).1);
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn a_waiting_preload_read_keeps_room_for_its_reply_or_its_network_fallback() {
    for asynchronous in [false, true] {
        for failure in [false, true] {
            let server = Server::new();
            let (release, held) = channel();
            let mut plan = Plan::new(200, &[], &vec![1; 64 << 10]);
            plan.held = Some(held);
            plan.disconnect = failure;
            server.plan("/room", plan);
            let mut def = definition(&server, "/room");
            def.request.method = "POST".into();
            let request = def.request.clone();
            let set = launch(&server, vec![def]);
            server.seen(1);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            for ticket in 1..=2 {
                core.run(job(ticket, request.clone()), None).unwrap();
            }
            let later_bytes = {
                let mut state = core.shared.state.lock().unwrap();
                let bytes = BYTES[0] - state.bytes[0];
                state.bytes[0] += bytes;
                bytes
            };
            if failure {
                for _ in 0..2 {
                    server.plan("/room", Plan::new(200, &[], &vec![2; 64 << 10]));
                }
            }
            release.send(()).unwrap();
            for expected in 1..=2 {
                let (ticket, outcome, timing) = collect(&core, &woke);
                assert_eq!(ticket, expected);
                assert_eq!(timing.cached, !failure);
                assert!(matches!(outcome, Outcome::Response(r) if r.body.len() == 64 << 10));
            }
            let bytes = {
                let mut state = core.shared.state.lock().unwrap();
                state.bytes[0] -= later_bytes;
                state.bytes
            };
            assert_eq!(bytes, [0, 0]);
            assert_eq!(server.count(), if failure { 3 } else { 1 });
        }
    }
}

#[test]
fn a_redirected_reply_checks_every_hops_scope_before_boot_and_queued_adoption() {
    for asynchronous in [false, true] {
        for pending in [false, true] {
            let first = Server::new();
            let middle = Server::new();
            let grants = format!(
                "net.fetch http://{}\nnet.fetch http://{}",
                first.address, middle.address
            );
            let target = middle.request("/middle").url;
            for _ in 0..2 {
                first.plan("/start", Plan::new(302, &[("Location", &target)], b""));
            }
            let target = first.request("/final").url;
            middle.plan("/middle", Plan::new(302, &[("Location", &target)], b""));
            let release = held_plan(&first, "/final", b"visited middle");
            let set = Preloads::start(
                vec![definition(&first, "/start")],
                &grants,
                ibex2::host::Host::new,
            );
            first.seen(2);
            let mut scoped = first.request("/start");
            scoped.grants = Some(format!("net.fetch http://{}", first.address));
            let restricted = job(1, scoped);
            if !pending {
                release.send(()).unwrap();
                complete_set(&set);
                assert!(set.control().settle(&restricted).is_none());
            }
            let (core, woke) = first.core_grants(asynchronous, true, &grants);
            adopt(&core, set);
            if !pending {
                assert!(core.settle_preload(&restricted).is_none());
            }
            core.run(restricted, None).unwrap();
            if pending {
                release.send(()).unwrap();
            }
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
            let (reply, cached) = read(&core, &woke, &first, 2, "/start");
            assert!(cached);
            assert_eq!(reply.body, b"visited middle");
            assert_eq!(first.count(), 3);
        }
    }
}

#[test]
fn refresh_admission_drops_equal_ready_and_pending_preloads_and_releases_waiters() {
    for asynchronous in [false, true] {
        for pending in [false, true] {
            for mode in 0..3 {
                for method in ["GET", "POST"] {
                    let server = Server::new();
                    let release = held_plan(&server, "/refresh", b"old");
                    let mut def = definition(&server, "/refresh");
                    def.request.method = method.into();
                    if method == "POST" {
                        def.request.body = b"query".to_vec();
                    }
                    let request = def.request.clone();
                    let set = launch(&server, vec![def]);
                    server.seen(1);
                    let (core, woke) = server.core(asynchronous, true);
                    adopt(&core, set);
                    if pending {
                        core.run(job(1, request.clone()), None).unwrap();
                        server.plan("/refresh", Plan::new(200, &[], b"new"));
                    } else {
                        release.send(()).unwrap();
                        ready(&core, 1);
                    }
                    server.plan(
                        "/refresh",
                        Plan::new(200, &[("Cache-Control", "max-age=60")], b"new"),
                    );
                    let mut refresh = job(2, request.clone());
                    match mode {
                        0 => refresh.forced = true,
                        1 => refresh.request.cache = exact_runner::CacheMode::Reload,
                        _ => refresh.request.cache = exact_runner::CacheMode::NoCache,
                    }
                    core.run(refresh, None).unwrap();
                    let dropped = {
                        let state = core.shared.state.lock().unwrap();
                        matches!(
                            state.cache.as_ref().unwrap().preloads[0].reply,
                            ReplyState::Gone
                        )
                    };
                    assert!(dropped);
                    if pending {
                        release.send(()).unwrap();
                        let (ticket, outcome, timing) = collect(&core, &woke);
                        assert_eq!(ticket, 1);
                        assert!(!timing.cached);
                        assert!(matches!(outcome, Outcome::Response(r) if r.body == b"new"));
                    }
                    let (ticket, outcome, timing) = collect(&core, &woke);
                    assert_eq!(ticket, 2);
                    assert!(!timing.cached);
                    assert!(matches!(outcome, Outcome::Response(r) if r.body == b"new"));
                    server.plan("/refresh", Plan::new(200, &[], b"new"));
                    assert_eq!(ask(&core, &woke, job(3, request)).0.body, b"new");
                    let journal = core.take_preload_journal();
                    assert_eq!(
                        journal
                            .iter()
                            .filter(|f| format!("{:?}", f.outcome) == "DroppedByRefresh")
                            .count(),
                        1
                    );
                }
            }
        }
    }
}

#[test]
fn a_post_preload_waiter_counts_as_an_effect_from_admission_and_a_hit_frees_it() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let release = held_plan(&server, "/preload", b"held");
        let mut def = definition(&server, "/preload");
        def.request.method = "POST".into();
        let request = def.request.clone();
        let set = launch(&server, vec![def]);
        server.seen(1);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        core.run(job(1, request), None).unwrap();
        let effects = effect_count(&core.shared.state.lock().unwrap());
        assert_eq!(effects, 1);
        let mut releases = Vec::new();
        for ticket in 2..=17 {
            let path = format!("/post/{ticket}");
            releases.push(held_plan(&server, &path, b"post"));
            let mut request = server.request(&path);
            request.method = "POST".into();
            core.run(job(ticket, request), None).unwrap();
        }
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.counts[0], 16);
            assert_eq!(state.fenced.front().unwrap().ticket(), 17);
        }
        release.send(()).unwrap();
        ready(&core, 1);
        core.resume_waiting();
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(effect_count(&state), 16);
            assert!(state.fenced.is_empty());
            assert!(state.completed[0].iter().any(|r| r.ticket == 1 && r.cached));
        }
        for release in releases {
            release.send(()).unwrap();
        }
        for ticket in 1..=17 {
            assert_eq!(collect(&core, &woke).0, ticket);
        }
    }
}

#[test]
fn a_seventeenth_post_waits_for_an_effect_place_on_either_preload_outcome_or_no_definition() {
    for asynchronous in [false, true] {
        for outcome in 0..3 {
            let server = Server::new();
            let mut def = definition(&server, "/preload");
            def.request.method = "POST".into();
            let request = def.request.clone();
            let (release, held) = channel();
            let mut plan = Plan::new(200, &[], b"held");
            plan.held = Some(held);
            plan.disconnect = outcome == 1;
            if outcome != 2 {
                server.plan("/preload", plan);
            }
            let set = launch(&server, if outcome == 2 { Vec::new() } else { vec![def] });
            let control = set.control();
            if outcome == 2 {
                control.definitions_pending(true);
            } else {
                server.seen(1);
            }
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            let mut releases = Vec::new();
            for ticket in 1..=16 {
                let path = format!("/post/{ticket}");
                releases.push(held_plan(&server, &path, b"post"));
                let mut request = server.request(&path);
                request.method = "POST".into();
                core.run(job(ticket, request), None).unwrap();
            }
            core.run(job(17, request), None).unwrap();
            let (count, fenced) = {
                let state = core.shared.state.lock().unwrap();
                (state.counts[0], state.fenced.front().map(Fenced::ticket))
            };
            assert_eq!(count, 16);
            assert_eq!(fenced, Some(17));
            if outcome == 2 {
                control.definitions_pending(false);
            } else {
                release.send(()).unwrap();
                let state = core.shared.state.lock().unwrap();
                let (state, timeout) = core
                    .shared
                    .ready
                    .wait_timeout_while(state, HANG, |s| {
                        matches!(
                            s.cache.as_ref().unwrap().preloads[0].reply,
                            ReplyState::Pending { .. }
                        )
                    })
                    .unwrap();
                drop(state);
                assert!(!timeout.timed_out());
            }
            if outcome != 0 {
                server.plan("/preload", Plan::new(200, &[], b"sent"));
            }
            core.resume_waiting();
            server.seen(if outcome == 2 { 6 } else { 7 });
            {
                let state = core.shared.state.lock().unwrap();
                assert_eq!(state.counts[0], 16);
                assert_eq!(state.fenced.front().unwrap().ticket(), 17);
                assert!(!state.jobs[0].iter().any(|j| j.ticket == 17));
                assert!(!state.running.iter().any(|r| r.ticket == 17));
            }
            assert_eq!(server.count(), if outcome == 2 { 6 } else { 7 });
            releases.remove(0).send(()).unwrap();
            assert_eq!(collect(&core, &woke).0, 1);
            {
                let state = core.shared.state.lock().unwrap();
                assert!(state.fenced.is_empty());
                assert!(state.ordered.contains(&17));
            }
            for release in releases {
                release.send(()).unwrap();
            }
            for ticket in 2..=17 {
                let (got, _, timing) = collect(&core, &woke);
                assert_eq!(got, ticket);
                if got == 17 {
                    assert_eq!(timing.cached, outcome == 0);
                }
            }
            assert_eq!(
                server.count(),
                if outcome == 0 || outcome == 2 { 17 } else { 18 }
            );
        }
    }
}

#[test]
fn refresh_admission_leaves_preloads_with_a_different_method_url_header_or_body() {
    for asynchronous in [false, true] {
        for difference in 0..4 {
            let server = Server::new();
            server.plan("/equal", Plan::new(200, &[], b"held"));
            let mut def = definition(&server, "/equal");
            def.request.method = "POST".into();
            def.request.headers = vec![("X-Query".into(), "one".into())];
            def.request.body = b"one".to_vec();
            let mut refresh = job(1, def.request.clone());
            refresh.forced = true;
            match difference {
                0 => refresh.request.method = "PUT".into(),
                1 => refresh.request.url.push_str("?other"),
                2 => refresh.request.headers[0].1 = "two".into(),
                _ => refresh.request.body.push(0),
            }
            let set = launch(&server, vec![def]);
            complete_set(&set);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            let release = held_plan(
                &server,
                if difference == 1 {
                    "/equal?other"
                } else {
                    "/equal"
                },
                b"new",
            );
            core.run(refresh, None).unwrap();
            server.seen(2);
            let held = {
                let state = core.shared.state.lock().unwrap();
                state.cache.as_ref().unwrap().preloads[0].used().is_some()
            };
            assert!(held);
            release.send(()).unwrap();
            assert!(!collect(&core, &woke).2.cached);
        }
    }
}

#[test]
fn an_empty_set_starts_no_preload_thread_or_workers_and_does_not_hold_reads() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let set = launch(&server, Vec::new());
        let location = set.destination.clone();
        assert_eq!(*location.threads.lock().unwrap(), 0);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        server.plan("/ordinary", Plan::new(200, &[], b"ordinary"));
        assert!(!read(&core, &woke, &server, 1, "/ordinary").1);
        assert_eq!(*location.threads.lock().unwrap(), 0);
    }
}

#[test]
fn definitions_arriving_after_the_wait_bound_still_hold_their_replies() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (set, clock) = fresh_clock(&server, Vec::new());
        let control = set.control();
        control.definitions_pending(true);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        server.plan("/late", Plan::new(200, &[], b"network"));
        core.run(job(1, server.request("/late")), None).unwrap();
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.jobs[0].len(), 1);
            assert!(state.running.is_empty());
        }
        *clock.lock().unwrap() += DEFINITIONS_WAIT;
        if let Some(machine) = &core.machine {
            machine.timer.expire((0, 0));
        } else {
            core.shared.ready.notify_all();
        }
        assert!(!collect(&core, &woke).2.cached);
        server.plan("/late", Plan::new(200, &[], b"preload"));
        control.extend(
            vec![definition(&server, "/late")],
            &format!("net.fetch http://{}", server.address),
            Arc::new(|_, _| {}),
        );
        control.definitions_pending(false);
        ready(&core, 1);
        let (reply, cached) = read(&core, &woke, &server, 2, "/late");
        assert!(cached);
        assert_eq!(reply.body, b"preload");
        assert_eq!(server.count(), 2);
    }
}

#[test]
fn held_request_allocations_are_charged_and_released_when_the_hold_ends() {
    for asynchronous in [false, true] {
        for expired in [false, true] {
            let server = Server::new();
            server.plan(
                "/retained",
                Plan::new(200, &[("Cache-Control", "max-age=60")], b"held"),
            );
            let mut def = definition(&server, "/retained");
            let mut value = String::with_capacity(3 << 20);
            value.push('x');
            def.request.headers.push(("X-Note".into(), value));
            def.secrets.push("session".into());
            let request = def.request.clone();
            let (set, clock) = fresh_clock(&server, vec![def]);
            complete_set(&set);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            let (bytes, ready) = {
                let state = core.shared.state.lock().unwrap();
                let cache = state.cache.as_ref().unwrap();
                (cache.bytes, cache.preloads[0].used().is_some())
            };
            assert!(ready);
            assert!(
                bytes >= 3 << 20,
                "request header capacity was not charged: {bytes}"
            );
            if expired {
                *clock.lock().unwrap() += HOLD;
                let mut state = core.shared.state.lock().unwrap();
                expire(state.cache.as_mut().unwrap());
            } else {
                core.drop_preload_secret("session");
            }
            let (gone, request_dropped, answered, headers, url, bytes) = {
                let state = core.shared.state.lock().unwrap();
                let cache = state.cache.as_ref().unwrap();
                let held = &cache.preloads[0];
                (
                    matches!(held.reply, ReplyState::Gone),
                    held.request.is_none(),
                    held.answered_at.is_none(),
                    held.key.headers.capacity(),
                    held.key.url.capacity(),
                    cache.bytes,
                )
            };
            assert!(gone && request_dropped && answered);
            assert_eq!((headers, url), (0, 0));
            assert!(bytes < 4096, "ended hold retained its charge");
            if expired {
                assert!(
                    ask(&core, &woke, job(1, request.clone())).1,
                    "HTTP freshness survives the hold"
                );
                core.drop_preload_secret("session");
            }
            server.plan("/retained", Plan::new(200, &[], b"sent"));
            assert!(
                !ask(&core, &woke, job(2, request)).1,
                "secret writes still invalidate the HTTP copy"
            );
            let facts = core.take_preload_journal();
            assert!(facts
                .iter()
                .all(|f| f.name == "retained" && f.origin == format!("http://{}", server.address)));
        }
    }
}

#[test]
fn equal_preloads_keep_every_secrets_invalidation_after_their_holds_expire() {
    for asynchronous in [false, true] {
        for secret in ["first", "second"] {
            let server = Server::new();
            let definitions = ["first", "second"].map(|name| {
                server.plan(
                    "/equal",
                    Plan::new(200, &[("Cache-Control", "max-age=60")], b"held"),
                );
                let mut def = definition(&server, "/equal");
                def.name = name.into();
                def.secrets.push(name.into());
                def
            });
            let (set, clock) = fresh_clock(&server, definitions.into());
            complete_set(&set);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            *clock.lock().unwrap() += HOLD;
            assert!(read(&core, &woke, &server, 1, "/equal").1);
            core.drop_preload_secret(secret);
            server.plan("/equal", Plan::new(200, &[], b"sent"));
            assert!(!read(&core, &woke, &server, 2, "/equal").1);
        }
    }
}
