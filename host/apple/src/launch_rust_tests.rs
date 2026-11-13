use super::*;
use crate::executor::PreloadEvent;
use exact_data::secrets::Backend;
use exact_runner::request::{Launch as Entry, LaunchContext, LaunchRequest};
use exact_runner::{Placement, Request};
#[path = "launch_rust_fixture.rs"]
mod fixture;
use fixture::{begin, composed, definitions, signal, source, Drive};
#[test]
fn rust_request_reaches_a_held_server_without_a_runner_and_boot_takes_it_once() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        for complete in [false, true] {
            let server = Server::new(1);
            let url = format!("http://{}/private-path?private-query", server.address);
            let caller = std::thread::current().id();
            let source = source(&server, move |ctx| {
                assert_ne!(caller, std::thread::current().id());
                ctx.preload(LaunchRequest::new("rust-feed", Request::get(&url)))
                    .unwrap();
            });
            let launch = fixture();
            begin(&launch, &source, false);
            server.request();
            assert_eq!(source.base.parsed.load(Ordering::Acquire), 0);
            if complete {
                server.release(0);
                ready(&launch);
            }
            let (mut drive, batch) = Drive::boot(&source, &plan());
            assert_eq!(batch.contains("\"text\":\"content\""), complete, "{batch}");
            server.release(0);
            drive.complete(&source);
            assert_eq!(server.count.load(Ordering::Acquire), 1);
            assert!(launch.mark(0).unwrap().1.committed > 0.);
            assert!(drive.logs().contains("Taken"));
        }
    }
}

#[test]
fn a_named_rust_preload_answers_only_the_apps_own_reads() {
    use exact_runner::CallOrigin;
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        for pending in [false, true] {
            let server = Server::new(4);
            let url = format!("http://{}/private-path?private-query", server.address);
            let source = source(&server, move |ctx| {
                ctx.preload(LaunchRequest::new("rust-feed", Request::get(&url)))
                    .unwrap();
            });
            let request = source
                .request
                .clone()
                .header("X-Trace", "new")
                .preload("rust-feed")
                .unwrap();
            let launch = fixture();
            begin(&launch, &source, false);
            server.request();
            if !pending {
                server.release(0);
                ready(&launch);
            }
            let control = &launch.0.set.get().unwrap().control;
            for origin in [CallOrigin::Tool, CallOrigin::Wake, CallOrigin::Device] {
                let mut other = request.clone();
                other.reach.origin = origin;
                assert!(control.settle(&read(1, other)).is_none());
            }
            let mut executor = Executing::new(&launch, &source.base.grants);
            for (index, origin) in [CallOrigin::Tool, CallOrigin::Wake, CallOrigin::Device]
                .into_iter()
                .enumerate()
            {
                let mut other = request.clone();
                other.reach.origin = origin;
                executor
                    .io
                    .run(read(index as u64 + 1, other), None)
                    .unwrap();
                server.request();
                server.release(index + 1);
                assert!(!executor.next().2.cached);
            }
            executor.io.run(read(4, request), None).unwrap();
            server.release(0);
            let (_, outcome, timing) = executor.next();
            assert!(timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"content"));
            assert_eq!(server.count.load(Ordering::Acquire), 4);
        }
    }
}

#[test]
fn invalid_rust_headers_and_urls_are_journaled_by_name_without_sending() {
    for (value, invalid_url) in [
        ("ok\nX-Other: yes", false),
        ("\rvalue", false),
        ("nul\0byte", false),
        (" leading", false),
        ("trailing\t", false),
        ("secret-value", true),
    ] {
        let server = Server::new(1);
        let url = format!("http://{}/private-path?private-query", server.address);
        let source = source(&server, move |ctx| {
            let request = if invalid_url {
                Request::get(&format!("http://[{value}"))
            } else {
                Request::get(&url).header("Authorization", value)
            };
            assert!(ctx
                .preload(LaunchRequest::new("rust-feed", request))
                .is_err());
        });
        let launch = fixture();
        begin(&launch, &source, false);
        definitions(&launch);
        let facts = launch.journal();
        let refused = facts.iter().find(|f| f.name == "rust-feed").unwrap();
        assert_eq!(
            refused.outcome,
            if invalid_url {
                PreloadEvent::InvalidUrl
            } else {
                PreloadEvent::InvalidHeader
            }
        );
        assert!(!format!("{facts:?}").contains(value));
        assert_eq!(server.count.load(Ordering::Acquire), 0);
        assert_eq!(launch.count(), 0);
    }
}

#[test]
fn token_refresh_is_durable_before_the_preload_uses_the_new_secret() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        use ibex2::secrets::SecretStore;
        let token = Server::new(1);
        let feed = Server::new(1);
        let directory = std::env::temp_dir().join(format!(
            "exact-launch-refresh-{}-{}",
            std::process::id(),
            feed.address.port()
        ));
        let grants = format!(
            "net.fetch http://{}\nnet.fetch http://{}\nsecret.keep session\nsecret.keep refresh",
            token.address, feed.address
        );
        let bindings = ibex2::host::Host::new()
            .with_secret_store(Box::new(ibex2::secrets::FileStore::new(directory.clone())))
            .endow(ibex2::grant::GrantSet::parse(&grants).unwrap());
        let backend = Arc::new(crate::store::launch_backend_in(
            bindings.secrets,
            Some(directory.clone()),
        ));
        backend.write("refresh", Some("old")).unwrap();
        let token_url = format!("http://{}/token", token.address);
        let feed_url = format!("http://{}/feed", feed.address);
        let mut source = source(&feed, move |ctx| {
            assert_eq!(ctx.secret("refresh").as_deref(), Some("old"));
            let reply = ctx.fetch(Request::get(&token_url)).unwrap();
            let value = String::from_utf8(reply.body).unwrap();
            ctx.set_secret("session", &value).unwrap();
            let mut request = Request::get(&feed_url);
            request.headers.push(("Authorization".into(), value));
            ctx.preload(LaunchRequest::new("rust-feed", request))
                .unwrap();
        });
        source.base.grants = grants;
        let launch = fixture();
        *launch.0.backend.lock().unwrap() = Some(backend.clone());
        begin(&launch, &source, false);
        assert!(token.request().contains("/token"));
        assert_eq!(feed.count.load(Ordering::Acquire), 0);
        token.release(0);
        assert!(feed
            .request()
            .to_ascii_lowercase()
            .contains("authorization: content"));
        assert_eq!(
            ibex2::secrets::FileStore::new(directory.clone())
                .get("session")
                .unwrap()
                .as_deref(),
            Some("content")
        );
        feed.release(0);
        ready(&launch);
        assert_eq!(launch.count(), 1);
        let mut executor = Executing::new(&launch, &source.base.grants);
        executor
            .io
            .run(
                read(
                    1,
                    Request::get(&format!("http://{}/feed", feed.address))
                        .preload("rust-feed")
                        .unwrap(),
                ),
                None,
            )
            .unwrap();
        assert!(executor.next().2.cached);
        launch.drop_secret("session");
        assert!(launch
            .journal()
            .iter()
            .any(|f| f.name == "rust-feed" && f.outcome == PreloadEvent::DroppedBySecret));
        drop(executor);
        drop(launch);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn an_app_secret_write_cancels_rust_work_and_leaves_the_public_manifest_preload() {
    for complete in [false, true] {
        let manifest = Server::new(1);
        let server = Server::new(1);
        let url = format!("http://{}/feed", server.address);
        let mut source = source(&server, move |ctx| {
            ctx.secret("session");
            ctx.preload(LaunchRequest::new("rust-feed", Request::get(&url)))
                .unwrap();
        });
        source.base.definitions = manifest.source(PreloadWhen::Start, false).definitions;
        source
            .base
            .grants
            .push_str(&format!("\nnet.fetch http://{}", manifest.address));
        source.manifest = true;
        let launch = fixture();
        begin(&launch, &source, false);
        manifest.request();
        server.request();
        manifest.release(0);
        if complete {
            server.release(0);
        }
        launch
            .0
            .set
            .get()
            .unwrap()
            .control
            .wait_ready(if complete { 2 } else { 1 });
        let bytes = contract::compile("shape Reply\n  value: string\ncomponent App\n  resource result = rotate() as shape Reply\n  resource feed = public() as shape Reply\n  view\n    text feed.value\n").unwrap().encode();
        let (mut drive, batch) = Drive::boot(&source, &bytes);
        assert!(batch.contains("\"text\":\"content\""), "{batch}");
        server.release(0);
        let journal = format!("{:?} {}", launch.journal(), drive.logs());
        assert!(
            journal.contains("rust-feed") && journal.contains("DroppedBySecret"),
            "{journal}"
        );
        assert!(journal.contains("Taken"), "{journal}");
        assert_eq!(manifest.count.load(Ordering::Acquire), 1);
        assert_eq!(server.count.load(Ordering::Acquire), 1);
    }
}

#[test]
fn a_manifest_start_leaves_while_rust_fetch_is_blocked_and_boot_does_not_wait() {
    for equal in [true, false] {
        let manifest = Server::new(1);
        let token = Server::new(1);
        let server = Server::new(1);
        let token_url = format!("http://{}/token", token.address);
        let feed_url = format!("http://{}/private-path?private-query", server.address);
        let mut source = source(&server, move |ctx| {
            ctx.fetch(Request::get(&token_url)).unwrap();
            if equal {
                ctx.preload(LaunchRequest::new("rust-feed", Request::get(&feed_url)))
                    .unwrap();
            }
        });
        source.base.definitions = manifest.source(PreloadWhen::Start, false).definitions;
        source.base.grants.push_str(&format!(
            "\nnet.fetch http://{}\nnet.fetch http://{}",
            token.address, manifest.address
        ));
        source.manifest = true;
        source.request = source.request.preload("rust-feed").unwrap();
        let launch = fixture();
        begin(&launch, &source, false);
        token.request();
        manifest.request();
        let (mut drive, batch) = Drive::boot(&source, &plan());
        assert!(!batch.contains("\"text\":\"content\""));
        assert_eq!(server.count.load(Ordering::Acquire), 0);
        token.release(0);
        server.request();
        server.release(0);
        manifest.release(0);
        drive.complete(&source);
        assert_eq!(server.count.load(Ordering::Acquire), 1);
        assert_eq!(launch.count(), if equal { 2 } else { 1 });
        let journal = drive.logs();
        assert_eq!(journal.contains("rust-feed"), equal, "{journal}");
    }
}

#[test]
fn manifest_foreground_leaves_at_opened_while_the_function_is_blocked_in_fetch() {
    for opened in 0..3 {
        let manifest = Server::new(1);
        let token = Server::new(1);
        let (fetched, fetching) = channel();
        let token_url = format!("http://{}/token", token.address);
        let mut source = source(&manifest, move |ctx| {
            let result = ctx.fetch(Request::get(&token_url));
            fetched.send(result).unwrap();
        });
        source.base.definitions = manifest.source(PreloadWhen::Foreground, false).definitions;
        source
            .base
            .grants
            .push_str(&format!("\nnet.fetch http://{}", token.address));
        source.manifest = true;
        let launch = fixture();
        if opened == 0 {
            signal(&launch, &source, true);
        } else {
            begin(&launch, &source, false);
        }
        token.request();
        let running = launch.0.rust.get().unwrap();
        if opened == 2 {
            running.expire_now();
        }
        if opened != 0 {
            assert_eq!(manifest.count.load(Ordering::Acquire), 0);
            signal(&launch, &source, true);
        }
        manifest.request();
        if opened == 2 {
            assert_eq!(
                fetching.recv_timeout(HANG).unwrap(),
                Err("launch deadline".into())
            );
        } else {
            assert!(
                fetching.try_recv().is_err(),
                "function's fetch is still held"
            );
        }
        manifest.release(0);
        token.release(0);
        running.wait_returned();
        definitions(&launch);
        ready(&launch);
        assert_eq!(manifest.count.load(Ordering::Acquire), 1);
    }
}

#[test]
fn a_launch_past_its_deadline_releases_reads_and_discards_late_requests() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        let server = Server::new(1);
        let (entered, entering) = channel();
        let (release, released) = channel();
        let released = Mutex::new(released);
        let url = format!("http://{}/private-path?private-query", server.address);
        let source = source(&server, move |ctx| {
            entered.send(()).unwrap();
            released.lock().unwrap().recv_timeout(HANG).unwrap();
            assert_eq!(
                ctx.preload(LaunchRequest::new("too-late", Request::get(&url))),
                Err("launch deadline".into())
            );
        });
        let launch = fixture();
        let (reading, reading_started) = channel();
        let (allow, allowed) = channel();
        let allowed = Mutex::new(allowed);
        *launch.0.reader.lock().unwrap() = Some(Arc::new(move |_, _, _| {
            reading.send(()).unwrap();
            allowed.lock().unwrap().recv_timeout(HANG).unwrap();
            BTreeMap::new()
        }));
        begin(&launch, &source, false);
        reading_started.recv_timeout(HANG).unwrap();
        let control = &launch.0.set.get().unwrap().control;
        let now = Arc::new(Mutex::new(std::time::Instant::now()));
        let clock = now.clone();
        control.test_clock(Arc::new(move || *clock.lock().unwrap()));
        control.definitions_pending(true);
        // Adoption arms the definition timer before Rust replaces that wait (LLP 1162.000 D2).
        let mut executor = Executing::new(&launch, &source.base.grants);
        allow.send(()).unwrap();
        entering.recv_timeout(HANG).unwrap();
        executor
            .io
            .run(
                read(1, source.request.clone().preload("feed").unwrap()),
                None,
            )
            .unwrap();
        *now.lock().unwrap() += Duration::from_secs(1);
        assert!(
            control.test_tick(),
            "reading definitions cannot end Rust's wait"
        );
        let running = launch.0.rust.get().unwrap();
        running.expire_now();
        server.request();
        server.release(0);
        let (ticket, outcome, timing) = executor.next();
        assert_eq!(ticket, 1);
        assert!(!timing.cached);
        assert!(matches!(outcome, Outcome::Response(r) if r.body == b"content"));
        release.send(()).unwrap();
        running.wait_returned();
        definitions(&launch);
        assert_eq!(launch.count(), 0);
        assert!(format!("{:?}", launch.journal()).contains("LaunchDeadline"));
        assert_eq!(server.count.load(Ordering::Acquire), 1);
    }
}

#[test]
fn a_panic_records_only_the_crate_name_and_leaves_manifest_work_running() {
    let server = Server::new(1);
    let mut source = source(&server, |_| panic!("private-panic-secret"));
    source.manifest = true;
    let launch = fixture();
    begin(&launch, &source, false);
    server.request();
    definitions(&launch);
    let journal = format!("{:?}", launch.journal());
    assert!(
        journal.contains("launch-data") && journal.contains("LaunchPanicked"),
        "{journal}"
    );
    assert!(!journal.contains("private-panic-secret"));
    server.release(0);
    ready(&launch);
    assert_eq!(launch.count(), 1);
}

#[test]
fn rust_requests_obey_grants_validation_unique_names_and_the_total_of_eight() {
    let server = Server::new(8);
    let url = format!("http://{}/feed", server.address);
    let mut source = source(&server, move |ctx| {
        let mut body = Request::get(&url);
        body.body = b"not allowed".to_vec();
        let mut requests = vec![
            LaunchRequest::new("outside", Request::get("http://127.0.0.1:1/private?secret")),
            LaunchRequest::new("get-body", body),
            LaunchRequest::new("feed", Request::get(&url)),
        ];
        for (name, mut request) in [
            (
                "userinfo",
                Request::get(&url.replace("http://", "http://user:password@")),
            ),
            ("fragment", Request::get(&format!("{url}#private"))),
            (
                "empty-userinfo",
                Request::get(&url.replace("http://", "http://@")),
            ),
            ("relative", Request::get("/feed")),
            ("method", Request::get(&url)),
            ("head-body", Request::get(&url)),
            ("oversized", Request::get(&url)),
            ("header", Request::get(&url)),
            ("repeated-header", Request::get(&url)),
            ("header-value", Request::get(&url)),
            ("BadName", Request::get(&url)),
        ] {
            match name {
                "method" => request.method = "bad method".into(),
                "head-body" => {
                    request.method = "HEAD".into();
                    request.body = vec![1];
                }
                "oversized" => {
                    request.method = "POST".into();
                    request.body = vec![0; (64 << 10) + 1];
                }
                "header" => request.headers.push(("Host".into(), "example.com".into())),
                "repeated-header" => {
                    request.headers = vec![
                        ("Accept".into(), "one".into()),
                        ("accept".into(), "two".into()),
                    ]
                }
                "header-value" => request
                    .headers
                    .push(("X-Test".into(), "private\r\ninjected".into())),
                _ => {}
            }
            requests.push(LaunchRequest::new(name, request));
        }
        requests
            .extend((0..8).map(|i| LaunchRequest::new(format!("rust-{i}"), Request::get(&url))));
        for request in requests {
            let _ = ctx.preload(request);
        }
    });
    source.manifest = true;
    let launch = fixture();
    begin(&launch, &source, false);
    for _ in 0..8 {
        server.request();
    }
    definitions(&launch);
    let journal = launch.journal();
    for name in [
        "outside",
        "get-body",
        "feed",
        "rust-7",
        "userinfo",
        "fragment",
        "empty-userinfo",
        "method",
        "head-body",
        "oversized",
        "header",
        "repeated-header",
    ] {
        assert!(
            journal.iter().any(|f| f.name == name
                && matches!(
                    f.outcome,
                    PreloadEvent::Refused | PreloadEvent::RefusedGrant
                )),
            "{name}: {journal:?}"
        );
    }
    assert!(journal
        .iter()
        .any(|f| f.name == "launch-data" && f.outcome == PreloadEvent::InvalidName));
    assert!(!journal.iter().any(|f| f.name == "BadName"));
    for (name, outcome) in [
        ("relative", PreloadEvent::InvalidUrl),
        ("header-value", PreloadEvent::InvalidHeader),
    ] {
        assert!(
            journal
                .iter()
                .any(|f| f.name == name && f.outcome == outcome),
            "{journal:?}"
        );
    }
    assert_eq!(server.count.load(Ordering::Acquire), 8);
    assert_eq!(launch.count(), 8);
    for i in 0..8 {
        server.release(i);
    }
}

#[test]
fn prewarming_runs_rust_only_at_launched_and_foreground_requests_wait_for_opened() {
    let server = Server::new(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let called = calls.clone();
    let url = format!("http://{}/feed", server.address);
    let source = source(&server, move |ctx| {
        called.fetch_add(1, Ordering::AcqRel);
        let mut request = LaunchRequest::new("foreground", Request::get(&url));
        request.when = PreloadWhen::Foreground;
        ctx.preload(request).unwrap();
    });
    let launch = fixture();
    begin(&launch, &source, true);
    assert_eq!(calls.load(Ordering::Acquire), 0);
    signal(&launch, &source, false);
    definitions(&launch);
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert_eq!(server.count.load(Ordering::Acquire), 0);
    signal(&launch, &source, true);
    signal(&launch, &source, false);
    begin(&launch, &source, false);
    server.request();
    assert_eq!(calls.load(Ordering::Acquire), 1);
    server.release(0);
}

#[test]
fn context_reads_each_plain_kept_secret_once_and_refuses_guarded_or_ungranted_access() {
    struct Backend {
        reads: Mutex<Vec<String>>,
        values: Mutex<BTreeMap<String, String>>,
    }
    impl exact_data::secrets::Backend for Backend {
        fn read(&self, name: &str) -> Result<Option<String>, String> {
            self.reads.lock().unwrap().push(name.into());
            if name == "locked" {
                return Err("before first unlock".into());
            }
            Ok(self.values.lock().unwrap().get(name).cloned())
        }
        fn write(&self, name: &str, value: Option<&str>) -> Result<(), String> {
            let mut values = self.values.lock().unwrap();
            match value {
                Some(value) => {
                    values.insert(name.into(), value.into());
                }
                None => {
                    values.remove(name);
                }
            }
            Ok(())
        }
    }
    let server = Server::new(0);
    let backend = Arc::new(Backend {
        reads: Default::default(),
        values: Mutex::new(BTreeMap::from([
            ("session".into(), "old".into()),
            ("guarded".into(), "private".into()),
        ])),
    });
    let mut source = source(&server, |ctx| {
        for _ in 0..2 {
            assert_eq!(ctx.secret("session").as_deref(), Some("old"));
        }
        for name in ["missing", "locked", "guarded", "ungranted"] {
            assert_eq!(ctx.secret(name), None);
        }
        for name in ["guarded", "ungranted"] {
            assert!(ctx.set_secret(name, "no").is_err());
            assert!(ctx.forget_secret(name).is_err());
        }
        ctx.set_secret("session", "new").unwrap();
        assert_eq!(ctx.secret("session").as_deref(), Some("new"));
        ctx.forget_secret("session").unwrap();
        assert_eq!(ctx.secret("session"), None);
    });
    source
        .base
        .grants
        .push_str("\nsecret.keep missing\nsecret.keep locked\nsecret.keep guarded user-presence");
    let launch = fixture();
    *launch.0.backend.lock().unwrap() = Some(backend.clone());
    begin(&launch, &source, false);
    definitions(&launch);
    assert_eq!(
        *backend.reads.lock().unwrap(),
        ["locked", "missing", "session"]
    );
    assert!(!backend.values.lock().unwrap().contains_key("session"));
    assert_eq!(
        backend.values.lock().unwrap().get("guarded").unwrap(),
        "private"
    );
}

#[test]
fn an_app_write_of_a_read_secret_refuses_the_rust_preload() {
    let server = Server::new(0);
    let (entered, entering) = channel();
    let (release, released) = channel();
    let released = Mutex::new(released);
    let url = format!("http://{}/private-path?private-query", server.address);
    let source = source(&server, move |ctx| {
        ctx.secret("session");
        entered.send(()).unwrap();
        released.lock().unwrap().recv_timeout(HANG).unwrap();
        assert_eq!(
            ctx.preload(LaunchRequest::new("rust-feed", Request::get(&url))),
            Err("launch preload secret changed".into())
        );
    });
    let launch = fixture();
    begin(&launch, &source, false);
    entering.recv_timeout(HANG).unwrap();
    launch.drop_secret("session");
    release.send(()).unwrap();
    definitions(&launch);
    let journal = launch.journal();
    assert!(
        journal
            .iter()
            .any(|f| f.name == "rust-feed" && f.outcome == PreloadEvent::DroppedBySecret),
        "{journal:?}"
    );
    assert_eq!(server.count.load(Ordering::Acquire), 0);
}

#[test]
fn context_fetch_neither_reads_nor_populates_the_preload_cache() {
    let server = Server::responding(vec![b"HTTP/1.1 200 OK\r\nCache-Control: max-age=3600\r\nContent-Length: 7\r\nConnection: close\r\n\r\ncontent".to_vec(); 3]);
    let url = format!("http://{}/private-path?private-query", server.address);
    let source = source(&server, move |ctx| {
        for _ in 0..2 {
            assert_eq!(ctx.fetch(Request::get(&url)).unwrap().body, b"content");
        }
    });
    let launch = fixture();
    begin(&launch, &source, false);
    for i in 0..2 {
        server.request();
        server.release(i);
    }
    definitions(&launch);
    let (mut drive, batch) = Drive::boot(&source, &plan());
    assert!(!batch.contains("\"text\":\"content\""));
    server.request();
    server.release(2);
    drive.complete(&source);
    assert_eq!(server.count.load(Ordering::Acquire), 3);
}

#[test]
fn context_fetch_checks_the_binary_grants_again_at_a_redirect() {
    let outside = Server::new(0);
    let response = format!("HTTP/1.1 302 Found\r\nLocation: http://{}/private?secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", outside.address);
    let server = Server::responding(vec![response.into_bytes()]);
    let url = format!("http://{}/redirect", server.address);
    let source = source(&server, move |ctx| {
        assert!(ctx.fetch(Request::get(&url)).is_err());
    });
    let launch = fixture();
    begin(&launch, &source, false);
    server.request();
    server.release(0);
    definitions(&launch);
    assert_eq!(outside.count.load(Ordering::Acquire), 0);
    assert_eq!(launch.count(), 0);
    let journal = launch.journal();
    assert!(
        journal
            .iter()
            .any(|f| matches!(f.outcome, PreloadEvent::LaunchRan { requests: 0, .. })),
        "{journal:?}"
    );
}

struct Executing {
    io: Box<dyn Io>,
    _wake: Box<Sender<()>>,
    wakes: Receiver<()>,
    ready: std::collections::VecDeque<(u64, Outcome, Option<exact_runner::CompletionTiming>)>,
}
impl Executing {
    fn new(launch: &LaunchPreloads, grants: &str) -> Self {
        extern "C" fn wake(ctx: *mut std::ffi::c_void) {
            // SAFETY: Executing retains the sender until the executor retires.
            #[allow(unsafe_code)]
            let sender = unsafe { &*ctx.cast::<Sender<()>>() };
            let _ = sender.send(());
        }
        let (send, wakes) = channel();
        let sender = Box::new(send);
        let io = start_io(
            memory_endow(grants, "", false).bindings,
            grants,
            Some((wake, (&*sender as *const Sender<()>).cast_mut().cast())),
        );
        launch.adopt(&*io);
        Self {
            io,
            _wake: sender,
            wakes,
            ready: Default::default(),
        }
    }
    fn next(&mut self) -> (u64, Outcome, exact_runner::CompletionTiming) {
        loop {
            if let Some((ticket, outcome, timing)) = self.ready.pop_front() {
                return (ticket, outcome, timing.unwrap());
            }
            self.io.begin_pump();
            self.ready.extend(self.io.drain());
            if self.ready.is_empty() {
                self.wakes.recv_timeout(HANG).unwrap();
            }
        }
    }
}
fn read(ticket: u64, request: Request) -> exact_runner::RequestOut {
    exact_runner::RequestOut {
        ticket,
        target: "feed".into(),
        request,
        class: exact_runner::RequestClass::Read,
        forced: false,
    }
}

#[test]
fn a_rust_redirected_reply_requires_the_readers_scope_at_every_hop() {
    for asynchronous in [false, true] {
        for named in [false, true] {
            ASYNCHRONOUS.with(|v| v.set(asynchronous));
            for pending in [false, true] {
                let final_hop = Server::new(1);
                let redirect = format!("HTTP/1.1 302 Found\r\nLocation: http://{}/final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", final_hop.address).into_bytes();
                let middle = Server::responding(vec![redirect]);
                let redirect = format!("HTTP/1.1 302 Found\r\nLocation: http://{}/middle\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", middle.address).into_bytes();
                let server = Server::responding(vec![redirect.clone(), redirect]);
                let url = format!("http://{}/private-path?private-query", server.address);
                let mut source = source(&server, move |ctx| {
                    ctx.preload(LaunchRequest::new("rust-feed", Request::get(&url)))
                        .unwrap();
                });
                source.base.grants.push_str(&format!(
                    "\nnet.fetch http://{}\nnet.fetch http://{}",
                    middle.address, final_hop.address
                ));
                let launch = fixture();
                begin(&launch, &source, false);
                server.request();
                server.release(0);
                middle.request();
                middle.release(0);
                final_hop.request();
                if !pending {
                    final_hop.release(0);
                    ready(&launch);
                }
                let request = if named {
                    source
                        .request
                        .clone()
                        .header("X-Trace", "new")
                        .preload("rust-feed")
                        .unwrap()
                } else {
                    source.request.clone()
                };
                let mut restricted = request.clone();
                restricted.grants = Some(format!(
                    "net.fetch http://{}\nnet.fetch http://{}",
                    server.address, final_hop.address
                ));
                assert!(launch
                    .0
                    .set
                    .get()
                    .unwrap()
                    .control
                    .settle(&read(1, restricted.clone()))
                    .is_none());
                let mut executor = Executing::new(&launch, &source.base.grants);
                executor.io.run(read(1, restricted), None).unwrap();
                if pending {
                    final_hop.release(0);
                }
                server.request();
                server.release(1);
                let (_, outcome, timing) = executor.next();
                assert!(!timing.cached);
                assert!(
                    matches!(
                        outcome,
                        Outcome::Failed {
                            kind: exact_runner::FailureKind::Refused,
                            ..
                        }
                    ),
                    "{outcome:?}"
                );
                executor.io.run(read(2, request), None).unwrap();
                let (_, outcome, timing) = executor.next();
                assert!(timing.cached);
                assert!(matches!(outcome, Outcome::Response(r) if r.body == b"content"));
                assert_eq!(server.count.load(Ordering::Acquire), 2);
                assert_eq!(final_hop.count.load(Ordering::Acquire), 1);
                assert_eq!(middle.count.load(Ordering::Acquire), 1);
            }
        }
    }
}

#[test]
fn an_equal_refresh_drops_a_rust_reply_and_cancels_its_exchange() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        for pending in [false, true] {
            for mode in 0..3 {
                let old = b"HTTP/1.1 503 Unavailable\r\nContent-Length: 3\r\nConnection: close\r\n\r\nold".to_vec();
                let new = b"HTTP/1.1 503 Unavailable\r\nContent-Length: 3\r\nConnection: close\r\n\r\nnew".to_vec();
                let server = Server::responding(vec![old, new.clone(), new.clone(), new]);
                let url = format!("http://{}/private-path?private-query", server.address);
                let source = source(&server, move |ctx| {
                    ctx.preload(LaunchRequest::new("rust-feed", Request::get(&url)))
                        .unwrap();
                });
                let launch = fixture();
                begin(&launch, &source, false);
                server.request();
                if !pending {
                    server.release(0);
                    ready(&launch);
                }
                for response in 1..4 {
                    server.release(response);
                }
                let mut executor = Executing::new(&launch, &source.base.grants);
                if pending {
                    executor
                        .io
                        .run(read(1, source.request.clone()), None)
                        .unwrap();
                }
                let mut refresh = read(2, source.request.clone());
                match mode {
                    0 => refresh.forced = true,
                    1 => refresh.request.cache = exact_runner::CacheMode::Reload,
                    _ => refresh.request.cache = exact_runner::CacheMode::NoCache,
                }
                executor.io.run(refresh, None).unwrap();
                assert!(launch
                    .journal()
                    .iter()
                    .any(|f| f.name == "rust-feed" && f.outcome == PreloadEvent::DroppedByRefresh));
                server.release(0);
                for ticket in if pending { vec![1, 2] } else { vec![2] } {
                    server.request();
                    let (got, outcome, timing) = executor.next();
                    assert_eq!(got, ticket);
                    assert!(!timing.cached);
                    assert!(matches!(outcome, Outcome::Response(r) if r.body == b"new"));
                }
                executor
                    .io
                    .run(read(3, source.request.clone()), None)
                    .unwrap();
                server.request();
                let (_, outcome, timing) = executor.next();
                assert!(!timing.cached);
                assert!(matches!(outcome, Outcome::Response(r) if r.body == b"new"));
                assert_eq!(
                    server.count.load(Ordering::Acquire),
                    if pending { 4 } else { 3 }
                );
            }
        }
    }
}

#[test]
fn boot_settles_rust_replies_once_in_ticket_order() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        for reask in [false, true] {
            let server = Server::new(if reask { 1 } else { 2 });
            let url = format!("http://{}/private-path?private-query", server.address);
            let mut source = source(&server, move |ctx| {
                ctx.preload(LaunchRequest::new("rust-feed", Request::get(&url)))
                    .unwrap();
            });
            source.base.reask = reask;
            source.base.first_unloaded = !reask;
            let launch = fixture();
            begin(&launch, &source, false);
            server.request();
            server.release(0);
            ready(&launch);
            let bytes = if reask {
                plan()
            } else {
                contract::compile("shape Reply\n  value: string\ncomponent App\n  resource first = first() as shape Reply\n  resource second = read() as shape Reply\n  view\n    column\n      text first.value\n      text second.value\n").unwrap().encode()
            };
            let (mut drive, batch) = Drive::boot(&source, &bytes);
            assert_eq!(
                source.base.parsed.load(Ordering::Acquire),
                usize::from(reask)
            );
            assert!(!batch.contains("\"text\":\"content\""), "{batch}");
            if reask {
                let size = drive.bridge.input_write(br#"{"op":"outstanding"}"#);
                let size = drive.bridge.agent(size);
                assert!(
                    String::from_utf8_lossy(drive.bridge.output_bytes(size as usize))
                        .contains("feed")
                );
            } else {
                assert!(server.request().contains("/first"));
                server.release(1);
                while source.base.parsed.load(Ordering::Acquire) < 2 {
                    drive.wakes.recv_timeout(HANG).unwrap();
                    drive.bridge.pump(0.);
                }
                assert_eq!(*source.base.order.lock().unwrap(), ["first", "read"]);
            }
        }
    }
}

#[test]
fn a_timeline_neither_adopts_rust_preloads_nor_runs_the_function() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        let server = Server::new(2);
        let calls = Arc::new(AtomicUsize::new(0));
        let called = calls.clone();
        let url = format!("http://{}/private-path?private-query", server.address);
        let source = source(&server, move |ctx| {
            called.fetch_add(1, Ordering::AcqRel);
            ctx.preload(LaunchRequest::new("rust-feed", Request::get(&url)))
                .unwrap();
        });
        let launch = fixture();
        begin(&launch, &source, false);
        server.request();
        server.release(0);
        ready(&launch);
        server.release(1);
        let bytes = widget_plan();
        let mut entry = Bridge::with_links(Links {
            io: Some(IoLinks {
                preloads: local,
                endow: memory_endow,
                start: start_io,
                ..IoLinks::LINKED
            }),
            ..Links::ALL
        });
        entry.input_write(&[bytes.as_slice(), b"feed-widget"].concat());
        entry.boot_entry(
            bytes.len(),
            "feed-widget".len(),
            0,
            0,
            composed(source.clone()),
            Hooks::none(),
            100.,
            100.,
        );
        let size = entry.entry_timeline(1000.);
        let result = String::from_utf8(entry.output_bytes(size as usize).to_vec()).unwrap();
        assert!(result.contains("timeline"), "{result}");
        assert_eq!(calls.load(Ordering::Acquire), 1);
        assert!(launch.0.set.get().unwrap().owner.lock().unwrap().is_some());
        server.request();
        let (_drive, batch) = Drive::boot(&source, &plan());
        assert!(batch.contains("\"text\":\"content\""), "{batch}");
        assert_eq!(calls.load(Ordering::Acquire), 1);
        assert_eq!(server.count.load(Ordering::Acquire), 2);
    }
}

#[test]
fn a_function_secret_write_drops_a_request_built_with_the_old_value() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        for complete in [false, true] {
            let server = Server::new(2);
            let (release, released) = channel();
            let released = Mutex::new(released);
            let url = format!("http://{}/feed", server.address);
            let source = source(&server, move |ctx| {
                let old = ctx.secret("session").unwrap();
                let request = Request::get(&url).header("Authorization", &old);
                ctx.preload(LaunchRequest::new("feed", request)).unwrap();
                released.lock().unwrap().recv_timeout(HANG).unwrap();
                ctx.set_secret("session", "B").unwrap();
            });
            let launch = fixture();
            launch
                .0
                .backend
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .write("session", Some("A"))
                .unwrap();
            begin(&launch, &source, false);
            assert!(server
                .request()
                .to_ascii_lowercase()
                .contains("authorization: a"));
            if complete {
                server.release(0);
                ready(&launch);
            }
            release.send(()).unwrap();
            launch.0.rust.get().unwrap().wait_returned();
            let journal = launch.journal();
            assert!(
                journal
                    .iter()
                    .any(|f| f.name == "feed" && f.outcome == PreloadEvent::DroppedBySecret),
                "{journal:?}"
            );
            let mut executor = Executing::new(&launch, &source.base.grants);
            let request = Request::get(&format!("http://{}/feed", server.address))
                .header("Authorization", "B")
                .preload("feed")
                .unwrap();
            executor.io.run(read(1, request), None).unwrap();
            assert!(server
                .request()
                .to_ascii_lowercase()
                .contains("authorization: b"));
            server.release(0);
            server.release(1);
            assert!(!executor.next().2.cached);
            assert_eq!(server.count.load(Ordering::Acquire), 2);
        }
    }
}

struct HeldWrite {
    value: Mutex<String>,
    entered: Sender<()>,
    release: Mutex<Receiver<()>>,
}
impl Backend for HeldWrite {
    fn read(&self, _: &str) -> Result<Option<String>, String> {
        Ok(Some(self.value.lock().unwrap().clone()))
    }
    fn write(&self, _: &str, value: Option<&str>) -> Result<(), String> {
        *self.value.lock().unwrap() = value.unwrap().into();
        if value == Some("A") {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv_timeout(HANG).unwrap();
        }
        Ok(())
    }
}

#[test]
fn a_handoff_waiting_for_secret_persistence_is_refused_at_the_launch_deadline() {
    let server = Server::new(1);
    let (write_entered, write_entering) = channel();
    let (release, released) = channel();
    let backend = Arc::new(HeldWrite {
        value: Mutex::new("old".into()),
        entered: write_entered,
        release: Mutex::new(released),
    });
    let (entered, entering) = channel();
    let (handoff, handing) = channel();
    let handing = Mutex::new(handing);
    let (answer, answered) = channel();
    let url = format!("http://{}/feed", server.address);
    let source = source(&server, move |ctx| {
        entered.send(()).unwrap();
        handing.lock().unwrap().recv_timeout(HANG).unwrap();
        answer
            .send(ctx.preload(LaunchRequest::new("late", Request::get(&url))))
            .unwrap();
    });
    let launch = fixture();
    *launch.0.backend.lock().unwrap() = Some(backend.clone());
    begin(&launch, &source, false);
    entering.recv_timeout(HANG).unwrap();
    let now = Arc::new(Mutex::new(std::time::Instant::now()));
    let clock = now.clone();
    let (checking, checked) = channel();
    let running = launch.0.rust.get().unwrap();
    running.test_clock(Arc::new(move || {
        let now = *clock.lock().unwrap();
        checking.send(()).unwrap();
        now
    }));
    let app = launch.clone();
    let stored = backend.clone();
    let writer = std::thread::spawn(move || {
        app.with_secret_write("session", || stored.write("session", Some("A")))
    });
    write_entering.recv_timeout(HANG).unwrap();
    assert!(launch.0.invalidated.try_lock().is_err());
    handoff.send(()).unwrap();
    checked.recv_timeout(HANG).unwrap();
    *now.lock().unwrap() += rust::LAUNCH_DEADLINE;
    release.send(()).unwrap();
    writer.join().unwrap().unwrap();
    let result = answered.recv_timeout(HANG).unwrap();
    running.wait_returned();
    definitions(&launch);
    assert_eq!(result, Err("launch deadline".into()));
    assert_eq!(launch.count(), 0);
    assert_eq!(server.count.load(Ordering::Acquire), 0);
    let journal = launch.journal();
    assert!(journal
        .iter()
        .any(|f| f.name == "late" && f.outcome == PreloadEvent::Refused));
    assert!(journal
        .iter()
        .any(|f| matches!(f.outcome, PreloadEvent::LaunchDeadline { .. })));
    assert!(!journal.iter().any(|f| matches!(
        f.outcome,
        PreloadEvent::Sent | PreloadEvent::LaunchPanicked { .. }
    )));
}

#[test]
fn launch_secret_persistence_holds_the_version_lock_against_the_app_writer() {
    let server = Server::new(0);
    let (entered, entering) = channel();
    let (release, released) = channel();
    let backend = Arc::new(HeldWrite {
        value: Mutex::new("old".into()),
        entered,
        release: Mutex::new(released),
    });
    let (written, writing) = channel();
    let (finish, finished) = channel();
    let finished = Mutex::new(finished);
    let url = format!("http://{}/feed", server.address);
    let source = source(&server, move |ctx| {
        ctx.set_secret("session", "A").unwrap();
        written.send(()).unwrap();
        finished.lock().unwrap().recv_timeout(HANG).unwrap();
        assert_eq!(ctx.secret("session").as_deref(), Some("A"));
        assert_eq!(
            ctx.preload(LaunchRequest::new("stale", Request::get(&url))),
            Err("launch preload secret changed".into())
        );
    });
    let launch = fixture();
    *launch.0.backend.lock().unwrap() = Some(backend.clone());
    begin(&launch, &source, false);
    entering.recv_timeout(HANG).unwrap();
    let serialized = launch.0.invalidated.try_lock().is_err();
    let app = launch.clone();
    let stored = backend.clone();
    let writer = std::thread::spawn(move || {
        app.with_secret_write("session", || stored.write("session", Some("B")))
    });
    release.send(()).unwrap();
    writing.recv_timeout(HANG).unwrap();
    writer.join().unwrap().unwrap();
    finish.send(()).unwrap();
    definitions(&launch);
    assert!(
        serialized,
        "persistence and version publication must share the writer lock"
    );
    assert_eq!(backend.read("session").unwrap().as_deref(), Some("B"));
    let journal = launch.journal();
    assert!(
        journal
            .iter()
            .any(|f| f.name == "stale" && f.outcome == PreloadEvent::DroppedBySecret),
        "{journal:?}"
    );
    assert!(!journal
        .iter()
        .any(|f| matches!(f.outcome, PreloadEvent::LaunchPanicked { .. })));
}

#[test]
fn a_handed_preload_reaches_the_server_during_a_slow_fetch_and_survives_the_launch_deadline() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        for foreground in [false, true] {
            let feed = Server::new(1);
            let token = Server::new(1);
            let feed_url = format!("http://{}/feed", feed.address);
            let token_url = format!("http://{}/token", token.address);
            let mut source = source(&feed, move |ctx| {
                let mut request = LaunchRequest::new("early", Request::get(&feed_url));
                if foreground {
                    request.when = PreloadWhen::Foreground;
                }
                ctx.preload(request).unwrap();
                assert_eq!(
                    ctx.fetch(Request::get(&token_url)),
                    Err("launch deadline".into())
                );
                assert_eq!(
                    ctx.preload(LaunchRequest::new("late", Request::get(&feed_url))),
                    Err("launch deadline".into())
                );
            });
            source
                .base
                .grants
                .push_str(&format!("\nnet.fetch http://{}", token.address));
            let launch = fixture();
            begin(&launch, &source, false);
            token.request();
            if foreground {
                assert_eq!(feed.count.load(Ordering::Acquire), 0);
                signal(&launch, &source, true);
            }
            feed.request();
            let running = launch.0.rust.get().unwrap();
            running.expire_now();
            feed.release(0);
            ready(&launch);
            token.release(0);
            running.wait_returned();
            assert_eq!(launch.count(), 1);
            assert!(launch
                .journal()
                .iter()
                .any(|f| f.name == "late" && f.outcome == PreloadEvent::Refused));
            let mut executor = Executing::new(&launch, &source.base.grants);
            executor
                .io
                .run(
                    read(
                        1,
                        Request::get(&format!("http://{}/feed", feed.address))
                            .preload("early")
                            .unwrap(),
                    ),
                    None,
                )
                .unwrap();
            assert!(executor.next().2.cached);
            assert_eq!(feed.count.load(Ordering::Acquire), 1);
        }
    }
}

#[test]
fn a_panic_cancels_the_functions_handed_and_foreground_requests() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        let server = Server::new(2);
        let url = format!("http://{}/feed", server.address);
        let (release, released) = channel();
        let released = Mutex::new(released);
        let source = source(&server, move |ctx| {
            ctx.preload(LaunchRequest::new("early", Request::get(&url)))
                .unwrap();
            let mut foreground = LaunchRequest::new("foreground", Request::get(&url));
            foreground.when = PreloadWhen::Foreground;
            ctx.preload(foreground).unwrap();
            released.lock().unwrap().recv_timeout(HANG).unwrap();
            panic!("private-panic");
        });
        let launch = fixture();
        begin(&launch, &source, false);
        server.request();
        release.send(()).unwrap();
        launch.0.rust.get().unwrap().wait_returned();
        signal(&launch, &source, true);
        assert!(launch.0.foreground.lock().unwrap().is_empty());
        let mut executor = Executing::new(&launch, &source.base.grants);
        executor
            .io
            .run(
                read(1, Request::get(&format!("http://{}/feed", server.address))),
                None,
            )
            .unwrap();
        server.request();
        server.release(0);
        server.release(1);
        assert!(!executor.next().2.cached);
        assert_eq!(server.count.load(Ordering::Acquire), 2);
    }
}

#[path = "launch_secret_tests.rs"]
mod secrets;

#[test]
fn an_invalid_handoff_name_is_journaled_only_by_crate_name() {
    let server = Server::new(0);
    let url = format!("http://{}/feed", server.address);
    let source = source(&server, move |ctx| {
        assert!(ctx
            .preload(LaunchRequest::new(
                "https://api.example/private/path?token=secret-value",
                Request::get(&url)
            ))
            .is_err());
    });
    let launch = fixture();
    begin(&launch, &source, false);
    definitions(&launch);
    let facts = launch.journal();
    assert!(facts
        .iter()
        .any(|f| f.name == "launch-data" && f.outcome == PreloadEvent::InvalidName));
    let journal = format!("{facts:?}");
    for private in ["/private/path", "token=", "secret-value"] {
        assert!(!journal.contains(private), "{journal}");
    }
    assert_eq!(launch.count(), 0);
    assert_eq!(server.count.load(Ordering::Acquire), 0);
}

#[test]
fn the_launch_deadline_aborts_its_blocking_fetch() {
    let server = Server::new(1);
    let url = format!("http://{}/token", server.address);
    let (fetched, fetching) = channel();
    let source = source(&server, move |ctx| {
        fetched.send(ctx.fetch(Request::get(&url))).unwrap();
    });
    let launch = fixture();
    begin(&launch, &source, false);
    server.request();
    let running = launch.0.rust.get().unwrap();
    running.expire_now();
    let result = fetching.recv_timeout(HANG);
    server.release(0);
    running.wait_returned();
    assert_eq!(result.unwrap(), Err("launch deadline".into()));
    let journal = launch.journal();
    assert!(journal
        .iter()
        .any(|f| matches!(f.outcome, PreloadEvent::LaunchDeadline { .. })));
    assert!(!journal
        .iter()
        .any(|f| matches!(f.outcome, PreloadEvent::LaunchPanicked { .. })));
}
