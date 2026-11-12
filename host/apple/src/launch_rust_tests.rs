use super::*;
use crate::executor::PreloadEvent;
use exact_data::secrets::Backend;
use exact_runner::request::{Launch as Entry, LaunchContext, LaunchRequest};
use exact_runner::{Placement, Request};

#[derive(Clone)]
struct RustSource {
    base: Source,
    manifest: bool,
    entry: Entry,
    request: Request,
}
impl DataSource for RustSource {
    fn launch(&self) -> Option<Entry> {
        Some(self.entry.clone())
    }
    fn preloads(&self) -> &[Baked] {
        if self.manifest {
            self.base.definitions
        } else {
            &[]
        }
    }
    fn grants(&self) -> &str {
        &self.base.grants
    }
    fn query(&mut self, name: &str, args: &[Value]) -> Result<Value, DataError> {
        self.base.query(name, args)
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        if source == "rotate" {
            self.base.answer(store, source, args)
        } else if source == "public" {
            Ok(Answer::Later(
                self.base.definitions[0].fill(|_| None).unwrap(),
            ))
        } else {
            Ok(Answer::Later(self.request.clone()))
        }
    }
    fn parse(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        self.base.parse(store, source, args, outcome)
    }
}
type Data = exact_logic::Swappable<exact_data::Mixed<(), exact_data::Placed<RustSource>>>;
fn composed(source: RustSource) -> Data {
    exact_logic::Swappable::off(
        exact_data::Mixed::new(
            (),
            exact_data::Placed::new(source, Placement::Main),
            &[],
            &["read", "rotate", "public"],
        )
        .unwrap(),
    )
}
fn source(
    server: &Server,
    run: impl Fn(&mut dyn LaunchContext) -> Vec<LaunchRequest> + Send + Sync + 'static,
) -> RustSource {
    let base = server.source(PreloadWhen::Start, false);
    let request = base.definitions[0].fill(|_| None).unwrap();
    RustSource {
        base,
        request,
        manifest: false,
        entry: Entry::new("launch-data", run),
    }
}
fn begin(launch: &LaunchPreloads, source: &RustSource, prewarm: bool) {
    let source = source.clone();
    launch.begin(prewarm, move || composed(source));
}
fn signal(launch: &LaunchPreloads, source: &RustSource, opened: bool) {
    let source = source.clone();
    launch.signal(opened, move || composed(source));
}
struct Drive {
    bridge: Bridge<Data>,
    _wake: Box<Sender<()>>,
    wakes: Receiver<()>,
}
impl Drive {
    fn boot(source: &RustSource, plan: &[u8]) -> (Self, String) {
        let (send, wakes) = channel();
        let sender = Box::new(send);
        extern "C" fn wake(ctx: *mut std::ffi::c_void) {
            // SAFETY: Drive retains the sender until after the bridge is dropped.
            #[allow(unsafe_code)]
            let send = unsafe { &*ctx.cast::<Sender<()>>() };
            let _ = send.send(());
        }
        let hooks = Hooks {
            wake: Some(wake),
            wake_ctx: (&*sender as *const Sender<()>).cast_mut().cast(),
            ..Hooks::none()
        };
        let mut bridge = Bridge::with_links(Links {
            io: Some(IoLinks {
                preloads: local,
                endow: memory_endow,
                ..IoLinks::LINKED
            }),
            ..Links::ALL
        });
        let len = bridge.boot(plan, composed(source.clone()), hooks, 100., 100.);
        let batch = String::from_utf8(bridge.output_bytes(len as usize).to_vec()).unwrap();
        assert!(!batch.contains("\"error\":\""), "{batch}");
        (
            Self {
                bridge,
                _wake: sender,
                wakes,
            },
            batch,
        )
    }
    fn complete(&mut self, source: &RustSource) {
        while source.base.parsed.load(Ordering::Acquire) == 0 {
            self.wakes.recv_timeout(HANG).unwrap();
            self.bridge.pump(0.);
        }
    }
    fn logs(&mut self) -> String {
        let len = self.bridge.input_write(br#"{"op":"logs"}"#);
        let len = self.bridge.agent(len);
        String::from_utf8(self.bridge.output_bytes(len as usize).to_vec()).unwrap()
    }
}
fn definitions(launch: &LaunchPreloads) {
    launch.0.set.get().unwrap().control.wait_definitions();
}

#[test]
fn rust_request_reaches_a_held_server_without_a_runner_and_boot_takes_it_once() {
    let server = Server::new(1);
    let url = format!("http://{}/private-path?private-query", server.address);
    let caller = std::thread::current().id();
    let source = source(&server, move |_| {
        assert_ne!(caller, std::thread::current().id());
        vec![LaunchRequest::new("rust-feed", Request::get(&url))]
    });
    let launch = fixture();
    begin(&launch, &source, false);
    server.request();
    assert_eq!(source.base.parsed.load(Ordering::Acquire), 0);
    let (mut drive, batch) = Drive::boot(&source, &plan());
    assert!(!batch.contains("\"text\":\"content\""));
    server.release(0);
    drive.complete(&source);
    assert_eq!(server.count.load(Ordering::Acquire), 1);
    assert!(launch.mark(0).unwrap().1.committed > 0.);
    assert!(drive.logs().contains("Taken"));
}

#[test]
fn token_refresh_is_durable_before_the_returned_request_uses_the_new_secret() {
    use ibex2::secrets::SecretStore;
    let token = Server::new(1);
    let feed = Server::new(1);
    let directory = std::env::temp_dir().join(format!(
        "exact-launch-refresh-{}-{}",
        std::process::id(),
        feed.address.port()
    ));
    let grants = format!(
        "net.fetch http://{}\nnet.fetch http://{}\nsecret.keep session",
        token.address, feed.address
    );
    let bindings = ibex2::host::Host::new()
        .with_secret_store(Box::new(ibex2::secrets::FileStore::new(directory.clone())))
        .endow(ibex2::grant::GrantSet::parse(&grants).unwrap());
    let backend = Arc::new(crate::store::launch_backend_in(
        bindings.secrets,
        Some(directory.clone()),
    ));
    backend.write("session", Some("old")).unwrap();
    let token_url = format!("http://{}/token", token.address);
    let feed_url = format!("http://{}/feed", feed.address);
    let mut source = source(&feed, move |ctx| {
        assert_eq!(ctx.secret("session").as_deref(), Some("old"));
        let reply = ctx.fetch(Request::get(&token_url)).unwrap();
        let value = String::from_utf8(reply.body).unwrap();
        ctx.set_secret("session", &value).unwrap();
        assert_eq!(ctx.secret("session").as_deref(), Some("content"));
        let mut request = Request::get(&feed_url);
        request.headers.push(("Authorization".into(), value));
        vec![LaunchRequest::new("rust-feed", request)]
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
    drop(launch);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn an_app_secret_write_cancels_rust_work_and_leaves_the_public_manifest_preload() {
    for complete in [false, true] {
        let manifest = Server::new(1);
        let server = Server::new(1);
        let url = format!("http://{}/feed", server.address);
        let mut source = source(&server, move |ctx| {
            ctx.secret("session");
            vec![LaunchRequest::new("rust-feed", Request::get(&url))]
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
                vec![LaunchRequest::new("rust-feed", Request::get(&feed_url))]
            } else {
                Vec::new()
            }
        });
        source.base.definitions = manifest.source(PreloadWhen::Start, false).definitions;
        source.base.grants.push_str(&format!(
            "\nnet.fetch http://{}\nnet.fetch http://{}",
            token.address, manifest.address
        ));
        source.manifest = true;
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
fn a_launch_past_its_deadline_releases_reads_and_discards_late_requests() {
    let server = Server::new(1);
    let (entered, entering) = channel();
    let (release, released) = channel();
    let released = Mutex::new(released);
    let url = format!("http://{}/private-path?private-query", server.address);
    let source = source(&server, move |_| {
        entered.send(()).unwrap();
        released.lock().unwrap().recv_timeout(HANG).unwrap();
        vec![LaunchRequest::new("too-late", Request::get(&url))]
    });
    let launch = fixture();
    begin(&launch, &source, false);
    entering.recv_timeout(HANG).unwrap();
    let (mut drive, batch) = Drive::boot(&source, &plan());
    assert!(!batch.contains("\"text\":\"content\""));
    let running = launch.0.rust.get().unwrap();
    running.expire_now();
    server.request();
    server.release(0);
    drive.complete(&source);
    release.send(()).unwrap();
    running.wait_returned();
    definitions(&launch);
    assert_eq!(launch.count(), 0);
    assert!(format!("{:?} {}", launch.journal(), drive.logs()).contains("LaunchDeadline"));
    assert_eq!(server.count.load(Ordering::Acquire), 1);
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
    let mut source = source(&server, move |_| {
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
        requests
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
        "relative",
        "empty-userinfo",
        "method",
        "head-body",
        "oversized",
        "header",
        "repeated-header",
        "header-value",
        "BadName",
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
    let source = source(&server, move |_| {
        called.fetch_add(1, Ordering::AcqRel);
        let mut request = LaunchRequest::new("foreground", Request::get(&url));
        request.when = PreloadWhen::Foreground;
        vec![request]
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
        Vec::new()
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
fn writing_a_read_secret_before_return_discards_the_rust_request() {
    let server = Server::new(0);
    let (entered, entering) = channel();
    let (release, released) = channel();
    let released = Mutex::new(released);
    let url = format!("http://{}/private-path?private-query", server.address);
    let source = source(&server, move |ctx| {
        ctx.secret("session");
        entered.send(()).unwrap();
        released.lock().unwrap().recv_timeout(HANG).unwrap();
        vec![LaunchRequest::new("rust-feed", Request::get(&url))]
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
        Vec::new()
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
        Vec::new()
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
