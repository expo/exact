use super::*;
use crate::store::{KeptStore, StoreRead};

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
