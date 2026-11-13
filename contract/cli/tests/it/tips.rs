//! `@exact/tips` in the runner (LLP 1035.006 row 11.08), through this
//! checkout's package installed beside the app, its implementation played by
//! the test as the host's native slot: on Apple's targets each send reaches
//! the implementation with what the call gave, a tool cannot offer a tip, and
//! every other target has no TipKit.

use exact_runner::{CallOrigin, Dispatch, Outcome, Reply, Response, Runner, Work};
use serde_json::{json, Value as Json};
#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const APP: &str = r#"use showTip, invalidateTip from "@exact/tips"

component App
  action offer
    showTip(id="add-tip", title="Add a book", message="Tap + to add one.", image="plus.circle", source="add")
  action add
    invalidateTip(id="add-tip")
  view
    column
      button press=add id="add" aria-label="Add"
        text "+"
"#;

struct App(PathBuf);
impl App {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("exact-tips-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("node_modules/@exact")).unwrap();
        let tips = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/tips");
        symlink_dir(
            tips.canonicalize().unwrap(),
            path.join("node_modules/@exact/tips"),
        )
        .unwrap();
        std::fs::write(path.join("app.contract"), APP).unwrap();
        let manifest =
            json!({ "name": "Tips", "app": { "id": "com.example.tips", "name": "Tips" } });
        std::fs::write(path.join("app.json"), manifest.to_string()).unwrap();
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
        ""
    }
}

type Calls = Arc<Mutex<Vec<Json>>>;

/// A runner over the app on `target`, the implementation bound under the
/// agent (`bound`) and played by the native slot, which answers nothing.
fn boot(name: &str, target: &str, bound: bool) -> (App, Runner<Grants>, Calls) {
    let app = App::new(name);
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
    let calls: Calls = Arc::default();
    let seen = calls.clone();
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            seen.lock()
                .unwrap()
                .push(serde_json::from_slice(&body).unwrap());
            reply.send(Outcome::Response(Response {
                status: 200,
                headers: Vec::new(),
                body: b"{}".to_vec(),
            }));
        })));
    r.bind_extensions(&[], if bound { &["tips"] } else { &[] });
    (app, r, calls)
}

fn pump(r: &mut Runner<Grants>) {
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

fn sent(calls: &Calls, name: &str) -> Vec<Json> {
    (calls.lock().unwrap().iter())
        .filter(|c| c["send"] == name)
        .cloned()
        .collect()
}

fn logs(r: &Runner<Grants>) -> String {
    exact_runner::agent::handle(r, r#"{"op":"logs"}"#)
}

/// Each send reaches the implementation with what the call gave; a tool's
/// `showTip` is refused before it, since a tip interrupts the person.
#[test]
fn a_tip_is_offered_and_invalidated_through_the_implementation() {
    for target in ["ios", "macos"] {
        let (_app, mut r, calls) = boot(target, target, true);
        pump(&mut r);
        r.act("offer", Vec::new()).unwrap();
        r.act("add", Vec::new()).unwrap();
        pump(&mut r);
        let offered = sent(&calls, "showTip");
        assert_eq!(offered.len(), 1, "{target}: {}", logs(&r));
        assert_eq!(offered[0]["extension"], "tips");
        assert_eq!(
            offered[0]["args"],
            json!({ "id": "add-tip", "title": "Add a book", "message": "Tap + to add one.", "image": "plus.circle", "source": "add" })
        );
        let invalidated = sent(&calls, "invalidateTip");
        assert_eq!(invalidated.len(), 1, "{target}: {}", logs(&r));
        assert_eq!(invalidated[0]["args"], json!({ "id": "add-tip" }));

        r.act_from(CallOrigin::Tool, "offer", Vec::new()).unwrap();
        pump(&mut r);
        assert_eq!(sent(&calls, "showTip").len(), 1, "{target}: {}", logs(&r));
        assert!(
            logs(&r).contains("tips.showTip: refused, person-required (tool origin)"),
            "{target}: {}",
            logs(&r)
        );
    }
}

/// Where there is no TipKit both sends fail `NotSupportedError`, under the
/// agent too, and nothing reaches a host.
#[test]
fn other_targets_have_no_tips() {
    for target in ["web", "android", "linux", "windows", "tvos"] {
        let (_app, mut r, calls) = boot(&format!("none-{target}"), target, false);
        pump(&mut r);
        r.act("offer", Vec::new()).unwrap();
        r.act("add", Vec::new()).unwrap();
        pump(&mut r);
        assert!(calls.lock().unwrap().is_empty(), "{target}");
        let said = logs(&r);
        assert_eq!(
            said.matches("NotSupportedError").count(),
            2,
            "{target}: {said}"
        );
    }
}
