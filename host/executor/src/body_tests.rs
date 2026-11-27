//! A request body read from an app file (`Request::body_from`, TypeScript's
//! `exactBodyFrom`; LLP 1108 D6 R2): the worker reads it under the request's
//! `fs.read` grant just before the request goes out, and refuses, sending
//! nothing, when it may not or cannot.
use super::*;
use std::io::Write as _;

/// Each body that reached the transport, with its headers.
type Sent = Arc<Mutex<Vec<(Vec<u8>, Vec<(String, String)>)>>>;

/// Records each body that reached the transport, with its headers.
struct Recorder(Sent);
impl Transport for Recorder {
    fn open(
        &self,
        request: &ibex2::stdlib::fetch::Request,
        signal: &AbortSignal,
    ) -> Result<StreamingResponse, HostError> {
        self.0
            .lock()
            .unwrap()
            .push((body_of(request), request.headers.entries().to_vec()));
        Ok(ibex2::stdlib::fetch::Response {
            status: 200,
            status_text: "OK".into(),
            headers: Headers::default(),
            body: b"sent".to_vec(),
            url: request.url.clone(),
            redirected: false,
        }
        .into_stream(request.body_limit(), signal.clone()))
    }
}

/// A request's body as a transport sends it: its bytes, or its file's,
/// read from the first byte.
fn body_of(request: &ibex2::stdlib::fetch::Request) -> Vec<u8> {
    match &request.body_file {
        Some(file) => {
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut file.0.reader().unwrap(), &mut bytes).unwrap();
            assert_eq!(bytes.len() as u64, file.0.len());
            bytes
        }
        None => request.body.clone().unwrap_or_default(),
    }
}

struct Files(std::path::PathBuf);
impl Files {
    fn new() -> Files {
        let root = std::env::temp_dir().join(format!(
            "exact-body-from-{}-{}",
            std::process::id(),
            ibex2::stdlib::crypto::random_uuid().unwrap()
        ));
        for dir in ["data", "cache", "tmp"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        Files(root)
    }
    fn roots(&self) -> [std::path::PathBuf; 3] {
        ["data", "cache", "tmp"].map(|dir| self.0.join(dir))
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn recording(grants: &str) -> (Core, Sent, Receiver<()>) {
    let sent = Sent::default();
    let owners = (0..WORKERS)
        .map(|_| {
            Some(
                ibex2::host::Host::with_transport(Box::new(Recorder(sent.clone()))).endow(
                    ibex2::grant::GrantSet::parse(&exact_runner::io_grants(grants)).unwrap(),
                ),
            )
        })
        .collect();
    let (wake, woke) = channel();
    let core = Core::with_owners(
        owners,
        grants,
        Box::new(move || {
            let _ = wake.send(());
        }),
    );
    (core, sent, woke)
}

fn upload(path: &str) -> Request {
    let mut request = Request::get("https://example.test/upload")
        .header("content-type", "image/jpeg")
        .body_from(path);
    request.method = "POST".into();
    request
}

const GRANTS: &str = "net.fetch https://example.test\nfs.read app:/tmp";

struct Gate {
    open: Arc<(Mutex<bool>, Condvar)>,
    entered: Receiver<()>,
}
impl Gate {
    fn new() -> (Self, Arc<dyn Fn() + Send + Sync>) {
        let open = Arc::new((Mutex::new(false), Condvar::new()));
        let waiting = open.clone();
        let (entered, entry) = channel();
        let hook = Arc::new(move || {
            entered.send(()).unwrap();
            let (lock, changed) = &*waiting;
            let (lock, _) = changed
                .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(30), |open| !*open)
                .unwrap();
            assert!(*lock, "held reader was not released");
        });
        (
            Self {
                open,
                entered: entry,
            },
            hook,
        )
    }
    fn wait(&self) {
        self.entered.recv_timeout(Duration::from_secs(30)).unwrap();
    }
    fn release(&self) {
        *self.open.0.lock().unwrap() = true;
        self.open.1.notify_all();
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        self.release();
    }
}

#[test]
fn a_finished_initializer_returns_its_reader_before_the_upload_continues() {
    let _turn = one_at_a_time();
    super::body::settle_readers();
    let files = Files::new();
    std::fs::write(files.0.join("tmp/photo.jpg"), b"photo").unwrap();
    let (core, _, _) = recording(GRANTS);
    core.set_app_roots(files.roots());
    let (held, hold) = Gate::new();
    let readers: Vec<_> = (1..super::body::MAX_READERS)
        .map(|_| {
            let hold = hold.clone();
            super::body::reader(move || hold()).unwrap()
        })
        .collect();
    for _ in &readers {
        held.wait();
    }
    let (reply, hook) = Gate::new();
    let once = std::sync::atomic::AtomicBool::new(false);
    *super::body::READERS.replied.lock().unwrap() = Some(Arc::new(move || {
        if !once.swap(true, Ordering::AcqRel) {
            hook();
        }
    }));
    let mut request = upload("app:/tmp/photo.jpg");
    let roots = super::body::prepare(&core.shared, &request, &|| None, None).unwrap();
    reply.wait();
    let abort = AbortController::new();
    let result = super::body::resolve(
        roots.as_ref(),
        GRANTS,
        &mut request,
        &|| None,
        None,
        abort.signal(),
    );
    reply.release();
    held.release();
    for reader in readers {
        reader.join().unwrap();
    }
    *super::body::READERS.replied.lock().unwrap() = None;
    let mut sent = ibex2::stdlib::fetch::Request::get("https://example.test/upload");
    sent.body_file = Some(result.unwrap().expect("a file body"));
    assert_eq!(body_of(&sent), b"photo");
}

/// A file's size is taken when it is opened: one that grows sends that
/// many bytes, and one cut shorter while it is sent fails its body, named,
/// rather than sending a short body under a long length (LLP 1133.008.000 D4).
#[test]
fn a_file_body_keeps_its_size_at_open_and_a_shrinking_one_fails_named() {
    let _turn = one_at_a_time();
    super::body::settle_readers();
    let files = Files::new();
    let path = files.0.join("tmp/clip.mp4");
    std::fs::write(&path, vec![3; 600_000]).unwrap();
    let (core, _, _) = recording(GRANTS);
    core.set_app_roots(files.roots());
    let opened = || {
        let mut request = upload("app:/tmp/clip.mp4");
        let roots = super::body::prepare(&core.shared, &request, &|| None, None).unwrap();
        let abort = AbortController::new();
        super::body::resolve(
            roots.as_ref(),
            GRANTS,
            &mut request,
            &|| None,
            None,
            abort.signal(),
        )
        .unwrap()
        .expect("a file body")
    };
    let file = opened();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&[9; 1000])
        .unwrap();
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut file.0.reader().unwrap(), &mut out).unwrap();
    assert_eq!(
        out,
        vec![3; 600_000],
        "a grown file sent more than its size at open"
    );
    let file = opened();
    assert_eq!(file.0.len(), 601_000);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(1000)
        .unwrap();
    let error =
        std::io::Read::read_to_end(&mut file.0.reader().unwrap(), &mut Vec::new()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("exactBodyFrom app:/tmp/clip.mp4: the file shrank while it was sent"),
        "{error}"
    );
}

#[test]
fn a_body_from_an_app_file_reaches_the_transport_byte_for_byte() {
    let _turn = one_at_a_time();
    let files = Files::new();
    let bytes: Vec<u8> = (0..300_000u32).map(|i| (i * 7 + i / 255) as u8).collect();
    std::fs::write(files.0.join("tmp/photo.jpg"), &bytes).unwrap();
    let (core, sent, woke) = recording(GRANTS);
    core.set_app_roots(files.roots());
    // Ordered, and on the independent lane.
    core.run(job(1, upload("app:/tmp/photo.jpg")), None)
        .unwrap();
    core.run(
        job(2, upload("app:/tmp/photo.jpg").independent_http(1024)),
        None,
    )
    .unwrap();
    let outcomes = collect(&core, &woke, 2);
    for (_, outcome) in &outcomes {
        assert!(
            matches!(outcome, Outcome::Response(r) if r.body == b"sent"),
            "{outcome:?}"
        );
    }
    let sent = sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    for (body, headers) in sent.iter() {
        assert!(*body == bytes, "the body differs from the file");
        // The author's Content-Type, and none added.
        let types: Vec<_> = headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .collect();
        assert_eq!(
            types,
            [&("content-type".to_string(), "image/jpeg".to_string())]
        );
    }
}

#[test]
fn a_body_from_a_file_is_refused_unsent_past_its_grant_scope_or_with_another_body() {
    let _turn = one_at_a_time();
    let files = Files::new();
    std::fs::write(files.0.join("tmp/photo.jpg"), b"photo").unwrap();
    std::fs::write(files.0.join("data/note"), b"note").unwrap();
    let (core, sent, woke) = recording(GRANTS);
    core.set_app_roots(files.roots());
    // Outside the app's grants; and a source scope without `fs.read`.
    core.run(job(1, upload("app:/data/note")), None).unwrap();
    let mut scoped = upload("app:/tmp/photo.jpg");
    scoped.grants = Some("net.fetch https://example.test".into());
    core.run(job(2, scoped), None).unwrap();
    core.run(job(3, upload("app:/tmp/../data/note")), None)
        .unwrap();
    for (ticket, outcome) in collect(&core, &woke, 3) {
        match outcome {
            Outcome::Failed { kind, message } => {
                assert_eq!(kind, FailureKind::Refused, "{ticket}: {message}");
                assert!(
                    message.starts_with("exactBodyFrom app:/"),
                    "{ticket}: {message}"
                );
                if ticket < 3 {
                    assert!(
                        message.contains(": denied: fs.read "),
                        "{ticket}: {message}"
                    );
                }
            }
            other => panic!("{ticket}: {other:?}"),
        }
    }
    // Two bodies, or a body on a GET, are refused at admission.
    let mut both = upload("app:/tmp/photo.jpg");
    both.body = b"x".to_vec();
    assert_eq!(
        core.run(job(4, both), None),
        Err("fetch: a request has one body: body or exactBodyFrom".into())
    );
    assert!(sent.lock().unwrap().is_empty(), "a refused body was sent");
}

/// The app's directories are opened at the first request whose body is one
/// of the app's files (not at boot: a cold boot opens no app storage) and
/// pinned from then on: a root replaced later by a symlink to another tree is
/// not followed, as storage's own handles do not follow it.
#[cfg(unix)]
#[test]
fn a_root_replaced_after_the_first_file_body_is_not_followed() {
    let _turn = one_at_a_time();
    let files = Files::new();
    let other = Files::new();
    std::fs::write(files.0.join("tmp/photo.jpg"), b"the app's").unwrap();
    std::fs::write(other.0.join("tmp/photo.jpg"), b"another's").unwrap();
    let (core, sent, woke) = recording(GRANTS);
    core.set_app_roots(files.roots());
    core.run(job(1, upload("app:/tmp/photo.jpg")), None)
        .unwrap();
    collect(&core, &woke, 1);
    std::fs::rename(files.0.join("tmp"), files.0.join("tmp-moved")).unwrap();
    std::os::unix::fs::symlink(other.0.join("tmp"), files.0.join("tmp")).unwrap();
    core.run(job(2, upload("app:/tmp/photo.jpg")), None)
        .unwrap();
    let outcomes = collect(&core, &woke, 1);
    assert!(
        matches!(&outcomes[0].1, Outcome::Response(_)),
        "{outcomes:?}"
    );
    let sent = sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1].0, b"the app's", "the replaced root was followed");
}

/// Naming the app's directories opens nothing, nor does a request without a
/// file body: a host whose app never sends one leaves them uncreated (a cold
/// boot opens no app storage).
#[test]
fn naming_the_roots_does_not_make_them() {
    let base = std::env::temp_dir().join(format!(
        "exact-roots-{}-{}",
        std::process::id(),
        ibex2::stdlib::crypto::random_uuid().unwrap()
    ));
    let roots = ["data", "cache", "tmp"].map(|d| base.join(d));
    let (core, _sent, woke) = recording(GRANTS);
    core.set_app_roots(roots);
    core.run(job(1, Request::get("https://example.test/read")), None)
        .unwrap();
    collect(&core, &woke, 1);
    assert!(
        !base.exists(),
        "naming the roots, or a plain request, made them"
    );
}

/// A root that is already a symbolic link when the first file body opens
/// the roots is refused, not pinned; nothing is sent or made, and the
/// refusal stands once the link is gone (Grok, Astra: review).
#[cfg(unix)]
#[test]
fn a_root_that_is_a_symlink_at_the_first_file_body_is_refused() {
    let _turn = one_at_a_time();
    let files = Files::new();
    let other = Files::new();
    std::fs::write(other.0.join("tmp/photo.jpg"), b"another's").unwrap();
    std::fs::remove_dir(files.0.join("data")).unwrap();
    std::fs::remove_dir(files.0.join("tmp")).unwrap();
    std::os::unix::fs::symlink(other.0.join("tmp"), files.0.join("tmp")).unwrap();
    let (core, sent, woke) = recording(GRANTS);
    core.set_app_roots(files.roots());
    let refused = |outcomes: &[(u64, Outcome)]| {
        matches!(&outcomes[0].1, Outcome::Failed { kind: FailureKind::Refused, message }
            if message.contains("is a symbolic link"))
    };
    core.run(job(1, upload("app:/tmp/photo.jpg")), None)
        .unwrap();
    let outcomes = collect(&core, &woke, 1);
    assert!(refused(&outcomes), "{outcomes:?}");
    assert!(!files.0.join("data").exists(), "a refused open made a root");
    std::fs::remove_file(files.0.join("tmp")).unwrap();
    std::fs::create_dir(files.0.join("tmp")).unwrap();
    std::fs::write(files.0.join("tmp/photo.jpg"), b"the app's").unwrap();
    core.run(job(2, upload("app:/tmp/photo.jpg")), None)
        .unwrap();
    let outcomes = collect(&core, &woke, 1);
    assert!(refused(&outcomes), "{outcomes:?}");
    assert!(
        sent.lock().unwrap().is_empty(),
        "the symlinked root was followed"
    );
}

/// A stream's body read from an app file opens the roots when it is the
/// first file body, as a worker's request does (Astra, Grok: review).
#[test]
fn a_stream_whose_body_is_the_first_app_file_opens_the_roots() {
    let _turn = one_at_a_time();
    let files = Files::new();
    std::fs::write(files.0.join("tmp/photo.jpg"), b"the app's").unwrap();
    let (core, sent, _woke) = recording(GRANTS);
    let streamed = sent.clone();
    let core = core.streams_on(move || {
        ibex2::host::Host::with_transport(Box::new(Recorder(streamed.clone())))
    });
    core.set_app_roots(files.roots());
    let mut request = upload("app:/tmp/photo.jpg");
    request.stream = true;
    request.http = HttpScheduling::Independent {
        max_response_bytes: 1 << 20,
    };
    core.run(job(1, request), None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while sent.lock().unwrap().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let sent = sent.lock().unwrap();
    assert_eq!(sent.first().map(|s| &s.0[..]), Some(&b"the app's"[..]));
}

/// A read that outlasts the request's deadline does not hold it: the worker
/// settles `Timeout` when the deadline passes, tells the read, which stops
/// at its next chunk, and drops its late bytes (Astra, round 3).
#[test]
fn a_stalled_read_is_settled_by_the_deadline_and_told_to_stop() {
    static READERS: super::body::Readers = super::body::Readers::new();
    let passed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = passed.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        flag.store(true, Ordering::Release);
    });
    let started = Instant::now();
    let result = super::body::acquire(
        &READERS,
        |cancel, _| {
            // A read of many chunks, each slow, that looks between them.
            for _ in 0..300 {
                if cancel.load(Ordering::Acquire) {
                    return Err(failed(FailureKind::Refused, "the request ended"));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(b"late".to_vec())
        },
        &|| {
            passed
                .load(Ordering::Acquire)
                .then(|| failed(FailureKind::Timeout, "the request timed out after 30 ms"))
        },
    );
    assert!(
        matches!(
            &result,
            Err(Outcome::Failed {
                kind: FailureKind::Timeout,
                ..
            })
        ),
        "{result:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    // Told, the reader stops at its next chunk and gives its place back.
    let until = Instant::now() + Duration::from_secs(2);
    while READERS.running() > 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(READERS.running(), 0);
    // A read that finishes after its request ended is not taken either.
    let result = super::body::acquire(&READERS, |_, _| Ok(b"bytes".to_vec()), &|| {
        Some(failed(FailureKind::Aborted, "native request aborted"))
    });
    assert!(matches!(
        result,
        Err(Outcome::Failed {
            kind: FailureKind::Aborted,
            ..
        })
    ));
}

/// Readers stuck on a stalled disk keep their places after their requests
/// were settled; past the bound a new file body is refused, unsent, until
/// one returns (Astra, post-landing).
#[test]
fn stuck_readers_are_bounded_and_keep_their_place_until_they_return() {
    static READERS: super::body::Readers = super::body::Readers::new();
    let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stuck = |release: Arc<std::sync::atomic::AtomicBool>| {
        // A read blocked in one call, as on a stalled disk: it cannot look.
        move |_: &std::sync::atomic::AtomicBool, _: &super::body::Place| {
            while !release.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(Vec::<u8>::new())
        }
    };
    let deadline_after_start = |first: &std::cell::Cell<bool>| {
        let fired = !first.get();
        first.set(false);
        fired.then(|| failed(FailureKind::Timeout, "the request timed out after 1 ms"))
    };
    for _ in 0..super::body::MAX_READERS {
        let first = std::cell::Cell::new(true);
        let result = super::body::acquire(&READERS, stuck(release.clone()), &|| {
            deadline_after_start(&first)
        });
        assert!(matches!(
            result,
            Err(Outcome::Failed {
                kind: FailureKind::Timeout,
                ..
            })
        ));
    }
    assert_eq!(READERS.running(), super::body::MAX_READERS);
    let first = std::cell::Cell::new(true);
    let refused = super::body::acquire(&READERS, |_, _| Ok(b"never".to_vec()), &|| {
        deadline_after_start(&first)
    });
    assert!(
        matches!(&refused, Err(Outcome::Failed { kind: FailureKind::Refused, message })
            if message.contains("file reads are still running")),
        "{refused:?}"
    );
    release.store(true, Ordering::Release);
    let until = Instant::now() + Duration::from_secs(2);
    while READERS.running() > 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(READERS.running(), 0);
    let ok = super::body::acquire(&READERS, |_, _| Ok(b"bytes".to_vec()), &|| None);
    assert_eq!(ok.unwrap(), b"bytes");
}

#[cfg(unix)]
#[test]
fn a_root_substituted_after_its_check_is_refused_without_sending() {
    let _turn = one_at_a_time();
    let files = Files::new();
    let other = Files::new();
    std::fs::write(other.0.join("tmp/photo.jpg"), b"outside").unwrap();
    let (core, sent, woke) = recording(GRANTS);
    core.set_app_roots(files.roots());
    let (root, outside) = (files.0.join("tmp"), other.0.join("tmp"));
    *core.shared.roots_checked.lock().unwrap() = Some(Arc::new(move || {
        std::fs::remove_dir(&root).unwrap();
        std::os::unix::fs::symlink(&outside, &root).unwrap();
    }));
    core.run(job(1, upload("app:/tmp/photo.jpg")), None)
        .unwrap();
    let outcomes = collect(&core, &woke, 1);
    assert!(
        matches!(
            &outcomes[0].1,
            Outcome::Failed {
                kind: FailureKind::Refused,
                ..
            }
        ),
        "{outcomes:?}"
    );
    assert!(sent.lock().unwrap().is_empty(), "outside bytes were sent");
}

#[test]
fn initialization_is_bounded_by_the_requests_deadline_and_abort() {
    let _turn = one_at_a_time();
    for destination in [false, true] {
        for timeout in [true, false] {
            let files = Files::new();
            std::fs::write(files.0.join("tmp/photo.jpg"), b"photo").unwrap();
            let (core, sent, woke) = recording(&format!("{GRANTS}\nfs.write app:/tmp"));
            core.set_app_roots(files.roots());
            let (held, hook) = Gate::new();
            *core.shared.roots_checked.lock().unwrap() = Some(hook);
            let (armed, deadline) = channel();
            *core.shared.deadline_armed.lock().unwrap() = Some(armed);
            let mut request = if destination {
                Request::get("https://example.test/download").body_to("app:/tmp/new")
            } else {
                upload("app:/tmp/photo.jpg")
            };
            if timeout {
                request = request.timeout(60_000);
            }
            core.run(job(1, request), None).unwrap();
            held.wait();
            if timeout {
                if let Some(machine) = &core.machine {
                    let run = {
                        let state = core.shared.state.lock().unwrap();
                        let run = &state.running[0];
                        (run.ticket, run.generation)
                    };
                    machine.timer.expire(run);
                } else {
                    deadline
                        .recv_timeout(Duration::from_secs(30))
                        .unwrap()
                        .expire();
                }
                let outcomes = collect(&core, &woke, 1);
                assert!(
                    matches!(
                        &outcomes[0].1,
                        Outcome::Failed {
                            kind: FailureKind::Timeout,
                            ..
                        }
                    ),
                    "{outcomes:?}"
                );
            } else {
                core.cancel(&[1]);
                let until = Instant::now() + Duration::from_secs(30);
                while core
                    .shared
                    .state
                    .lock()
                    .unwrap()
                    .running
                    .iter()
                    .any(|r| !r.terminal)
                {
                    assert!(Instant::now() < until, "abort did not settle");
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            assert!(sent.lock().unwrap().is_empty());
            assert!(!files.0.join("tmp/new").exists());
            assert!(core.shared.roots.get().is_none());
            assert_eq!(
                super::super::body::READERS.running(),
                1,
                "a stalled initializer must keep its reader permit"
            );
            held.release();
            super::body_to::physically_ended(&core);
            let until = Instant::now() + Duration::from_secs(30);
            while super::super::body::READERS.running() != 0 && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(super::super::body::READERS.running(), 0);
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.counts, [0, 0]);
            assert_eq!(state.bytes, [0, 0]);
            assert_eq!(state.book.reservations(), (0, 0, 0));
            drop(state);
            assert!(core.drain().is_empty());
            assert!(sent.lock().unwrap().is_empty());
            assert!(!files.0.join("tmp/new").exists());
        }
    }
}

fn request_for_invalid_case(case: &str) -> Request {
    let mut request = Request::get("https://example.test/read");
    if case == "header" {
        request = request.header("x-test", "a\0b");
    } else {
        request.method = "G\0ET".into();
    }
    request
}

fn check_response_fixture(case: &str) {
    let _turn = one_at_a_time();
    struct Source;
    impl exact_runner::DataSource for Source {
        fn grants(&self) -> &str {
            GRANTS
        }
        fn query(
            &mut self,
            _: &str,
            _: &[exact_runner::Value],
        ) -> Result<exact_runner::Value, exact_runner::DataError> {
            unreachable!()
        }
    }
    let mut builder = exact_plan::builder::PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    builder.node(
        exact_kernel::NodeType::View as u8,
        None,
        None,
        0,
        &[],
        &[],
        None,
    );
    let mut runner = exact_runner::Runner::boot(
        builder.finish().unwrap(),
        Source,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let files = Files::new();
    std::fs::write(files.0.join("tmp/present"), b"upload").unwrap();
    let (core, sent, woke) = recording(GRANTS);
    core.set_app_roots(files.roots());
    let prefix = "https://example.test/";
    {
        runner
            .faults()
            .answer(
                prefix,
                &serde_json::json!({"status": 200, "headers": {}, "body": "fixture"}),
            )
            .unwrap();
        let mut request = upload(if case == "missing" {
            "app:/tmp/missing"
        } else {
            "app:/tmp/present"
        });
        if case == "header" {
            request = request.header("x-test", "a\0b");
        }
        if case == "method" {
            request.method = "G\0ET".into();
        }
        if case == "denied" {
            request.grants = Some("net.fetch https://example.test".into());
        }
        if case == "destination" {
            request.body_to = Some("app:/tmp/result".into());
        }
        if case == "ceiling" {
            request = request.independent_http(1);
        }
        if case == "deadline" {
            request = request.timeout(60_000);
            if core.machine.is_none() {
                let (armed, deadline) = channel();
                *core.shared.deadline_armed.lock().unwrap() = Some(armed);
                let deadline = Mutex::new(deadline);
                *core.shared.roots_checked.lock().unwrap() = Some(Arc::new(move || {
                    deadline
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap()
                        .expire();
                }));
            }
        }
        let held = if case == "abort" || (case == "deadline" && core.machine.is_some()) {
            let (held, hook) = Gate::new();
            *core.shared.roots_checked.lock().unwrap() = Some(hook);
            Some(held)
        } else {
            None
        };
        let out = job(1, request);
        let Some(exact_runner::Dispatch::Run(work)) = runner.fault_dispatch(&out) else {
            panic!("fixture dispatch");
        };
        let admitted = core.run_owned(out, Some(work));
        if matches!(case, "header" | "method") {
            assert!(admitted.is_err(), "invalid request was admitted");
            let (real, _, _) = recording(GRANTS);
            assert!(
                real.run_owned(job(2, request_for_invalid_case(case)), None)
                    .is_err(),
                "real transport must reject the same syntax"
            );
        } else {
            admitted.unwrap();
        }
        if matches!(case, "header" | "method") {
            assert!(core.drain().is_empty());
        } else if let Some(held) = &held {
            held.wait();
            if case == "deadline" {
                let run = {
                    let state = core.shared.state.lock().unwrap();
                    let running = state.running.iter().find(|r| r.ticket == 1).unwrap();
                    (running.ticket, running.generation)
                };
                core.machine.as_ref().unwrap().timer.expire(run);
                let outcome = collect(&core, &woke, 1).remove(0).1;
                held.release();
                assert!(
                    matches!(
                        outcome,
                        Outcome::Failed {
                            kind: FailureKind::Timeout,
                            ..
                        }
                    ),
                    "{outcome:?}"
                );
            } else {
                core.cancel(&[1]);
                let cancelled = core
                    .shared
                    .state
                    .lock()
                    .unwrap()
                    .running
                    .iter()
                    .all(|r| r.exchange.as_ref().is_some_and(|a| a.signal().aborted()));
                held.release();
                super::body_to::physically_ended(&core);
                assert!(cancelled, "Core::cancel did not signal fixture preparation");
                assert!(core.drain().is_empty());
            }
        } else {
            let outcome = collect(&core, &woke, 1).remove(0).1;
            if case == "valid" {
                assert!(
                    matches!(outcome, Outcome::Response(ref r) if r.body == b"fixture"),
                    "{outcome:?}"
                );
            } else {
                let expected = match case {
                    "deadline" => FailureKind::Timeout,
                    "abort" => FailureKind::Aborted,
                    _ => FailureKind::Refused,
                };
                assert!(
                    matches!(outcome, Outcome::Failed { kind, .. } if kind == expected),
                    "{case}: {outcome:?}"
                );
            }
        }
        let entries: serde_json::Value = serde_json::from_str(&runner.faults().json()).unwrap();
        assert_eq!(entries[0]["hits"], u32::from(case == "valid"), "{case}");
        runner.take_fault_logs();
        assert_eq!(
            runner
                .journal()
                .filter(|line| line.contains("fetch answered (driver fixture)"))
                .count(),
            usize::from(case == "valid"),
            "{case}"
        );
    }
    assert!(
        sent.lock().unwrap().is_empty(),
        "a fixture reached the network"
    );
}

#[test]
fn response_fixture_invalid_header_is_refused() {
    check_response_fixture("header");
}
#[test]
fn response_fixture_invalid_method_is_refused() {
    check_response_fixture("method");
}

#[test]
fn response_fixture_missing_file_is_refused() {
    check_response_fixture("missing");
}
#[test]
fn response_fixture_denied_file_is_refused() {
    check_response_fixture("denied");
}
#[test]
fn response_fixture_destination_refusal_has_no_hit() {
    check_response_fixture("destination");
}
#[test]
fn response_fixture_ceiling_refusal_has_no_hit() {
    check_response_fixture("ceiling");
}
#[test]
fn response_fixture_valid_file_is_prepared_without_transport() {
    check_response_fixture("valid");
}

#[test]
fn response_fixture_expired_during_file_preparation_has_no_hit() {
    check_response_fixture("deadline");
}
#[test]
fn response_fixture_aborted_during_file_preparation_has_no_hit() {
    check_response_fixture("abort");
}

#[test]
fn response_fixture_reads_keep_the_128_request_admission_limit() {
    let (core, sent, woke) = recording(GRANTS);
    for ticket in 1..=128 {
        let mut out = job(ticket, Request::get("https://example.test/read"));
        out.class = RequestClass::Read;
        core.run_owned(
            out,
            Some(OwnedWork::HttpResponse(Arc::new(|| {
                Outcome::Response(Response {
                    status: 200,
                    headers: vec![],
                    body: b"fixture".to_vec(),
                })
            }))),
        )
        .unwrap_or_else(|why| panic!("read {ticket}: {why}"));
    }
    let mut out = job(129, Request::get("https://example.test/read"));
    out.class = RequestClass::Read;
    let refusal = core.run_owned(
        out,
        Some(OwnedWork::HttpResponse(Arc::new(|| {
            Outcome::Response(Response {
                status: 200,
                headers: vec![],
                body: vec![],
            })
        }))),
    );
    assert_eq!(
        refusal,
        Err("129 ordered requests requested (128 in use, maximum 128)".into())
    );
    let outcomes = collect(&core, &woke, 128);
    assert!(outcomes
        .iter()
        .all(|(_, out)| matches!(out, Outcome::Response(_))));
    assert!(sent.lock().unwrap().is_empty());
}

#[test]
fn response_fixture_and_transport_normalize_headers_before_validation() {
    for fixture in [false, true] {
        let (core, sent, woke) = recording(GRANTS);
        let request = Request::get("https://example.test/read").header("X-Token", " \r\ntoken\t\n");
        let work = fixture.then(|| {
            OwnedWork::HttpResponse(Arc::new(|| {
                Outcome::Response(exact_runner::Response {
                    status: 200,
                    headers: vec![],
                    body: b"fixture".to_vec(),
                })
            }) as ResponseWork)
        });
        core.run_owned(job(1, request), work).unwrap();
        assert!(matches!(
            collect(&core, &woke, 1)[0].1,
            Outcome::Response(_)
        ));
        if !fixture {
            assert!(sent.lock().unwrap()[0]
                .1
                .contains(&("x-token".into(), "token".into())));
        }
        let request = Request::get("https://example.test/read").header("X-Token", "a\nb");
        assert!(core.run_owned(job(2, request), None).is_err());
    }
}

#[test]
fn response_fixture_uses_the_normal_http_cache() {
    let (core, sent, woke) = recording(GRANTS);
    core.enable_http_cache();
    for ticket in 1..=2 {
        let mut out = job(ticket, Request::get("https://example.test/read"));
        out.class = RequestClass::Read;
        let work = (ticket == 1).then(|| {
            OwnedWork::HttpResponse(Arc::new(|| {
                Outcome::Response(Response {
                    status: 200,
                    headers: vec![("cache-control".into(), "max-age=60".into())],
                    body: b"fixture".to_vec(),
                })
            }))
        });
        core.run_owned(out, work).unwrap();
        let outcome = collect(&core, &woke, 1).remove(0).1;
        assert!(
            matches!(&outcome, Outcome::Response(r) if r.body == b"fixture"),
            "{outcome:?}"
        );
    }
    assert!(sent.lock().unwrap().is_empty());
}
