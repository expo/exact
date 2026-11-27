//! The listener over real `unix:` sockets: lines in and out, the ends a
//! connection can have, the hold within the process (D2) and publication
//! under the lock (D4). Real sockets and wall-clock waits: the async lane.
use super::*;
use std::io::{BufRead, BufReader};
use std::sync::mpsc;

/// One event as the tests read it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seen {
    Listening,
    Connection(u64),
    Line(u64, String),
    End(u64, &'static str),
    Standing(&'static str),
    Error(&'static str),
}

/// A socket's path must fit `sun_path` (104 bytes on macOS), and the per-user
/// `TMPDIR` there takes about 50 of them: keep `name` short.
fn dir(name: &str) -> crate::TempDir {
    let base = std::env::temp_dir().canonicalize().unwrap();
    let dir = base.join(format!("exact-listen-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    crate::TempDir(dir)
}

fn config(path: &std::path::Path, takeover: bool) -> Config {
    Config {
        address: format!("unix:{}", path.display()),
        path: path.to_path_buf(),
        takeover,
        first_line: None,
        max_line: 64,
        max_connections: 4,
    }
}

fn open(config: Config) -> (Listener, mpsc::Receiver<Seen>) {
    let (tx, rx) = mpsc::channel();
    let sink = Box::new(move |event: Event| {
        let seen = match event {
            Event::Listening => Seen::Listening,
            Event::Connection(id) => Seen::Connection(id),
            Event::Line(id, line) => Seen::Line(id, String::from_utf8_lossy(line).into()),
            Event::End(id, reason) => Seen::End(id, reason),
            Event::Standing(word) => Seen::Standing(word),
            Event::Error(failure) => Seen::Error(failure.kind()),
        };
        let _ = tx.send(seen);
        Taken::Queued
    });
    (Listener::open(config, sink).unwrap(), rx)
}

fn next(rx: &mpsc::Receiver<Seen>) -> Seen {
    rx.recv_timeout(Duration::from_secs(10)).expect("an event")
}

/// The next standing, past the probe connections publication makes.
fn standing(rx: &mpsc::Receiver<Seen>) -> Seen {
    loop {
        match next(rx) {
            Seen::Connection(_) | Seen::End(_, "peer") => {}
            seen => return seen,
        }
    }
}

#[test]
#[ignore = "async lane: binds real unix sockets"]
fn lines_in_and_a_reply_out() {
    let d = dir("echo");
    let path = d.join("a.sock");
    let (listener, rx) = open(config(&path, false));
    assert_eq!(next(&rx), Seen::Listening);
    let mut client = UnixStream::connect(&path).unwrap();
    assert_eq!(next(&rx), Seen::Connection(1));
    client.write_all(b"hello\r\nwor").unwrap();
    assert_eq!(next(&rx), Seen::Line(1, "hello".into()));
    client.write_all(b"ld\n").unwrap();
    assert_eq!(next(&rx), Seen::Line(1, "world".into()));
    assert_eq!(listener.view(), vec![(1, 2, 0)]);
    listener.send(1, Some(b"ok"), true).unwrap();
    let mut reply = String::new();
    BufReader::new(&client).read_line(&mut reply).unwrap();
    assert_eq!(reply, "ok\n");
    assert_eq!(next(&rx), Seen::End(1, "app"));
    // Ended: closed both ways, and a later send is dropped.
    let mut rest = Vec::new();
    client.read_to_end(&mut rest).unwrap();
    assert!(rest.is_empty());
    assert_eq!(listener.send(1, Some(b"late"), false), Err(Refused::Gone));
    listener.close();
    assert!(!path.exists(), "the socket file goes with the listener");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
#[ignore = "async lane: binds real unix sockets"]
fn peer_timeout_overflow_and_the_connection_bound() {
    let d = dir("ends");
    let path = d.join("a.sock");
    let mut c = config(&path, false);
    c.first_line = Some(Duration::from_millis(200));
    c.max_connections = 2;
    let (listener, rx) = open(c);
    assert_eq!(next(&rx), Seen::Listening);
    let quiet = UnixStream::connect(&path).unwrap();
    assert_eq!(next(&rx), Seen::Connection(1));
    assert_eq!(next(&rx), Seen::End(1, "timeout"));
    drop(quiet);
    let mut long = UnixStream::connect(&path).unwrap();
    assert_eq!(next(&rx), Seen::Connection(2));
    long.write_all(b"short\n").unwrap();
    assert_eq!(next(&rx), Seen::Line(2, "short".into()));
    long.write_all(&[b'x'; 100]).unwrap();
    assert_eq!(next(&rx), Seen::End(2, "overflow"));
    let a = UnixStream::connect(&path).unwrap();
    assert_eq!(next(&rx), Seen::Connection(3));
    let _b = UnixStream::connect(&path).unwrap();
    assert_eq!(next(&rx), Seen::Connection(4));
    // A third at once is closed with no event.
    let mut over = UnixStream::connect(&path).unwrap();
    let mut rest = Vec::new();
    over.read_to_end(&mut rest).unwrap();
    drop(a);
    assert_eq!(next(&rx), Seen::End(3, "peer"));
    // A line past the send bound is refused, not dropped.
    let big = vec![b'y'; SEND_BYTES];
    assert_eq!(listener.send(4, Some(&big), false), Err(Refused::Quota));
    listener.close();
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
#[ignore = "async lane: binds real unix sockets"]
fn within_the_process_a_newcomer_waits_or_takes_over() {
    let d = dir("hold");
    let path = d.join("a.sock");
    let (first, rx1) = open(config(&path, false));
    assert_eq!(next(&rx1), Seen::Listening);
    // Without takeover it waits, and binds when the holder lets go.
    let (second, rx2) = open(config(&path, false));
    assert_eq!(next(&rx2), Seen::Standing("waiting"));
    first.close();
    assert_eq!(next(&rx2), Seen::Listening);
    // With it, the newest takes the address; the older keeps its connection.
    let kept = UnixStream::connect(&path).unwrap();
    assert_eq!(next(&rx2), Seen::Connection(1));
    let (third, rx3) = open(config(&path, true));
    assert_eq!(next(&rx3), Seen::Listening);
    assert_eq!(next(&rx3), Seen::Standing("took over"));
    assert_eq!(next(&rx2), Seen::Standing("displaced"));
    let _new = UnixStream::connect(&path).unwrap();
    assert_eq!(next(&rx3), Seen::Connection(1));
    second.send(1, Some(b"still mine"), false).unwrap();
    let mut line = String::new();
    BufReader::new(&kept).read_line(&mut line).unwrap();
    assert_eq!(line, "still mine\n");
    // The newer lets go: the older takes the address back.
    third.close();
    assert_eq!(next(&rx2), Seen::Standing("holding"));
    let _back = UnixStream::connect(&path).unwrap();
    assert_eq!(next(&rx2), Seen::Connection(2));
    second.close();
    assert!(!path.exists());
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
#[ignore = "async lane: binds real unix sockets"]
fn another_process_taking_over_displaces_and_its_close_gives_it_back() {
    let d = dir("watch");
    let path = d.join("a.sock");
    let (listener, rx) = open(config(&path, false));
    assert_eq!(next(&rx), Seen::Listening);
    // What another process's takeover does to the path, under the lock.
    let other = bind::publish(&path, true, false).unwrap();
    // Its probe of the live socket is a connection that ends at once.
    assert_eq!(standing(&rx), Seen::Standing("displaced"));
    bind::unpublish(&path, other.identity);
    drop(other);
    assert_eq!(standing(&rx), Seen::Standing("holding"));
    // Connection 1 was the probe, when it was accepted before the takeover.
    let _c = UnixStream::connect(&path).unwrap();
    assert!(matches!(next(&rx), Seen::Connection(1 | 2)));
    listener.close();
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
#[ignore = "async lane: binds real unix sockets"]
fn a_takeover_while_the_older_closes_keeps_the_newer_socket() {
    let d = dir("race");
    let path = d.join("a.sock");
    for _ in 0..50 {
        let older = bind::publish(&path, false, false).unwrap();
        let (p1, p2) = (path.clone(), path.clone());
        let ours = older.identity;
        let closing = std::thread::spawn(move || bind::unpublish(&p1, ours));
        let newer = std::thread::spawn(move || bind::publish(&p2, true, false).unwrap());
        closing.join().unwrap();
        let newer = newer.join().unwrap();
        drop(older);
        assert_eq!(bind::identity(&path), Some(newer.identity));
        assert!(UnixStream::connect(&path).is_ok());
        bind::unpublish(&path, newer.identity);
    }
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
#[ignore = "async lane: binds real unix sockets"]
fn a_live_socket_without_takeover_is_an_error() {
    let d = dir("busy");
    let path = d.join("a.sock");
    let held = bind::publish(&path, false, false).unwrap();
    let (_listener, rx) = open(config(&path, false));
    assert_eq!(next(&rx), Seen::Error("AddressInUse"));
    bind::unpublish(&path, held.identity);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
#[ignore = "binds real unix sockets; `cargo test -- --ignored` runs it"]
fn a_connection_can_be_answered_before_its_callback_returns() {
    let d = dir("greeting");
    let path = d.join("a.sock");
    let (seen, events) = mpsc::channel();
    let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let callback_gate = gate.clone();
    let listener = Listener::open(
        config(&path, false),
        Box::new(move |event| {
            match event {
                Event::Listening => {
                    seen.send(None).unwrap();
                }
                Event::Connection(id) => {
                    seen.send(Some(id)).unwrap();
                    let (lock, changed) = &*callback_gate;
                    let guard = lock.lock().unwrap();
                    let (guard, wait) = changed
                        .wait_timeout_while(guard, Duration::from_secs(10), |open| !*open)
                        .unwrap();
                    assert!(*guard && !wait.timed_out());
                }
                _ => {}
            }
            Taken::Queued
        }),
    )
    .unwrap();
    assert_eq!(events.recv_timeout(Duration::from_secs(10)).unwrap(), None);
    let client = UnixStream::connect(&path).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let id = events
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let sent = listener.send(id, Some(b"welcome"), false);
    *gate.0.lock().unwrap() = true;
    gate.1.notify_one();
    assert_eq!(sent, Ok(()));
    let mut line = String::new();
    BufReader::new(client).read_line(&mut line).unwrap();
    assert_eq!(line, "welcome\n");
    listener.close();
    std::fs::remove_dir_all(&d).unwrap();
}

/// A socket directory renamed away and made again at its path is watched
/// there: another process's takeover in it displaces, and its close gives
/// the address back.
#[test]
#[ignore = "async lane: binds real unix sockets"]
fn a_replaced_directory_is_watched_at_its_path() {
    let d = dir("moved");
    let path = d.join("a.sock");
    let away = crate::TempDir(d.with_extension("old"));
    let _ = std::fs::remove_dir_all(&away);
    let (listener, rx) = open(config(&path, false));
    assert_eq!(next(&rx), Seen::Listening);
    std::fs::rename(&d, &away).unwrap();
    // Made again at its path: by this listener, binding again where its
    // socket was, or here, whichever comes first.
    let _ = std::fs::create_dir(&d);
    let other = bind::publish(&path, true, false).unwrap();
    assert_eq!(standing(&rx), Seen::Standing("displaced"));
    bind::unpublish(&path, other.identity);
    drop(other);
    assert_eq!(standing(&rx), Seen::Standing("holding"));
    listener.close();
    std::fs::remove_dir_all(&d).unwrap();
    std::fs::remove_dir_all(&away).unwrap();
}

/// Build an unbound `Loop` with its own wake pipe for deterministic turn tests.
struct LoopHarness {
    looped: Loop,
    wake_write: RawFd,
    events: mpsc::Receiver<Seen>,
}

impl LoopHarness {
    fn new(config: Config) -> Self {
        let (read, write) = pipe().unwrap();
        let (tx, rx) = mpsc::channel();
        let signal = Arc::new(hold::Signal::new(move || wake(write)));
        let shared = Arc::new(Shared {
            out: Mutex::default(),
            signal,
            wake: write,
        });
        let sink: Sink = Box::new(move |event: Event| {
            let seen = match event {
                Event::Listening => Seen::Listening,
                Event::Connection(id) => Seen::Connection(id),
                Event::Line(id, line) => Seen::Line(id, String::from_utf8_lossy(line).into()),
                Event::End(id, reason) => Seen::End(id, reason),
                Event::Standing(word) => Seen::Standing(word),
                Event::Error(failure) => Seen::Error(failure.kind()),
            };
            let _ = tx.send(seen);
            Taken::Queued
        });
        let looped = Loop {
            config,
            sink,
            shared,
            wake: read,
            bound: None,
            ever_bound: false,
            hold: None,
            watch: None,
            conns: HashMap::new(),
            next: 1,
            stopped: false,
        };
        LoopHarness {
            looped,
            wake_write: write,
            events: rx,
        }
    }
}

impl Drop for LoopHarness {
    fn drop(&mut self) {
        self.looped.teardown();
        // SAFETY: `wake` and `wake_write` belong to this harness and are closed once.
        unsafe {
            libc::close(self.looped.wake);
            libc::close(self.wake_write);
        }
    }
}

#[test]
#[ignore = "async lane: binds real unix sockets"]
fn displaced_before_initial_bind_never_overwrites_new_holder() {
    let d = dir("prebind");
    let path = d.join("a.sock");
    let mut a = LoopHarness::new(config(&path, true));
    let mut b = LoopHarness::new(config(&path, true));

    // A enters first, then pauses before calling `bind`. B takes over and binds.
    let (hold_a, start_a) = hold::enter(&path, true, a.looped.shared.signal.clone());
    assert_eq!(start_a, hold::Start::Bind);
    a.looped.hold = Some(hold_a);

    let (hold_b, start_b) = hold::enter(&path, true, b.looped.shared.signal.clone());
    assert_eq!(start_b, hold::Start::TakeOver);
    b.looped.hold = Some(hold_b);
    assert!(b.looped.bind(true));
    let id_b = b.looped.bound.as_ref().unwrap().identity;
    assert_eq!(bind::identity(&path), Some(id_b));

    // A now attempts its delayed initial bind: it must skip binding and keep B intact.
    assert!(!a.looped.bind(false));
    assert!(a.looped.bound.is_none());
    assert_eq!(bind::identity(&path), Some(id_b));
    b.looped.look();
    assert!(b.looped.bound.is_some());
    assert!(UnixStream::connect(&path).is_ok());

    drop(a);
    drop(b);
    assert!(!path.exists());
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
#[ignore = "async lane: binds real unix sockets"]
fn concurrent_takeover_opens_leave_a_live_holder_and_no_orphaned_socket() {
    let d = dir("race-open");
    let path = d.join("a.sock");
    for round in 0..20 {
        let start = Arc::new(std::sync::Barrier::new(2));
        let spawn = |p: PathBuf, b: Arc<std::sync::Barrier>| {
            std::thread::spawn(move || {
                b.wait();
                open(config(&p, true))
            })
        };
        let t1 = spawn(path.clone(), start.clone());
        let t2 = spawn(path.clone(), start);
        let (l1, rx1) = t1.join().unwrap();
        let (l2, rx2) = t2.join().unwrap();

        // Drain each listener's startup/takeover events until one has taken over
        // and the other is waiting or displaced.
        let mut s1 = Vec::new();
        let mut s2 = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        let settled = |a: &[Seen], b: &[Seen]| {
            let took = |s: &[Seen]| s.contains(&Seen::Standing("took over"));
            let yielded = |s: &[Seen]| {
                s.contains(&Seen::Standing("waiting")) || s.contains(&Seen::Standing("displaced"))
            };
            (took(a) && yielded(b)) || (took(b) && yielded(a))
        };
        while !settled(&s1, &s2) {
            assert!(
                Instant::now() < deadline,
                "round {round} did not settle: {s1:?} / {s2:?}"
            );
            while let Ok(ev) = rx1.try_recv() {
                s1.push(ev);
            }
            while let Ok(ev) = rx2.try_recv() {
                s2.push(ev);
            }
            if !settled(&s1, &s2) {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        assert!(
            !s1.iter().any(|e| matches!(e, Seen::Error(_)))
                && !s2.iter().any(|e| matches!(e, Seen::Error(_))),
            "round {round} unexpected error: {s1:?} / {s2:?}"
        );

        let winner_rx = if s1.contains(&Seen::Standing("took over")) {
            &rx1
        } else {
            &rx2
        };
        let mut client = UnixStream::connect(&path).expect("winning holder accepts connections");
        client.write_all(b"ping\n").unwrap();
        loop {
            match next(winner_rx) {
                Seen::Connection(_) | Seen::End(_, "peer") => {}
                Seen::Line(_, line) => {
                    assert_eq!(line, "ping");
                    break;
                }
                other => panic!("round {round} unexpected winner event: {other:?}"),
            }
        }
        drop(client);
        l1.close();
        l2.close();
        assert!(!path.exists(), "round {round} left an orphaned socket");
    }
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
#[ignore = "async lane: binds real unix sockets"]
fn waiting_loop_with_both_acquire_and_displaced_never_steals_or_fails() {
    for takeover in [false, true] {
        let d = dir(if takeover { "seq2-to" } else { "seq2-no" });
        let path = d.join("a.sock");
        let mut l1 = LoopHarness::new(config(&path, false));
        let mut l2 = LoopHarness::new(config(&path, takeover));
        let mut l3 = LoopHarness::new(config(&path, true));

        let (h1, s1) = hold::enter(&path, false, l1.looped.shared.signal.clone());
        assert_eq!(s1, hold::Start::Bind);
        l1.looped.hold = Some(h1);
        assert!(l1.looped.bind(false));

        let (h2, s2) = hold::enter(&path, false, l2.looped.shared.signal.clone());
        assert_eq!(s2, hold::Start::Wait);
        l2.looped.hold = Some(h2);

        // L1 closes -> raises `acquire` on L2. Before L2 turns, L3 takes over -> raises `displaced`.
        drop(l1);
        let (h3, s3) = hold::enter(&path, true, l3.looped.shared.signal.clone());
        assert_eq!(s3, hold::Start::TakeOver);
        l3.looped.hold = Some(h3);
        assert!(l3.looped.bind(true));
        let id_l3 = l3.looped.bound.as_ref().unwrap().identity;
        assert!(l2.looped.shared.signal.acquire.load(Ordering::SeqCst));
        assert!(l2.looped.shared.signal.displaced.load(Ordering::SeqCst));

        // L2 turns while `!holding()`: must neither bind over L3 nor fail with AddressInUse.
        l2.looped.turn();
        assert!(l2.looped.bound.is_none());
        assert!(!l2.looped.stopped);
        assert!(l2.events.try_recv().is_err());
        assert_eq!(bind::identity(&path), Some(id_l3));
        assert!(UnixStream::connect(&path).is_ok());

        // When L3 leaves, L2 now holds and binds cleanly on its next turn.
        drop(l3);
        l2.looped.turn();
        assert!(l2.looped.bound.is_some());
        assert_eq!(next(&l2.events), Seen::Listening);
        assert!(UnixStream::connect(&path).is_ok());

        drop(l2);
        assert!(!path.exists());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
