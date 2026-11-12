use super::*;
use crate::abi::{Bridge, Hooks};
use crate::link::{IoLinks, Links};
use exact_plan::Value;
use exact_runner::request::{Preload as Baked, PreloadPart, PreloadWhen};
use exact_runner::{Answer, DataError, Outcome, Store};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
const HANG: Duration = Duration::from_secs(10);

struct Server {
    address: std::net::SocketAddr,
    requests: Receiver<String>,
    releases: Vec<Sender<()>>,
    count: Arc<AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}
impl Server {
    fn new(replies: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (send, requests) = channel();
        let (releases, held): (Vec<_>, Vec<_>) = (0..replies).map(|_| channel()).unzip();
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut clients = Vec::new();
            let mut held = held.into_iter();
            for socket in listener.incoming() {
                let mut socket = socket.unwrap();
                if stopping.load(Ordering::Acquire) {
                    break;
                }
                let held = held.next().expect("unexpected duplicate request");
                let send = send.clone();
                let seen = seen.clone();
                clients.push(std::thread::spawn(move || {
                    socket.set_read_timeout(Some(HANG)).unwrap();
                    let mut head = Vec::new();
                    while !head.ends_with(b"\r\n\r\n") {
                        let mut byte = [0];
                        if socket.read(&mut byte).unwrap_or(0) == 0 { return; }
                        head.push(byte[0]);
                    }
                    seen.fetch_add(1, Ordering::AcqRel);
                    send.send(String::from_utf8(head).unwrap()).unwrap();
                    let _ = held.recv_timeout(HANG);
                    let _ = socket.write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 7\r\nConnection: close\r\n\r\ncontent");
                }));
            }
            for client in clients {
                client.join().unwrap();
            }
        });
        Self {
            address,
            requests,
            releases,
            count,
            thread: Some(thread),
            stop,
        }
    }
    fn request(&self) -> String {
        self.requests.recv_timeout(HANG).unwrap()
    }
    fn release(&self, i: usize) {
        let _ = self.releases[i].send(());
    }
    fn source(&self, when: PreloadWhen, secret: bool) -> Source {
        let url: &'static str = Box::leak(
            format!("http://{}/private-path?private-query", self.address).into_boxed_str(),
        );
        let url = Box::leak(vec![PreloadPart::Literal(url)].into_boxed_slice());
        let definitions = Box::leak(
            vec![Baked {
                name: "feed",
                method: "GET",
                url,
                headers: if secret {
                    &[("Authorization", &[PreloadPart::Secret("session")])]
                } else {
                    &[]
                },
                body: None,
                timeout_ms: None,
                when,
                secrets: if secret { &["session"] } else { &[] },
            }]
            .into_boxed_slice(),
        );
        Source {
            definitions,
            grants: format!("net.fetch http://{}\nsecret.keep session", self.address),
            parsed: Arc::new(AtomicUsize::new(0)),
            activated: Arc::new(AtomicUsize::new(0)),
            forget: false,
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.releases.clear();
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        self.thread.take().unwrap().join().unwrap();
    }
}
#[derive(Clone)]
struct Source {
    definitions: &'static [Baked],
    grants: String,
    parsed: Arc<AtomicUsize>,
    activated: Arc<AtomicUsize>,
    forget: bool,
}
impl DataSource for Source {
    fn preloads(&self) -> &[Baked] {
        self.definitions
    }
    fn grants(&self) -> &str {
        &self.grants
    }
    fn activate(&mut self) -> Result<(), DataError> {
        self.activated.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("launch evaluated {name}")
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        _: &[Value],
    ) -> Result<Answer, DataError> {
        if source == "rotate" {
            if self.forget {
                store.forget("session").unwrap();
            } else {
                store.set("session", "new").unwrap();
            }
            return Ok(Answer::Now(Value::record(vec![Value::str("changed")])));
        }
        Ok(Answer::Later(
            self.definitions[usize::from(source == "after")]
                .fill(|_| Some("present-secret".into()))
                .unwrap(),
        ))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let Outcome::Response(reply) = outcome else {
            panic!("{outcome:?}")
        };
        self.parsed.fetch_add(1, Ordering::AcqRel);
        Ok(Answer::Now(Value::record(vec![Value::str(
            &String::from_utf8(reply.body).unwrap(),
        )])))
    }
}
thread_local! { static LOCAL: std::cell::RefCell<Option<LaunchPreloads>> = const { std::cell::RefCell::new(None) }; }
fn local() -> Option<LaunchPreloads> {
    LOCAL.with(|l| l.borrow().clone())
}
fn fixture() -> LaunchPreloads {
    let launch = LaunchPreloads::default();
    LOCAL.with(|l| *l.borrow_mut() = Some(launch.clone()));
    launch
}
fn factory(source: &Source) -> impl FnOnce() -> Source + Send + 'static {
    let source = source.clone();
    move || source
}
fn ready(launch: &LaunchPreloads) {
    launch.0.set.get().unwrap().control.wait_ready(1);
}
struct Driven {
    bridge: Bridge<Source>,
    _wake: Box<Sender<()>>,
}
impl std::ops::Deref for Driven {
    type Target = Bridge<Source>;
    fn deref(&self) -> &Self::Target {
        &self.bridge
    }
}
impl std::ops::DerefMut for Driven {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bridge
    }
}
fn boot(source: Source, plan: &[u8], candidate: bool, wakes: Sender<()>) -> (Driven, String) {
    let mut bridge = Bridge::with_links(Links {
        io: Some(IoLinks {
            preloads: local,
            ..IoLinks::LINKED
        }),
        ..Links::ALL
    });
    extern "C" fn wake(ctx: *mut std::ffi::c_void) {
        // SAFETY: the caller keeps this sender alive until the bridge is dropped.
        #[allow(unsafe_code)]
        let send = unsafe { &*ctx.cast::<Sender<()>>() };
        let _ = send.send(());
    }
    let sender = Box::new(wakes);
    let hooks = Hooks {
        wake: Some(wake),
        wake_ctx: (&*sender as *const Sender<()>).cast_mut().cast(),
        ..Hooks::none()
    };
    let len = if candidate {
        bridge.input_write(plan);
        bridge.prepare_plan(plan.len(), source, hooks, 100., 100.);
        assert_eq!(
            local().unwrap().mark(0).unwrap().1.committed,
            0.,
            "a prepared candidate has not carried content"
        );
        bridge.commit_plan()
    } else {
        bridge.boot(plan, source, hooks, 100., 100.)
    };
    let batch = String::from_utf8(bridge.output_bytes(len as usize).to_vec()).unwrap();
    assert!(!batch.contains("\"error\":\""), "{batch}");
    (
        Driven {
            bridge,
            _wake: sender,
        },
        batch,
    )
}
fn plan() -> Vec<u8> {
    contract::compile("shape Reply\n  value: string\ncomponent App\n  resource feed = read() as shape Reply\n  view\n    text feed.value\n").unwrap().encode()
}

#[test]
fn begin_fetches_without_a_runner_and_ready_content_is_in_the_first_batch() {
    for candidate in [false, true] {
        let server = Server::new(1);
        let source = server.source(PreloadWhen::Start, false);
        let launch = fixture();
        launch.begin(false, factory(&source));
        assert!(server.request().contains("private-path?private-query"));
        assert_eq!(
            source.activated.load(Ordering::Acquire),
            0,
            "no engine or source activation at launch"
        );
        assert_eq!(
            source.parsed.load(Ordering::Acquire),
            0,
            "no runner at launch"
        );
        server.release(0);
        ready(&launch);
        let (wake, _) = channel();
        let (_bridge, batch) = boot(source.clone(), &plan(), candidate, wake);
        assert!(batch.contains("\"text\":\"content\""), "{batch}");
        assert_eq!(source.parsed.load(Ordering::Acquire), 1);
        assert_eq!(server.count.load(Ordering::Acquire), 1);
        let (_, mark) = launch.mark(0).unwrap();
        assert!(
            mark.handed > 0. && mark.completed >= mark.handed && mark.committed >= mark.completed
        );
        assert!(mark.sequence > 0);
        for fact in launch.journal() {
            assert_eq!(fact.origin, format!("http://{}", server.address));
        }
    }
}

#[test]
fn a_ready_preload_settles_a_dependent_boot_read_in_the_same_first_batch() {
    let server = Server::new(2);
    let mut source = server.source(PreloadWhen::Start, false);
    let second = server.source(PreloadWhen::Start, false);
    let mut definition = second.definitions[0];
    definition.name = "dependent";
    definition.url = Box::leak(
        vec![PreloadPart::Literal(Box::leak(
            format!("http://{}/dependent", server.address).into_boxed_str(),
        ))]
        .into_boxed_slice(),
    );
    source.definitions = Box::leak(vec![source.definitions[0], definition].into_boxed_slice());
    let launch = fixture();
    launch.begin(false, factory(&source));
    server.request();
    server.request();
    server.release(0);
    server.release(1);
    launch.0.set.get().unwrap().control.wait_ready(2);
    let plan = contract::compile("shape Reply\n  value: string\ncomponent App\n  resource first = read() as shape Reply\n  resource second = after(first.value) as shape Reply\n  view\n    text second.value\n").unwrap().encode();
    let (wake, _) = channel();
    let (_bridge, batch) = boot(source.clone(), &plan, false, wake);
    assert!(batch.contains("\"text\":\"content\""), "{batch}");
    assert_eq!(source.parsed.load(Ordering::Acquire), 2);
    assert_eq!(server.count.load(Ordering::Acquire), 2);
    assert!(launch.mark(1).unwrap().1.committed > 0.);
}

#[test]
fn discarding_a_prepared_batch_does_not_mark_an_unrelated_reply_as_preload_content() {
    let server = Server::new(2);
    let mut source = server.source(PreloadWhen::Start, false);
    let launch = fixture();
    launch.begin(false, factory(&source));
    server.request();
    server.release(0);
    ready(&launch);
    let thread = Box::new(std::thread::current());
    let mut bridge = Bridge::with_links(Links {
        io: Some(IoLinks {
            preloads: local,
            ..IoLinks::LINKED
        }),
        ..Links::ALL
    });
    extern "C" fn wake(ctx: *mut std::ffi::c_void) {
        // SAFETY: thread outlives the bridge and its retired callbacks.
        #[allow(unsafe_code)]
        unsafe { &*ctx.cast::<std::thread::Thread>() }.unpark();
    }
    let hooks = Hooks {
        wake: Some(wake),
        wake_ctx: (&*thread as *const std::thread::Thread).cast_mut().cast(),
        ..Hooks::none()
    };
    let plan = plan();
    bridge.input_write(&plan);
    bridge.prepare_plan(plan.len(), source.clone(), hooks, 100., 100.);
    bridge.discard_plan();
    assert_eq!(launch.mark(0).unwrap().1.committed, 0.);
    let mut different = source.definitions[0];
    different.url = Box::leak(
        vec![PreloadPart::Literal(Box::leak(
            format!("http://{}/other", server.address).into_boxed_str(),
        ))]
        .into_boxed_slice(),
    );
    source.definitions = Box::leak(vec![different].into_boxed_slice());
    bridge.boot(&plan, source.clone(), hooks, 100., 100.);
    server.request();
    server.release(1);
    let deadline = std::time::Instant::now() + HANG;
    while source.parsed.load(Ordering::Acquire) < 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "the unrelated reply did not land"
        );
        std::thread::park_timeout(deadline.saturating_duration_since(std::time::Instant::now()));
        bridge.pump(0.);
    }
    assert_eq!(
        launch.mark(0).unwrap().1.committed,
        0.,
        "the accepted reply used another URL"
    );
    drop(bridge);
}

#[test]
fn a_held_reply_does_not_delay_the_first_batch_and_delivers_without_a_second_request() {
    let server = Server::new(1);
    let source = server.source(PreloadWhen::Start, false);
    let launch = fixture();
    launch.begin(false, factory(&source));
    server.request();
    let (wake, wakes) = channel();
    let (mut bridge, batch) = boot(source.clone(), &plan(), false, wake);
    assert!(!batch.contains("\"text\":\"content\""));
    assert_eq!(source.parsed.load(Ordering::Acquire), 0);
    let len = bridge.input_write(br#"{"op":"outstanding"}"#);
    let len = bridge.agent(len);
    assert!(String::from_utf8_lossy(bridge.output_bytes(len as usize)).contains("feed"));
    assert_eq!(launch.mark(0).unwrap().1.committed, 0.);
    server.release(0);
    while source.parsed.load(Ordering::Acquire) == 0 {
        wakes.recv_timeout(HANG).unwrap();
        bridge.pump(0.);
    }
    assert_eq!(server.count.load(Ordering::Acquire), 1);
    assert!(launch.mark(0).unwrap().1.committed > 0.);
}

#[test]
fn prewarming_and_reordered_entry_points_send_each_definition_once_without_an_engine() {
    for order in [
        vec![0, 0, 1, 1, 2, 2],
        vec![1, 1, 0, 2, 0],
        vec![2, 2, 1, 0, 0],
    ] {
        let server = Server::new(1);
        let source = server.source(PreloadWhen::Start, false);
        let launch = fixture();
        for (i, op) in order.into_iter().enumerate() {
            match op {
                0 => launch.begin(true, factory(&source)),
                1 => launch.signal(false, factory(&source)),
                _ => launch.signal(true, factory(&source)),
            }
            if i == 0 && op == 0 {
                assert!(
                    launch.0.set.get().is_none(),
                    "prewarming starts no launch thread"
                );
            }
        }
        server.request();
        server.release(0);
        ready(&launch);
        assert_eq!(server.count.load(Ordering::Acquire), 1);
        assert_eq!(source.activated.load(Ordering::Acquire), 0);
    }
    let server = Server::new(1);
    let source = server.source(PreloadWhen::Start, false);
    let launch = fixture();
    launch.begin(false, factory(&source));
    launch.begin(false, factory(&source));
    launch.signal(false, factory(&source));
    launch.signal(false, factory(&source));
    server.request();
    server.release(0);
    ready(&launch);
    assert_eq!(server.count.load(Ordering::Acquire), 1);
}

#[test]
fn foreground_waits_for_opened_and_can_start_after_adoption() {
    let server = Server::new(1);
    let source = server.source(PreloadWhen::Foreground, false);
    let launch = fixture();
    launch.begin(false, factory(&source));
    launch.signal(false, factory(&source));
    launch.0.set.get().unwrap().control.wait_definitions();
    assert_eq!(server.count.load(Ordering::Acquire), 0);
    let (wake, _) = channel();
    let empty = contract::compile("component App\n  view\n    text \"empty\"\n")
        .unwrap()
        .encode();
    let (_bridge, _) = boot(source.clone(), &empty, false, wake);
    launch.signal(true, factory(&source));
    launch.signal(true, factory(&source));
    server.request();
    server.release(0);
    assert_eq!(server.count.load(Ordering::Acquire), 1);
}

#[test]
fn exclusions_do_not_even_construct_the_data_source() {
    const CHILD: &str = "EXACT_PRELOAD_EXCLUSION_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let launch = LaunchPreloads::default();
        launch.begin(false, || -> Source { panic!("excluded factory") });
        launch.signal(false, || -> Source { panic!("excluded factory") });
        launch.signal(true, || -> Source { panic!("excluded factory") });
        assert!(launch.0.set.get().is_none());
        return;
    }
    for exclusion in EXCLUSIONS {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "launch_preloads::tests::exclusions_do_not_even_construct_the_data_source",
            ])
            .env(CHILD, "1")
            .env(exclusion, "")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{exclusion}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

struct LaunchSecret {
    reads: Arc<AtomicUsize>,
    caller: std::thread::ThreadId,
    mode: u8,
}
impl ibex2::secrets::SecretStore for LaunchSecret {
    fn get(&self, _: &str) -> Result<Option<String>, ibex2::boundary::HostError> {
        assert_ne!(self.caller, std::thread::current().id());
        self.reads.fetch_add(1, Ordering::AcqRel);
        match self.mode {
            1 => Ok(None),
            2 => Err(ibex2::boundary::HostError::Failed(
                "locked before first unlock".into(),
            )),
            _ => Ok(Some("present-secret".into())),
        }
    }
    fn set(&self, _: &str, _: &str) -> Result<(), ibex2::boundary::HostError> {
        unreachable!()
    }
    fn forget(&self, _: &str) -> Result<(), ibex2::boundary::HostError> {
        unreachable!()
    }
}
fn secret_reader(launch: &LaunchPreloads, mode: u8, reads: Arc<AtomicUsize>) {
    let caller = std::thread::current().id();
    *launch.0.reader.lock().unwrap() = Some(Arc::new(move |grants, _, defs| {
        let backend = ibex2::host::Host::new().with_secret_store(Box::new(LaunchSecret {
            reads: reads.clone(),
            caller,
            mode,
        }));
        let bindings =
            backend.endow(ibex2::grant::GrantSet::parse(&exact_runner::io_grants(grants)).unwrap());
        crate::store::read_launch_secrets(&bindings, grants, defs)
    }));
}

#[test]
fn secrets_are_read_once_off_the_caller_and_unavailable_definitions_are_named_without_request_bytes(
) {
    let server = Server::new(1);
    let source = server.source(PreloadWhen::Start, true);
    let launch = fixture();
    let reads = Arc::new(AtomicUsize::new(0));
    secret_reader(&launch, 0, reads.clone());
    launch.begin(false, factory(&source));
    assert!(server.request().contains("present-secret"));
    server.release(0);
    ready(&launch);
    assert_eq!(reads.load(Ordering::Acquire), 1);
    let facts = launch.journal();
    assert_eq!(facts[0].name, "feed");
    let text = format!("{facts:?}");
    for private in [
        "private-path",
        "private-query",
        "Authorization",
        "present-secret",
    ] {
        assert!(!text.contains(private), "{text}");
    }

    for unavailable in [1, 2, 3] {
        let launch = fixture();
        let mut source = source.clone();
        if unavailable == 3 {
            source.grants.push_str(" user-presence");
        }
        let reads = Arc::new(AtomicUsize::new(0));
        secret_reader(&launch, unavailable, reads.clone());
        launch.begin(false, factory(&source));
        launch.0.set.get().unwrap().control.wait_definitions();
        assert_eq!(reads.load(Ordering::Acquire), usize::from(unavailable != 3));
        let facts = launch.journal();
        assert!(
            facts
                .iter()
                .any(|f| f.name == "feed"
                    && f.outcome == crate::executor::PreloadEvent::MissingSecret)
        );
        assert_eq!(server.count.load(Ordering::Acquire), 1);
    }
}

#[test]
fn a_boot_during_the_secret_read_adopts_before_dispatch_and_does_not_send_twice() {
    let server = Server::new(1);
    let source = server.source(PreloadWhen::Start, true);
    let launch = fixture();
    let (entered, entering) = channel();
    let (release, released) = channel();
    let released = Mutex::new(released);
    *launch.0.reader.lock().unwrap() = Some(Arc::new(move |_, _, _| {
        entered.send(()).unwrap();
        released.lock().unwrap().recv_timeout(HANG).unwrap();
        BTreeMap::from([("session".into(), "present-secret".into())])
    }));
    launch.begin(false, factory(&source));
    entering.recv_timeout(HANG).unwrap();
    let (wake, wakes) = channel();
    let (mut bridge, batch) = boot(source.clone(), &plan(), false, wake);
    assert!(!batch.contains("\"text\":\"content\""));
    assert_eq!(server.count.load(Ordering::Acquire), 0);
    release.send(()).unwrap();
    server.request();
    server.release(0);
    while source.parsed.load(Ordering::Acquire) == 0 {
        wakes.recv_timeout(HANG).unwrap();
        bridge.pump(0.);
    }
    assert_eq!(server.count.load(Ordering::Acquire), 1);
}

#[test]
fn setting_or_forgetting_a_secret_drops_inflight_and_ready_preloads_before_and_after_adoption() {
    for adopted in [false, true] {
        for complete in [false, true] {
            for forget in [false, true] {
                let server = Server::new(1);
                let mut source = server.source(PreloadWhen::Start, true);
                source.forget = forget;
                let launch = fixture();
                *launch.0.reader.lock().unwrap() = Some(Arc::new(|_, _, _| {
                    BTreeMap::from([("session".into(), "present-secret".into())])
                }));
                launch.begin(false, factory(&source));
                server.request();
                if complete {
                    server.release(0);
                    ready(&launch);
                }
                let (wake, _) = channel();
                let text = if adopted {
                    "shape Reply\n  value: string\ncomponent App\n  mutation result as shape Reply\n  action change\n    send result = rotate()\n  view\n    button press=change testId=\"change\"\n      text \"change\"\n"
                } else {
                    "shape Reply\n  value: string\ncomponent App\n  resource result = rotate() as shape Reply\n  view\n    text result.value\n"
                };
                let bytes = contract::compile(text).unwrap().encode();
                let (mut bridge, batch) = boot(source, &bytes, false, wake);
                if adopted {
                    let json: serde_json::Value = serde_json::from_str(&batch).unwrap();
                    let id = json["ops"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|op| op["kind"] == "button" || op["props"]["testId"] == "change")
                        .and_then(|op| op["id"].as_u64())
                        .expect("button in first batch") as u32;
                    bridge.dispatch(id, 0, 0, 0.);
                }
                server.release(0);
                let journal = launch.journal();
                // The host may already have drained it into the runner's journal.
                let len = bridge.input_write(br#"{"op":"logs"}"#);
                let len = bridge.agent(len);
                let logs = String::from_utf8_lossy(bridge.output_bytes(len as usize));
                assert!(
                    journal
                        .iter()
                        .any(|f| f.outcome == crate::executor::PreloadEvent::DroppedBySecret)
                        || logs.contains("DroppedBySecret"),
                    "{journal:?} {logs}"
                );
                assert_eq!(server.count.load(Ordering::Acquire), 1);
            }
        }
    }
}
