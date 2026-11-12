use super::*;

#[test]
fn a_304_preserves_validators_and_byte_fields_and_updates_other_metadata() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        let modified = "Sun, 06 Nov 1994 08:49:37 GMT";
        server.plan(
            "/a",
            Plan::new(
                200,
                &[
                    ("Cache-Control", "max-age=0"),
                    ("ETag", "W/\"v1\""),
                    ("Content-Type", "text/plain"),
                    ("Last-Modified", modified),
                    ("X-Revision", "old"),
                ],
                b"original",
            ),
        );
        server.plan(
            "/a",
            Plan::new(
                304,
                &[
                    ("Cache-Control", "max-age=60"),
                    ("X-Revision", "new"),
                    ("ETag", "\"v1\""),
                    ("Last-Modified", "Mon, 07 Nov 1994 08:49:37 GMT"),
                    ("Content-Type", "application/json"),
                    ("Content-Encoding", "gzip"),
                    ("Content-Range", "bytes 0-0/1"),
                ],
                b"",
            ),
        );
        read(&core, &woke, &server, 1, "/a");
        let (reply, cached) = read(&core, &woke, &server, 2, "/a");
        assert!(cached);
        assert_eq!(
            (reply.status, reply.body.as_slice()),
            (200, b"original".as_slice())
        );
        assert_eq!(first(&reply.headers, "x-revision"), Some("new"));
        assert_eq!(first(&reply.headers, "etag"), Some("W/\"v1\""));
        assert_eq!(first(&reply.headers, "last-modified"), Some(modified));
        assert_eq!(
            first(&reply.headers, "content-type"),
            Some("application/json")
        );
        assert_eq!(first(&reply.headers, "content-length"), Some("8"));
        assert_eq!(first(&reply.headers, "content-encoding"), None);
        assert_eq!(first(&reply.headers, "content-range"), None);
        assert_eq!(first(&server.headers(1), "if-none-match"), Some("W/\"v1\""));
        assert_eq!(
            first(&server.headers(1), "if-modified-since"),
            Some(modified)
        );
        let (hit, cached) = read(&core, &woke, &server, 3, "/a");
        assert!(cached);
        assert_eq!(
            first(&hit.headers, "content-type"),
            Some("application/json")
        );
        assert_eq!(first(&hit.headers, "etag"), Some("W/\"v1\""));
        assert_eq!(server.count(), 2);
    }
}

#[test]
fn revalidation_200_replaces_and_a_network_failure_stays_a_failure() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan(
            "/a",
            Plan::new(
                200,
                &[("Cache-Control", "max-age=0"), ("ETag", "\"old\"")],
                b"old",
            ),
        );
        server.plan("/a", Plan::new(200, FRESH, b"new"));
        read(&core, &woke, &server, 1, "/a");
        assert_eq!(read(&core, &woke, &server, 2, "/a").0.body, b"new");
        assert_eq!(first(&server.headers(1), "if-none-match"), Some("\"old\""));
        let (reply, cached) = read(&core, &woke, &server, 3, "/a");
        assert_eq!(reply.body, b"new");
        assert!(cached);
        server.plan(
            "/failure",
            Plan::new(
                200,
                &[("Cache-Control", "no-cache"), ("ETag", "\"v\"")],
                b"stale",
            ),
        );
        let mut failure = Plan::new(200, &[], b"");
        failure.disconnect = true;
        server.plan("/failure", failure);
        read(&core, &woke, &server, 4, "/failure");
        core.run(job(5, server.request("/failure")), None).unwrap();
        assert!(matches!(
            collect(&core, &woke).1,
            Outcome::Failed {
                kind: FailureKind::Network,
                ..
            }
        ));
    }
}

#[test]
fn an_unredirected_cache_conditional_renews_its_own_entry() {
    for asynchronous in [false, true] {
        for old in [None, Some("W/\"abc\""), Some("\"abc\"")] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            let mut headers = vec![
                ("Cache-Control", "max-age=0"),
                ("Last-Modified", "Sun, 06 Nov 1994 08:49:37 GMT"),
            ];
            if let Some(tag) = old {
                headers.push(("ETag", tag));
            }
            server.plan("/a", Plan::new(200, &headers, b"stored"));
            server.plan(
                "/a",
                Plan::new(
                    304,
                    &[("ETag", "\"abc\""), ("Cache-Control", "max-age=60")],
                    b"",
                ),
            );
            read(&core, &woke, &server, 1, "/a");
            let (reply, cached) = read(&core, &woke, &server, 2, "/a");
            assert!(cached);
            assert_eq!(reply.status, 200);
            assert_eq!(reply.body, b"stored");
            assert_eq!(first(&reply.headers, "etag"), old);
            assert_eq!(
                first(&reply.headers, "last-modified"),
                Some("Sun, 06 Nov 1994 08:49:37 GMT")
            );
            assert_eq!(first(&reply.headers, "content-length"), Some("6"));
            assert!(read(&core, &woke, &server, 3, "/a").1);
            let mut caller = server.request("/a");
            caller
                .headers
                .push(("If-None-Match".into(), "\"abc\"".into()));
            server.plan("/a", Plan::new(304, &[("ETag", "\"abc\"")], b""));
            assert_eq!(ask(&core, &woke, job(4, caller)).0.status, 304);
        }
    }
}

#[test]
fn error_revalidation_keeps_the_entry_unless_the_reply_forbids_storage() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan(
            "/a",
            Plan::new(
                200,
                &[("Cache-Control", "max-age=0"), ("ETag", "\"kept\"")],
                b"kept",
            ),
        );
        read(&core, &woke, &server, 1, "/a");
        for status in [500, 404] {
            server.plan("/a", Plan::new(status, &[], b"error"));
            assert_eq!(read(&core, &woke, &server, 2, "/a").0.status, status);
            assert_eq!(
                first(&server.headers(server.count() - 1), "if-none-match"),
                Some("\"kept\"")
            );
            assert!(
                ask(
                    &core,
                    &woke,
                    job(3, server.request("/a").cache(CacheMode::ForceCache))
                )
                .1
            );
        }
        server.plan("/a", Plan::new(304, FRESH, b""));
        assert_eq!(read(&core, &woke, &server, 4, "/a").0.body, b"kept");
        server.plan(
            "/a",
            Plan::new(500, &[("Cache-Control", "no-store")], b"error"),
        );
        ask(
            &core,
            &woke,
            job(5, server.request("/a").cache(CacheMode::NoCache)),
        );
        core.run(
            job(6, server.request("/a").cache(CacheMode::OnlyIfCached)),
            None,
        )
        .unwrap();
        assert!(matches!(collect(&core, &woke).1, Outcome::Failed { .. }));
    }
}

#[test]
fn a_redirected_cache_conditional_retries_without_validators_and_drops_the_old_entry() {
    for asynchronous in [false, true] {
        for (redirect_again, cacheable) in
            [(false, false), (false, true), (true, false), (true, true)]
        {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            server.plan(
                "/a",
                Plan::new(
                    200,
                    &[("Cache-Control", "max-age=0"), ("ETag", "\"v\"")],
                    b"old",
                ),
            );
            read(&core, &woke, &server, 1, "/a");
            server.plan("/a", Plan::new(302, &[("Location", "/b")], b""));
            server.plan("/b", Plan::new(304, &[("ETag", "\"v\"")], b""));
            let final_reply = Plan::new(200, if cacheable { FRESH } else { &[] }, b"new");
            if redirect_again {
                server.plan("/a", Plan::new(302, &[("Location", "/b")], b""));
                server.plan("/b", final_reply);
            } else {
                server.plan("/a", final_reply);
            }
            let (reply, cached) = read(&core, &woke, &server, 2, "/a");
            assert_eq!(reply.status, 200);
            assert_eq!(reply.body, b"new");
            assert!(!cached);
            assert_eq!(server.count(), if redirect_again { 5 } else { 4 });
            assert_eq!(first(&server.headers(2), "if-none-match"), Some("\"v\""));
            for index in 3..server.count() {
                assert_eq!(first(&server.headers(index), "if-none-match"), None);
                assert_eq!(first(&server.headers(index), "if-modified-since"), None);
            }
            core.run(
                job(3, server.request("/a").cache(CacheMode::OnlyIfCached)),
                None,
            )
            .unwrap();
            let (_, outcome, timing) = collect(&core, &woke);
            if cacheable && !redirect_again {
                let Outcome::Response(reply) = outcome else {
                    panic!("{outcome:?}");
                };
                assert!(timing.cached);
                assert_eq!(reply.body, b"new");
            } else {
                assert!(matches!(
                    outcome,
                    Outcome::Failed {
                        kind: FailureKind::Network,
                        ..
                    }
                ));
            }
        }
    }
}
