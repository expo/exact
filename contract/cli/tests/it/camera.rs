//! The camera's admission in the runner (LLP 1133.008 D6, §10 drive 6),
//! through this checkout's `@exact/camera` and `@exact/media-capture`,
//! installed beside the app, their implementations played by the test as the
//! host's native slot under the agent: `video=` asks the camera's permission
//! and `audio=` the microphone's, one after the other; a tool's capture
//! start is refused before anything is asked.

use exact_runner::{CallOrigin, Dispatch, Outcome, Reply, Response, Runner, Value, Work};
use serde_json::{json, Value as Json};
#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const APP: &str = r#"use getUserMedia, cameraStatus, microphoneStatus, MediaStream, CameraStatus, MicrophoneStatus from "@exact/media-capture"
use takePhoto, Photo from "@exact/camera"

component App
  resource camera = cameraStatus() as shape CameraStatus
  resource microphone = microphoneStatus() as shape MicrophoneStatus
  mutation opened as shape MediaStream
  mutation shot as shape Photo
  action open
    send opened = getUserMedia(video=true)
  action openBoth
    send opened = getUserMedia(audio=true, video=true)
  action shutter
    send shot = takePhoto(track="track-1")
  view
    column
      text camera.state
"#;

struct App(PathBuf);
impl App {
    fn new(name: &str) -> Self {
        Self::with_source(name, APP)
    }

    fn with_source(name: &str, source: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("exact-camera-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        std::fs::create_dir_all(path.join("node_modules/@exact")).unwrap();
        for package in ["camera", "media-capture"] {
            let to = packages.join(package).canonicalize().unwrap();
            symlink_dir(to, path.join("node_modules/@exact").join(package)).unwrap();
        }
        std::fs::write(
            path.join("package.json"),
            r#"{"dependencies":{"@exact/camera":"1.0.0"}}"#,
        )
        .unwrap();
        std::fs::write(path.join("app.contract"), source).unwrap();
        Self(path.canonicalize().unwrap())
    }
}

#[test]
fn a_camera_view_does_not_capture_an_app_data_source() {
    for view in [
        "    exact-camera\n",
        "    text result\ncomponent Unused\n  view\n    exact-camera\n",
    ] {
        let source =
            format!("component App\n  resource result = picture() as shape string\n  view\n{view}");
        let app = App::with_source("data-source", &source);
        let plan = contract::compile_path(&app.0.join("app.contract")).unwrap();
        assert!(plan.sources.iter().any(|s| plan.str(s.name) == "picture"));
        assert!(plan.module_named("picture").is_none());
    }
    let app = App::with_source(
        "aliased-source",
        r#"use picture as cameraPicture, CameraPicture from "@exact/camera"
component App
  resource result = picture() as shape string
  resource fixture = cameraPicture() as shape CameraPicture
  mutation taken as shape string
  action take
    send taken = picture()
  view
    exact-camera
"#,
    );
    let plan = contract::compile_path(&app.0.join("app.contract")).unwrap();
    assert!(plan.module_named("picture").is_none());
    assert!(plan
        .modules
        .iter()
        .any(|m| plan.str(m.declared) == "picture"));
}

#[test]
fn a_view_only_camera_accepts_a_typed_preferred_picture() {
    let app = App::with_source(
        "view-only",
        "component App\n  view\n    exact-camera testId=\"finder\"\n",
    );
    let plan = contract::compile_path(&app.0.join("app.contract")).unwrap();
    assert_eq!(plan.views.len(), 1);
    assert_eq!(plan.str(plan.views[0].extension), "camera");
    assert!(plan
        .modules
        .iter()
        .any(|m| plan.str(m.declared) == "picture"));
    assert!(plan
        .modules
        .iter()
        .all(|m| plan.str(m.extension) == "camera"));
    let mut r = Runner::boot(
        plan,
        Granted(CAMERA_LINES),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let picture = json!({ "path": "assets/test/isbn-9780747532743.png" });
    let prefer = json!({ "op": "prefer", "extension": { "camera.picture": picture } });
    let reply = r.extension_agent(&prefer.to_string()).unwrap().1.unwrap();
    let reply: Json = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply["extension"]["camera.picture"], picture);
    assert!(r.take_requests().is_empty(), "no camera source is read");

    let told: Calls = Arc::default();
    let seen = told.clone();
    r.native_slot().host(Some(Arc::new(move |body, _reply| {
        seen.lock()
            .unwrap()
            .push(serde_json::from_slice(&body).unwrap());
    })));
    r.bind_extensions(&[], &["camera"]);
    assert_eq!(preferred(&told)["picture"], picture);

    for value in [
        json!({"path": 7}),
        json!({"unknown": "x"}),
        json!(false),
        Json::Null,
    ] {
        let request = json!({ "op": "prefer", "extension": { "camera.picture": value } });
        assert!(r.extension_agent(&request.to_string()).unwrap().1.is_err());
    }
    let unknown = r#"{"op":"prefer","extension":{"camera.unknown":"x"}}"#;
    assert!(r
        .extension_agent(unknown)
        .unwrap()
        .1
        .unwrap_err()
        .contains("no source `camera.unknown`"));
    assert_eq!(preferred(&told)["picture"], picture);

    let next = json!({ "path": "assets/test/none.png" });
    let request = json!({ "op": "prefer", "extension": { "camera.picture": next } });
    r.extension_agent(&request.to_string()).unwrap().1.unwrap();
    assert_eq!(preferred(&told)["picture"], next);
    r.extension_agent(r#"{"op":"prefer","extension":{"camera.picture":"clear"}}"#)
        .unwrap()
        .1
        .unwrap();
    assert_eq!(preferred(&told), json!({}));
}

fn preferred(told: &Calls) -> Json {
    told.lock()
        .unwrap()
        .iter()
        .rev()
        .find(|v| v["extension"] == "camera" && v["preferred"].is_object())
        .expect("the camera was told its preferred values")["preferred"]
        .clone()
}

#[test]
fn an_unused_camera_view_does_not_admit_preferences() {
    let app = App::with_source(
        "unused-view",
        "component App\n  view\n    text \"plain\"\ncomponent Unused\n  view\n    exact-camera\n",
    );
    let plan = contract::compile_path(&app.0.join("app.contract")).unwrap();
    assert!(plan.modules.is_empty());
    assert!(plan.views.is_empty());
    let mut r = Runner::boot(
        plan,
        Granted(CAMERA_LINES),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    for key in ["camera.picture", "unknown.picture"] {
        let request = json!({ "op": "prefer", "extension": { key: { "path": "x.png" } } });
        let error = r
            .extension_agent(&request.to_string())
            .unwrap()
            .1
            .unwrap_err();
        assert!(error.contains(&format!("no source `{key}`")), "{error}");
    }
}

#[test]
fn shelfs_viewfinder_receives_the_preferred_scan_fixture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/shelf/app.contract");
    let plan = contract::compile_path(&path).unwrap();
    assert!(plan.view_named("exact-camera").is_some());
    let mut r = Runner::boot(
        plan,
        Granted(CAMERA_LINES),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let told: Calls = Arc::default();
    let seen = told.clone();
    r.native_slot().host(Some(Arc::new(move |body, _reply| {
        seen.lock()
            .unwrap()
            .push(serde_json::from_slice(&body).unwrap());
    })));
    r.bind_extensions(&[], &["camera"]);
    let picture = json!({ "path": "assets/test/isbn-9780747532743.png" });
    let request = json!({ "op": "prefer", "extension": { "camera.picture": picture } });
    r.extension_agent(&request.to_string()).unwrap().1.unwrap();
    assert_eq!(preferred(&told)["picture"], picture);
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The app's grant lines.
struct Granted(&'static str);
impl exact_runner::DataSource for Granted {
    fn query(
        &mut self,
        source: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(source.into()))
    }
    fn answer(
        &mut self,
        _: &mut exact_runner::Store,
        _: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Answer, exact_runner::DataError> {
        // Shelf's app data stays pending; this fixture drives the module seam.
        Ok(exact_runner::Answer::Later(exact_runner::Request::native(
            Vec::new(),
        )))
    }
    fn grants(&self) -> &str {
        self.0
    }
}

const CAMERA_LINES: &str = "device.camera purpose.camera\ndevice.microphone purpose.microphone\n";

type Calls = Arc<Mutex<Vec<Json>>>;

/// A runner over the app, the two implementations bound under the agent and
/// played by the native slot: `getUserMedia` answers a stream of one track.
fn boot(name: &str) -> (App, Runner<Granted>, Calls) {
    boot_with(App::new(name), CAMERA_LINES)
}

/// [`boot`] over `app`, which grants `lines`.
fn boot_with(app: App, lines: &'static str) -> (App, Runner<Granted>, Calls) {
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = Runner::boot(
        plan,
        Granted(lines),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let calls: Calls = Arc::default();
    let seen = calls.clone();
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            let v: Json = serde_json::from_slice(&body).unwrap();
            seen.lock().unwrap().push(v.clone());
            let answer = match v["send"].as_str() {
                Some("getUserMedia") => json!({ "id": "stream-1", "active": true, "tracks": [] }),
                Some("takePhoto") => {
                    json!({ "path": "app:/tmp/camera/1.jpg", "type": "image/jpeg", "size": 1, "width": 1, "height": 1 })
                }
                // The implementation's agent path: the picture for `take`, a cancel otherwise.
                Some("takePhotoWithCameraApp") if v["preferred"]["tap"] == "take" => {
                    json!({ "path": "app:/tmp/camera/2.jpg", "type": "image/jpeg", "size": 1, "width": 1, "height": 1 })
                }
                Some("takePhotoWithCameraApp") => json!({ "error": "AbortError" }),
                _ => json!({}),
            };
            reply.send(Outcome::Response(Response {
                status: 200,
                headers: Vec::new(),
                body: answer.to_string().into_bytes(),
            }));
        })));
    r.bind_extensions(&[], &["camera", "media-capture"]);
    (app, r, calls)
}

/// The host's part: each request dispatched, released work run, each
/// outcome fulfilled, until nothing is left to do.
fn pump(r: &mut Runner<Granted>, parked: &mut Vec<(u64, u64)>) {
    for _ in 0..8 {
        let outs = r.take_requests();
        let released = r.release_work();
        if outs.is_empty() && released.is_empty() && r.apply_announced().0.is_empty() {
            return;
        }
        for out in outs {
            let dispatch = match out.request.continuation {
                Some(token) => {
                    parked.push((token, out.ticket));
                    r.dispatch_work(token)
                }
                None => r.native_work(&out.request),
            };
            run(r, out.ticket, dispatch);
        }
        for (token, dispatch) in released {
            let ticket = parked.iter().find(|(t, _)| *t == token).expect("parked").1;
            run(r, ticket, dispatch);
        }
    }
}

fn run(r: &mut Runner<Granted>, ticket: u64, dispatch: Dispatch) {
    let outcome = match dispatch {
        Dispatch::Run(Work::Now(work)) => work(),
        Dispatch::Run(Work::Later(work)) => {
            let (tx, rx) = std::sync::mpsc::channel();
            work(Reply::new(move |o| tx.send(o).unwrap()));
            rx.recv().unwrap()
        }
        _ => return,
    };
    let _ = r.fulfill(ticket, outcome);
}

fn sent(calls: &Calls, name: &str) -> usize {
    (calls.lock().unwrap().iter())
        .filter(|c| c["send"] == name)
        .count()
}

fn logs(r: &Runner<Granted>) -> String {
    exact_runner::agent::handle(r, r#"{"op":"logs"}"#)
}

/// The one ticket held: its capability's permission, answered `allow`.
fn allow(r: &mut Runner<Granted>, parked: &mut Vec<(u64, u64)>) -> String {
    let holds = r.device_holds();
    assert_eq!(holds.len(), 1, "{}", logs(r));
    let args: Json = serde_json::from_str(&holds[0].args).unwrap();
    let tap = format!(
        r#"{{"op":"tap","ticket":{},"choice":"allow"}}"#,
        holds[0].ticket
    );
    exact_runner::agent::answer(r, &tap).unwrap();
    pump(r, parked);
    args["capability"].as_str().unwrap_or_default().to_owned()
}

/// `video=` asks the camera's permission, and `cameraStatus` reads it; a
/// call with both kinds asks each in turn before the implementation hears
/// of it (LLP 1133.008 D6, LLP 1133.009 D3).
#[test]
fn video_asks_the_camera_and_audio_the_microphone_in_turn() {
    let (_app, mut r, calls) = boot("permissions");
    let mut parked = Vec::new();
    pump(&mut r, &mut parked);
    r.act("open", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert_eq!(sent(&calls, "getUserMedia"), 0, "held under the ticket");
    assert_eq!(allow(&mut r, &mut parked), "camera");
    assert_eq!(sent(&calls, "getUserMedia"), 1, "{}", logs(&r));
    let state: Json = serde_json::from_str(&r.extensions_json().unwrap()).unwrap();
    assert_eq!(
        state["media-capture"]["permissions"],
        json!({ "camera": "granted", "microphone": "prompt" })
    );
    assert!(format!("{:?}", r.resource("camera")).contains("\"granted\""));

    r.act("openBoth", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert_eq!(allow(&mut r, &mut parked), "microphone");
    assert_eq!(sent(&calls, "getUserMedia"), 2, "{}", logs(&r));

    let denied = r#"{"op":"prefer","permission":{"camera":"denied"}}"#;
    r.extension_agent(denied).unwrap().1.unwrap();
    r.act("openBoth", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert!(r.device_holds().is_empty(), "a denied camera asks nothing");
    assert_eq!(sent(&calls, "getUserMedia"), 2);
}

/// Drive 6's admission: a tool's `takePhoto`, and a tool's `getUserMedia`
/// while the camera is `prompt`, are refused `person-required` before the
/// implementation hears of them; the app's own reaches it (§14).
#[test]
fn a_tool_s_capture_start_is_refused() {
    let (_app, mut r, calls) = boot("tool");
    let mut parked = Vec::new();
    pump(&mut r, &mut parked);
    r.act_from(CallOrigin::Tool, "open", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert!(r.device_holds().is_empty(), "a tool raises no ticket");
    r.act_from(CallOrigin::Tool, "shutter", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert_eq!(sent(&calls, "takePhoto"), 0);
    assert!(
        logs(&r).contains("camera.takePhoto: refused, person-required (tool origin)"),
        "{}",
        logs(&r)
    );
    r.act("shutter", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert_eq!(sent(&calls, "takePhoto"), 1, "{}", logs(&r));
}

/// An implementation's disclosure (D8 item 9): what it says through its
/// `changed` is listed as its extension's `implementation`, each field
/// replacing the one of its name said before.
#[test]
fn an_implementation_s_disclosure_is_listed() {
    let (_app, mut r, _calls) = boot("disclosed");
    let mut parked = Vec::new();
    pump(&mut r, &mut parked);
    r.listen(Arc::new(|| {}));
    let say = |r: &Runner<Granted>, extension: &str, state: Json| {
        let body = json!({ "extension": extension, "state": state });
        r.native_slot().changed(&format!("exact-state:{body}"));
    };
    say(
        &r,
        "media-capture",
        json!({ "tracks": [{ "id": "track-1", "lease": true }] }),
    );
    say(&r, "camera", json!({ "photos": [] }));
    say(
        &r,
        "camera",
        json!({ "photos": [{ "path": "app:/tmp/camera/1.jpg" }] }),
    );
    r.apply_announced();
    let state: Json = serde_json::from_str(&r.extensions_json().unwrap()).unwrap();
    assert_eq!(
        state["media-capture"]["implementation"]["tracks"][0]["lease"],
        true
    );
    assert_eq!(
        state["camera"]["implementation"],
        json!({ "photos": [{ "path": "app:/tmp/camera/1.jpg" }] })
    );
    say(
        &r,
        "media-capture",
        json!({ "tracks": [{ "id": "track-1", "lease": false }] }),
    );
    r.apply_announced();
    let state: Json = serde_json::from_str(&r.extensions_json().unwrap()).unwrap();
    assert_eq!(
        state["media-capture"]["implementation"]["tracks"][0]["lease"],
        false
    );
    assert!(
        state["media-capture"]["permissions"].is_object(),
        "beside the runner's own"
    );
}

const CAMERA_APP: &str = r#"use takePhotoWithCameraApp, Photo from "@exact/camera"

component App
  mutation shot as shape Photo
  derive shotError = match failure(shot) { case some(f) => f.code, case none => "" }
  derive shotPath = match shot { case some(p) => p.path, case none => "" }
  action snap
    send shot = takePhotoWithCameraApp(facingMode="environment")
  view
    column
      text shotError
"#;

/// The camera app (`takePhotoWithCameraApp`) is system UI the person works,
/// like a picker: it needs no grant line, so an app that uses only it
/// derives no camera permission. Under the agent its sheet holds before the
/// implementation hears of the call, which is told the tap: `take` or
/// `cancel`. A tool's is refused; it exists on Android, iOS and the web, not
/// on a Mac.
#[test]
fn the_camera_app_needs_no_line_and_its_sheet_takes_take_or_cancel() {
    let (_app, mut r, calls) = boot_with(App::with_source("camera-app", CAMERA_APP), "");
    r.set_module_target("android");
    let mut parked = Vec::new();
    pump(&mut r, &mut parked);
    r.act("snap", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    let holds = r.device_holds();
    assert_eq!(holds.len(), 1, "{}", logs(&r));
    assert_eq!(holds[0].capability, "camera-app");
    assert_eq!(holds[0].choices, ["take"]);
    let args: Json = serde_json::from_str(&holds[0].args).unwrap();
    assert_eq!(args["options"]["facingMode"], "environment");
    assert_eq!(sent(&calls, "takePhotoWithCameraApp"), 0, "held");

    let tap = |r: &mut Runner<Granted>, parked: &mut Vec<(u64, u64)>, choice: &str| {
        let ticket = r.device_holds()[0].ticket;
        let line = json!({ "op": "tap", "ticket": ticket, "choice": choice }).to_string();
        exact_runner::agent::answer(r, &line).unwrap();
        pump(r, parked);
    };
    tap(&mut r, &mut parked, "take");
    assert_eq!(sent(&calls, "takePhotoWithCameraApp"), 1, "{}", logs(&r));
    let told = |calls: &Calls| {
        let calls = calls.lock().unwrap();
        let last = calls
            .iter()
            .rev()
            .find(|c| c["send"] == "takePhotoWithCameraApp");
        last.unwrap()["preferred"].clone()
    };
    assert_eq!(told(&calls), json!({ "tap": "take" }));
    assert_eq!(
        r.derive("shotPath"),
        Some(&Value::str("app:/tmp/camera/2.jpg"))
    );

    r.act("snap", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    tap(&mut r, &mut parked, "cancel");
    assert_eq!(told(&calls), json!({ "tap": "cancel" }));
    assert!(
        r.derive("shotError") == Some(&Value::str("AbortError")),
        "{}",
        logs(&r)
    );

    r.act_from(CallOrigin::Tool, "snap", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert!(r.device_holds().is_empty(), "a tool raises no sheet");
    assert!(
        logs(&r).contains("camera.takePhotoWithCameraApp: refused, person-required (tool origin)"),
        "{}",
        logs(&r)
    );

    r.set_module_target("macos");
    r.act("snap", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert!(
        r.device_holds().is_empty(),
        "no sheet where it does not exist"
    );
    assert!(
        r.derive("shotError") == Some(&Value::str("NotSupportedError")),
        "{}",
        logs(&r)
    );
    assert_eq!(sent(&calls, "takePhotoWithCameraApp"), 2);
}

/// Android's `CAMERA` comes from opening the camera in the app: any use of
/// `getUserMedia` under the `device.camera` line, since data code may compute
/// its options (`getUserMedia(JSON.parse(saved))` names no `video`). The line
/// alone, which iOS's system camera UI needs for its usage key, derives none:
/// an app that only uses the camera app asks nothing on Android, and an
/// audio-only app has no camera line (LLP 1133.008 D6 as amended).
#[test]
fn android_s_camera_permission_comes_from_opening_the_camera_in_the_app() {
    let plan = |source: &str| {
        let app = App::with_source("derives", source);
        contract::compile_path_all(&app.0.join("app.contract"), false)
            .unwrap_or_else(|e| panic!("{e:?}"))
            .0
    };
    let camera = ["android.permission.CAMERA"];
    let both = Some("device.camera purpose.camera\ndevice.microphone purpose.microphone");
    let mic = Some("device.microphone purpose.microphone");
    let permissions = contract::native::android_permissions;
    assert!(permissions(&plan(CAMERA_APP), both, CAMERA_APP).is_empty());
    let opens = plan(APP);
    assert_eq!(permissions(&opens, both, APP), camera);
    // Computed in data code: no `video` in the source, still the camera.
    let computed = "const stream = await getUserMedia(JSON.parse(saved));\n";
    assert_eq!(permissions(&opens, both, computed), camera);
    // An app whose lines grant no camera opens none.
    assert!(permissions(&opens, mic, computed).is_empty());
}
