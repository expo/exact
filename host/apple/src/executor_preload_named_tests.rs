use super::*;

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
