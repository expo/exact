use super::*;
use ibex2::host::Host;
use ibex2::stdlib::fetch::{Response, Transport};
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

type Reply = dyn Fn(&Request, usize, f64, &AbortSignal) -> Result<StreamingResponse, HostError>
    + Send
    + Sync;
struct Origin {
    sent: Arc<Mutex<Vec<Request>>>,
    now: Arc<AtomicU64>,
    reply: Arc<Reply>,
}
impl Transport for Origin {
    fn open(
        &self,
        request: &Request,
        signal: &AbortSignal,
    ) -> Result<StreamingResponse, HostError> {
        let n = {
            let mut sent = self.sent.lock().unwrap();
            sent.push(request.clone());
            sent.len()
        };
        (self.reply)(request, n, self.now.load(Ordering::Relaxed) as f64, signal)
    }
}
struct Fixture {
    cache: SharedHttpCache,
    fetch: Fetch,
    sent: Arc<Mutex<Vec<Request>>>,
    now: Arc<AtomicU64>,
}
fn response(request: &Request, n: usize, headers: &[(&str, &str)]) -> Response {
    let mut head = Headers::new();
    for (k, v) in headers {
        head.append(k, v);
    }
    Response {
        status: 200,
        status_text: "OK".into(),
        headers: head,
        body: format!("answer {n}").into_bytes(),
        url: request.url.clone(),
        redirected: false,
    }
}
fn fixture(headers: &[(&str, &str)]) -> Fixture {
    let headers: Vec<_> = headers
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Fixture::new(
        Limits {
            entries: 256,
            bytes: 64 << 20,
            entry: 8 << 20,
        },
        move |r, n, _, signal| {
            let headers: Vec<_> = headers
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            Ok(response(r, n, &headers).into_stream(r.body_limit(), signal.clone()))
        },
    )
}
impl Fixture {
    fn new(
        limits: Limits,
        reply: impl Fn(&Request, usize, f64, &AbortSignal) -> Result<StreamingResponse, HostError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let now = Arc::new(AtomicU64::new(
            super::super::cache::http_date("Fri, 30 Oct 2026 12:00:00 GMT").unwrap(),
        ));
        let host = Host::with_transport(Box::new(Origin {
            sent: sent.clone(),
            now: now.clone(),
            reply: Arc::new(reply),
        }));
        let fetch = host
            .endow(ibex2::grant::GrantSet::parse("net.fetch https://api.test").unwrap())
            .fetch;
        let clock = now.clone();
        let cache = SharedHttpCache::with_clock(
            limits,
            Arc::new(move || clock.load(Ordering::Relaxed) as f64),
        );
        Self {
            cache,
            fetch,
            sent,
            now,
        }
    }
    fn request(&self, path: &str) -> Request {
        Request::get(&format!("https://api.test{path}"))
    }
    fn stream(&self, request: Request, mode: CacheMode) -> StreamingResponse {
        self.cache
            .stream(&self.fetch, request, mode, &AbortSignal::default())
            .unwrap()
    }
    fn ask(&self, request: Request, mode: CacheMode) -> Response {
        self.stream(request, mode).collect().unwrap()
    }
    fn get(&self, path: &str) -> Response {
        self.ask(self.request(path), CacheMode::Default)
    }
    fn advance(&self, seconds: u64) {
        self.now.fetch_add(seconds, Ordering::Relaxed);
    }
    fn count(&self) -> usize {
        self.sent.lock().unwrap().len()
    }
    fn refreshed(&self) {
        let inner = &self.cache.owner.inner;
        let until = Instant::now() + Duration::from_secs(10);
        let mut state = inner.state.lock().unwrap();
        while !state.refreshing.is_empty() {
            let left = until.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "cache refetch did not retire");
            state = inner.ready.wait_timeout(state, left).unwrap().0;
        }
    }
}

#[test]
fn freshness_uses_explicit_lifetimes_age_and_shared_precedence() {
    for (headers, fresh_for) in [
        (vec![("cache-control", "public, max-age=60")], 60),
        (vec![("cache-control", "max-age=600, s-maxage=10")], 10),
        (vec![("expires", "Fri, 30 Oct 2026 12:00:30 GMT")], 30),
        (vec![("cache-control", "max-age=60"), ("age", "50")], 10),
        (
            vec![
                ("cache-control", "max-age=60"),
                ("date", "Fri, 30 Oct 2026 11:59:10 GMT"),
            ],
            10,
        ),
    ] {
        let f = fixture(&headers);
        let first = f.get("/a");
        f.advance(fresh_for - 1);
        let hit = f.get("/a");
        assert_eq!(hit.body, first.body);
        assert_eq!(hit.url, "https://api.test/a");
        assert!(hit.headers.has("age"));
        assert_eq!(f.count(), 1);
        f.advance(1);
        assert_ne!(f.get("/a").body, first.body);
        assert_eq!(f.count(), 2);
    }
}

#[test]
fn forbidden_shared_answers_and_missing_lifetimes_always_reach_the_origin() {
    for headers in [
        vec![("cache-control", "no-store, max-age=60")],
        vec![("cache-control", "private, max-age=60")],
        vec![("cache-control", "no-cache, max-age=60")],
        vec![],
        vec![("cache-control", "max-age=0")],
        vec![("cache-control", "max-age=60"), ("vary", "*")],
        vec![("expires", "invalid")],
    ] {
        let f = fixture(&headers);
        assert_ne!(f.get("/a").body, f.get("/a").body, "{headers:?}");
        assert_eq!(f.count(), 2);
        assert_eq!(f.cache.owner.inner.bytes.load(Ordering::Relaxed), 0);
    }
    for status in [200, 203, 204, 404, 405, 410, 414, 501, 206, 302, 500] {
        let f = Fixture::new(
            Limits {
                entries: 4,
                bytes: 4096,
                entry: 1024,
            },
            move |r, n, _, signal| {
                let mut reply = response(r, n, &[("cache-control", "max-age=60")]);
                reply.status = status;
                if status == 204 {
                    reply.body.clear();
                }
                Ok(reply.into_stream(r.body_limit(), signal.clone()))
            },
        );
        f.get("/a");
        f.get("/a");
        assert_eq!(
            f.count(),
            if matches!(status, 206 | 302 | 500) {
                2
            } else {
                1
            },
            "{status}"
        );
    }
}

#[test]
fn credentials_need_explicit_sharing_even_for_an_anonymous_entry() {
    for cc in [
        "max-age=60",
        "public, max-age=60",
        "s-maxage=60",
        "max-age=60, must-revalidate",
    ] {
        let f = fixture(&[("cache-control", cc)]);
        let first = f.get("/a").body;
        let mut auth = f.request("/a");
        auth.headers.set("authorization", "Bearer secret");
        let shared = cc != "max-age=60";
        assert_eq!(
            f.ask(auth.clone(), CacheMode::Default).body == first,
            shared
        );
        assert_eq!(f.ask(auth, CacheMode::Default).body == first, shared);
        assert_eq!(f.get("/a").body, first);
        assert_eq!(f.count(), if shared { 1 } else { 3 });
    }
    let f = fixture(&[("cache-control", "max-age=60")]);
    let mut auth = f.request("/a");
    auth.headers.set("authorization", "Bearer secret");
    assert_ne!(
        f.ask(auth.clone(), CacheMode::Default).body,
        f.ask(auth, CacheMode::Default).body
    );
}

#[test]
fn vary_compares_header_names_without_changing_header_values() {
    let f = fixture(&[
        ("cache-control", "max-age=60"),
        ("vary", "Accept-Language, X-Mode"),
    ]);
    let mut fr = f.request("/v");
    fr.headers.set("Accept-Language", "fr");
    let first = f.ask(fr.clone(), CacheMode::Default).body;
    assert_eq!(f.ask(fr.clone(), CacheMode::Default).body, first);
    fr.headers.set("x-mode", "A");
    let a = f.ask(fr.clone(), CacheMode::Default).body;
    assert_ne!(a, first);
    assert_eq!(f.ask(fr.clone(), CacheMode::Default).body, a);
    fr.headers.set("X-MODE", "a");
    assert_ne!(f.ask(fr, CacheMode::Default).body, a);
    assert_eq!(f.count(), 3);
}

#[test]
fn cache_modes_request_directives_and_methods_keep_their_origin_semantics() {
    let f = fixture(&[("cache-control", "max-age=60"), ("etag", "\"v1\"")]);
    assert!(f
        .cache
        .stream(
            &f.fetch,
            f.request("/a"),
            CacheMode::OnlyIfCached,
            &AbortSignal::default()
        )
        .is_err());
    assert_eq!(f.count(), 0);
    let first = f.get("/a").body;
    assert_eq!(f.ask(f.request("/a"), CacheMode::OnlyIfCached).body, first);
    for cc in ["no-store", "NO-STORE"] {
        let mut request = f.request("/a");
        request.headers.set("cache-control", cc);
        assert_ne!(f.ask(request, CacheMode::Default).body, first);
        assert_eq!(f.get("/a").body, first);
    }
    assert_ne!(f.ask(f.request("/a"), CacheMode::NoStore).body, first);
    assert_eq!(f.get("/a").body, first);
    let mut post = f.request("/a");
    post.method = "POST".into();
    post.body = Some(b"x".to_vec());
    assert_ne!(f.ask(post, CacheMode::Default).body, first);
    assert_eq!(f.get("/a").body, first);
    for mode in [CacheMode::Reload, CacheMode::NoCache] {
        let previous = f.get("/a").body;
        let brought = f.ask(f.request("/a"), mode).body;
        assert_ne!(brought, previous);
        assert_eq!(f.get("/a").body, brought);
    }
    for cc in ["no-cache", "max-age=0"] {
        let previous = f.get("/a").body;
        let mut request = f.request("/a");
        request.headers.set("cache-control", cc);
        let brought = f.ask(request, CacheMode::Default).body;
        assert_ne!(brought, previous);
        assert_eq!(f.get("/a").body, brought);
    }
}

#[test]
fn only_if_cached_never_refreshes_a_stale_answer() {
    for (cc, asked) in [
        ("max-age=1, stale-while-revalidate=60", ""),
        ("max-age=1", "max-stale=60"),
    ] {
        let f = fixture(&[("cache-control", cc)]);
        let first = f.get("/a").body;
        f.advance(2);
        let mut request = f.request("/a");
        request.headers.set("cache-control", asked);
        assert_eq!(f.ask(request.clone(), CacheMode::OnlyIfCached).body, first);
        f.refreshed();
        assert_eq!(f.count(), 1, "{cc}; {asked}");
        f.advance(60);
        assert!(f
            .cache
            .stream(
                &f.fetch,
                request,
                CacheMode::OnlyIfCached,
                &AbortSignal::default()
            )
            .is_err());
        f.refreshed();
        assert_eq!(f.count(), 1);
    }
}

#[test]
fn validators_survive_hits_and_a_callers_conditional_reaches_the_origin() {
    let f = Fixture::new(
        Limits {
            entries: 4,
            bytes: 4096,
            entry: 1024,
        },
        |r, n, _, signal| {
            let mut reply = response(
                r,
                n,
                &[
                    ("cache-control", "max-age=10"),
                    ("etag", "\"v1\""),
                    ("last-modified", "Fri, 30 Oct 2026 11:00:00 GMT"),
                ],
            );
            if r.headers.has("if-none-match") {
                reply.status = 304;
                reply.body.clear();
            }
            Ok(reply.into_stream(r.body_limit(), signal.clone()))
        },
    );
    let first = f.get("/a");
    assert_eq!(f.get("/a").headers.get("etag"), Some("\"v1\""));
    let mut conditional = f.request("/a");
    conditional.headers.set("if-none-match", "\"v1\"");
    assert_eq!(f.ask(conditional, CacheMode::Default).status, 304);
    assert_eq!(f.get("/a").body, first.body);
    f.advance(10);
    assert_ne!(f.get("/a").body, first.body);
    assert!(!f
        .sent
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .headers
        .has("if-none-match"));
}

#[test]
fn a_callers_conditional_full_answer_replaces_or_removes_the_old_answer() {
    for condition in ["if-none-match", "if-modified-since"] {
        for cc in ["max-age=60", "private, max-age=60", "no-store", "no-cache"] {
            let f = Fixture::new(
                Limits {
                    entries: 4,
                    bytes: 4096,
                    entry: 1024,
                },
                move |r, n, _, signal| {
                    let cc = if n == 2 { cc } else { "max-age=60" };
                    Ok(response(r, n, &[("cache-control", cc), ("etag", "\"v1\"")])
                        .into_stream(r.body_limit(), signal.clone()))
                },
            );
            assert_eq!(f.get("/a").body, b"answer 1");
            let mut request = f.request("/a");
            request.headers.set(
                condition,
                if condition == "if-none-match" {
                    "\"v2\""
                } else {
                    "Fri, 30 Oct 2026 11:00:00 GMT"
                },
            );
            assert_eq!(f.ask(request, CacheMode::Default).body, b"answer 2");
            let stored = cc == "max-age=60";
            assert_eq!(
                f.get("/a").body,
                if stored { b"answer 2" } else { b"answer 3" }
            );
            assert_eq!(f.count(), if stored { 2 } else { 3 });
        }
    }
}

#[test]
fn a_callers_304_renews_only_the_matching_stored_representation() {
    for (kept, validator, asked, matches) in [
        ("\"v1\"", Some("\"v1\""), "\"v1\"", true),
        ("\"v1\"", Some("\"other\""), "\"other\"", false),
        ("\"v1\"", Some("W/\"v1\""), "\"v1\"", true),
        ("W/\"v1\"", Some("\"v1\""), "\"v1\"", false),
        ("\"v1\"", None, "\"v1\"", true),
        ("\"v1\"", None, "\"v1\", \"v2\"", false),
        ("\"v1\"", None, "*", false),
    ] {
        let f = Fixture::new(
            Limits {
                entries: 4,
                bytes: 4096,
                entry: 1024,
            },
            move |r, n, _, signal| {
                let mut reply = response(r, n, &[("cache-control", "max-age=10"), ("etag", kept)]);
                if r.headers.has("if-none-match") {
                    reply.status = 304;
                    reply.body.clear();
                    reply.headers.delete("etag");
                    if let Some(validator) = validator {
                        reply.headers.set("etag", validator);
                    }
                    reply.headers.set("cache-control", "max-age=60");
                    reply.headers.set("x-revision", "renewed");
                }
                Ok(reply.into_stream(r.body_limit(), signal.clone()))
            },
        );
        assert_eq!(f.get("/a").body, b"answer 1");
        f.advance(9);
        let mut request = f.request("/a");
        request.headers.set("if-none-match", asked);
        assert_eq!(f.ask(request, CacheMode::Default).status, 304);
        f.advance(2);
        let next = f.get("/a");
        assert_eq!(next.body, if matches { b"answer 1" } else { b"answer 3" });
        assert_eq!(next.headers.get("x-revision"), matches.then_some("renewed"));
        assert_eq!(f.count(), if matches { 2 } else { 3 });
    }
}

#[test]
fn a_fresh_answer_is_dropped_by_a_304_that_does_not_validate_it_or_forbids_keeping_it() {
    for (validator, cc, kept) in [
        (Some("\"v1\""), None, true),
        (Some("\"v1\""), Some("no-store"), false),
        (Some("\"v2\""), None, false),
        (Some("\"v2\""), Some("no-store"), false),
        (None, None, false),
    ] {
        let f = Fixture::new(
            Limits {
                entries: 4,
                bytes: 4096,
                entry: 1024,
            },
            move |r, n, _, signal| {
                let mut reply = response(
                    r,
                    n,
                    &[("cache-control", "max-age=3600"), ("etag", "\"v1\"")],
                );
                if r.headers.has("if-none-match") {
                    reply.status = 304;
                    reply.body.clear();
                    reply.headers.delete("etag");
                    reply.headers.delete("cache-control");
                    if let Some(validator) = validator {
                        reply.headers.set("etag", validator);
                    }
                    if let Some(cc) = cc {
                        reply.headers.set("cache-control", cc);
                    }
                }
                Ok(reply.into_stream(r.body_limit(), signal.clone()))
            },
        );
        assert_eq!(f.get("/a").body, b"answer 1");
        let mut request = f.request("/a");
        request
            .headers
            .set("if-none-match", validator.unwrap_or("\"v2\""));
        assert_eq!(f.ask(request, CacheMode::Default).status, 304);
        let label = format!("{validator:?} {cc:?}");
        assert_eq!(
            f.get("/a").body,
            if kept { &b"answer 1"[..] } else { b"answer 3" },
            "{label}"
        );
        assert_eq!(f.count(), if kept { 2 } else { 3 }, "{label}");
    }
}

#[test]
fn a_stale_answer_serves_until_its_refetch_body_completes_and_survives_its_failure() {
    struct Held {
        reading: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<bool>,
        done: bool,
    }
    impl BodySource for Held {
        fn read(&mut self, out: &mut [u8]) -> Result<usize, HostError> {
            if self.done {
                return Ok(0);
            }
            self.done = true;
            self.reading.send(()).unwrap();
            if !self.release.recv_timeout(Duration::from_secs(10)).unwrap() {
                return Err(HostError::Failed("refetch body failed".into()));
            }
            out[..5].copy_from_slice(b"fresh");
            Ok(5)
        }
    }
    for completes in [true, false] {
        let (reading, read) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let released = Mutex::new(Some(released));
        let f = Fixture::new(
            Limits {
                entries: 4,
                bytes: 4096,
                entry: 1024,
            },
            move |r, n, _, signal| {
                let mut reply = response(
                    r,
                    n,
                    &[("cache-control", "s-maxage=10, stale-while-revalidate=30")],
                )
                .into_stream(r.body_limit(), signal.clone());
                if n == 2 {
                    let held = Held {
                        reading: reading.clone(),
                        release: released.lock().unwrap().take().unwrap(),
                        done: false,
                    };
                    reply.body = Body::new(Box::new(held), r.body_limit(), signal.clone());
                }
                Ok(reply)
            },
        );
        assert_eq!(f.get("/a").body, b"answer 1");
        f.advance(15);
        assert_eq!(f.get("/a").body, b"answer 1");
        // The refetch has its headers and is reading its body.
        read.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(f.get("/a").body, b"answer 1");
        assert_eq!(f.count(), 2);
        release.send(completes).unwrap();
        f.refreshed();
        if completes {
            assert_eq!(f.get("/a").body, b"fresh");
            assert_eq!(f.count(), 2);
        } else {
            assert_eq!(f.get("/a").body, b"answer 1");
            f.refreshed();
            assert_eq!(f.get("/a").body, b"answer 3");
            assert_eq!(f.count(), 3);
        }
    }
}

#[test]
fn a_callers_304_preserves_vary_authorization_and_storage_restrictions() {
    for kind in [
        "modified",
        "vary",
        "authorization",
        "private",
        "no-store",
        "no-cache",
    ] {
        let f = Fixture::new(
            Limits {
                entries: 4,
                bytes: 4096,
                entry: 1024,
            },
            move |r, n, _, signal| {
                let mut reply = response(
                    r,
                    n,
                    &[
                        ("cache-control", "max-age=10"),
                        ("last-modified", "Fri, 30 Oct 2026 11:00:00 GMT"),
                        ("vary", "x-mode"),
                    ],
                );
                if r.headers.has("if-modified-since") {
                    reply.status = 304;
                    reply.body.clear();
                    let cc = match kind {
                        "private" => "private, max-age=60",
                        "no-store" => "no-store, max-age=60",
                        "no-cache" => "no-cache, max-age=60",
                        _ => "max-age=60",
                    };
                    reply.headers.set("cache-control", cc);
                    // A validator identifies the stored bytes; this field cannot relabel them.
                    reply.headers.set("content-length", "0");
                }
                Ok(reply.into_stream(r.body_limit(), signal.clone()))
            },
        );
        assert_eq!(f.get("/a").body, b"answer 1");
        f.advance(9);
        let mut request = f.request("/a");
        request
            .headers
            .set("if-modified-since", "Fri, 30 Oct 2026 11:00:00 GMT");
        if kind == "vary" {
            request.headers.set("x-mode", "other");
        }
        if kind == "authorization" {
            request.headers.set("authorization", "secret");
        }
        assert_eq!(f.ask(request, CacheMode::Default).status, 304);
        f.advance(2);
        assert_eq!(
            f.get("/a").body,
            if kind == "modified" {
                b"answer 1"
            } else {
                b"answer 3"
            }
        );
        assert_eq!(f.count(), if kind == "modified" { 2 } else { 3 });
    }
}

#[test]
fn redirects_are_not_kept_and_each_destination_still_needs_a_grant() {
    let f = Fixture::new(
        Limits {
            entries: 4,
            bytes: 4096,
            entry: 1024,
        },
        |r, n, _, signal| {
            let mut reply = response(r, n, &[("cache-control", "max-age=60")]);
            if r.url.ends_with("/a") {
                reply.status = 302;
                reply.headers.set("location", "/target");
            }
            if r.url.ends_with("/denied") {
                reply.status = 302;
                reply.headers.set("location", "https://other.test/target");
            }
            Ok(reply.into_stream(r.body_limit(), signal.clone()))
        },
    );
    let a = f.get("/a");
    assert!(a.redirected);
    assert_ne!(f.get("/a").body, a.body);
    assert_eq!(f.count(), 4);
    assert!(f
        .cache
        .stream(
            &f.fetch,
            f.request("/denied"),
            CacheMode::Default,
            &AbortSignal::default()
        )
        .is_err());
    assert_eq!(f.count(), 5);
    assert_eq!(f.cache.owner.inner.bytes.load(Ordering::Relaxed), 0);
}

#[test]
fn request_age_bounds_and_revalidation_flags_control_stale_answers() {
    let f = fixture(&[("cache-control", "max-age=60")]);
    let first = f.get("/a").body;
    f.advance(10);
    let mut request = f.request("/a");
    request.headers.set("cache-control", "max-age=20");
    assert_eq!(f.ask(request.clone(), CacheMode::Default).body, first);
    request.headers.set("cache-control", "max-age=1");
    let younger = f.ask(request.clone(), CacheMode::Default).body;
    assert_ne!(younger, first);
    f.advance(50);
    request.headers.set("cache-control", "min-fresh=5");
    assert_eq!(f.ask(request.clone(), CacheMode::Default).body, younger);
    request.headers.set("cache-control", "min-fresh=20");
    let fresh = f.ask(request.clone(), CacheMode::Default).body;
    assert_ne!(fresh, younger);
    f.advance(65);
    request.headers.set("cache-control", "max-stale=5");
    assert_ne!(f.ask(request.clone(), CacheMode::Default).body, fresh); // age == lifetime + grace
    let fresh = f.get("/a").body;
    f.advance(65);
    request.headers.set("cache-control", "max-stale=10");
    assert_eq!(f.ask(request, CacheMode::Default).body, fresh);
    f.refreshed();
    assert_ne!(f.get("/a").body, fresh);
    for cc in [
        "max-age=1, must-revalidate, stale-while-revalidate=60",
        "max-age=1, proxy-revalidate, stale-while-revalidate=60",
        "s-maxage=1, stale-while-revalidate=59",
    ] {
        let f = fixture(&[("cache-control", cc)]);
        let first = f.get("/a").body;
        f.advance(2);
        let mut request = f.request("/a");
        request.headers.set("cache-control", "max-stale");
        assert_eq!(
            f.ask(request, CacheMode::Default).body == first,
            cc.starts_with("s-maxage")
        );
        f.refreshed();
    }
}

#[test]
fn the_least_recently_served_entry_is_evicted_at_the_count_and_byte_bounds() {
    for limits in [
        Limits {
            entries: 2,
            bytes: 4096,
            entry: 1024,
        },
        Limits {
            entries: 256,
            bytes: 24,
            entry: 8,
        },
    ] {
        let f = Fixture::new(limits, |r, n, _, signal| {
            Ok(response(r, n, &[("cache-control", "max-age=60")])
                .into_stream(r.body_limit(), signal.clone()))
        });
        let a = f.get("/1").body;
        f.get("/2");
        assert_eq!(f.get("/1").body, a);
        f.get("/3");
        assert_eq!(f.get("/1").body, a);
        assert_eq!(f.count(), 3);
        assert_eq!(f.get("/2").body, b"answer 4");
        assert!(f.cache.owner.inner.bytes.load(Ordering::Relaxed) <= limits.bytes);
    }
}

#[test]
fn oversized_and_underdeclared_bodies_stop_collection_but_remain_readable() {
    for declared in [None, Some("8"), Some("16384")] {
        let f = Fixture::new(
            Limits {
                entries: 4,
                bytes: 4096,
                entry: 1024,
            },
            move |r, _, _, signal| {
                let mut reply = response(r, 1, &[("cache-control", "max-age=60")]);
                reply.body = vec![0; 16384];
                if let Some(length) = declared {
                    reply.headers.set("content-length", length);
                }
                Ok(reply.into_stream(r.body_limit(), signal.clone()))
            },
        );
        for _ in 0..2 {
            let mut stream = f.stream(f.request("/large"), CacheMode::Default);
            let mut chunk = [0; 256];
            let mut size = 0;
            loop {
                let n = stream.body.read(&mut chunk).unwrap();
                assert!(f.cache.owner.inner.bytes.load(Ordering::Relaxed) <= 4096);
                if n == 0 {
                    break;
                }
                size += n;
            }
            assert_eq!(size, 16384);
            assert_eq!(f.cache.owner.inner.bytes.load(Ordering::Relaxed), 0);
        }
        assert_eq!(f.count(), 2);
    }
}

#[test]
fn pending_entries_share_the_byte_bound_and_release_reservations_after_rejection() {
    let f = Fixture::new(
        Limits {
            entries: 2,
            bytes: 1024,
            entry: 1024,
        },
        |r, n, _, signal| {
            let mut reply = response(r, n, &[("cache-control", "max-age=60")]);
            reply.body = vec![0; if n < 3 { 768 } else { 128 }];
            Ok(reply.into_stream(r.body_limit(), signal.clone()))
        },
    );
    let mut a = f.stream(f.request("/a"), CacheMode::Default);
    let mut b = f.stream(f.request("/b"), CacheMode::Default);
    assert_eq!(a.body.read(&mut [0; 768]).unwrap(), 768);
    assert_eq!(b.body.read(&mut [0; 768]).unwrap(), 768);
    assert_eq!(f.cache.owner.inner.bytes.load(Ordering::Relaxed), 768);
    assert_eq!(a.body.read(&mut [0; 1]).unwrap(), 0);
    assert_eq!(b.body.read(&mut [0; 1]).unwrap(), 0);
    assert_eq!(f.cache.owner.inner.bytes.load(Ordering::Relaxed), 0);
    assert_eq!(f.get("/a").body.len(), 128);
    assert_eq!(f.get("/a").body.len(), 128);
    assert_eq!(f.count(), 3);
    assert_eq!(f.cache.owner.inner.storing.load(Ordering::Relaxed), 0);
    // An evicted answer still being read retains its reservation.
    let kept = f.stream(f.request("/a"), CacheMode::Default);
    for path in ["/b", "/c", "/d"] {
        f.get(path);
    }
    assert!(f.cache.owner.inner.bytes.load(Ordering::Relaxed) <= 1024);
    assert_eq!(kept.collect().unwrap().body.len(), 128);
}

#[test]
fn dropped_failed_and_aborted_bodies_are_never_stored_and_hits_obey_the_body_limit() {
    struct Broken;
    impl BodySource for Broken {
        fn read(&mut self, _: &mut [u8]) -> Result<usize, HostError> {
            Err(HostError::Failed("broken body".into()))
        }
    }
    let f = Fixture::new(
        Limits {
            entries: 2,
            bytes: 4096,
            entry: 1024,
        },
        |r, n, _, signal| {
            let mut reply = response(r, n, &[("cache-control", "max-age=60")])
                .into_stream(r.body_limit(), signal.clone());
            if r.url.ends_with("/broken") {
                reply.body = Body::new(Box::new(Broken), 1024, signal.clone());
            }
            Ok(reply)
        },
    );
    let mut unfinished = f.stream(f.request("/a"), CacheMode::Default);
    assert_eq!(unfinished.body.read(&mut [0; 3]).unwrap(), 3);
    drop(unfinished);
    assert_eq!(f.cache.owner.inner.bytes.load(Ordering::Relaxed), 0);
    let abort = AbortController::new();
    let mut unfinished = f
        .cache
        .stream(
            &f.fetch,
            f.request("/a"),
            CacheMode::Default,
            &abort.signal(),
        )
        .unwrap();
    unfinished.body.read(&mut [0; 3]).unwrap();
    abort.abort();
    assert!(unfinished.collect().is_err());
    for _ in 0..2 {
        assert!(f
            .stream(f.request("/broken"), CacheMode::Default)
            .collect()
            .is_err());
    }
    assert_eq!(f.cache.owner.inner.bytes.load(Ordering::Relaxed), 0);
    let first = f.get("/a");
    assert_eq!(first.body, b"answer 5");
    let mut small = f.request("/a");
    small.max_body = Some(1);
    assert!(f.stream(small, CacheMode::Default).collect().is_err());
    assert_eq!(f.count(), 5);
    assert!(f
        .cache
        .stream(
            &f.fetch,
            f.request("/a"),
            CacheMode::Default,
            &abort.signal()
        )
        .is_err());
    assert_eq!(f.count(), 5);
}

#[test]
fn stale_refetches_are_coalesced_and_outlive_the_request_but_not_the_cache_owner() {
    for drop_owner in [false, true] {
        let (began, beginning) = std::sync::mpsc::channel();
        let (ended, ending) = std::sync::mpsc::channel();
        let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let held = gate.clone();
        let f = Fixture::new(
            Limits {
                entries: 4,
                bytes: 4096,
                entry: 1024,
            },
            move |r, n, _, signal| {
                if n > 1 {
                    let abort_gate = held.clone();
                    let _abort = signal.register(move || {
                        let (state, ready) = &*abort_gate;
                        let _state = state.lock().unwrap();
                        ready.notify_all();
                    });
                    began.send(()).unwrap();
                    let (state, ready) = &*held;
                    let mut released = state.lock().unwrap();
                    while !*released && !signal.aborted() {
                        let (next, timeout) = ready
                            .wait_timeout(released, Duration::from_secs(10))
                            .unwrap();
                        released = next;
                        assert!(!timeout.timed_out(), "refetch did not end");
                    }
                    ended.send(signal.aborted()).unwrap();
                    signal.check()?;
                }
                Ok(response(
                    r,
                    n,
                    &[("cache-control", "s-maxage=10, stale-while-revalidate=30")],
                )
                .into_stream(r.body_limit(), signal.clone()))
            },
        );
        let first = f.get("/a").body;
        f.advance(15);
        let abort = AbortController::new();
        assert_eq!(
            f.cache
                .stream(
                    &f.fetch,
                    f.request("/a"),
                    CacheMode::Default,
                    &abort.signal()
                )
                .unwrap()
                .collect()
                .unwrap()
                .body,
            first
        );
        beginning.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(f.get("/a").body, first);
        assert_eq!(f.count(), 2);
        abort.abort();
        if drop_owner {
            drop(f);
            assert!(ending.recv_timeout(Duration::from_secs(10)).unwrap());
        } else {
            let (state, ready) = &*gate;
            *state.lock().unwrap() = true;
            ready.notify_all();
            assert!(!ending.recv_timeout(Duration::from_secs(10)).unwrap());
            f.refreshed();
            assert_eq!(f.get("/a").body, b"answer 2");
            f.advance(45);
            assert_eq!(f.get("/a").body, b"answer 3");
        }
    }
}

#[test]
fn different_cores_share_answers_without_sharing_or_widening_grants() {
    use crate::cache::tests::{collect, job, read, Plan, Server};
    let server = Server::new();
    let cache = SharedHttpCache::new();
    server.plan(
        "/a",
        Plan::new(200, &[("Cache-Control", "public, max-age=60")], b"shared"),
    );
    {
        let (core, woke) = server.core(false, false);
        core.set_shared_http_cache(cache.clone());
        assert_eq!(read(&core, &woke, &server, 1, "/a").0.body, b"shared");
    }
    {
        let (core, woke) = server.core(false, false);
        core.set_shared_http_cache(cache.clone());
        assert_eq!(read(&core, &woke, &server, 2, "/a").0.body, b"shared");
        let mut request = server.request("/a");
        request.grants = Some("net.fetch https://other.test".into());
        core.run(job(3, request), None).unwrap();
        assert!(matches!(
            collect(&core, &woke).1,
            exact_runner::Outcome::Failed {
                kind: exact_runner::FailureKind::Refused,
                ..
            }
        ));
        // Fetch modes reach the shared cache before conversion to wire headers.
        server.plan(
            "/a",
            Plan::new(200, &[("Cache-Control", "max-age=60")], b"not stored"),
        );
        let request = server.request("/a").cache(CacheMode::NoStore);
        assert_eq!(
            crate::cache::tests::ask(&core, &woke, job(4, request))
                .0
                .body,
            b"not stored"
        );
        assert_eq!(read(&core, &woke, &server, 5, "/a").0.body, b"shared");
        server.plan(
            "/a",
            Plan::new(200, &[("Cache-Control", "max-age=60")], b"refreshed"),
        );
        let mut forced = job(6, server.request("/a"));
        forced.forced = true;
        assert_eq!(
            crate::cache::tests::ask(&core, &woke, forced).0.body,
            b"refreshed"
        );
        assert_eq!(read(&core, &woke, &server, 7, "/a").0.body, b"refreshed");
    }
    let (core, woke) = server.core_grants(false, false, "net.fetch https://other.test");
    core.set_shared_http_cache(cache);
    core.run(job(8, server.request("/a")), None).unwrap();
    assert!(matches!(
        collect(&core, &woke).1,
        exact_runner::Outcome::Failed {
            kind: exact_runner::FailureKind::Refused,
            ..
        }
    ));
    assert_eq!(server.count(), 3);
}

#[test]
fn repeated_wire_cache_controls_keep_every_restriction() {
    use crate::cache::tests::{read, Plan, Server};
    let server = Server::new();
    let (core, woke) = server.core(false, false);
    core.set_shared_http_cache(SharedHttpCache::new());
    for (index, restriction) in ["private", "no-store", "no-cache"].iter().enumerate() {
        let path = format!("/{index}");
        let headers = &[
            ("Cache-Control", *restriction),
            ("cache-control", "max-age=60"),
        ];
        server.plan(&path, Plan::new(200, headers, b"first"));
        server.plan(&path, Plan::new(200, headers, b"second"));
        assert_eq!(read(&core, &woke, &server, 1, &path).0.body, b"first");
        assert_eq!(
            read(&core, &woke, &server, 2, &path).0.body,
            b"second",
            "{restriction}"
        );
    }
    assert_eq!(server.count(), 6);
}

#[test]
fn repeated_wire_vary_fields_keep_every_selector_and_the_wildcard() {
    use crate::cache::tests::{ask, job, read, Plan, Server};
    let server = Server::new();
    let (core, woke) = server.core(false, false);
    core.set_shared_http_cache(SharedHttpCache::new());
    for selector in ["Accept-Language", "X-Mode"] {
        let headers = &[
            ("Cache-Control", "max-age=60"),
            ("Vary", "Accept-Language"),
            ("vary", "X-Mode"),
        ];
        let path = format!("/{selector}");
        server.plan(&path, Plan::new(200, headers, b"first"));
        server.plan(&path, Plan::new(200, headers, b"second"));
        assert_eq!(read(&core, &woke, &server, 1, &path).0.body, b"first");
        assert_eq!(read(&core, &woke, &server, 2, &path).0.body, b"first");
        let mut request = server.request(&path);
        request.headers.push((selector.into(), "different".into()));
        assert_eq!(ask(&core, &woke, job(3, request)).0.body, b"second");
    }
    let headers = &[
        ("Cache-Control", "max-age=60"),
        ("Vary", "*"),
        ("vary", "X-Mode"),
    ];
    server.plan("/star", Plan::new(200, headers, b"first"));
    server.plan("/star", Plan::new(200, headers, b"second"));
    assert_eq!(read(&core, &woke, &server, 1, "/star").0.body, b"first");
    assert_eq!(read(&core, &woke, &server, 2, "/star").0.body, b"second");
    assert_eq!(server.count(), 6);
}

#[test]
fn distinct_wire_targets_do_not_share_answers() {
    use crate::cache::tests::{read, Plan, Server};
    let server = Server::new();
    let (core, woke) = server.core(false, false);
    core.set_shared_http_cache(SharedHttpCache::new());
    let paths = ["/a/../b", "/b", "/a/%2e%2e/b", "/x/./y", "/x/y"];
    for path in paths {
        server.plan(
            path,
            Plan::new(200, &[("Cache-Control", "max-age=60")], path.as_bytes()),
        );
    }
    for path in paths {
        for ticket in 1..=2 {
            assert_eq!(
                read(&core, &woke, &server, ticket, path).0.body,
                path.as_bytes()
            );
        }
    }
    assert_eq!(server.count(), paths.len());
}
