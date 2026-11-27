//! Child processes under the agent's clock, answered by scripts (LLP
//! 1133.007 §7): the events, restarts, stdin's queue and `drain`, the
//! origin rule, release and its escalation, and the refusals that come
//! before anything runs.
use super::super::Core;
use super::{admit_one_shot, ONE_SHOT_BYTES};
use exact_runner::{
    CallOrigin, FailureKind, Outcome, ProcessOp, Request, RequestOut, SocketOp, SocketOrigin,
    SocketSend, WorkClock,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

const GRANTS: &str = "process.spawn path:t3\nprocess.spawn /usr/bin/tar\nenv.read login:PATH\nenv.read HOME\nnet.fetch http://127.0.0.1";

/// A wall-clock wait's limit, which only stops a hanging test
/// (docs/testing.md, "Waiting").
const BOUND: Duration = Duration::from_secs(30);

/// A child's script, written to a file of its own; displayed as its path,
/// and removed when dropped.
struct ScriptFile(std::path::PathBuf);
impl std::fmt::Display for ScriptFile {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.join("script.json").display().fmt(f)
    }
}
impl Drop for ScriptFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn script(json: &str) -> ScriptFile {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "exact-process-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("script.json"), json).unwrap();
    ScriptFile(dir)
}

fn setup(processes: &[(&str, &str)]) -> (Core, Receiver<()>) {
    let (wake, woke) = channel();
    let core = Core::under_agent(processes, || {
        Core::start(
            None,
            GRANTS,
            Box::new(move || {
                let _ = wake.send(());
            }),
        )
    });
    (core, woke)
}

fn request(ticket: u64, body: &str, stream: bool) -> RequestOut {
    // The app's own child process (`process.rs` refuses another origin's start).
    let mut request = Request::process(body.as_bytes().to_vec(), stream, 1 << 20);
    request.reach.origin = exact_runner::CallOrigin::App;
    outgoing(ticket, request)
}

fn outgoing(ticket: u64, request: Request) -> RequestOut {
    RequestOut {
        completion: None,
        class: exact_runner::RequestClass::Ordered,
        ticket,
        target: "server".into(),
        request,
        forced: false,
    }
}

/// Every outcome queued now, in order.
fn taken(core: &Core) -> Vec<Outcome> {
    let mut out = Vec::new();
    loop {
        core.begin_pump();
        match core.drain().pop() {
            Some(done) => out.push(done.1),
            None => return out,
        }
    }
}

/// The next outcome, waiting for it on the wall (a one-shot's thread).
fn next(core: &Core, woke: &Receiver<()>) -> Outcome {
    let deadline = Instant::now() + BOUND;
    loop {
        core.begin_pump();
        if let Some(done) = core.drain().pop() {
            return done.1;
        }
        woke.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("an outcome within five seconds");
    }
}

/// Each event as `name data`, data trimmed of the incarnation's details.
fn events(outcomes: &[Outcome]) -> Vec<String> {
    outcomes
        .iter()
        .map(|o| match o {
            Outcome::Message(m) if m.event == "stdout" || m.event == "stderr" => {
                format!("{} {}", m.event, m.data)
            }
            Outcome::Message(m) => {
                let data: serde_json::Value = serde_json::from_str(&m.data).unwrap_or_default();
                match m.event.as_str() {
                    "spawn" => format!("spawn {} {}", data["pid"], data["restarts"]),
                    "exit" => format!("exit {} {}", data["code"], data["signal"]),
                    "close" => format!("close {} {}", data["code"], data["drained"]),
                    other => other.to_string(),
                }
            }
            Outcome::Closed {
                reason, was_clean, ..
            } => {
                let data: serde_json::Value = serde_json::from_str(reason).unwrap();
                format!("end {} {} {was_clean}", data["code"], data["signal"])
            }
            Outcome::Failed { kind, message } => format!("failed {kind:?} {message}"),
            other => format!("{other:?}"),
        })
        .collect()
}

/// The core's clock, moved as the runner moves it.
fn clock(core: &Core) -> std::sync::Arc<dyn WorkClock> {
    core.work_clock().expect("under the agent")
}

/// Wait for the supervisor to start the child: its `spawn` is queued. The
/// next advance waits for the rest of its first turn (the clock counts it
/// as running until it first waits).
fn waiting(core: &Core) {
    let deadline = Instant::now() + BOUND;
    loop {
        let spawned =
            (core.shared.state.lock().unwrap().completed.iter().flatten()).any(|done| done.message);
        if spawned {
            break;
        }
        assert!(Instant::now() < deadline, "the supervisor never started");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn ops(ops: Vec<(u32, ProcessOp)>) -> SocketSend {
    SocketSend {
        origin: SocketOrigin::Mutation("send".into()),
        ops: (ops.into_iter())
            .map(|(incarnation, op)| (1, "server".into(), SocketOp::Process { incarnation, op }))
            .collect(),
    }
}

#[test]
fn a_scripted_child_streams_its_events_and_restarts_on_the_agents_clock() {
    let file = script(
        r#"{"incarnations":[
            {"pid":11,"steps":[{"at":0,"stdout":"hello\n"},{"at":100,"exit":0}]},
            {"pid":12,"steps":[{"at":0,"stderr":"again"},{"at":50,"exit":78}]}]}"#,
    );
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let body = r#"{"kind":"spawn","program":"path:t3","args":["--bootstrap-fd","0"],"encoding":"utf8",
        "restart":{"when":"always","delay":500,"maxDelay":10000,"preventExitStatus":[78]}}"#;
    core.run(request(1, body, true), None).unwrap();
    waiting(&core);
    let clock = clock(&core);
    clock.advance(0.0);
    assert_eq!(events(&taken(&core)), ["spawn 11 0", "stdout hello\n"]);
    assert_eq!(clock.due(), Some(100.0));
    clock.advance(100.0);
    assert_eq!(events(&taken(&core)), ["exit 0 null", "close 0 true"]);
    assert_eq!(clock.due(), Some(600.0), "500 ms after the close");
    clock.advance(599.0);
    assert!(taken(&core).is_empty());
    clock.advance(600.0);
    assert_eq!(events(&taken(&core)), ["spawn 12 1", "stderr again"]);
    clock.advance(650.0);
    // 78 is prevented: the stream ends with the last close.
    let end = Instant::now() + BOUND;
    let mut seen = taken(&core);
    while seen.len() < 2 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
        seen.extend(taken(&core));
    }
    assert_eq!(events(&seen), ["exit 78 null", "end 78 null true"]);
}

#[test]
fn stdin_queues_and_drain_follows_once_it_is_empty() {
    let file = script(
        r#"{"incarnations":[{"pid":21,"steps":[{"at":10,"read":70000},{"at":20,"read":70000},{"at":30,"exit":0}]}]}"#,
    );
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    core.run(
        request(1, r#"{"kind":"spawn","program":"path:t3"}"#, true),
        None,
    )
    .unwrap();
    waiting(&core);
    let clock = clock(&core);
    clock.advance(0.0);
    assert_eq!(events(&taken(&core)), ["spawn 21 0"]);
    // The handle answered `false`: past 64 KiB.
    let write = |n: usize| {
        ops(vec![(
            1,
            ProcessOp::Write {
                data: vec![b'x'; n],
                drain: true,
            },
        )])
    };
    let sent = core.socket(&write(100_000)).unwrap();
    assert_eq!(sent, vec![(1, 1, 100_000)]);
    assert_eq!(
        core.socket_reader()(1),
        Some(100_000),
        "writableLength's start"
    );
    // Past 1 MiB is refused whole.
    let over = core.socket(&write(1 << 20)).unwrap_err();
    assert!(over.contains("QuotaExceededError"), "{over}");
    // 30,000 bytes left: below 64 KiB, and no drain until it is empty (Node's).
    clock.advance(10.0);
    assert!(events(&taken(&core)).is_empty());
    assert_eq!(core.socket_reader()(1), Some(30_000));
    clock.advance(20.0);
    assert_eq!(events(&taken(&core)), ["drain"]);
    assert_eq!(core.socket_reader()(1), Some(0));
    // Stale incarnation, then end and a write after it.
    assert!(core
        .socket(&ops(vec![(2, ProcessOp::End)]))
        .unwrap_err()
        .contains("incarnation"));
    core.socket(&ops(vec![(1, ProcessOp::End)])).unwrap();
    assert!(core.socket(&write(1)).unwrap_err().contains("ended"));
    clock.advance(30.0);
    let seen = events(&taken(&core));
    assert!(seen.contains(&"exit 0 null".to_string()), "{seen:?}");
}

#[test]
fn only_a_write_that_answered_false_hears_drain() {
    let file = script(
        r#"{"incarnations":[{"pid":22,"steps":[{"at":10,"read":1000},{"at":20,"read":70000},{"at":30,"exit":0}]}]}"#,
    );
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    core.run(
        request(1, r#"{"kind":"spawn","program":"path:t3"}"#, true),
        None,
    )
    .unwrap();
    waiting(&core);
    let clock = clock(&core);
    clock.advance(0.0);
    taken(&core);
    let write = |n: usize, drain: bool| {
        ops(vec![(
            1,
            ProcessOp::Write {
                data: vec![0; n],
                drain,
            },
        )])
    };
    core.socket(&write(1000, false)).unwrap();
    // The supervisor sees the write on its next turn: move the clock by nothing.
    clock.advance(1.0);
    clock.advance(10.0);
    assert!(events(&taken(&core)).is_empty(), "it answered true");
    core.socket(&write(70_000, true)).unwrap();
    clock.advance(11.0);
    clock.advance(20.0);
    assert_eq!(events(&taken(&core)), ["drain"]);
}

#[test]
fn a_first_login_capture_needs_the_apps_origin() {
    let (core, woke) = setup(&[]);
    let login = |ticket, origin| {
        let mut out = request(ticket, r#"{"kind":"loginEnv"}"#, false);
        out.request.reach.origin = origin;
        core.run(out, None).unwrap();
        next(&core, &woke)
    };
    match login(1, CallOrigin::Tool) {
        Outcome::Failed { message, .. } => {
            assert!(message.starts_with("person-required"), "{message}")
        }
        other => panic!("{other:?}"),
    }
    // Once the app's origin has captured it, any origin reads it.
    assert!(matches!(login(2, CallOrigin::App), Outcome::Response(_)));
    assert!(matches!(login(3, CallOrigin::Tool), Outcome::Response(_)));
}

#[test]
fn a_start_from_another_origin_is_refused_by_the_host_too() {
    let file = script(r#"{"incarnations":[{"pid":51,"steps":[]}]}"#);
    let (core, woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    for (ticket, kind) in [(1, "spawn"), (2, "execFile")] {
        let body = format!(r#"{{"kind":"{kind}","program":"path:t3"}}"#);
        let mut out = request(ticket, &body, kind == "spawn");
        out.request.reach.origin = CallOrigin::Tool;
        core.run(out, None).unwrap();
        match next(&core, &woke) {
            Outcome::Failed { message, .. } => {
                assert!(message.starts_with("person-required"), "{kind}: {message}")
            }
            other => panic!("{kind}: {other:?}"),
        }
    }
}

#[test]
fn every_kind_the_runner_declares_is_one_the_host_runs() {
    let (core, woke) = setup(&[]);
    for (ticket, (kind, _)) in (1..).zip(exact_runner::process::KINDS) {
        let body = format!(r#"{{"kind":"{kind}","program":"path:t3"}}"#);
        core.run(request(ticket, &body, kind == "spawn"), None)
            .unwrap();
        if let Outcome::Failed { message, .. } = next(&core, &woke) {
            assert!(!message.contains("no call"), "{kind}: {message}");
        }
    }
    core.run(request(9, r#"{"kind":"shell"}"#, false), None)
        .unwrap();
    match next(&core, &woke) {
        Outcome::Failed { message, .. } => assert!(message.contains("no call"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn release_signals_the_group_and_escalates_after_the_grace() {
    let file = script(
        r#"{"incarnations":[{"pid":41,"ignore":["SIGTERM"],"steps":[{"at":0,"stdout":"up"}]}]}"#,
    );
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let body = r#"{"kind":"spawn","program":"path:t3","encoding":"utf8","forceKillAfter":2000}"#;
    core.run(request(1, body, true), None).unwrap();
    waiting(&core);
    let clock = clock(&core);
    clock.advance(0.0);
    assert_eq!(events(&taken(&core)), ["spawn 41 0", "stdout up"]);
    core.forget(|_| false);
    // The release reaches the supervisor on the wall; its escalation then
    // waits out the grace.
    let deadline = Instant::now() + BOUND;
    while clock.due() != Some(2000.0) {
        assert!(Instant::now() < deadline, "{:?}", clock.due());
        std::thread::sleep(Duration::from_millis(2));
    }
    clock.advance(2000.0);
    let deadline = Instant::now() + BOUND;
    while !core.shared.state.lock().unwrap().running.is_empty() {
        assert!(Instant::now() < deadline, "the stream never ended");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        taken(&core).is_empty(),
        "a released child's later events are dropped"
    );
}

#[test]
fn nothing_runs_under_the_agent_unless_the_drive_names_it() {
    let (core, woke) = setup(&[]);
    core.run(
        request(1, r#"{"kind":"spawn","program":"path:t3"}"#, true),
        None,
    )
    .unwrap();
    match next(&core, &woke) {
        Outcome::Failed { message, .. } => {
            assert!(message.contains("--process path:t3="), "{message}")
        }
        other => panic!("{other:?}"),
    }
    core.run(
        request(2, r#"{"kind":"spawn","program":"path:ssh"}"#, true),
        None,
    )
    .unwrap();
    match next(&core, &woke) {
        Outcome::Failed { message, .. } => {
            assert_eq!(message, "outside the app's grants (process.spawn path:ssh)")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn one_shots_answer_from_the_script_and_the_drive() {
    let file = script(
        r#"{"execFile":{"stdout":"1.2.0\n","stderr":"","code":0,"after":0},"path":"/drive/t3"}"#,
    );
    let (core, woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let body = |r: &Outcome| match r {
        Outcome::Response(r) => serde_json::from_slice::<serde_json::Value>(&r.body).unwrap(),
        other => panic!("{other:?}"),
    };
    core.run(
        request(
            1,
            r#"{"kind":"execFile","program":"path:t3","encoding":"utf8"}"#,
            false,
        ),
        None,
    )
    .unwrap();
    let r = body(&next(&core, &woke));
    assert_eq!(
        (r["stdout"].as_str(), r["code"].as_i64()),
        (
            Some(exact_runner::agent::base64(b"1.2.0\n").as_str()),
            Some(0)
        )
    );
    core.run(
        request(2, r#"{"kind":"execPath","program":"path:t3"}"#, false),
        None,
    )
    .unwrap();
    assert_eq!(body(&next(&core, &woke))["path"], "/drive/t3");
    core.run(request(3, r#"{"kind":"loginEnv"}"#, false), None)
        .unwrap();
    assert_eq!(
        body(&next(&core, &woke)),
        serde_json::json!({ "PATH": "/drive/bin" })
    );
}

/// Wait until no stream runs.
fn ended(core: &Core) {
    let deadline = Instant::now() + BOUND;
    while !core.shared.state.lock().unwrap().running.is_empty()
        || !core.shared.processes.children.lock().unwrap().is_empty()
    {
        assert!(Instant::now() < deadline, "the stream never ended");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_child_let_go_before_its_first_event_is_released() {
    let file = script(
        r#"{"incarnations":[{"pid":51,"ignore":["SIGTERM"],"steps":[{"at":0,"stdout":"up"}]}]}"#,
    );
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let body = r#"{"kind":"spawn","program":"path:t3","forceKillAfter":2000}"#;
    core.run(request(1, body, true), None).unwrap();
    // Forgotten at once: its `spawn` finds nobody to take it.
    core.forget(|_| false);
    let clock = clock(&core);
    let deadline = Instant::now() + BOUND;
    while clock.due() != Some(2000.0) {
        assert!(Instant::now() < deadline, "the group was never signalled");
        std::thread::sleep(Duration::from_millis(2));
    }
    clock.advance(2000.0);
    ended(&core);
}

#[test]
fn an_apps_abort_ends_a_one_shots_child() {
    let file = script(r#"{"execFile":{"stdout":"","stderr":"","code":0,"after":5000}}"#);
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let body = r#"{"kind":"execFile","program":"path:t3"}"#;
    core.run(request(1, body, false), None).unwrap();
    // Its `signal` (`Core::cancel`), with the agent's clock still at 0.
    core.cancel(&[1]);
    ended(&core);
}

#[test]
fn on_failure_restarts_no_child_the_app_signalled() {
    let body = r#"{"kind":"spawn","program":"path:t3","encoding":"utf8",
        "restart":{"when":"on-failure","delay":100}}"#;
    let run = |json: &str, kill: bool| {
        let file = script(json);
        let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
        core.run(request(1, body, true), None).unwrap();
        waiting(&core);
        let clock = clock(&core);
        clock.advance(0.0);
        let mut seen = events(&taken(&core));
        if kill {
            let sigint = ProcessOp::Kill("SIGINT".into());
            core.socket(&ops(vec![(1, sigint)])).unwrap();
        }
        for t in [1.0, 10.0, 110.0, 111.0] {
            clock.advance(t);
            seen.extend(events(&taken(&core)));
        }
        // The app's `kill` reaches the supervisor on the wall, not at an
        // advance: wait for the stream's end.
        let deadline = Instant::now() + BOUND;
        while kill && !seen.iter().any(|e| e.starts_with("end ")) {
            assert!(Instant::now() < deadline, "{seen:?}");
            std::thread::sleep(Duration::from_millis(5));
            seen.extend(events(&taken(&core)));
        }
        seen
    };
    // The app's own `kill`: the stream ends at the close.
    let seen = run(
        r#"{"incarnations":[{"pid":61,"steps":[]},{"pid":62,"steps":[]}]}"#,
        true,
    );
    assert_eq!(
        seen,
        [
            "spawn 61 0",
            "exit null \"SIGINT\"",
            "end null \"SIGINT\" true"
        ]
    );
    // A signal it did not send is a failure: it restarts.
    let seen = run(
        r#"{"incarnations":[{"pid":63,"steps":[{"at":10,"signal":"SIGSEGV"}]},{"pid":64,"steps":[]}]}"#,
        false,
    );
    assert_eq!(
        seen,
        [
            "spawn 63 0",
            "exit null \"SIGSEGV\"",
            "close null true",
            "spawn 64 1"
        ]
    );
}

#[test]
fn an_operation_for_an_ended_incarnation_reaches_no_later_one() {
    let file = script(
        r#"{"incarnations":[
            {"pid":71,"steps":[{"at":0,"exit":1}]},
            {"pid":72,"steps":[{"at":0,"stdout":"two"},{"at":10,"read":70000}]}]}"#,
    );
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let body = r#"{"kind":"spawn","program":"path:t3","encoding":"utf8",
        "restart":{"when":"always","delay":0}}"#;
    core.run(request(1, body, true), None).unwrap();
    waiting(&core);
    let clock = clock(&core);
    clock.advance(0.0);
    let deadline = Instant::now() + BOUND;
    let mut seen = Vec::new();
    // The restart waits the 100 ms floor, though the delay says 0.
    clock.advance(99.0);
    assert!(!events(&taken(&core)).contains(&"spawn 72 1".to_string()));
    while !seen.iter().any(|e: &String| e == "stdout two") {
        assert!(Instant::now() < deadline, "{seen:?}");
        clock.advance(100.0);
        seen.extend(events(&taken(&core)));
    }
    // Checked while the first ran, sent after the second began: dropped.
    let stale = [
        SocketOp::Process {
            incarnation: 1,
            op: ProcessOp::End,
        },
        SocketOp::Process {
            incarnation: 1,
            op: ProcessOp::Write {
                data: vec![0; 500],
                drain: false,
            },
        },
    ];
    let sent = super::send(&core.shared, &[(1, &stale[0]), (1, &stale[1])]);
    assert!(sent.is_empty(), "{sent:?}");
    assert_eq!(core.socket_reader()(1), Some(0));
    // The second's stdin is open, and counts what it was sent until read.
    let write = |n: usize| {
        ops(vec![(
            2,
            ProcessOp::Write {
                data: vec![b'x'; n],
                drain: false,
            },
        )])
    };
    core.socket(&write(100_000)).unwrap();
    for t in [101.0, 102.0, 103.0] {
        clock.advance(t);
        assert_eq!(core.socket_reader()(1), Some(100_000));
    }
    core.socket(&write(50_000)).unwrap();
    assert_eq!(core.socket_reader()(1), Some(150_000));
    clock.advance(110.0);
    let deadline = Instant::now() + BOUND;
    while core.socket_reader()(1) != Some(80_000) {
        assert!(Instant::now() < deadline, "{:?}", core.socket_reader()(1));
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Outcomes, on the wall, until one is a stream's or a one-shot's last.
fn to_the_end(core: &Core) -> Vec<Outcome> {
    let deadline = Instant::now() + BOUND;
    let mut seen = taken(core);
    while !seen.iter().any(|o| !matches!(o, Outcome::Message(_))) {
        assert!(Instant::now() < deadline, "{:?}", events(&seen));
        std::thread::sleep(Duration::from_millis(2));
        seen.extend(taken(core));
    }
    seen
}

#[test]
fn a_release_ends_a_child_its_signal_ends_without_waiting_out_the_grace() {
    // No `ignore`: the release's SIGTERM ends the script at once.
    let file = script(
        r#"{"incarnations":[{"pid":81,"steps":[{"at":0,"stdout":"up"},{"at":10000,"exit":0}]}]}"#,
    );
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let body = r#"{"kind":"spawn","program":"path:t3","forceKillAfter":5000}"#;
    core.run(request(1, body, true), None).unwrap();
    waiting(&core);
    clock(&core).advance(0.0);
    taken(&core);
    core.forget(|_| false);
    // The agent's clock stays at 0: neither the script's exit nor the grace comes.
    ended(&core);
}

#[test]
fn readiness_that_fails_ends_the_stream_as_a_timeout() {
    let file = script(r#"{"incarnations":[{"pid":82,"steps":[{"at":0,"stdout":"up"}]}]}"#);
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let body = r#"{"kind":"spawn","program":"path:t3","encoding":"utf8",
        "ready":{"url":"http://127.0.0.1/ready","interval":50,"timeout":100,"onTimeout":"fail"}}"#;
    core.run(request(1, body, true), None).unwrap();
    waiting(&core);
    let clock = clock(&core);
    clock.advance(0.0);
    assert_eq!(events(&taken(&core)), ["spawn 82 0", "stdout up"]);
    clock.advance(50.0);
    clock.advance(100.0);
    // `ready-timeout`, then the release (whose events a stream never hears,
    // D2) and the stream's end as D4 says, with no wait for the grace.
    let seen = events(&to_the_end(&core));
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert_eq!(seen[0], "ready-timeout");
    assert!(seen[1].starts_with("failed Timeout"), "{seen:?}");
    ended(&core);
}

#[test]
fn readiness_that_retries_says_so_and_goes_on() {
    let file =
        script(r#"{"incarnations":[{"pid":83,"ready":250,"steps":[{"at":0,"stdout":"up"}]}]}"#);
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let body = r#"{"kind":"spawn","program":"path:t3","encoding":"utf8",
        "ready":{"url":"http://127.0.0.1/ready","interval":50,"timeout":100}}"#;
    core.run(request(1, body, true), None).unwrap();
    waiting(&core);
    let clock = clock(&core);
    clock.advance(0.0);
    taken(&core);
    clock.advance(100.0);
    assert_eq!(events(&taken(&core)), ["ready-timeout"]);
    clock.advance(250.0);
    assert_eq!(events(&taken(&core)), ["ready"]);
}

#[test]
fn an_exec_files_signal_deadline_ends_it_as_a_timeout() {
    let file = script(r#"{"execFile":{"stdout":"","stderr":"","code":0,"after":5000}}"#);
    let (core, woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let mut out = request(1, r#"{"kind":"execFile","program":"path:t3"}"#, false);
    // What `AbortSignal.timeout(100)` gives a request (js/src/fetch.js).
    out.request.timeout_ms = Some(100);
    core.run(out, None).unwrap();
    let clock = clock(&core);
    let deadline = Instant::now() + BOUND;
    while clock.due() != Some(100.0) {
        assert!(Instant::now() < deadline, "the deadline was never armed");
        std::thread::sleep(Duration::from_millis(2));
    }
    clock.advance(100.0);
    match next(&core, &woke) {
        Outcome::Failed { kind, .. } => assert_eq!(kind, FailureKind::Timeout),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_host_refuses_what_the_binding_never_sends() {
    let (core, woke) = setup(&[("path:t3", "script:/nowhere.json")]);
    let refused = [
        (r#"{"kind":"spawn","program":"path:t3","shell":true}"#, true),
        (
            r#"{"kind":"execFile","program":"path:t3","maxBuffer":0}"#,
            false,
        ),
        (
            r#"{"kind":"execFile","program":"path:t3","input":"!!"}"#,
            false,
        ),
        (
            r#"{"kind":"execFile","program":"path:t3","timeout":-1}"#,
            false,
        ),
        (
            r#"{"kind":"spawn","program":"path:t3","killSignal":"SIGSEGV"}"#,
            true,
        ),
        (
            r#"{"kind":"spawn","program":"path:t3","restart":{"when":"sometimes"}}"#,
            true,
        ),
        (
            r#"{"kind":"spawn","program":"path:t3","stdio":["inherit"]}"#,
            true,
        ),
        // A spawn with no stream's place, and a one-shot as a stream.
        (r#"{"kind":"spawn","program":"path:t3"}"#, false),
        (r#"{"kind":"execFile","program":"path:t3"}"#, true),
    ];
    for (ticket, (body, stream)) in (1..).zip(refused) {
        core.run(request(ticket, body, stream), None).unwrap();
        match next(&core, &woke) {
            Outcome::Failed { kind, message } => {
                assert_eq!(kind, FailureKind::Refused, "{body}");
                assert!(message.starts_with("process"), "{body}: {message}");
            }
            other => panic!("{body}: {other:?}"),
        }
    }
}

fn held_network(core: Core) -> (Core, std::sync::Arc<crate::tests::Fixture>) {
    let fixture = std::sync::Arc::new(crate::tests::Fixture::default());
    let held = fixture.clone();
    let core = core.streams_on(move || {
        ibex2::host::Host::with_transport(Box::new(crate::tests::Fake(held.clone())))
    });
    (core, fixture)
}

fn feed(ticket: u64, upload: bool) -> RequestOut {
    let mut req = Request::get("http://127.0.0.1/hold").independent_http(4096);
    if upload {
        req = req.upload_progress();
        req.method = "POST".into();
        req.body = vec![0];
    }
    let exact_runner::Answer::Later(req) = exact_runner::Answer::stream(req) else {
        unreachable!()
    };
    outgoing(ticket, req)
}

#[test]
fn streaming_children_and_network_streams_hold_separate_slots() {
    use crate::{STREAMING_CHILDREN, STREAMS};
    assert_eq!(STREAMING_CHILDREN, 16);
    let script = script(
        r#"{"incarnations":[{"pid":51,"steps":[{"at":10,"stdout":"up"}]}],
        "execFile":{"stdout":"","stderr":"","code":0,"after":5000}}"#,
    );
    let (core, woke) = setup(&[("path:t3", &format!("script:{script}"))]);
    let (core, fixture) = held_network(core);
    let child = |ticket| {
        let mut r = request(ticket, r#"{"kind":"spawn","program":"path:t3"}"#, true);
        r.request.http = exact_runner::HttpScheduling::Independent {
            max_response_bytes: 4096,
        };
        r
    };
    for ticket in 1..=STREAMS as u64 {
        core.run(feed(ticket, false), None).unwrap();
    }
    fixture.wait_held(STREAMS);
    for ticket in 100..100 + STREAMING_CHILDREN as u64 {
        core.run(child(ticket), None).unwrap();
    }
    for _ in 0..STREAMING_CHILDREN {
        assert!(matches!(next(&core, &woke), Outcome::Message(m) if m.event == "spawn"));
    }
    clock(&core).advance(10.0);
    for _ in 0..STREAMING_CHILDREN {
        assert!(matches!(next(&core, &woke), Outcome::Message(m) if m.event == "stdout"));
    }
    assert_eq!(
        core.run(feed(80, false), None).unwrap_err(),
        "17 network streams requested (16 in use, maximum 16). Long polls, event feeds, WebSockets, listeners and running uploads each use a network slot"
    );
    assert_eq!(
        core.run(child(200), None).unwrap_err(),
        "17 streaming child processes requested (16 in use, maximum 16)"
    );
    let shot = |ticket| request(ticket, r#"{"kind":"execFile","program":"path:t3"}"#, false);
    for ticket in 700..716 {
        core.run(shot(ticket), None).unwrap();
    }
    assert_eq!(
        core.run(shot(716), None).unwrap_err(),
        "17 one-shot child processes requested (16 in use, maximum 16)"
    );
    // Undrained replies keep their independent request slots even when the
    // transport has already refused the ungranted origin.
    let read = |ticket| {
        outgoing(
            ticket,
            Request::get("https://read.invalid/").independent_http(4096),
        )
    };
    for ticket in 400..400 + crate::COUNTS[1] as u64 {
        core.run(read(ticket), None).unwrap();
    }
    assert_eq!(
        core.run(read(600), None).unwrap_err(),
        "129 independent requests requested (128 in use, maximum 128)"
    );
    core.forget(|ticket| ticket < 400);
    core.run(feed(300, true), None).unwrap();
    assert_eq!(core.shared.uploads.waiting().0, 1);
    core.forget(|ticket| ticket != 100);
    let deadline = Instant::now() + BOUND;
    while core.shared.state.lock().unwrap().streams.len() == STREAMS + STREAMING_CHILDREN {
        assert!(Instant::now() < deadline, "the child slot was not released");
        std::thread::sleep(Duration::from_millis(2));
    }
    core.run(child(201), None).unwrap();
    assert!(core
        .run(feed(81, false), None)
        .unwrap_err()
        .starts_with("17 network streams requested (16 in use, maximum 16)"));
    assert_eq!(
        core.shared.uploads.waiting().0,
        1,
        "a child slot cannot start an upload"
    );
    core.forget(|ticket| ticket != 1);
    fixture.wait_held(STREAMS + 1);
    assert_eq!(core.shared.uploads.waiting().0, 0);
    core.forget(|_| false);
    ended(&core);
    assert_eq!(core.shared.state.lock().unwrap().streams.len(), 0);
}

#[test]
fn draining_an_earlier_child_does_not_release_a_network_slot() {
    let (core, woke) = setup(&[]);
    let (core, fixture) = held_network(core);
    let child = request(1, r#"{"kind":"spawn","program":"ungranted"}"#, true);
    core.run(child, None).unwrap();
    let by = Instant::now() + BOUND;
    while !core.shared.state.lock().unwrap().completed[1]
        .iter()
        .any(|d| !d.message)
    {
        assert!(Instant::now() < by);
        std::thread::sleep(Duration::from_millis(2));
    }
    core.run(feed(1, false), None).unwrap();
    fixture.wait_held(1);
    {
        let state = core.shared.state.lock().unwrap();
        assert_eq!((state.streams.len(), state.streams.network), (2, 1));
    }
    let _ = next(&core, &woke);
    let (total, network) = {
        let state = core.shared.state.lock().unwrap();
        (state.streams.len(), state.streams.network)
    };
    assert_eq!((total, network), (1, 1));
    core.forget(|_| false);
    ended(&core);
}

#[test]
fn one_shots_have_room_of_their_own() {
    let file = script(r#"{"execFile":{"stdout":"","stderr":"","code":0,"after":5000}}"#);
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let body = r#"{"kind":"execFile","program":"path:t3"}"#;
    for ticket in 1..=16 {
        core.run(request(ticket, body, false), None).unwrap();
    }
    // None holds a count or bytes of the lane the app's other requests use.
    let lane = |core: &Core| {
        let state = core.shared.state.lock().unwrap();
        (state.counts[1], state.bytes[1])
    };
    assert_eq!(lane(&core), (0, 0));
    let refused = core.run(request(17, body, false), None).unwrap_err();
    assert_eq!(
        refused,
        "17 one-shot child processes requested (16 in use, maximum 16)"
    );
    core.forget(|_| false);
    ended(&core);
    assert_eq!(lane(&core), (0, 0));
    core.run(request(18, body, false), None).unwrap();
}

#[test]
fn a_sole_one_shot_can_exceed_the_byte_limit_until_it_releases_its_room() {
    let (core, _woke) = setup(&[]);
    let held = ONE_SHOT_BYTES + 1;
    admit_one_shot(&core.shared, 1, held).unwrap();
    assert_eq!(admit_one_shot(&core.shared, 2, 0).unwrap_err(),
        format!("{held} one-shot child process bytes requested ({held} in use, maximum {ONE_SHOT_BYTES})"));
}

/// A one-shot never waits in its lane's queue, so its `timeout_ms` arms no
/// queued deadline: failing to arm one cannot refuse it after it has taken
/// a one-shot's room.
#[test]
fn a_one_shot_with_a_timeout_arms_no_queued_deadline() {
    let file = script(r#"{"execFile":{"stdout":"","stderr":"","code":0,"after":5000}}"#);
    let (core, _woke) = setup(&[("path:t3", &format!("script:{file}"))]);
    let body = r#"{"kind":"execFile","program":"path:t3"}"#;
    for ticket in 1..=16 {
        let mut out = request(ticket, body, false);
        out.request.timeout_ms = Some(60_000);
        core.run(out, None).unwrap();
        let armed = (core.shared.queued_deadlines).load(std::sync::atomic::Ordering::Acquire);
        assert_eq!(armed, 0, "one-shot {ticket} armed a queued deadline");
    }
    core.forget(|_| false);
    ended(&core);
}

#[test]
fn a_spawn_without_readiness_makes_no_network_host() {
    // Only `exactReady`'s probes use one. Counted on the stream's own
    // thread: the lane's workers, which start with the first request, make
    // theirs.
    let file = script(r#"{"incarnations":[{"pid":91,"steps":[]}]}"#);
    let made = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let host: super::super::StreamHost = {
        let made = made.clone();
        std::sync::Arc::new(move || {
            let name = std::thread::current().name().map(str::to_string);
            if name.is_some_and(|n| n.starts_with("exact-stream-")) {
                made.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            ibex2::host::Host::new()
        })
    };
    let (wake, _woke) = channel::<()>();
    let wake = Box::new(move || {
        let _ = wake.send(());
    });
    let core = Core::under_agent(&[("path:t3", &format!("script:{file}"))], || {
        Core::start_on(None, GRANTS, wake, host, super::super::WorkerBudget::Thread)
    });
    let body = r#"{"kind":"spawn","program":"path:t3"}"#;
    core.run(request(1, body, true), None).unwrap();
    waiting(&core);
    assert_eq!(made.load(std::sync::atomic::Ordering::Relaxed), 0);
}
