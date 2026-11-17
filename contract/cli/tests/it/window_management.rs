//! `@exact/window-management` (LLP 1178) through this checkout's package,
//! installed beside the app: the targets whose build binds the package's
//! own implementation, what a Windows app reads with it bound, and what the
//! declared substitute answers on each target.

use exact_runner::{agent, Dispatch, Outcome, Reply, Response, Runner, Work};
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const APP: &str = r#"use getScreenDetails, getPointerPosition, windowManagementStatus, ScreenDetails, PointerPosition, WindowManagementStatus from "@exact/window-management"

component App
  resource details = getScreenDetails() as shape ScreenDetails
  resource status = windowManagementStatus() as shape WindowManagementStatus
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

fn boot(app: &App, target: &str) -> Runner<Grants> {
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = Runner::boot(
        plan,
        Grants,
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

fn show(r: &Runner<Grants>, name: &str) -> String {
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
            CaptureGrants(grants),
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
        runner.act_from(origin, "capture", Vec::new()).unwrap();
        pump(&mut runner);
        assert_eq!(
            *calls.lock().unwrap(),
            usize::from(allowed),
            "{grants}: {origin:?}"
        );
    }
}
