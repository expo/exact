use super::*;
use crate::store::{KeptStore, StoreRead};

#[test]
fn a_second_secret_read_keeps_the_start_value_and_refuses_a_stale_write_and_handoff() {
    let server = Server::new(0);
    let url = format!("http://{}/stale", server.address);
    let (entered, entering) = channel();
    let (release, released) = channel();
    let released = Mutex::new(released);
    let (answer, answered) = channel();
    let source = source(&server, move |ctx| {
        assert_eq!(ctx.secret("session").as_deref(), Some("A"));
        entered.send(()).unwrap();
        released.lock().unwrap().recv_timeout(HANG).unwrap();
        answer
            .send((
                ctx.secret("session"),
                ctx.set_secret("session", "C"),
                ctx.preload(LaunchRequest::new("stale", Request::get(&url))),
            ))
            .unwrap();
    });
    let launch = fixture();
    let backend = launch.0.backend.lock().unwrap().clone().unwrap();
    backend.write("session", Some("A")).unwrap();
    begin(&launch, &source, false);
    entering.recv_timeout(HANG).unwrap();
    launch
        .with_secret_write("session", || backend.write("session", Some("B")))
        .unwrap();
    release.send(()).unwrap();
    let (value, write, handoff) = answered.recv_timeout(HANG).unwrap();
    definitions(&launch);
    assert_eq!(value.as_deref(), Some("A"));
    assert_eq!(write, Err("launch secret changed".into()));
    assert_eq!(handoff, Err("launch preload secret changed".into()));
    assert_eq!(backend.read("session").unwrap().as_deref(), Some("B"));
    assert_eq!(launch.count(), 0);
    assert_eq!(server.count.load(Ordering::Acquire), 0);
}

#[test]
fn launch_secret_snapshot_holds_the_version_lock_against_the_app_writer() {
    struct HeldRead {
        value: Mutex<String>,
        entered: Sender<()>,
        release: Mutex<Receiver<()>>,
    }
    impl Backend for HeldRead {
        fn read(&self, _: &str) -> Result<Option<String>, String> {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv_timeout(HANG).unwrap();
            Ok(Some(self.value.lock().unwrap().clone()))
        }
        fn write(&self, _: &str, value: Option<&str>) -> Result<(), String> {
            *self.value.lock().unwrap() = value.unwrap().into();
            Ok(())
        }
    }
    let server = Server::new(0);
    let (entered, entering) = channel();
    let (release, released) = channel();
    let backend = Arc::new(HeldRead {
        value: Mutex::new(String::new()),
        entered,
        release: Mutex::new(released),
    });
    let (snapshot, snapped) = channel();
    let (finish, finished) = channel();
    let finished = Mutex::new(finished);
    let (answer, answered) = channel();
    let url = format!("http://{}/feed", server.address);
    let source = source(&server, move |ctx| {
        snapshot.send(ctx.secret("session")).unwrap();
        finished.lock().unwrap().recv_timeout(HANG).unwrap();
        answer
            .send((
                ctx.set_secret("session", "C"),
                ctx.preload(LaunchRequest::new("stale", Request::get(&url))),
            ))
            .unwrap();
    });
    let launch = fixture();
    *launch.0.backend.lock().unwrap() = Some(backend.clone());
    launch
        .with_secret_write("session", || backend.write("session", Some("A")))
        .unwrap();
    begin(&launch, &source, false);
    entering.recv_timeout(HANG).unwrap();
    let serialized = launch.0.invalidated.try_lock().is_err();
    let (attempt, attempted) = channel();
    let (written, writing) = channel();
    let app = launch.clone();
    let stored = backend.clone();
    let writer = std::thread::spawn(move || {
        attempt.send(()).unwrap();
        app.with_secret_write("session", || stored.write("session", Some("B")))
            .unwrap();
        written.send(()).unwrap();
    });
    attempted.recv_timeout(HANG).unwrap();
    let completed_while_reading = writing.try_recv().is_ok();
    release.send(()).unwrap();
    let value = snapped.recv_timeout(HANG).unwrap();
    writer.join().unwrap();
    finish.send(()).unwrap();
    let (write, handoff) = answered.recv_timeout(HANG).unwrap();
    definitions(&launch);
    assert!(
        serialized,
        "secret values and versions must share the writer lock"
    );
    assert!(!completed_while_reading);
    assert_eq!(value.as_deref(), Some("A"));
    assert_eq!(write, Err("launch secret changed".into()));
    assert_eq!(handoff, Err("launch preload secret changed".into()));
    assert_eq!(*backend.value.lock().unwrap(), "B");
    assert_eq!(launch.0.invalidated.lock().unwrap()["session"], 2);
    assert_eq!(server.count.load(Ordering::Acquire), 0);
}

thread_local! {
    static READS: Arc<AtomicUsize> = Arc::default();
    static DURING_SNAPSHOT: std::cell::RefCell<Option<Sender<()>>> = const { std::cell::RefCell::new(None) };
}
struct CountedStore(Box<dyn KeptStore>, Arc<AtomicUsize>);
impl KeptStore for CountedStore {
    fn snapshot(&self, previous: Vec<(String, Vec<u8>)>) -> StoreRead {
        self.1.fetch_add(1, Ordering::AcqRel);
        self.0.snapshot(previous)
    }
    fn write(&self, write: &exact_runner::StoreWrite) -> Result<(), String> {
        self.0.write(write)
    }
    fn kept_failures(&self) -> Vec<String> {
        self.0.kept_failures()
    }
}
fn endowed(grants: &str, app: &str, fresh: bool) -> crate::store::Endowed {
    let mut endowed = crate::store::endowed(grants, app, fresh);
    if let Some(store) = endowed.secrets.take() {
        endowed.secrets = Some(Box::new(CountedStore(store, READS.with(Clone::clone))));
    }
    DURING_SNAPSHOT.with(|held| {
        if let Some(release) = held.borrow_mut().take() {
            release.send(()).unwrap();
            local().unwrap().0.rust.get().unwrap().wait_returned();
        }
    });
    endowed
}

#[test]
fn a_launch_secret_write_reaches_the_host_before_or_after_activation_and_during_snapshot() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        for phase in 0..6 {
            for value in [Some("B"), None] {
                READS.with(|reads| reads.store(0, Ordering::Release));
                let server = Server::new(2);
                let (release, released) = channel();
                let released = Mutex::new(released);
                let (entered, entering) = channel();
                let mut source = source(&server, move |ctx| {
                    assert_eq!(ctx.secret("session").as_deref(), Some("A"));
                    entered.send(()).unwrap();
                    released.lock().unwrap().recv_timeout(HANG).unwrap();
                    match value {
                        Some(value) => ctx.set_secret("session", value).unwrap(),
                        None => ctx.forget_secret("session").unwrap(),
                    }
                });
                source.reads_session = true;
                let launch = fixture();
                let bindings = crate::store::endow(&source.base.grants).unwrap();
                bindings.secrets.set("session", "A").unwrap();
                *launch.0.backend.lock().unwrap() = Some(Arc::new(
                    crate::store::launch_backend_in(bindings.secrets, None),
                ));
                begin(&launch, &source, false);
                entering.recv_timeout(HANG).unwrap();
                if phase == 3 {
                    release.send(()).unwrap();
                    launch.0.rust.get().unwrap().wait_returned();
                    definitions(&launch);
                }
                if phase == 2 {
                    DURING_SNAPSHOT.with(|held| *held.borrow_mut() = Some(release.clone()));
                }
                let (mut drive, _) = Drive::boot_endowed(&source, &plan(), endowed);
                if phase == 1 {
                    drive.bridge.data_ready();
                }
                while drive.wakes.try_recv().is_ok() {}
                if !(2..4).contains(&phase) {
                    release.send(()).unwrap();
                    launch.0.rust.get().unwrap().wait_returned();
                    drive
                        .wakes
                        .recv_timeout(HANG)
                        .expect("secret write must wake its host");
                }
                if phase == 5 {
                    drive.bridge.pump(0.);
                }
                if phase >= 4 {
                    let bytes = plan();
                    let len = drive.bridge.input_write(&bytes);
                    let hooks = drive.hooks;
                    drive
                        .bridge
                        .boot_plan(len, composed(source.clone()), hooks, 100., 100.);
                }
                drive.bridge.pump(0.);
                if phase != 1 {
                    drive.bridge.data_ready();
                }
                // A request asked before rotation may already have left; its replacement uses the adopted store.
                let mut head = server.request().to_ascii_lowercase();
                if phase == 1 && head.contains("authorization: a") {
                    server.release(0);
                    head = server.request().to_ascii_lowercase();
                }
                assert_eq!(head.contains("authorization: b"), value.is_some(), "{head}");
                assert!(!head.contains("authorization: a"), "{head}");
                if value.is_none() {
                    assert!(!head.contains("authorization:"), "{head}");
                }
                server.release(0);
                server.release(1);
                drive.complete(&source);
                assert_eq!(
                    READS.with(|reads| reads.load(Ordering::Acquire)) > 0,
                    phase != 3
                );
            }
        }
    }
}

#[test]
fn a_launch_function_without_secret_writes_neither_refreshes_nor_wakes_its_host() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        let server = Server::new(0);
        let (release, released) = channel();
        let released = Mutex::new(released);
        let (entered, entering) = channel();
        let source = source(&server, move |_| {
            entered.send(()).unwrap();
            released.lock().unwrap().recv_timeout(HANG).unwrap();
        });
        let launch = fixture();
        begin(&launch, &source, false);
        entering.recv_timeout(HANG).unwrap();
        let bytes = contract::compile("component App\n  view\n    text \"ready\"\n")
            .unwrap()
            .encode();
        let (mut drive, _) = Drive::boot_endowed(&source, &bytes, endowed);
        drive.bridge.data_ready();
        while drive.wakes.try_recv().is_ok() {}
        let reads = READS.with(|reads| reads.load(Ordering::Acquire));
        release.send(()).unwrap();
        launch.0.rust.get().unwrap().wait_returned();
        definitions(&launch);
        assert!(drive.wakes.try_recv().is_err());
        drive.bridge.pump(0.);
        assert_eq!(READS.with(|reads| reads.load(Ordering::Acquire)), reads);
    }
}

#[test]
fn launch_secret_writes_compare_the_last_observed_version() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        for read_first in [false, true] {
            for app_writes in [false, true] {
                for forget in [false, true] {
                    let token = Server::new(1);
                    let feed = Server::new(3);
                    let token_url = format!("http://{}/token", token.address);
                    let (written, writing) = channel();
                    let mut source = source(&feed, move |ctx| {
                        if read_first {
                            assert_eq!(ctx.secret("session").as_deref(), Some("A"));
                        }
                        ctx.fetch(Request::get(&token_url)).unwrap();
                        let result = if forget {
                            ctx.forget_secret("session")
                        } else {
                            ctx.set_secret("session", "refreshed")
                        };
                        written.send(result).unwrap();
                    });
                    source
                        .base
                        .grants
                        .push_str(&format!("\nnet.fetch http://{}", token.address));
                    source.reads_session = true;
                    let launch = fixture();
                    let bindings = crate::store::endow(&source.base.grants).unwrap();
                    let backend = Arc::new(crate::store::launch_backend_in(bindings.secrets, None));
                    *launch.0.backend.lock().unwrap() = Some(backend.clone());
                    backend.write("session", Some("A")).unwrap();
                    begin(&launch, &source, false);
                    token.request();
                    let bytes = if app_writes {
                        contract::compile("shape Reply\n  value: string\ncomponent App\n  resource changed = rotate() as shape Reply\n  resource feed = read() as shape Reply\n  view\n    text feed.value\n").unwrap().encode()
                    } else {
                        plan()
                    };
                    let (mut drive, _) = Drive::boot_endowed(&source, &bytes, endowed);
                    drive.bridge.data_ready();
                    assert_eq!(
                        backend.read("session").unwrap().as_deref(),
                        Some(if app_writes { "new" } else { "A" })
                    );
                    token.release(0);
                    let result = writing.recv_timeout(HANG).unwrap();
                    launch.0.rust.get().unwrap().wait_returned();
                    definitions(&launch);
                    assert_eq!(
                        result,
                        if app_writes {
                            Err("launch secret changed".into())
                        } else {
                            Ok(())
                        }
                    );
                    let expected = if app_writes {
                        Some("new")
                    } else if forget {
                        None
                    } else {
                        Some("refreshed")
                    };
                    assert_eq!(backend.read("session").unwrap().as_deref(), expected);
                    drive.bridge.pump(0.);
                    let mut head = feed.request().to_ascii_lowercase();
                    if !app_writes && head.contains("authorization: a") {
                        feed.release(0);
                        head = feed.request().to_ascii_lowercase();
                    }
                    assert_eq!(head.contains("authorization: new"), app_writes, "{head}");
                    assert_eq!(
                        head.contains("authorization: refreshed"),
                        !app_writes && !forget,
                        "{head}"
                    );
                    assert!(!head.contains("authorization: a"), "{head}");
                    for i in 0..3 {
                        feed.release(i);
                    }
                    drive.complete(&source);
                }
            }
        }
    }
}

struct FailOnce {
    backend: Arc<dyn Backend>,
    fail: AtomicBool,
}
impl Backend for FailOnce {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        self.backend.read(name)
    }
    fn write(&self, name: &str, value: Option<&str>) -> Result<(), String> {
        self.backend.write(name, value)?;
        if self.fail.swap(false, Ordering::AcqRel) {
            Err("private backend failure".into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn a_failed_launch_secret_write_invalidates_its_uses_without_publishing_to_the_host() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        for forget in [false, true] {
            for read_first in [false, true] {
                let manifest = Server::new(2);
                let public = Server::new(1);
                let url = format!("http://{}/public", public.address);
                let stale_url = format!("http://{}/stale", manifest.address);
                let (entered, entering) = channel();
                let (release, released) = channel();
                let released = Mutex::new(released);
                let (answer, answered) = channel();
                let (other, others) = channel();
                let others = Mutex::new(others);
                let (written, writing) = channel();
                let mut source = source(&manifest, move |ctx| {
                    if read_first {
                        ctx.preload(LaunchRequest::new("public", Request::get(&url)))
                            .unwrap();
                        assert_eq!(ctx.secret("session").as_deref(), Some("A"));
                    }
                    entered.send(()).unwrap();
                    released.lock().unwrap().recv_timeout(HANG).unwrap();
                    let failed = if forget {
                        ctx.forget_secret("session")
                    } else {
                        ctx.set_secret("session", "refreshed")
                    };
                    let set = ctx.set_secret("session", "retry");
                    let forgotten = ctx.forget_secret("session");
                    let public = if !read_first {
                        ctx.preload(LaunchRequest::new("public", Request::get(&url)))
                    } else {
                        Ok(())
                    };
                    let snapshot = ctx.secret("session");
                    let stale = ctx.preload(LaunchRequest::new("stale", Request::get(&stale_url)));
                    answer
                        .send((failed, set, forgotten, public, snapshot, stale))
                        .unwrap();
                    others.lock().unwrap().recv_timeout(HANG).unwrap();
                    written
                        .send(ctx.set_secret("other", "independent"))
                        .unwrap();
                });
                source.base.definitions = manifest.source(PreloadWhen::Start, true).definitions;
                source.base.grants.push_str(&format!(
                    "\nnet.fetch http://{}\nsecret.keep other",
                    public.address
                ));
                source.manifest = true;
                source.reads_session = true;
                let launch = fixture();
                let bindings = crate::store::endow(&source.base.grants).unwrap();
                let backend: Arc<dyn Backend> =
                    Arc::new(crate::store::launch_backend_in(bindings.secrets, None));
                backend.write("session", Some("A")).unwrap();
                let manifest_backend = backend.clone();
                *launch.0.reader.lock().unwrap() = Some(Arc::new(move |_, _, _| {
                    BTreeMap::from([(
                        "session".into(),
                        manifest_backend.read("session").unwrap().unwrap(),
                    )])
                }));
                *launch.0.backend.lock().unwrap() = Some(Arc::new(FailOnce {
                    backend: backend.clone(),
                    fail: AtomicBool::new(true),
                }));
                begin(&launch, &source, false);
                entering.recv_timeout(HANG).unwrap();
                manifest.request();
                let (mut drive, _) = Drive::boot_endowed(&source, &plan(), endowed);
                while drive.wakes.try_recv().is_ok() {}
                let reads = READS.with(|v| v.load(Ordering::Acquire));
                release.send(()).unwrap();
                let (failed, set, forgotten, accepted, snapshot, stale) =
                    answered.recv_timeout(HANG).unwrap();
                assert_eq!(failed, Err("private backend failure".into()));
                assert_eq!(set, Err("launch secret write failed".into()));
                assert_eq!(forgotten, Err("launch secret write failed".into()));
                assert_eq!(stale, Err("launch secret write failed".into()));
                // A request handed over after the failed write is refused, whether or not the
                // function had read the secret: it may carry the value it tried to write.
                assert_eq!(
                    accepted,
                    if read_first {
                        Ok(())
                    } else {
                        Err("launch secret write failed".into())
                    }
                );
                assert_eq!(snapshot.as_deref(), Some("A"));
                assert_eq!(
                    backend.read("session").unwrap().as_deref(),
                    if forget { None } else { Some("refreshed") }
                );
                assert!(drive.wakes.try_recv().is_err());
                let journal = launch.journal();
                drive.bridge.pump(0.);
                assert_eq!(READS.with(|v| v.load(Ordering::Acquire)), reads);
                drive.bridge.data_ready();
                let head = manifest.request().to_ascii_lowercase();
                assert!(head.contains("authorization: a"), "{head}");
                if read_first {
                    assert!(public.request().contains("/public"));
                }
                assert!(journal
                    .iter()
                    .any(|f| f.name == "feed" && f.outcome == PreloadEvent::DroppedBySecret));
                assert!(journal
                    .iter()
                    .any(|f| f.name == "stale" && f.outcome == PreloadEvent::DroppedBySecret));
                assert!(!format!("{journal:?}").contains("private backend failure"));
                other.send(()).unwrap();
                assert_eq!(writing.recv_timeout(HANG).unwrap(), Ok(()));
                launch.0.rust.get().unwrap().wait_returned();
                definitions(&launch);
                assert_eq!(
                    backend.read("other").unwrap().as_deref(),
                    Some("independent")
                );
                assert_eq!(
                    launch.with_secret_write("session", || backend.write("session", Some("app"))),
                    Ok(())
                );
                assert_eq!(backend.read("session").unwrap().as_deref(), Some("app"));
                manifest.release(0);
                manifest.release(1);
                if read_first {
                    public.release(0);
                }
                drop(drive);
                assert_eq!(manifest.count.load(Ordering::Acquire), 2);
                assert_eq!(
                    public.count.load(Ordering::Acquire),
                    usize::from(read_first)
                );
            }
        }
    }
}

#[test]
fn an_apps_failed_secret_write_refuses_the_launch_functions_stale_write_and_handoff() {
    for asynchronous in [false, true] {
        ASYNCHRONOUS.with(|v| v.set(asynchronous));
        for forget in [false, true] {
            let server = Server::new(0);
            let url = format!("http://{}/stale", server.address);
            let (entered, entering) = channel();
            let (release, released) = channel();
            let released = Mutex::new(released);
            let (answer, answered) = channel();
            let source = source(&server, move |ctx| {
                assert_eq!(ctx.secret("session").as_deref(), Some("A"));
                entered.send(()).unwrap();
                released.lock().unwrap().recv_timeout(HANG).unwrap();
                answer
                    .send((
                        ctx.set_secret("session", "retry"),
                        ctx.forget_secret("session"),
                        ctx.preload(LaunchRequest::new("stale", Request::get(&url))),
                    ))
                    .unwrap();
            });
            let launch = fixture();
            let backend = launch.0.backend.lock().unwrap().clone().unwrap();
            backend.write("session", Some("A")).unwrap();
            begin(&launch, &source, false);
            entering.recv_timeout(HANG).unwrap();
            let failed = FailOnce {
                backend: backend.clone(),
                fail: AtomicBool::new(true),
            };
            assert_eq!(
                launch.with_secret_write("session", || failed
                    .write("session", if forget { None } else { Some("app") })),
                Err("private backend failure".into())
            );
            release.send(()).unwrap();
            let (set, forgotten, preload) = answered.recv_timeout(HANG).unwrap();
            definitions(&launch);
            assert_eq!(set, Err("launch secret changed".into()));
            assert_eq!(forgotten, Err("launch secret changed".into()));
            assert_eq!(preload, Err("launch preload secret changed".into()));
            assert_eq!(
                backend.read("session").unwrap().as_deref(),
                if forget { None } else { Some("app") }
            );
            assert_eq!(launch.count(), 0);
            assert_eq!(server.count.load(Ordering::Acquire), 0);
        }
    }
}
