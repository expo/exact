//! LLP 1126.004 D4, §6d: a live press of a control the binary offers as a
//! recording may open the microphone though the app is not in front; a
//! press of any other control, and a press asking for the camera, may not.

use crate::source::{boxed, Source};
use exact_runner::{agent, CallHow, Dispatch, Outcome, Reply, Response, Runner, Value, Work};
#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SOURCE: &str = r#"use getUserMedia, MediaStream from "@exact/media-capture"
use startMediaRecorder, MediaRecorderInfo from "@exact/recording"

routes nav
  home "/"

component App
  derive current = top(nav)
  state n = 0
  mutation opened as shape MediaStream then arm
  mutation started as shape MediaRecorderInfo
  tool record writes "Record." = record
  tool film writes "Film." = film
  action record
    send opened = getUserMedia(audio=true)
  action arm
    match opened
      case some(s)
        send started = startMediaRecorder(stream=s.id, path="app:/data/takes/take")
      case none
        n = n + 1
  action film
    send opened = getUserMedia(audio=true, video=true)
  state trips = 0
  tool trip writes "Trip." = trip
  action trip
    trips = trips + 1
  task sneaky when trips > 0 key=trips
    change(sneak)
  action sneak
    send opened = getUserMedia(audio=true)
  action follow(url: string)
    nav = go(nav, url)
  view
    main navigationKey=`${current.id}` navigate=follow
      text "App"
"#;

struct App(PathBuf);
impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn app(name: &str) -> App {
    let path = std::env::temp_dir().join(format!("exact-recording-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(path.join("node_modules/@exact")).unwrap();
    for package in ["media-capture", "recording"] {
        let to = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages")
            .join(package);
        symlink_dir(
            to.canonicalize().unwrap(),
            path.join("node_modules/@exact").join(package),
        )
        .unwrap();
    }
    std::fs::write(path.join("app.contract"), SOURCE).unwrap();
    let controls = serde_json::json!([
        { "kind": "rec", "title": "Record", "symbol": "mic", "tool": "record" },
        { "kind": "film", "title": "Film", "symbol": "video", "tool": "film" },
        { "kind": "plain", "title": "Record", "symbol": "mic", "tool": "record" },
        { "kind": "trip", "title": "Trip", "symbol": "mic", "tool": "trip" },
    ]);
    let manifest = serde_json::json!({ "app": { "id": "com.exact.recording", "name": "Recording" }, "controls": controls });
    std::fs::write(path.join("app.json"), manifest.to_string()).unwrap();
    App(path.canonicalize().unwrap())
}

struct Granted;
impl exact_runner::DataSource for Granted {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(source.into()))
    }
    fn grants(&self) -> &str {
        "device.microphone mic\ndevice.microphone-background bg\ndevice.camera cam\nfs.write app:/data/takes"
    }
}

fn state(r: &Runner<Source>) -> serde_json::Value {
    serde_json::from_str(&agent::handle(r, r#"{"op":"state"}"#)).unwrap()
}

/// Press `kind` and run it to its end, the host answering each module call:
/// its tool's journal lines.
fn press(r: &mut Runner<Source>, kind: &str) -> String {
    press_how(r, kind, CallHow::default())
}

/// [`press`] as the host delivers it with `how`.
fn press_how(r: &mut Runner<Source>, kind: &str, how: CallHow) -> String {
    let rows = state(r)["controls"].clone();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == kind)
        .unwrap()
        .clone();
    r.press_control(kind, row["digest"].as_str().unwrap(), how)
        .unwrap();
    for _ in 0..4 {
        let at = r.now_ms() + 50.0;
        r.advance(at).unwrap();
        for out in r.take_requests() {
            let outcome = match r.native_work(&out.request) {
                Dispatch::Run(Work::Now(work)) => work(),
                Dispatch::Run(Work::Later(work)) => {
                    let (tx, rx) = std::sync::mpsc::channel();
                    work(Reply::new(move |o| tx.send(o).unwrap()));
                    rx.recv().unwrap()
                }
                _ => continue,
            };
            let _ = r.fulfill(out.ticket, outcome);
        }
    }
    agent::handle(r, r#"{"op":"logs"}"#)
}

/// A runner over `app` whose host answers the microphone and the recorder,
/// and each call as the host saw it: its name and origin.
fn boot(app: &App) -> (Runner<Source>, Arc<Mutex<Vec<(String, String)>>>) {
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = Runner::boot(
        plan,
        boxed(Granted),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    // The host's implementations: a stream for the microphone, a recorder
    // for it. Each call as the host saw it: its name and origin.
    let sent: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    let seen = sent.clone();
    r.bind_extensions(&[], &["media-capture", "recording"]);
    r.native_slot().host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let name = v["send"].as_str().or(v["source"].as_str()).unwrap_or_default().to_owned();
        let answer = match name.as_str() {
            "getUserMedia" => serde_json::json!({ "id": "s1", "active": true, "tracks": [] }),
            "startMediaRecorder" => serde_json::json!({ "id": "r1", "state": "recording", "mimeType": "audio/mp4", "path": "app:/data/takes/take", "duration": 0 }),
            _ => serde_json::json!({}),
        };
        let mut origin = v["origin"].as_str().unwrap_or("none").to_owned();
        // A press's capture names the press, which the host asks after.
        if v["press"].is_u64() {
            origin += "+id";
        }
        seen.lock().unwrap().push((name, origin));
        reply.send(Outcome::Response(Response { status: 200, headers: Vec::new(), body: answer.to_string().into_bytes() }));
    })));
    (r, sent)
}

#[test]
fn only_a_recording_press_opens_the_microphone_out_of_the_foreground() {
    let app = app("press");
    let (mut r, sent) = boot(&app);
    let recording = serde_json::json!({ "offered": [{ "from": "18.0", "intent": "recording" }], "qualifying": "control-recording-unqualified" });
    let tool = serde_json::json!({ "offered": [{ "from": "26.0", "intent": "tool" }] });
    let compat = serde_json::json!({ "inputs": { "controls": [
        { "control": { "kind": "rec" }, "placement": recording },
        { "control": { "kind": "film" }, "placement": recording },
        { "control": { "kind": "plain" }, "placement": tool },
    ] } });
    r.place_controls(&compat.to_string());
    let refusals = |logs: &str| logs.matches("not in the foreground").count();

    let logs = press(&mut r, "plain");
    assert_eq!(
        refusals(&logs),
        1,
        "a plain press keeps the foreground rule: {logs}"
    );
    // A press never prompts: while the microphone's permission is `prompt` it is refused (D4).
    let logs = press(&mut r, "rec");
    assert!(
        logs.contains("getUserMedia: refused, person-required (action origin)"),
        "{logs}"
    );
    let granted = r#"{"op":"prefer","permission":{"microphone":"granted","camera":"granted"}}"#;
    r.extension_agent(granted).expect("a prefer").1.unwrap();
    let logs = press(&mut r, "rec");
    assert_eq!(
        refusals(&logs),
        1,
        "a recording press opens the microphone: {logs}"
    );
    let calls = sent.lock().unwrap().clone();
    let origin = |name: &str| {
        calls
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, o)| o.clone())
    };
    // `press`: the implementation starts the microphone for it as for the app.
    assert_eq!(
        origin("getUserMedia").as_deref(),
        Some("press+id"),
        "{calls:?}"
    );
    assert_eq!(
        origin("startMediaRecorder").as_deref(),
        Some("press+id"),
        "and its `then` starts the recorder: {calls:?} {logs}"
    );
    let logs = press(&mut r, "film");
    assert_eq!(refusals(&logs), 2, "and never the camera: {logs}");
    // A call of the same tool through a door is no press.
    let opened = |sent: &Arc<Mutex<Vec<(String, String)>>>| {
        sent.lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "getUserMedia")
            .count()
    };
    let before = opened(&sent);
    let call = r#"{"op":"call","tool":"record","args":{},"door":"agent"}"#;
    r.call_agent("call", call).unwrap().unwrap();
    for _ in 0..3 {
        let at = r.now_ms() + 50.0;
        r.advance(at).unwrap();
    }
    let calls = state(&r)["intents"]["calls"].clone();
    let last = calls.as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        (last["origin"].as_str(), last["reason"].as_str()),
        (Some("tool"), Some("person-required")),
        "{last}"
    );
    assert_eq!(opened(&sent), before, "and nothing opened");
}
#[test]
fn a_press_that_opened_the_app_starts_only_the_two_captures_and_only_while_it_runs() {
    let app = app("opened");
    let (mut r, sent) = boot(&app);
    let opens = serde_json::json!({ "offered": [{ "from": "10", "intent": "foreground" }] });
    let compat = serde_json::json!({ "inputs": { "controls": [
        { "control": { "kind": "rec" }, "placement": opens },
        { "control": { "kind": "film" }, "placement": opens },
        { "control": { "kind": "trip" }, "placement": opens },
    ] } });
    r.place_controls(&compat.to_string());
    let front = || CallHow {
        foreground: true,
        ..CallHow::default()
    };
    // The press asks for nothing: the app in front asks in its own UI.
    let logs = press_how(&mut r, "rec", front());
    assert!(
        logs.contains("getUserMedia: refused, person-required (action origin)"),
        "{logs}"
    );
    let granted = r#"{"op":"prefer","permission":{"microphone":"granted","camera":"granted"}}"#;
    r.extension_agent(granted).expect("a prefer").1.unwrap();
    let opened = |sent: &Arc<Mutex<Vec<(String, String)>>>| {
        let calls = sent.lock().unwrap().clone();
        calls
            .into_iter()
            .filter(|(n, _)| !n.is_empty())
            .collect::<Vec<_>>()
    };
    let logs = press_how(&mut r, "rec", front());
    let calls = opened(&sent);
    assert_eq!(
        calls,
        [
            ("getUserMedia".into(), "press+id".into()),
            ("startMediaRecorder".into(), "press+id".into())
        ],
        "the press's own origin, never the app's: {logs}"
    );
    // Nothing else is admitted: a camera is refused before anything opens.
    let logs = press_how(&mut r, "film", front());
    assert!(logs.contains("not in the foreground"), "{logs}");
    assert_eq!(opened(&sent), calls, "nothing opened");
    // A task the press armed fires as the press's work: once the press has
    // ended it starts nothing, though the app is in front.
    let logs = press_how(&mut r, "trip", front());
    for _ in 0..4 {
        let at = r.now_ms() + 50.0;
        r.advance(at).unwrap();
    }
    assert!(logs.contains("control:trip trip dispatched"), "{logs}");
    let after = agent::handle(&r, r#"{"op":"logs"}"#);
    assert!(
        after.contains("getUserMedia: refused, not in the foreground"),
        "the task ran as the ended press, refused: {after}"
    );
    assert_eq!(opened(&sent), calls, "the task opened nothing");
    // While the page is hidden, even a running press starts nothing.
    let page = r.page();
    r.set_page(exact_runner::Page {
        hidden: true,
        ..page
    })
    .unwrap();
    let logs = press_how(&mut r, "rec", front());
    assert!(logs.contains("refused, unavailable"), "{logs}");
    assert_eq!(opened(&sent), calls, "nothing opened while hidden");
}
