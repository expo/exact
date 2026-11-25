//! LLP 1126.004 D2: a person's press of a control runs its tool as an
//! invocation of origin `action`, from the compiled control's kind and
//! digest alone; the agent's seam and the tool doors cannot make one, and
//! a press whose caller cancelled it is admitted no further name held to
//! the person. Work a press scheduled keeps its cause after it ends (LLP
//! 1133.023 D7a).

use crate::source::{boxed, Source};
use exact_runner::{agent, CallHow, Runner, Value};
#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;
use std::path::{Path, PathBuf};

const SOURCE: &str = r#"use requestPinShortcut, ShortcutItem, PinShortcutResult from "@exact/quick-actions"
use speak, SpeechUtteranceInfo from "@exact/speech-synthesis"

routes nav
  home "/"

shape Ack
  ok: bool

component App
  derive current = top(nav)
  state n = 0
  state ticking = false
  mutation logged as shape Ack then pin
  mutation pinned as shape PinShortcutResult
  mutation spoken as shape SpeechUtteranceInfo
  resource level = load(n) as shape Ack
  tool tick writes "Tick." = tick
  tool note writes "Note." = note
  tool count writes "Count." = count
  action tick
    send logged = add(1)
  action note(text: option<string>)
    n = n + 1
  action pin
    send pinned = requestPinShortcut(shortcut=ShortcutItem(name="Pinned", short_name=none, description=none, url="/", icons=none, apple=none, android=none))
  action say
    send spoken = speak(text="hi")
  action count
    ticking = true
  action beat
    ticking = false
    n = n + 1
    focus("nothing")
    pin()
  task countdown when ticking
    after(2000, beat)
  task leveled when n > 0 and level.ok
    after(1, pin)
  action follow(url: string)
    nav = go(nav, url)
  view
    main navigationKey=`${current.id}` navigate=follow
      text `${n}`
"#;

struct App(PathBuf);
impl App {
    fn new(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("exact-presses-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        std::fs::create_dir_all(path.join("node_modules/@exact")).unwrap();
        for package in ["quick-actions", "speech-synthesis"] {
            let to = packages.join(package).canonicalize().unwrap();
            symlink_dir(to, path.join("node_modules/@exact").join(package)).unwrap();
        }
        std::fs::write(path.join("app.contract"), SOURCE).unwrap();
        let controls = serde_json::json!([
            { "kind": "go", "title": "Tick", "symbol": "plus", "tool": "tick" },
            { "kind": "jot", "title": "Note", "symbol": "pencil", "tool": "note" },
            { "kind": "count", "title": "Count", "symbol": "timer", "tool": "count" },
            { "kind": "home", "title": "Home", "symbol": "house", "url": "/" },
        ]);
        let manifest = serde_json::json!({ "app": { "id": "com.exact.presses", "name": "Presses" }, "controls": controls });
        std::fs::write(path.join("app.json"), manifest.to_string()).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `add` answers when the test says.
struct Later;
impl exact_runner::DataSource for Later {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(source.into()))
    }
    fn answer(
        &mut self,
        _: &mut exact_runner::Store,
        _: &str,
        _: &[Value],
    ) -> Result<exact_runner::Answer, exact_runner::DataError> {
        Ok(exact_runner::Answer::Later(exact_runner::Request::native(
            b"{}".to_vec(),
        )))
    }
    fn parse(
        &mut self,
        _: &mut exact_runner::Store,
        _: &str,
        _: &[Value],
        _: exact_runner::Outcome,
    ) -> Result<exact_runner::Answer, exact_runner::DataError> {
        Ok(exact_runner::Answer::Now(Value::record(vec![Value::Bool(
            true,
        )])))
    }
    fn grants(&self) -> &str {
        ""
    }
}

fn boot(app: &App) -> Runner<Source> {
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));
    Runner::boot(
        plan,
        boxed(Later),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn digest(r: &Runner<Source>, kind: &str) -> String {
    let state = state(r);
    let rows = state["controls"].as_array().unwrap();
    let row = rows.iter().find(|c| c["kind"] == kind).unwrap();
    row["digest"].as_str().unwrap().to_owned()
}

fn state(r: &Runner<Source>) -> serde_json::Value {
    serde_json::from_str(&agent::handle(r, r#"{"op":"state"}"#)).unwrap()
}

fn logs(r: &Runner<Source>) -> String {
    agent::handle(r, r#"{"op":"logs"}"#)
}

fn calls(r: &Runner<Source>) -> Vec<serde_json::Value> {
    state(r)["intents"]["calls"].as_array().unwrap().clone()
}

fn settle(r: &mut Runner<Source>) {
    let at = r.now_ms();
    r.advance(at).unwrap();
}

/// Answer the one `add` in flight.
fn answer_add(r: &mut Runner<Source>) {
    let out = r.take_requests();
    let add = out
        .iter()
        .find(|o| o.target == "logged")
        .expect("add in flight");
    let ok = exact_runner::Outcome::Response(exact_runner::Response {
        status: 200,
        headers: Vec::new(),
        body: b"{}".to_vec(),
    });
    r.fulfill(add.ticket, ok).unwrap();
    settle(r);
}

fn press(r: &mut Runner<Source>, kind: &str, digest: &str) -> u64 {
    let id = r.press_control(kind, digest, CallHow::default()).unwrap();
    settle(r);
    id
}

#[test]
fn a_press_is_the_persons_action_and_needs_the_plans_digest() {
    let app = App::new("press");
    let mut r = boot(&app);
    assert_eq!(
        state(&r)["controls"][0]["placement"],
        "absent",
        "a host that places none offers none"
    );
    let go = digest(&r, "go");
    press(&mut r, "go", &go);
    let c = &calls(&r)[0];
    assert_eq!(
        (c["origin"].as_str(), c["state"].as_str()),
        (Some("action"), Some("running"))
    );
    assert!(
        logs(&r).contains("control:go tick dispatched"),
        "{}",
        logs(&r)
    );
    answer_add(&mut r);
    assert!(
        !logs(&r).contains("person-required"),
        "a live press may call a name held to the person: {}",
        logs(&r)
    );
    let ended = &calls(&r)[0];
    assert_eq!(
        (ended["outcome"].as_str(), ended["control"].as_str()),
        (Some("ok"), Some("go"))
    );

    // An optional parameter is passed as none.
    let jot = digest(&r, "jot");
    press(&mut r, "jot", &jot);
    assert_eq!(calls(&r)[1]["outcome"], "ok", "{}", logs(&r));
    assert_eq!(state(&r)["slots"]["n"], 1);

    // A control another plan compiled.
    press(&mut r, "jot", "0123");
    let stale = &calls(&r)[2];
    assert_eq!(
        (stale["outcome"].as_str(), stale["reason"].as_str()),
        (Some("refused"), Some("unavailable"))
    );
    assert!(stale["message"].as_str().unwrap().contains("digest"));

    // A location's control is opened by its host, and an unknown kind is none.
    assert!(r.press_control("home", "", CallHow::default()).is_err());
    assert!(r.press_control("nothing", "", CallHow::default()).is_err());
}

#[test]
fn a_second_press_while_one_runs_is_busy() {
    let app = App::new("busy");
    let mut r = boot(&app);
    let go = digest(&r, "go");
    press(&mut r, "go", &go);
    press(&mut r, "go", &go);
    let second = &calls(&r)[0];
    assert_eq!(
        (second["outcome"].as_str(), second["reason"].as_str()),
        (Some("refused"), Some("busy"))
    );
}

#[test]
fn no_caller_but_the_host_can_press() {
    let app = App::new("forged");
    let mut r = boot(&app);
    let go = digest(&r, "go");
    // A tool door's call naming the control door, with the control's digest.
    let how = CallHow {
        door: "control".into(),
        digest: Some(go.clone()),
        ..Default::default()
    };
    r.call_tool("tick", "{}", how).unwrap();
    let call =
        format!(r#"{{"op":"call","tool":"tick","args":{{}},"door":"control","digest":"{go}"}}"#);
    r.call_agent("call", &call).unwrap().unwrap();
    settle(&mut r);
    for c in calls(&r) {
        assert_eq!(
            (c["origin"].as_str(), c["outcome"].as_str()),
            (Some("tool"), Some("refused")),
            "{c}"
        );
    }
    assert!(r
        .call_agent("press", r#"{"op":"press","control":"go"}"#)
        .is_none());
}

#[test]
fn a_cancelled_press_keeps_its_work_and_loses_the_persons_names() {
    let app = App::new("cancelled");
    let mut r = boot(&app);
    let go = digest(&r, "go");
    let id = press(&mut r, "go", &go);
    let reply = r.call_agent("cancel", &format!(r#"{{"op":"cancel","id":{id}}}"#));
    assert!(reply.unwrap().unwrap().contains("running"));
    answer_add(&mut r);
    assert!(
        logs(&r).contains("person-required"),
        "its `then` may not ask for the pin: {}",
        logs(&r)
    );
}

#[test]
fn a_press_past_its_bound_loses_the_persons_names() {
    let app = App::new("late");
    let mut r = boot(&app);
    let go = digest(&r, "go");
    press(&mut r, "go", &go);
    let at = r.now_ms() + 10_001.0;
    r.advance(at).unwrap();
    assert_eq!(calls(&r)[0]["outcome"], "timeout", "{}", logs(&r));
    answer_add(&mut r);
    assert!(logs(&r).contains("person-required"), "{}", logs(&r));
}

#[test]
fn an_app_with_only_location_controls_lists_them() {
    let app = App::new("located");
    let source = SOURCE
        .replace("  tool tick writes \"Tick.\" = tick\n", "")
        .replace("  tool note writes \"Note.\" = note\n", "");
    std::fs::write(app.0.join("app.contract"), source).unwrap();
    let manifest = serde_json::json!({ "app": { "id": "com.exact.presses", "name": "Presses" }, "controls": [{ "kind": "home", "title": "Home", "symbol": "house", "url": "/" }] });
    std::fs::write(app.0.join("app.json"), manifest.to_string()).unwrap();
    let r = boot(&app);
    let controls = &state(&r)["controls"];
    assert_eq!(controls[0]["url"], "/", "{controls}");
    assert_eq!(controls[0]["placement"], "absent");
}

#[test]
fn a_binary_presses_only_the_controls_its_bake_offered() {
    let app = App::new("placed");
    let mut r = boot(&app);
    let (go, jot) = (digest(&r, "go"), digest(&r, "jot"));
    let offered = serde_json::json!({ "offered": [{ "from": "18.0", "intent": "tool" }] });
    let compat = serde_json::json!({ "inputs": { "controls": [
        { "control": { "kind": "go" }, "placement": offered },
        { "control": { "kind": "jot" }, "placement": { "omitted": "control-process" } },
    ] } });
    r.place_controls(&compat.to_string());
    let rows = state(&r)["controls"].clone();
    let row = |kind: &str| {
        rows.as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == kind)
            .unwrap()
            .clone()
    };
    assert_eq!(row("go")["placement"], offered);
    assert_eq!(row("jot")["placement"]["omitted"], "control-process");
    assert_eq!(
        row("home")["placement"],
        "absent",
        "a kind the compat leaves out"
    );
    let err = r
        .press_control("jot", &jot, CallHow::default())
        .unwrap_err();
    assert!(err.contains("not offered"), "{err}");
    press(&mut r, "go", &go);
}

/// LLP 1133.023 D7a: a countdown a press starts runs on after the press, as
/// the press's work: it may not ask for the pin, and D4a's command rules
/// hold. The person's own act in the app still may.
#[test]
fn a_task_a_press_armed_keeps_the_presss_cause_after_it_ends() {
    let app = App::new("countdown");
    let mut r = boot(&app);
    let count = digest(&r, "count");
    press(&mut r, "count", &count);
    assert_eq!(calls(&r)[0]["outcome"], "ok", "{}", logs(&r));
    let at = r.now_ms() + 2_000.0;
    r.advance(at).unwrap();
    assert_eq!(state(&r)["slots"]["n"], 1, "the countdown ran");
    let logs_now = logs(&r);
    assert!(logs_now.contains("person-required"), "{logs_now}");
    assert!(
        logs_now.contains("focus: skipped in a tool call"),
        "{logs_now}"
    );
    let before = logs_now.matches("person-required").count();
    r.act("pin", Vec::new()).unwrap();
    settle(&mut r);
    assert_eq!(logs(&r).matches("person-required").count(), before);
}

/// LLP 1133.023 D7a: data code under a cancelled press's request in flight
/// keeps the press's origin, `action`, which may not use a foreground name
/// on a page that shows; it is not read as a `tool`, which may.
#[test]
fn data_code_under_a_cancelled_press_is_held_to_the_actions_rules() {
    let app = App::new("data");
    let mut r = boot(&app);
    let go = digest(&r, "go");
    let id = press(&mut r, "go", &go);
    r.call_agent("cancel", &format!(r#"{{"op":"cancel","id":{id}}}"#))
        .unwrap()
        .unwrap();
    let body = r#"{"extension":"speech-synthesis","send":"speak","args":{"text":"hi"}}"#;
    let request = exact_runner::Request::native(body.as_bytes().to_vec());
    let _ = r.native_work(&request);
    assert!(
        logs(&r).contains("speak: refused, not in the foreground"),
        "{}",
        logs(&r)
    );
}

/// LLP 1133.023 D7a, a join: a press that changes the arguments of the app's
/// request in flight makes its answer the press's work too, so what the
/// answer arms after the press has ended may not ask for the pin.
#[test]
fn an_answer_a_press_kept_is_the_presss_after_it_ends() {
    let app = App::new("kept");
    let mut r = boot(&app);
    let jot = digest(&r, "jot");
    press(&mut r, "jot", &jot);
    assert_eq!(calls(&r)[0]["outcome"], "ok", "{}", logs(&r));
    assert!(logs(&r).contains("keep request"), "{}", logs(&r));
    let out = r.take_requests();
    let level = out
        .iter()
        .find(|o| o.target == "level")
        .expect("level in flight");
    let ok = exact_runner::Outcome::Response(exact_runner::Response {
        status: 200,
        headers: Vec::new(),
        body: b"{}".to_vec(),
    });
    r.fulfill(level.ticket, ok).unwrap();
    let at = r.now_ms() + 1.0;
    r.advance(at).unwrap();
    let logs = logs(&r);
    assert!(
        logs.contains("requestPinShortcut: refused, person-required"),
        "{logs}"
    );
}

#[test]
fn a_control_that_opens_the_app_is_pressed_from_it_in_front() {
    let app = App::new("foreground");
    let mut r = boot(&app);
    let (go, jot) = (digest(&r, "go"), digest(&r, "jot"));
    let compat = serde_json::json!({ "inputs": { "controls": [
        { "control": { "kind": "go" }, "placement": { "offered": [{ "from": "10", "intent": "foreground" }] } },
        { "control": { "kind": "jot" }, "placement": { "offered": [{ "from": "10", "intent": "tool" }] } },
    ] } });
    r.place_controls(&compat.to_string());
    let front = CallHow {
        foreground: true,
        ..CallHow::default()
    };
    // Each is pressed only from where its placement says.
    let err = r.press_control("go", &go, CallHow::default()).unwrap_err();
    assert!(err.contains("opens the app"), "{err}");
    let err = r.press_control("jot", &jot, front.clone()).unwrap_err();
    assert!(err.contains("where it is pressed"), "{err}");

    // A page hidden by the time the press dispatches refuses it.
    let page = r.page();
    r.set_page(exact_runner::Page {
        hidden: true,
        ..page
    })
    .unwrap();
    r.press_control("go", &go, front.clone()).unwrap();
    settle(&mut r);
    let hidden = &calls(&r)[0];
    assert_eq!(
        (hidden["outcome"].as_str(), hidden["reason"].as_str()),
        (Some("refused"), Some("unavailable")),
        "{hidden}"
    );
    assert!(r.take_requests().iter().all(|o| o.target != "logged"));

    r.set_page(page).unwrap();
    r.press_control("go", &go, front).unwrap();
    settle(&mut r);
    assert_eq!(calls(&r)[1]["origin"], "action", "{}", logs(&r));
    answer_add(&mut r);
    assert_eq!(calls(&r)[1]["outcome"], "ok", "{}", logs(&r));
}

#[test]
fn a_press_that_opened_the_app_loses_the_persons_names_once_cancelled() {
    let app = App::new("opened-cancelled");
    let mut r = boot(&app);
    let go = digest(&r, "go");
    let compat = serde_json::json!({ "inputs": { "controls": [
        { "control": { "kind": "go" }, "placement": { "offered": [{ "from": "10", "intent": "foreground" }] } },
    ] } });
    r.place_controls(&compat.to_string());
    let front = CallHow {
        foreground: true,
        ..CallHow::default()
    };
    let id = r.press_control("go", &go, front).unwrap();
    settle(&mut r);
    assert_eq!(calls(&r)[0]["origin"], "action");
    let reply = r.call_agent("cancel", &format!(r#"{{"op":"cancel","id":{id}}}"#));
    assert!(reply.unwrap().unwrap().contains("running"));
    answer_add(&mut r);
    assert!(
        logs(&r).contains("person-required"),
        "its `then` may not ask for the pin: {}",
        logs(&r)
    );
}
