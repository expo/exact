use super::*;

#[test]
fn no_store_neither_reads_nor_replaces_an_entry_or_stores_a_miss() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan(
            "/a",
            Plan::new(
                200,
                &[("Cache-Control", "max-age=60"), ("ETag", "kept")],
                b"kept",
            ),
        );
        server.plan("/a", Plan::new(200, FRESH, b"uncached"));
        read(&core, &woke, &server, 1, "/a");
        let (reply, cached) = ask(
            &core,
            &woke,
            job(2, server.request("/a").cache(CacheMode::NoStore)),
        );
        assert!(!cached);
        assert_eq!(reply.body, b"uncached");
        assert!(first(&server.headers(1), "if-none-match").is_none());
        let (reply, cached) = read(&core, &woke, &server, 3, "/a");
        assert!(cached);
        assert_eq!(reply.body, b"kept");
        for body in [b"not stored".as_slice(), b"network"] {
            server.plan("/miss", Plan::new(200, FRESH, body));
        }
        assert!(
            !ask(
                &core,
                &woke,
                job(4, server.request("/miss").cache(CacheMode::NoStore))
            )
            .1
        );
        let (reply, cached) = read(&core, &woke, &server, 5, "/miss");
        assert!(!cached);
        assert_eq!(reply.body, b"network");
        assert_eq!(server.count(), 4);
    }
}

#[test]
fn reload_bypasses_a_fresh_validator_and_stores_the_reply() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan(
            "/a",
            Plan::new(
                200,
                &[("Cache-Control", "max-age=60"), ("ETag", "old")],
                b"old",
            ),
        );
        server.plan("/a", Plan::new(200, FRESH, b"reloaded"));
        read(&core, &woke, &server, 1, "/a");
        let (reply, cached) = ask(
            &core,
            &woke,
            job(2, server.request("/a").cache(CacheMode::Reload)),
        );
        assert!(!cached);
        assert_eq!(reply.body, b"reloaded");
        assert!(first(&server.headers(1), "if-none-match").is_none());
        let (reply, cached) = read(&core, &woke, &server, 3, "/a");
        assert!(cached);
        assert_eq!(reply.body, b"reloaded");
        assert_eq!(server.count(), 2);
    }
}

#[test]
fn no_cache_revalidates_a_fresh_reply_or_asks_without_a_validator() {
    for asynchronous in [false, true] {
        for validator in [false, true] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            let mut headers = FRESH.to_vec();
            if validator {
                headers.push(("ETag", "version"));
            }
            server.plan("/a", Plan::new(200, &headers, b"original"));
            server.plan(
                "/a",
                if validator {
                    Plan::new(304, FRESH, b"")
                } else {
                    Plan::new(200, FRESH, b"new")
                },
            );
            read(&core, &woke, &server, 1, "/a");
            let (reply, cached) = ask(
                &core,
                &woke,
                job(2, server.request("/a").cache(CacheMode::NoCache)),
            );
            assert_eq!(cached, validator);
            assert_eq!(
                first(&server.headers(1), "if-none-match"),
                validator.then_some("version")
            );
            assert!(first(&server.headers(1), "if-modified-since").is_none());
            assert_eq!(
                reply.body,
                if validator {
                    b"original".as_slice()
                } else {
                    b"new"
                }
            );
            let (kept, cached) = read(&core, &woke, &server, 3, "/a");
            assert!(cached);
            assert_eq!(kept.body, reply.body);
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn force_cache_and_only_if_cached_use_stale_entries_but_only_force_cache_fills_a_miss() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan(
            "/a",
            Plan::new(
                200,
                &[
                    ("Cache-Control", "no-cache, must-revalidate, max-age=0"),
                    ("ETag", "stale"),
                ],
                b"stale",
            ),
        );
        let request = server.request("/a");
        assert!(
            !ask(
                &core,
                &woke,
                job(1, request.clone().cache(CacheMode::ForceCache))
            )
            .1
        );
        for (ticket, mode) in [(2, CacheMode::ForceCache), (3, CacheMode::OnlyIfCached)] {
            let (reply, cached) = ask(&core, &woke, job(ticket, request.clone().cache(mode)));
            assert!(cached);
            assert_eq!(reply.body, b"stale");
        }
        core.run(
            job(4, server.request("/missing").cache(CacheMode::OnlyIfCached)),
            None,
        )
        .unwrap();
        let (_, outcome, timing) = collect(&core, &woke);
        assert!(
            matches!(outcome, Outcome::Failed { kind: FailureKind::Network, ref message } if message.contains("no reply is stored")),
            "{outcome:?}"
        );
        assert!(!timing.cached);
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn a_forced_ask_goes_to_the_network_under_every_cache_mode() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan("/a", Plan::new(200, FRESH, b"old"));
        read(&core, &woke, &server, 1, "/a");
        for (index, mode) in [
            CacheMode::Default,
            CacheMode::NoStore,
            CacheMode::Reload,
            CacheMode::NoCache,
            CacheMode::ForceCache,
            CacheMode::OnlyIfCached,
        ]
        .into_iter()
        .enumerate()
        {
            let body = mode.name().as_bytes();
            server.plan("/a", Plan::new(200, FRESH, body));
            let mut request = job(index as u64 + 2, server.request("/a").cache(mode));
            request.forced = true;
            let (reply, cached) = ask(&core, &woke, request);
            assert!(!cached);
            assert_eq!(reply.body, body);
        }
        assert_eq!(server.count(), 7);
        let (reply, cached) = read(&core, &woke, &server, 8, "/a");
        assert!(cached);
        assert_eq!(reply.body, b"only-if-cached");
    }
}

#[test]
fn requests_outside_the_cache_ignore_modes_except_only_if_cached() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        for case in [
            "mutation",
            "post",
            "independent",
            "conditional",
            "stream",
            "disabled",
        ] {
            let (uncached, uncached_woke) = server.core(asynchronous, false);
            let (core, woke) = if case == "disabled" {
                (&uncached, &uncached_woke)
            } else {
                (&core, &woke)
            };
            let path = format!("/{case}");
            let mut request = job(1, server.request(&path));
            match case {
                "mutation" => request.class = RequestClass::Ordered,
                "post" => request.request.method = "POST".into(),
                "independent" => request.request = request.request.independent_http(4096),
                "conditional" => request
                    .request
                    .headers
                    .push(("If-None-Match".into(), "v".into())),
                "stream" => {
                    request.request.stream = true;
                    request.request = request.request.independent_http(4096);
                }
                _ => {}
            }
            let before = server.count();
            for (index, mode) in [
                CacheMode::Default,
                CacheMode::NoStore,
                CacheMode::Reload,
                CacheMode::NoCache,
                CacheMode::ForceCache,
            ]
            .into_iter()
            .enumerate()
            {
                server.plan(&path, Plan::new(200, FRESH, mode.name().as_bytes()));
                request.ticket = index as u64 + 1;
                request.request.cache = mode;
                let (reply, cached) = ask(core, woke, request.clone());
                assert!(!cached, "{case}: {mode:?}");
                assert_eq!(reply.body, mode.name().as_bytes());
            }
            request.ticket = 6;
            request.request.cache = CacheMode::OnlyIfCached;
            core.run(request, None).unwrap();
            let (_, outcome, _) = collect(core, woke);
            assert!(
                matches!(
                    outcome,
                    Outcome::Failed {
                        kind: FailureKind::Network,
                        ..
                    }
                ),
                "{case}: {outcome:?}"
            );
            assert_eq!(server.count(), before + 5, "{case}");
        }
    }
}

#[test]
fn native_cache_modes_set_intermediary_headers_after_the_key_is_taken() {
    for mode in [
        CacheMode::Unspecified,
        CacheMode::Default,
        CacheMode::NoStore,
        CacheMode::Reload,
        CacheMode::NoCache,
        CacheMode::ForceCache,
        CacheMode::OnlyIfCached,
    ] {
        for headers in [
            vec![],
            vec![("Cache-Control".into(), "custom".into())],
            vec![("Pragma".into(), "custom".into())],
        ] {
            let mut request = Request::get("https://api.test/").cache(mode);
            request.headers = headers.clone();
            let key = Key::of(&request);
            let sent = fetch_request(request.clone(), false);
            let refresh = matches!(mode, CacheMode::NoStore | CacheMode::Reload);
            let expected = first(&headers, "cache-control").or(if refresh {
                Some("no-cache")
            } else if mode == CacheMode::NoCache {
                Some("max-age=0")
            } else {
                None
            });
            assert_eq!(sent.headers.get("cache-control"), expected);
            assert_eq!(
                sent.headers.get("pragma"),
                first(&headers, "pragma").or(refresh.then_some("no-cache"))
            );
            assert!(Key::of(&request) == key);
        }
    }
}
