//! `@exact/window-management` (LLP 1178) through this checkout's package,
//! installed beside the app: the targets whose build binds the package's
//! own implementation, what a Windows app reads with it bound, and what the
//! declared substitute answers on each target.

use crate::source::{boxed, Source};
use exact_runner::{agent, Dispatch, Outcome, Reply, Response, Runner, Work};
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const APP: &str = r#"use getWindowBounds, WindowBounds, getScreenDetails, getPointerPosition, windowManagementStatus, ScreenDetails, PointerPosition, WindowManagementStatus from "@exact/window-management"

component App
  resource details = getScreenDetails() as shape ScreenDetails
  resource status = windowManagementStatus() as shape WindowManagementStatus
  mutation bounds as shape WindowBounds
  derive rectangle = match bounds { case some(b) => `${b.screenX},${b.screenY} ${b.outerWidth}x${b.outerHeight}`, case none => "" }
  action readFrame
    send bounds = getWindowBounds()
  mutation at as shape PointerPosition
  action ask
    send at = getPointerPosition()
  view
    text "Displays"
"#;

struct App(PathBuf);

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let from = entry.unwrap().path();
        let into = to.join(from.file_name().unwrap());
        if from.is_dir() {
            copy_tree(&from, &into);
        } else {
            std::fs::copy(&from, into).unwrap();
        }
    }
}

impl App {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("exact-displays-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        copy_tree(
            &packages.join("window-management"),
            &path.join("node_modules/@exact/window-management"),
        );
        std::fs::write(path.join("app.contract"), APP).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Grants;
impl exact_runner::DataSource for Grants {
    fn query(
        &mut self,
        source: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(source.into()))
    }
    fn grants(&self) -> &str {
        "device.window-management\n"
    }
}

fn boot(app: &App, target: &str) -> Runner<Source> {
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = Runner::boot(
        plan,
        boxed(Grants),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.set_module_target(target);
    settle(&mut r);
    r
}

fn settle<D: exact_runner::DataSource>(r: &mut Runner<D>) {
    for _ in 0..8 {
        let at = r.now_ms();
        r.advance(at).unwrap();
        if r.apply_announced().0.is_empty() {
            break;
        }
    }
}

fn show(r: &Runner<Source>, name: &str) -> String {
    format!("{:?}", r.resource(name).or(r.slot(name)))
}

/// LLP 1178 D6, expo/exact#726: a desktop build binds the package's
/// implementation, which reads the host's displays. A target with none
/// bound is answered by the declared substitute in a shipped app too, and
/// on a target the declaration does not name `unsupported` that is the
/// agent's fixture display.
#[test]
fn a_desktop_build_binds_the_packages_implementation() {
    let app = App::new("bound");
    for target in ["linux", "windows"] {
        let bound = contract::native::bound(&app.0, &["window-management"], target).unwrap();
        let [bound] = &bound[..] else {
            panic!("{target}: {bound:?}");
        };
        assert_eq!(bound.class, "WindowManagement", "{target}");
        assert!(
            bound.root.ends_with("window-management/linux/lib.rs"),
            "{target}: {}",
            bound.root.display()
        );
    }
}

#[test]
fn a_windows_build_watches_the_manifest_that_selects_its_shared_rust_source() {
    const CHILD: &str = "EXACT_WINDOW_MANAGEMENT_WATCH_TEST";
    if let Some(root) = std::env::var_os(CHILD) {
        contract::native::rust_packages(Path::new(&root), &["window-management"], "windows")
            .unwrap();
        return;
    }
    let app = App::new("watch");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "window_management::a_windows_build_watches_the_manifest_that_selects_its_shared_rust_source",
            "--nocapture",
        ])
        .env(CHILD, &app.0)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let manifest = app
        .0
        .join("node_modules/@exact/window-management/windows/exact.json");
    assert!(
        stdout
            .lines()
            .filter_map(|line| line.strip_prefix("cargo:rerun-if-changed="))
            .any(|path| Path::new(path) == manifest),
        "{stdout}"
    );
}

/// Hand the runner's module calls to the bound implementation, and its
/// answers back.
fn pump<D: exact_runner::DataSource>(r: &mut Runner<D>) {
    for _ in 0..8 {
        let outs = r.take_requests();
        if outs.is_empty() && r.apply_announced().0.is_empty() {
            return;
        }
        for out in outs {
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
}

/// expo/exact#726: with what a Windows build binds bound as the host binds
/// it in a shipped app, every name is the implementation's to answer, and
/// the app reads its answers, not the agent's fixture.
#[test]
fn a_windows_app_reads_what_its_build_binds() {
    let app = App::new("shipped");
    let mut r = boot(&app, "windows");
    let bound = contract::native::bound(&app.0, &["window-management"], "windows").unwrap();
    let bound: Vec<&str> = bound.iter().map(|b| b.extension.as_str()).collect();
    let calls: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = calls.clone();
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            let body: Json = serde_json::from_slice(&body).unwrap();
            let name = body["source"].as_str().or(body["send"].as_str());
            let name = name.unwrap_or_default().to_owned();
            let mut screen = json!({
                "left": 0, "top": 0, "width": 2048, "height": 1152,
                "availLeft": 0, "availTop": 0, "availWidth": 2048, "availHeight": 1114,
                "devicePixelRatio": 1.25, "isPrimary": true, "isInternal": false,
                "label": "Left",
            });
            let answer = match name.as_str() {
                "getScreenDetails" => json!({ "screens": [screen], "currentScreen": screen }),
                "windowManagementStatus" => json!({ "available": true, "state": "granted" }),
                _ => {
                    screen["clientRect"] = json!({ "x": 0, "y": 0, "width": 2048, "height": 1152 });
                    json!({
                        "screenX": 2000, "screenY": 400, "clientX": 2000, "clientY": 400,
                        "screen": screen,
                    })
                }
            };
            seen.lock().unwrap().push(name);
            reply.send(Outcome::Response(Response {
                status: 200,
                headers: Vec::new(),
                body: answer.to_string().into_bytes(),
            }));
        })));
    r.bind_extensions(&bound, &[]);
    pump(&mut r);
    r.act("ask", Vec::new()).unwrap();
    pump(&mut r);
    let (details, at) = (show(&r, "details"), show(&r, "at"));
    assert!(
        details.contains("Left") && !details.contains("Agent display"),
        "{details}"
    );
    assert!(at.contains("2000") && !at.contains("720"), "{at}");
    let mut calls = calls.lock().unwrap().clone();
    calls.sort();
    calls.dedup();
    assert_eq!(
        calls,
        [
            "getPointerPosition",
            "getScreenDetails",
            "windowManagementStatus"
        ]
    );
}

#[test]
fn a_window_bounds_read_is_supported_on_mac_and_web() {
    let app = App::new("bounds");
    for target in [
        "macos", "web", "ios", "tvos", "watchos", "visionos", "android", "linux", "windows",
        "terminal",
    ] {
        let mut r = boot(&app, target);
        r.act("readFrame", Vec::new()).unwrap();
        settle(&mut r);
        if matches!(target, "macos" | "web") {
            assert_eq!(
                r.derive("rectangle").and_then(|v| v.as_str()),
                Some("0,0 420x860"),
                "{target}"
            );
        } else {
            let logs = agent::handle(&r, r#"{"op":"logs"}"#);
            assert!(
                logs.contains("getWindowBounds: NotSupportedError"),
                "{target}: {logs}"
            );
        }
    }
}

/// D6: under the agent, with no implementation bound, the declaration
/// answers its fixture on a target that has an implementation, and where
/// none exists the calls fail `NotSupportedError` and the status reads
/// unavailable.
#[test]
fn the_substitute_answers_only_where_an_implementation_exists() {
    let app = App::new("substitute");
    for target in ["macos", "linux", "windows"] {
        let mut r = boot(&app, target);
        let (details, status) = (show(&r, "details"), show(&r, "status"));
        assert!(details.contains("Agent display"), "{target}: {details}");
        assert!(
            status.contains("true") && status.contains("granted"),
            "{target}: {status}"
        );
        r.act("ask", Vec::new()).unwrap();
        settle(&mut r);
        assert!(
            show(&r, "at").contains("720"),
            "{target}: {}",
            show(&r, "at")
        );
    }
    for target in ["ios", "android", "tvos", "terminal"] {
        let mut r = boot(&app, target);
        let status = show(&r, "status");
        assert!(
            status.contains("false") && status.contains("denied"),
            "{target}: {status}"
        );
        r.act("ask", Vec::new()).unwrap();
        settle(&mut r);
        let logs = agent::handle(&r, r#"{"op":"logs"}"#);
        assert!(
            !show(&r, "at").contains("720"),
            "{target}: {}",
            show(&r, "at")
        );
        assert!(
            logs.contains("getScreenDetails: NotSupportedError")
                && logs.contains("getPointerPosition: NotSupportedError"),
            "{target}: {logs}"
        );
    }
}

#[test]
fn a_still_capture_is_a_granted_send_with_desktop_implementations() {
    let app = App::new("capture");
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/screen-capture");
    copy_tree(&package, &app.0.join("node_modules/@exact/screen-capture"));
    std::fs::write(app.0.join("app.contract"), r#"use captureScreens, screenRecordingStatus, requestScreenRecording, ScreenCaptures, ScreenRecordingStatus from "@exact/screen-capture"
component App
  resource status = screenRecordingStatus() as shape ScreenRecordingStatus
  mutation shot as shape ScreenCaptures
  action capture
    send shot = captureScreens(directory="app:/tmp/freeze")
  action root
    send shot = captureScreens(directory="app:/tmp")
  action slash
    send shot = captureScreens(directory="app:/tmp/freeze/")
  view
    button "Capture" press=capture
"#).unwrap();
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(plan
        .modules
        .iter()
        .any(|m| plan.str(m.declared) == "captureScreens" && m.send));
    assert_eq!(
        contract::native::bound(&app.0, &["screen-capture"], "windows")
            .unwrap()
            .len(),
        1
    );
    struct CaptureGrants(&'static str);
    impl exact_runner::DataSource for CaptureGrants {
        fn query(
            &mut self,
            name: &str,
            _: &[exact_runner::Value],
        ) -> Result<exact_runner::Value, exact_runner::DataError> {
            Err(exact_runner::DataError::UnknownSource(name.into()))
        }
        fn grants(&self) -> &str {
            self.0
        }
    }
    for (grants, origin, allowed) in [
        (
            "device.screen-recording shot.purpose\nfs.write app:/tmp",
            exact_runner::CallOrigin::App,
            true,
        ),
        (
            "device.screen-recording shot.purpose",
            exact_runner::CallOrigin::App,
            false,
        ),
        ("fs.write app:/tmp", exact_runner::CallOrigin::App, false),
        (
            "device.screen-recording shot.purpose\nfs.write app:/tmp",
            exact_runner::CallOrigin::Tool,
            false,
        ),
        (
            "device.screen-recording shot.purpose\nfs.write app:/tmp",
            exact_runner::CallOrigin::Wake,
            false,
        ),
    ] {
        let mut runner = Runner::boot(
            plan.clone(),
            boxed(CaptureGrants(grants)),
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        runner.set_module_target("windows");
        let calls = Arc::new(Mutex::new(0));
        let called = calls.clone();
        runner
            .native_slot()
            .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
                let body: Json = serde_json::from_slice(&body).unwrap();
                let answer = if body["send"] == "captureScreens" {
                    *called.lock().unwrap() += 1;
                    json!({"captures":[]})
                } else {
                    json!({"available":true,"state":"granted"})
                };
                reply.send(Outcome::Response(Response {
                    status: 200,
                    headers: Vec::new(),
                    body: answer.to_string().into_bytes(),
                }));
            })));
        runner.bind_extensions(&["screen-capture"], &[]);
        pump(&mut runner);
        // A directory option admits an app root and a trailing slash.
        for action in ["capture", "root", "slash"] {
            runner.act_from(origin, action, Vec::new()).unwrap();
            pump(&mut runner);
        }
        assert_eq!(
            *calls.lock().unwrap(),
            3 * usize::from(allowed),
            "{grants}: {origin:?}"
        );
    }
}

#[test]
fn the_guide_reads_a_scripted_hit_only_from_input_or_a_delivered_event_with_its_grant() {
    use exact_runner::{CallOrigin, Event};
    let app = App::new("guide-hit");
    for package in ["desktop-accessibility", "global-shortcuts"] {
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../packages")
                .join(package),
            &app.0.join("node_modules/@exact").join(package),
        );
    }
    let entry = app.0.join("app.contract");
    std::fs::write(
        &entry,
        r#"use elementAtPoint, AccessibleElement, accessibilityStatus, requestAccessibility, AccessibilityStatus from "@exact/desktop-accessibility"
use globalshortcut from "@exact/global-shortcuts"
component App
  resource trust = accessibilityStatus() as shape AccessibilityStatus
  mutation permission as shape AccessibilityStatus
  action requestTrust
    send permission = requestAccessibility()
  mutation hit as shape AccessibleElement
  derive why = match failure(hit) { case some(f) => f.code, case none => "" }
  derive label = match hit { case some(h) => `${h.name} ${h.role} in ${h.app.name} (${h.app.pid}) at ${h.frame.x},${h.frame.y} ${h.frame.width}x${h.frame.height}`, case none => "" }
  action ask
    send hit = elementAtPoint(x=7, y=10)
  action checked(on: bool)
    send hit = elementAtPoint(x=7, y=10)
  action changed(text: string)
    send hit = elementAtPoint(x=7, y=10)
  view
    column globalshortcut=ask
      button "Describe" testId="describe" press=ask pointerup=ask
      input type="checkbox" testId="check" change=checked
      input testId="field" change=changed
      button "Allow" testId="allow" press=requestTrust
      text trust.state
      text why
      text label
"#,
    )
    .unwrap();
    std::fs::write(
        app.0.join("app.json"),
        r#"{"name":"Guide","app":{"id":"test.guide","name":"Guide"}}"#,
    )
    .unwrap();
    struct Grant(&'static str);
    impl exact_runner::DataSource for Grant {
        fn query(
            &mut self,
            name: &str,
            _: &[exact_runner::Value],
        ) -> Result<exact_runner::Value, exact_runner::DataError> {
            Err(exact_runner::DataError::UnknownSource(name.into()))
        }
        fn grants(&self) -> &str {
            self.0
        }
    }
    for (grant, enabled) in [
        ("device.accessibility", true),
        ("device.global-shortcuts", false),
    ] {
        std::fs::write(
            app.0.join("app.ts"),
            format!("export const grants = '{grant}';"),
        )
        .unwrap();
        let (result, warnings) = contract::compile_path_all_with_warnings(&entry, false);
        assert_eq!(
            warnings
                .iter()
                .any(|w| w.id == "grant-missing" && w.message.contains("device.accessibility")),
            !enabled,
            "{warnings:?}"
        );
        let (plan, _) = result.unwrap();
        let mut r = Runner::boot(
            plan.clone(),
            boxed(Grant(grant)),
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        r.set_module_target("windows");
        r.set_module_drive(true);
        r.native_slot()
            .host(Some(Arc::new(|body: Vec<u8>, reply: Reply| {
                let body: Json = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["source"], "accessibilityStatus");
                reply.send(Outcome::Response(Response {
                    status: 200,
                    headers: Vec::new(),
                    body: br#"{"available":true,"state":"denied"}"#.to_vec(),
                }));
            })));
        r.bind_extensions(&["desktop-accessibility"], &[]);
        r.listen(std::sync::Arc::new(|| {}));
        let mut prefer = json!({"op":"prefer","extension":{"desktop-accessibility.elementAtPoint":{
            "role":"button","name":"Export","frame":{"x":5,"y":8,"width":35,"height":20},
            "app":{"name":"Paint","processName":"mspaint.exe","pid":42}}}});
        r.extension_agent(&prefer.to_string()).unwrap().1.unwrap();
        for origin in [
            CallOrigin::App,
            CallOrigin::Device,
            CallOrigin::Wake,
            CallOrigin::Tool,
            CallOrigin::Action,
        ] {
            r.act_from(origin, "ask", vec![]).unwrap();
            settle(&mut r);
            assert_eq!(
                r.derive("why").and_then(exact_runner::Value::as_str),
                Some("NotAllowedError"),
                "{origin:?}"
            );
        }
        let key = r.kernel().find_by_test_id("describe")[0];
        let describe_id = r.kernel().node_by_key(key).unwrap().id;
        r.dispatch(describe_id, Event::Press).unwrap();
        settle(&mut r);
        let result = |r: &Runner<Source>, name| {
            r.derive(name)
                .and_then(exact_runner::Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        assert_eq!(
            result(&r, "why"),
            if enabled { "" } else { "NotAllowedError" }
        );
        assert_eq!(
            result(&r, "label"),
            if enabled {
                "Export button in Paint (42) at 5,8 35x20"
            } else {
                ""
            }
        );
        let field = r.kernel().find_by_test_id("field")[0];
        let field_id = r.kernel().node_by_key(field).unwrap().id;
        r.dispatch(field_id, Event::Change("finished".into()))
            .unwrap();
        settle(&mut r);
        assert_eq!(result(&r, "why"), "NotAllowedError");
        let mouse = exact_runner::PointerEvent::parse("7,10,0,0,0,mouse,1,7,10").unwrap();
        r.dispatch(describe_id, Event::Pointerup(mouse)).unwrap();
        settle(&mut r);
        assert_eq!(
            result(&r, "why"),
            if enabled { "" } else { "NotAllowedError" }
        );
        prefer["extension"]["desktop-accessibility.elementAtPoint"]["name"] = json!("Checkbox");
        r.extension_agent(&prefer.to_string()).unwrap().1.unwrap();
        let check = r.kernel().find_by_test_id("check")[0];
        let id = r.kernel().node_by_key(check).unwrap().id;
        r.dispatch(id, Event::Change(exact_runner::ControlValue::Checked(true)))
            .unwrap();
        settle(&mut r);
        assert_eq!(show(&r, "hit").contains("Checkbox"), enabled);
        prefer["extension"]["desktop-accessibility.elementAtPoint"]["name"] = json!("Delivered");
        r.extension_agent(&prefer.to_string()).unwrap().1.unwrap();
        let event = r#"{"extension":"global-shortcuts","event":"globalshortcut","args":["hot",{"id":"hot","keys":"Meta+G"}]}"#;
        let native = r.native_slot();
        native.changed(&format!("exact-event:{event}"));
        assert!(r.apply_announced().1.is_none());
        settle(&mut r);
        assert_eq!(show(&r, "hit").contains("Delivered"), enabled);
        for origin in [CallOrigin::Device, CallOrigin::App] {
            let next = format!("Export from {origin:?}");
            prefer["extension"]["desktop-accessibility.elementAtPoint"]["name"] = json!(next);
            r.extension_agent(&prefer.to_string()).unwrap().1.unwrap();
            r.raise_module_event(
                "global-shortcuts",
                "globalshortcut",
                r#"["hot",{"id":"hot","keys":"Meta+G"}]"#,
                origin,
            )
            .unwrap();
            let state: Json =
                serde_json::from_str(&agent::handle(&r, r#"{"op":"state"}"#)).unwrap();
            assert_eq!(
                state["extensions"]["global-shortcuts"]["queued"][0]["origin"],
                origin.word()
            );
            // A queued event keeps its scripted hit after an attached driver leaves.
            r.set_module_drive(false);
            settle(&mut r);
            pump(&mut r);
            assert!(show(&r, "trust").contains("denied"));
            r.set_module_drive(true);
            settle(&mut r);
            assert!(show(&r, "trust").contains("granted"));
            assert_eq!(
                show(&r, "hit").contains(&next),
                enabled,
                "{origin:?}: {}",
                agent::handle(&r, r#"{"op":"logs"}"#)
            );
        }
        if enabled {
            r.set_module_target("linux");
            r.act_from(CallOrigin::App, "ask", vec![]).unwrap();
            settle(&mut r);
            assert_eq!(result(&r, "why"), "NotSupportedError");
            for target in ["macos", "windows"] {
                r.set_module_target(target);
                for scope in ["device.accessibility", "device.global-shortcuts"] {
                    let mut call = exact_runner::Request::native(
                        json!({"extension":"desktop-accessibility", "source":"accessibilityStatus", "args":{}}).to_string().into_bytes(),
                    );
                    call.grants = Some(scope.into());
                    let Dispatch::Run(Work::Now(answer)) = r.native_work(&call) else {
                        panic!("a scripted trust status answers immediately");
                    };
                    let Outcome::Response(response) = answer() else {
                        panic!("a trust status reply")
                    };
                    let status: Json = serde_json::from_slice(&response.body).unwrap();
                    assert_eq!(status["available"], true, "{target}: {status}");
                    assert_eq!(status["state"], "granted", "{target}: {status}");
                }
            }
            r.set_module_target("macos");
            for permission in ["prompt", "denied", "granted"] {
                r.extension_agent(
                    &json!({"op":"prefer","permission":{"accessibility":permission}}).to_string(),
                )
                .unwrap()
                .1
                .unwrap();
                settle(&mut r);
                assert!(show(&r, "trust").contains(permission));
                r.dispatch(describe_id, Event::Press).unwrap();
                settle(&mut r);
                assert_eq!(
                    result(&r, "why"),
                    if permission != "granted" {
                        "NotAllowedError"
                    } else {
                        ""
                    }
                );
            }
            let allow = r.kernel().find_by_test_id("allow")[0];
            let id = r.kernel().node_by_key(allow).unwrap().id;
            r.dispatch(id, Event::Press).unwrap();
            settle(&mut r);
            assert!(show(&r, "permission").contains("granted"));
        }
    }
}
