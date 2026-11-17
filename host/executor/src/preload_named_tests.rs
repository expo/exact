use super::*;

#[test]
fn boot_cannot_settle_an_equal_hold_before_the_launch_can_define_its_name() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (set, _clock) = fresh_clock(&server, Vec::new());
        let control = set.control();
        control.definitions_pending(true);
        server.plan("/feed", Plan::new(200, &[], b"base"));
        let mut base = definition(&server, "/feed");
        base.name = "base".into();
        control.extend(
            vec![base],
            &format!("net.fetch http://{}", server.address),
            Arc::new(|_, _| {}),
        );
        complete_set(&set);
        let request = job(1, server.request("/feed").preload("feed").unwrap());
        assert!(control.settle(&request).is_none());
        let (core, _) = server.core(asynchronous, true);
        adopt(&core, set);
        assert!(core.settle_preload(&request).is_none());
        control.launch_pending(Some(Duration::from_secs(5)));
        assert!(core.settle_preload(&request).is_none());
        control.launch_pending(None);
        let (outcome, timing) = core.settle_preload(&request).unwrap();
        assert!(timing.cached);
        assert!(matches!(outcome, Outcome::Response(r) if r.body == b"base"));
    }
}

#[test]
fn named_reads_wait_across_manifest_handoff_and_are_released_together() {
    for asynchronous in [false, true] {
        for ending in 0..4 {
            let server = Server::new();
            let (set, clock) = fresh_clock(&server, Vec::new());
            let control = set.control();
            control.definitions_pending(true);
            server.plan("/feed", Plan::new(200, &[], b"base"));
            let mut base = definition(&server, "/feed");
            base.name = "base".into();
            control.extend(
                vec![base],
                &format!("net.fetch http://{}", server.address),
                Arc::new(|_, _| {}),
            );
            complete_set(&set);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            for ticket in 1..=2 {
                let mut request = server.request("/feed").preload("feed").unwrap();
                if ending == 3 {
                    request = request.header("X-Trace", "caller");
                }
                if ticket == 1 {
                    request = request.prefetch();
                }
                core.run(job(ticket, request), None).unwrap();
            }
            let snapshot = {
                let state = core.shared.state.lock().unwrap();
                (
                    state.completed[0].len(),
                    state.running.len(),
                    state.jobs[0].len(),
                    state.jobs[0].iter().all(|j| waiting(&state, j)),
                    state.cache.as_ref().unwrap().preloads.len(),
                )
            };
            assert_eq!(snapshot, (0, 0, 2, true, 1));
            *clock.lock().unwrap() += DEFINITIONS_WAIT;
            control.test_tick();
            let completed = core.shared.state.lock().unwrap().completed[0].len();
            assert_eq!(completed, 0);
            if ending != 2 {
                control.launch_pending(Some(Duration::from_secs(5)));
            }
            if ending == 0 {
                server.plan("/feed", Plan::new(200, &[], b"named"));
                let mut feed = definition(&server, "/feed");
                feed.request = feed.request.header("X-Trace", "launch");
                control.extend(vec![feed], &core.grants, Arc::new(|_, _| {}));
            } else {
                if ending == 3 {
                    for _ in 0..2 {
                        server.plan("/feed", Plan::new(200, &[], b"network"));
                    }
                }
                if ending == 2 {
                    control.definitions_pending(false);
                } else {
                    control.launch_pending(None);
                }
            }
            for ticket in 1..=2 {
                let (got, outcome, timing) = collect(&core, &woke);
                assert_eq!(got, ticket);
                assert_eq!(timing.cached, ending != 3);
                let expected = match ending {
                    0 => b"named".as_slice(),
                    3 => b"network",
                    _ => b"base",
                };
                assert!(matches!(outcome, Outcome::Response(r) if r.body == expected));
            }
            assert_eq!(
                server.count(),
                match ending {
                    0 => 2,
                    3 => 3,
                    _ => 1,
                }
            );
        }
    }
}

#[test]
fn a_named_read_waits_past_an_equal_hold_until_its_name_or_the_launch_ends() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            for end in 0..5 {
                let server = Server::new();
                let (set, clock) = fresh_clock(&server, Vec::new());
                let control = set.control();
                if end != 4 {
                    control.launch_pending(Some(Duration::from_secs(5)));
                }
                let (core, woke) = server.core(asynchronous, true);
                adopt(&core, set);
                server.plan(
                    "/feed",
                    Plan::new(200, &[("Cache-Control", "max-age=60")], b"equal"),
                );
                if prefetch {
                    core.run(job(1, server.request("/feed").prefetch()), None)
                        .unwrap();
                } else {
                    let mut def = definition(&server, "/feed");
                    def.name.clear();
                    control.extend(vec![def], &core.grants, Arc::new(|_, _| {}));
                }
                ready(&core, 1);
                if prefetch {
                    assert!(collect(&core, &woke).2.cached);
                }
                let request = job(2, server.request("/feed").preload("feed").unwrap());
                core.run(request, None).unwrap();
                if end != 4 {
                    let (completed, running, queued, waits) = {
                        let state = core.shared.state.lock().unwrap();
                        (
                            state.completed[0].len(),
                            state.running.len(),
                            state.jobs[0].len(),
                            state.jobs[0].front().is_some_and(|j| waiting(&state, j)),
                        )
                    };
                    assert_eq!((completed, running, queued, waits), (0, 0, 1, true));
                    assert_eq!(server.count(), 1);
                    match end {
                        0 => {
                            let release = held_plan(&server, "/feed", b"named");
                            let mut def = definition(&server, "/feed");
                            def.request = def.request.header("X-Trace", "launch");
                            control.extend(vec![def], &core.grants, Arc::new(|_, _| {}));
                            server.seen(2);
                            assert!(core.shared.state.lock().unwrap().completed[0].is_empty());
                            release.send(()).unwrap();
                        }
                        1 => control.launch_pending(None),
                        2 => control.defined("feed"),
                        _ => {
                            *clock.lock().unwrap() += Duration::from_secs(5);
                            control.test_tick();
                        }
                    }
                }
                let (ticket, outcome, timing) = collect(&core, &woke);
                assert_eq!(ticket, 2);
                assert!(timing.cached);
                let expected = if end == 0 {
                    b"named".as_slice()
                } else {
                    b"equal"
                };
                assert!(matches!(outcome, Outcome::Response(r) if r.body == expected));
                assert_eq!(server.count(), if end == 0 { 2 } else { 1 });
            }
        }
    }
}

#[test]
fn the_launch_deadline_sends_a_named_read_without_the_function_reporting_its_end() {
    for asynchronous in [false, true] {
        for after_adoption in [false, true] {
            let server = Server::new();
            let (set, clock) = fresh_clock(&server, Vec::new());
            let control = set.control();
            let deadline = Duration::from_secs(5);
            if !after_adoption {
                control.launch_pending(Some(deadline));
            }
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            assert_eq!(control.thread_count(), 0);
            if after_adoption {
                control.launch_pending(Some(deadline));
            }
            server.plan("/feed", Plan::new(200, &[], b"sent"));
            core.run(
                job(1, server.request("/feed").preload("feed").unwrap()),
                None,
            )
            .unwrap();
            {
                let state = core.shared.state.lock().unwrap();
                assert_eq!(state.jobs[0].len(), 1);
                assert!(waiting(&state, &state.jobs[0][0]));
                assert!(state.running.is_empty());
                assert_eq!(server.count(), 0);
            }
            *clock.lock().unwrap() += deadline;
            if let Some(machine) = &core.machine {
                machine.timer.expire((0, 0));
            } else {
                core.shared.ready.notify_all();
            }
            // The deadline must start the read before a host pumps again.
            server.seen(1);
            let (ticket, outcome, timing) = collect(&core, &woke);
            assert_eq!(ticket, 1);
            assert!(!timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"sent"));
            assert_eq!(server.count(), 1);
        }
    }
}

#[test]
fn a_named_read_takes_different_headers_and_body_ready_or_inflight() {
    for asynchronous in [false, true] {
        for pending in [false, true] {
            let server = Server::new();
            let release = held_plan(&server, "/feed", b"preloaded");
            let mut def = definition(&server, "/feed");
            def.request = Request::post_json(&def.request.url, "old").header("X-Trace", "old");
            let request = Request::post_json(&def.request.url, "new")
                .header("X-Trace", "new")
                .preload("feed")
                .unwrap();
            let set = launch(&server, vec![def]);
            server.seen(1);
            if !pending {
                release.send(()).unwrap();
                complete_set(&set);
            }
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            core.run(job(1, request), None).unwrap();
            if pending {
                let state = core.shared.state.lock().unwrap();
                assert!(state.running.is_empty());
                assert!(state.completed[0].is_empty());
                drop(state);
                release.send(()).unwrap();
            }
            let (_, outcome, timing) = collect(&core, &woke);
            assert!(timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"preloaded"));
            assert_eq!(server.count(), 1);
        }
    }
}

#[test]
fn a_name_does_not_override_method_or_url_and_a_missing_name_uses_equality() {
    for asynchronous in [false, true] {
        for difference in 0..3 {
            let server = Server::new();
            server.plan("/feed", Plan::new(200, &[], b"preloaded"));
            let mut def = definition(&server, "/feed");
            def.request.method = "POST".into();
            let mut request = def.request.clone().preload("feed").unwrap();
            match difference {
                0 => request.url.push_str("?other"),
                1 => request.method = "PUT".into(),
                _ => request.preload = Some("missing".into()),
            }
            let set = launch(&server, vec![def]);
            complete_set(&set);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            if difference < 2 {
                server.plan(
                    if difference == 0 {
                        "/feed?other"
                    } else {
                        "/feed"
                    },
                    Plan::new(200, &[], b"sent"),
                );
            }
            let (reply, cached) = ask(&core, &woke, job(1, request));
            assert_eq!(cached, difference == 2);
            assert_eq!(
                reply.body,
                if cached {
                    b"preloaded".as_slice()
                } else {
                    b"sent".as_slice()
                }
            );
            assert_eq!(server.count(), if cached { 1 } else { 2 });
        }
    }
}

#[test]
fn a_selected_update_disables_names_before_boot_and_after_adoption_but_keeps_equality() {
    for asynchronous in [false, true] {
        for pending in [false, true] {
            let server = Server::new();
            let release = held_plan(&server, "/feed", b"preloaded");
            let def = definition(&server, "/feed");
            let equal = def.request.clone().preload("feed").unwrap();
            let different = equal.clone().header("X-Trace", "new");
            let set = launch(&server, vec![def]);
            let control = set.control();
            control.select_update();
            server.seen(1);
            if !pending {
                release.send(()).unwrap();
                complete_set(&set);
                assert!(control.settle(&job(1, different.clone())).is_none());
                assert!(control.settle(&job(1, equal.clone())).unwrap().1.cached);
            }
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            assert!(core.settle_preload(&job(1, different.clone())).is_none());
            server.plan("/feed", Plan::new(200, &[], b"sent"));
            let (reply, cached) = ask(&core, &woke, job(1, different));
            assert!(!cached);
            assert_eq!(reply.body, b"sent");
            core.run(job(2, equal), None).unwrap();
            if pending {
                release.send(()).unwrap();
            }
            let (_, outcome, timing) = collect(&core, &woke);
            assert!(timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"preloaded"));
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn an_invalid_rust_preload_name_is_refused_before_contact() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, _) = server.core(asynchronous, true);
        let mut request = server.request("/never");
        request.preload = Some("Bad".into());
        assert_eq!(
            core.run(job(1, request), None),
            Err("exactPreload must match ^[a-z][a-z0-9-]*$")
        );
        assert_eq!(server.count(), 0);
    }
}

#[test]
fn a_named_read_cannot_take_an_ended_hold() {
    for asynchronous in [false, true] {
        for expired in [false, true] {
            let server = Server::new();
            server.plan("/feed", Plan::new(200, &[], b"held"));
            let mut def = definition(&server, "/feed");
            def.secrets.push("session".into());
            let request = def
                .request
                .clone()
                .header("X-Trace", "new")
                .preload("feed")
                .unwrap();
            let (set, clock) = fresh_clock(&server, vec![def]);
            complete_set(&set);
            let control = set.control();
            assert!(control.settle(&job(1, request.clone())).unwrap().1.cached);
            let (core, woke) = server.core(asynchronous, true);
            adopt(&core, set);
            assert!(
                core.settle_preload(&job(1, request.clone()))
                    .unwrap()
                    .1
                    .cached
            );
            if expired {
                *clock.lock().unwrap() += HOLD;
                control.test_tick();
            } else {
                core.drop_preload_secret("session");
            }
            assert!(core.settle_preload(&job(1, request.clone())).is_none());
            server.plan("/feed", Plan::new(200, &[], b"sent"));
            let (reply, cached) = ask(&core, &woke, job(1, request));
            assert!(!cached);
            assert_eq!(reply.body, b"sent");
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn held_names_and_request_names_are_each_charged_once() {
    let server = Server::new();
    let mut charges = Vec::new();
    for (capacity, request_name) in [(16, 0), (1 << 20, 0), (16, 1 << 20)] {
        server.plan("/feed", Plan::new(200, &[], b"held"));
        let mut def = definition(&server, "/feed");
        def.name = String::with_capacity(capacity);
        def.name.push_str("feed");
        if request_name > 0 {
            def.request = def.request.preload(&"a".repeat(request_name)).unwrap();
        }
        let set = launch(&server, vec![def]);
        complete_set(&set);
        charges.push(update(&set.destination, |cache| cache.bytes).unwrap());
    }
    assert_eq!(charges[1] - charges[0], (1 << 20) - 16);
    assert_eq!(charges[2] - charges[0], 1 << 20);
}

#[test]
fn a_named_refresh_drops_the_hold_it_would_take_unless_an_update_disables_names() {
    for asynchronous in [false, true] {
        for pending in [false, true] {
            for selected in [false, true] {
                for mode in 0..3 {
                    let server = Server::new();
                    let (release, held) = channel();
                    let mut plan = Plan::new(200, &[("Cache-Control", "max-age=3600")], b"old");
                    plan.held = Some(held);
                    server.plan("/feed", plan);
                    let def = definition(&server, "/feed");
                    let request = def
                        .request
                        .clone()
                        .header("X-Trace", "new")
                        .preload("feed")
                        .unwrap();
                    let set = launch(&server, vec![def]);
                    if selected {
                        set.control().select_update();
                    }
                    server.seen(1);
                    if !pending {
                        release.send(()).unwrap();
                        complete_set(&set);
                    }
                    let (core, woke) = server.core(asynchronous, true);
                    adopt(&core, set);
                    server.plan("/feed", Plan::new(200, &[], b"new"));
                    let mut refresh = job(1, request.clone());
                    match mode {
                        0 => refresh.forced = true,
                        1 => refresh.request.cache = exact_runner::CacheMode::Reload,
                        _ => refresh.request.cache = exact_runner::CacheMode::NoCache,
                    }
                    let (reply, cached) = ask(&core, &woke, refresh);
                    assert!(!cached);
                    assert_eq!(reply.body, b"new");
                    if pending {
                        release.send(()).unwrap();
                    }
                    server.plan("/feed", Plan::new(200, &[], b"new"));
                    let (reply, cached) = ask(&core, &woke, job(2, request));
                    assert!(!cached);
                    assert_eq!(reply.body, b"new");
                    if selected {
                        ready(&core, 1);
                        assert!(read(&core, &woke, &server, 3, "/feed").1);
                    } else {
                        server.plan("/feed", Plan::new(200, &[], b"new"));
                        let (reply, cached) = ask(
                            &core,
                            &woke,
                            job(3, server.request("/feed").preload("feed").unwrap()),
                        );
                        assert!(!cached);
                        assert_eq!(reply.body, b"new");
                        assert!(core
                            .take_preload_journal()
                            .iter()
                            .any(|f| f.outcome == Event::DroppedByRefresh));
                    }
                }
            }
        }
    }
}

#[test]
fn a_running_function_holds_only_names_without_definitions() {
    for asynchronous in [false, true] {
        for selected in [false, true] {
            for end in 0..4 {
                let server = Server::new();
                let (set, clock) = fresh_clock(&server, Vec::new());
                let control = set.control();
                control.launch_pending(Some(Duration::from_secs(5)));
                if selected {
                    control.select_update();
                }
                let (core, woke) = server.core(asynchronous, true);
                adopt(&core, set);
                server.plan("/other", Plan::new(200, &[], b"other"));
                server.plan("/send", Plan::new(200, &[], b"send"));
                let feed = held_plan(&server, "/feed", b"feed");
                core.run(job(1, server.request("/other")), None).unwrap();
                let mut send = job(2, server.request("/send"));
                send.class = RequestClass::Ordered;
                core.run(send, None).unwrap();
                for expected in 1..=2 {
                    assert_eq!(collect(&core, &woke).0, expected);
                }
                let named = server.request("/feed").preload("feed").unwrap();
                core.run(job(3, named), None).unwrap();
                if !selected {
                    let state = core.shared.state.lock().unwrap();
                    assert_eq!(state.jobs[0].len(), 1);
                    assert!(waiting(&state, &state.jobs[0][0]));
                    assert_eq!(server.count(), 2);
                }
                if selected {
                    server.seen(3);
                } else {
                    match end {
                        0 => control.extend(
                            vec![definition(&server, "/feed")],
                            &format!("net.fetch http://{}", server.address),
                            Arc::new(|_, _| {}),
                        ),
                        1 => control.launch_pending(None),
                        2 => control.defined("feed"),
                        _ => {
                            *clock.lock().unwrap() += Duration::from_secs(5);
                            control.test_tick();
                        }
                    }
                    server.seen(3);
                }
                feed.send(()).unwrap();
                let (ticket, outcome, timing) = collect(&core, &woke);
                assert_eq!(ticket, 3);
                assert_eq!(timing.cached, !selected && end == 0);
                assert!(matches!(outcome, Outcome::Response(r) if r.body == b"feed"));
                assert_eq!(server.count(), 3);
            }
        }
    }
}

#[test]
fn an_existing_name_waits_only_for_a_matching_hold_during_the_function() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let feed = held_plan(&server, "/feed", b"held");
        let (set, _) = fresh_clock(&server, vec![definition(&server, "/feed")]);
        let control = set.control();
        control.launch_pending(Some(Duration::from_secs(5)));
        server.seen(1);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        server.plan("/other", Plan::new(200, &[], b"sent"));
        let (reply, cached) = ask(
            &core,
            &woke,
            job(1, server.request("/other").preload("feed").unwrap()),
        );
        assert!(!cached);
        assert_eq!(reply.body, b"sent");
        core.run(
            job(2, server.request("/feed").preload("feed").unwrap()),
            None,
        )
        .unwrap();
        feed.send(()).unwrap();
        assert!(collect(&core, &woke).2.cached);
        assert_eq!(server.count(), 2);
    }
}
