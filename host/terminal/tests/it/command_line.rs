//! The command line of LLP 1101.003, run as a shell runs it: the binary,
//! its stdout a pipe, its exit status read. Print mode's SGR bytes go into
//! a terminal emulator, so what a person would see is what is checked.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_exact-terminal");

/// An entry `source` in a directory of its own.
fn entry(name: &str, source: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("exact-command-line-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.contract");
    std::fs::write(&path, source).unwrap();
    path
}

fn todo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/todo/terminal.contract")
}

fn run(path: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .arg(path)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `--json`'s object, parsed.
fn json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}\n{}", text(&out.stdout), text(&out.stderr)))
}

/// A list of `n` rows in a column as tall as the window, as todo's is: in
/// print mode the column takes its content's height (D1). Each row is wide,
/// so a few thousand fill a pipe's buffer.
fn long_list(n: usize) -> String {
    format!(
        "component App\n  state rows = range({n})\n  view\n    column height=\"100%\"\n      scroll flex=1 min-height=0\n        for r in rows key=r\n          text `row ${{r}} ............................................................`\n"
    )
}

/// A state larger than a pipe's buffer, and one row to show.
fn large_state(n: usize) -> String {
    format!("component App\n  state rows = range({n})\n  view\n    text \"x\"\n")
}

/// A pipe out of a child that nobody reads until the test says: the
/// child's writes block once the pipe is full.
fn spawn_blocked(path: &Path, args: &[&str]) -> std::process::Child {
    Command::new(BIN)
        .arg(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawns")
}

#[cfg(unix)]
fn signal(child: &std::process::Child, signal: i32) {
    assert_eq!(unsafe { libc::kill(child.id() as i32, signal) }, 0);
}

/// A child writing a long list, its first rows read (so it has booted and
/// its handlers are in), then left to fill the pipe and block.
#[cfg(unix)]
fn writing(name: &str, timeout: &str) -> (std::process::Child, Instant) {
    let path = entry(name, &long_list(6000));
    let started = Instant::now();
    let mut child = spawn_blocked(&path, &["--print", "--timeout", timeout]);
    let mut first = [0; 4096];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut first)
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    (child, started)
}

/// Wait for `child`, reading its stdout to the end; its status code and
/// how long it took from `from`.
fn finish(mut child: std::process::Child, from: Instant) -> (i32, String, Duration) {
    let mut out = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    let status = child.wait().unwrap();
    (status.code().unwrap_or(-1), out, from.elapsed())
}

#[test]
fn todo_prints_its_document_once_and_exits_zero() {
    let out = run(&todo(), &["--print"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let printed = text(&out.stdout);
    assert!(printed.contains("Read LLP 1101"), "{printed}");
    assert!(printed.contains("2 left"), "{printed}");
    assert!(
        !printed.contains('\x1b'),
        "a pipe takes plain text: {printed:?}"
    );
    // No trailing blanks, every row ends in `\n`.
    assert!(printed.ends_with('\n') && printed.lines().all(|l| l == l.trim_end()));
    // A pipe on stdout is print mode without a flag.
    let piped = run(&todo(), &[]);
    assert_eq!(text(&piped.stdout), printed);
}

/// `--color=always` writes SGR rows; read through a terminal emulator they
/// are the plain rows, with no cursor move or redraw among them (D1, D6).
#[test]
fn sgr_rows_through_an_emulator_show_what_plain_rows_say() {
    let plain = text(&run(&todo(), &["--print", "--columns", "60"]).stdout);
    let out = run(&todo(), &["--print", "--columns", "60", "--color=always"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let sgr = text(&out.stdout);
    assert!(sgr.contains("\x1b["), "{sgr:?}");
    for move_ in ["\x1b[?2026h", "\x1b[H", "\x1b[J", "A\x1b", "\x1b[6n"] {
        assert!(!sgr.contains(move_), "{move_:?} in {sgr:?}");
    }
    let rows = plain.lines().count();
    let mut emulator = vt100::Parser::new(rows as u16 + 1, 60, 0);
    // A terminal's line discipline turns each `\n` into CR LF.
    emulator.process(sgr.replace('\n', "\r\n").as_bytes());
    // A row's styled blank cells (a header's fill) are spaces on screen.
    let screen: Vec<String> = (emulator.screen().contents().lines())
        .map(|l| l.trim_end().to_string())
        .collect();
    let plain_rows: Vec<&str> = plain.lines().collect();
    assert_eq!(screen[..plain_rows.len()], plain_rows[..]);
    assert!(plain.lines().all(|l| l.chars().count() <= 60), "{plain}");
}

/// D1's offer: a percentage height against print mode's indefinite height
/// is `auto`, so a list longer than a screen prints whole.
#[test]
fn a_list_longer_than_a_screen_prints_whole() {
    let out = run(&entry("long", &long_list(40)), &["--print"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let printed = text(&out.stdout);
    assert!(
        printed.contains("row 0 ") && printed.contains("row 39 "),
        "{printed}"
    );
}

#[test]
fn json_is_the_settled_state_with_navigation_and_exit() {
    let out = run(&todo(), &["--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let state = json(&out);
    assert_eq!(state["navigation"]["url"], "/");
    assert_eq!(
        state["exit"],
        serde_json::json!({"status": 0, "reason": "settled", "head": null})
    );
    assert_eq!(state["derives"]["left"], 2);
}

/// An armed `after` holds the run (D2): what it sets is what prints.
#[test]
fn a_one_shot_task_holds_the_run_until_it_fires() {
    let source = "component App\n  state step = \"waiting\"\n  task finish when step == \"waiting\"\n    after(300, done)\n  action done\n    step = \"done\"\n  view\n    text step\n";
    let started = Instant::now();
    let out = run(&entry("after", source), &["--print"]);
    assert_eq!(text(&out.stdout), "done\n", "{}", text(&out.stderr));
    assert!(started.elapsed() >= Duration::from_millis(300));
}

/// A repeating timer holds nothing: a spinner cannot keep a command alive.
#[test]
fn a_repeating_timer_does_not_hold_the_run() {
    let source = "component App\n  state n = 0\n  task spin when true\n    every(50, step)\n  action step\n    n = n + 1\n  view\n    text \"ready\"\n";
    let out = run(&entry("every", source), &["--print", "--timeout", "5"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "ready\n");
}

/// The location: argv's words, refused before boot when nothing would
/// answer it (D3).
#[test]
fn the_location_is_checked_before_boot() {
    let source = "component App\n  view\n    text \"one\"\n";
    let path = entry("no-router", source);
    let miss = run(&path, &["/x"]);
    assert_eq!(miss.status.code(), Some(2));
    assert!(text(&miss.stderr).contains("/x"), "{}", text(&miss.stderr));
    let words = run(&path, &["some", "words"]);
    assert_eq!(words.status.code(), Some(2));
    // A query on `/` is allowed.
    let query = run(&path, &["/?q=1", "--x", "two"]);
    assert_eq!(query.status.code(), Some(0), "{}", text(&query.stderr));
    let usage = run(&path, &["--json", "/x"]);
    assert_eq!(usage.status.code(), Some(2));
    let exit = &json(&usage)["exit"];
    assert_eq!(exit["reason"], "usage");
    assert_eq!(exit["status"], 2);
    assert!(exit["message"].as_str().unwrap().contains("/x"));
    for args in [
        &["--json", "--drive", "print"][..],
        &["--print", "--size", "80x24"][..],
        &["--color=red"][..],
        &[".."][..],
        &["/a", "b"][..],
        &["-x"][..],
    ] {
        assert_eq!(run(&path, args).status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn routes_take_the_launch_location_and_a_miss_is_a_usage_error() {
    let source = "routes nav\n  home \"/\"\n  item \"/item/:id\"\ncomponent App\n  derive here = top(nav)\n  view\n    text `${here.name} ${searchParam(here, \"q\")}`\n";
    let path = entry("routes", source);
    let out = run(&path, &["item", "seven", "--q", "a b"]);
    assert_eq!(text(&out.stdout), "item a b\n", "{}", text(&out.stderr));
    let out = run(&path, &["/item/7?q=one", "--q", "two"]);
    assert_eq!(
        text(&out.stdout),
        "item one\n",
        "the location's own value wins"
    );
    let out = run(&path, &["--json", "--", "/tmp/milk"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "one segment, which no route takes"
    );
    assert_eq!(run(&path, &["/nowhere"]).status.code(), Some(2));
    let help = run(&path, &["--help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(
        text(&help.stdout).contains("/item/:id"),
        "{}",
        text(&help.stdout)
    );
}

#[test]
fn a_declared_not_found_route_boots_and_its_head_status_exits_one() {
    let source = "routes nav\n  home \"/\"\n  notfound\ncomponent App\n  derive here = top(nav)\n  view\n    column\n      if here.name == \"notfound\"\n        head status=404\n        text \"missing\"\n      else\n        text \"home\"\n";
    let path = entry("not-found", source);
    let out = run(&path, &["/nowhere"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "missing\n");
    assert!(
        text(&out.stderr).contains("status 404 at /nowhere"),
        "{}",
        text(&out.stderr)
    );
    let state = json(&run(&path, &["--json", "/nowhere"]));
    assert_eq!(
        state["exit"],
        serde_json::json!({"status": 1, "reason": "settled", "head": 404})
    );
}

/// `close()` under `head status=503`: closed, and the head still decides.
#[test]
fn close_under_a_failed_head_exits_one() {
    let source = "component App\n  state n = 0\n  task leave when n == 0\n    after(1, quit)\n  action quit\n    close()\n  view\n    column\n      head status=503\n      text \"failed\"\n";
    let state = json(&run(&entry("close", source), &["--json"]));
    assert_eq!(
        state["exit"],
        serde_json::json!({"status": 1, "reason": "closed", "head": 503})
    );
}

/// A runner refusal ends the fixpoint (D2) and exits 1 (D8).
#[test]
fn a_runner_error_ends_the_run() {
    let source = "component App\n  state n = 0\n  task boom when n == 0\n    after(1, explode)\n  action explode\n    n = range(0 / 0).length\n  view\n    text `n ${n}`\n";
    let out = run(&entry("error", source), &["--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    let exit = &json(&out)["exit"];
    assert_eq!(exit["reason"], "error");
    assert!(exit["message"].is_string());
}

/// Only a terminal plan runs here, and a broken one is a failure, never a
/// panic (D8, D9).
#[test]
fn a_gui_plan_or_an_unreadable_one_is_refused_with_one() {
    let dir = entry("plans", "component App\n  view\n    text \"x\"\n");
    let gui = dir.with_file_name("gui.plan");
    let plan = contract::compile("component App\n  view\n    text \"x\"\n").unwrap();
    std::fs::write(&gui, plan.encode()).unwrap();
    let out = run(&gui, &["--print"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("WrongSurface"),
        "{}",
        text(&out.stderr)
    );
    let terminal = dir.with_file_name("terminal.plan");
    let plan = contract::compile_terminal("component App\n  view\n    text \"x\"\n").unwrap();
    std::fs::write(&terminal, plan.encode()).unwrap();
    assert_eq!(text(&run(&terminal, &["--print"]).stdout), "x\n");
    let bad = dir.with_file_name("bad.plan");
    std::fs::write(&bad, "hi").unwrap();
    for path in [bad, dir.with_file_name("missing.plan")] {
        let out = run(&path, &["--print"]);
        assert_eq!(out.status.code(), Some(1), "{path:?}");
        assert!(
            !text(&out.stderr).contains("panicked"),
            "{}",
            text(&out.stderr)
        );
    }
}

/// A closed stdout ends the run quietly with 141 (D8): in print mode, and
/// under `--drive`.
#[cfg(unix)]
#[test]
fn a_closed_pipe_exits_141_without_a_panic() {
    let rows = entry("epipe", &long_list(3000));
    let state = entry("epipe-json", &large_state(30000));
    for (path, args) in [
        (&rows, &["--print"][..]),
        (&state, &["--json"][..]),
        (&rows, &["--drive", "layout", "runner-tree"][..]),
    ] {
        let mut child = spawn_blocked(path, args);
        let mut first = [0; 10];
        child
            .stdout
            .as_mut()
            .unwrap()
            .read_exact(&mut first)
            .unwrap();
        drop(child.stdout.take());
        let mut err = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut err)
            .unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(141), "{args:?}: {err}");
        assert!(!err.contains("panicked"), "{err}");
    }
}

/// A run that never settles is stopped by `--timeout`: 124 (D2, D8).
#[test]
fn a_timeout_exits_124() {
    let source = "component App\n  view\n    text \"working\" aria-busy=true\n";
    let path = entry("busy", source);
    let started = Instant::now();
    let out = run(&path, &["--json", "--timeout", "0.3"]);
    assert_eq!(out.status.code(), Some(124));
    assert_eq!(json(&out)["exit"]["reason"], "timeout");
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// `beforeunload` fires at shutdown; its work holds the exit up to a second
/// while a node is busy (D2's steps 1–3).
#[test]
fn a_timeout_runs_beforeunload_and_waits_a_second_for_busy_nodes() {
    let source = "component App\n  state stopped = false\n  action stop\n    stopped = true\n  view\n    column beforeunload=stop\n      text (stopped ? \"stopped\" : \"working\") aria-busy=true\n";
    let started = Instant::now();
    let out = run(&entry("unload", source), &["--timeout", "0.2"]);
    assert_eq!(out.status.code(), Some(124));
    assert_eq!(text(&out.stdout), "stopped\n");
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(1150) && took < Duration::from_secs(3),
        "{took:?}"
    );
}

/// A timeout while a write is blocked: the watchdog forces 124 at the
/// deadline plus 2 s.
#[cfg(unix)]
#[test]
fn a_timeout_during_a_blocked_write_is_forced_at_the_grace() {
    let (child, started) = writing("blocked-timeout", "1");
    // The deadline counts from the process's start, a moment after this.
    std::thread::sleep(Duration::from_millis(5000).saturating_sub(started.elapsed()));
    let (code, out, took) = finish(child, started);
    assert_eq!(code, 124, "{took:?}, {} bytes", out.len());
    assert!(took >= Duration::from_secs(3), "{took:?}");
}

/// SIGTERM with stdout blocked and no deadline: forced at receipt plus 2 s.
/// A timeout, then SIGTERM, during a blocked write: 143, the signal ranks
/// first.
#[cfg(unix)]
#[test]
fn a_signal_during_a_blocked_write_is_forced_at_the_grace() {
    for timeout in ["0", "1"] {
        let (child, started) = writing(&format!("blocked-signal-{timeout}"), timeout);
        // With a deadline, after it: the timeout is recorded first.
        std::thread::sleep(Duration::from_millis(1100).saturating_sub(started.elapsed()));
        let sent = Instant::now();
        signal(&child, libc::SIGTERM);
        std::thread::sleep(Duration::from_millis(2600));
        let (code, _, _) = finish(child, started);
        assert_eq!(code, 143, "--timeout {timeout}");
        assert!(sent.elapsed() >= Duration::from_secs(2));
    }
}

/// SIGTERM between rows of a long list: 143, and the list cut short.
#[cfg(unix)]
#[test]
fn a_signal_between_rows_cuts_the_list_short() {
    let path = entry("rows-signal", &long_list(6000));
    let started = Instant::now();
    let mut child = spawn_blocked(&path, &["--print", "--timeout", "0"]);
    let mut first = [0; 4096];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut first)
        .unwrap();
    signal(&child, libc::SIGTERM);
    let (code, rest, _) = finish(child, started);
    assert_eq!(code, 143);
    assert!(!rest.contains("row 5999 "), "the list ran to its end");
}

/// A second SIGINT forces the exit at once, with the first's status; so does
/// SIGTERM after SIGINT.
#[cfg(unix)]
#[test]
fn a_second_signal_exits_at_once_with_the_first_status() {
    for second in [libc::SIGINT, libc::SIGTERM] {
        let (child, _) = writing(&format!("twice-{second}"), "0");
        let sent = Instant::now();
        signal(&child, libc::SIGINT);
        std::thread::sleep(Duration::from_millis(100));
        signal(&child, second);
        let (code, _, took) = finish(child, sent);
        assert_eq!(code, 130);
        assert!(took < Duration::from_millis(1900), "{took:?}");
    }
}

/// A signal during a JSON write that then finishes: the object is whole and
/// says what was captured (0), the process says what cut it (143) (D2, D7).
#[cfg(unix)]
#[test]
fn a_signal_during_a_json_write_that_finishes() {
    let path = entry("json-signal", &large_state(30000));
    let started = Instant::now();
    let mut child = spawn_blocked(&path, &["--json", "--timeout", "0"]);
    let mut first = [0; 1024];
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_exact(&mut first)
        .unwrap();
    signal(&child, libc::SIGTERM);
    let (code, rest, _) = finish(child, started);
    assert_eq!(code, 143);
    let whole = format!("{}{rest}", String::from_utf8_lossy(&first));
    let state: serde_json::Value = serde_json::from_str(&whole).expect("a whole object");
    assert_eq!(state["exit"]["status"], 0);
}

/// Help names the host's flags and the entry's routes, and exits 0.
#[test]
fn help_names_the_flags() {
    let out = run(&todo(), &["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let help = text(&out.stdout);
    for flag in [
        "--print",
        "--json",
        "--columns",
        "--timeout",
        "--drive",
        "Locations:",
    ] {
        assert!(help.contains(flag), "{flag}: {help}");
    }
    let mut child = Command::new(BIN)
        .arg(todo())
        .arg("-h")
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"").unwrap();
    assert!(child.wait().unwrap().success());
}
