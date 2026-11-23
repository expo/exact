//! A newer binary from the store, admitted by the runner (LLP 1133.038),
//! through this checkout's `@exact/store-update` installed beside the app,
//! its implementation played by the test as the host's native slot under the
//! agent: Play's flow held as a `store-update` ticket on Android, the origin
//! rule, the lookup's grant line, and the targets with no store.

use crate::source::{boxed, Source};
use exact_runner::{CallOrigin, Dispatch, Outcome, Reply, Response, Runner, Work};
use serde_json::{json, Value as Json};
#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const APP: &str = r#"use storeUpdateStatus, openStorePage, StoreUpdateStatus from "@exact/store-update"
use startStoreUpdate, completeStoreUpdate, StoreUpdateResult from "@exact/store-update/android"

component App
  resource store = storeUpdateStatus() as shape StoreUpdateStatus
  mutation started as shape StoreUpdateResult
  action open
    openStorePage()
  action now
    send started = startStoreUpdate(appUpdateType="immediate")
  action restart
    completeStoreUpdate()
  view
    column
      text store.updateAvailability
"#;

const LOOKUP: &str = "net.fetch https://itunes.apple.com\n";

struct App(PathBuf);
impl App {
    fn new(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("exact-su-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/store-update");
        std::fs::create_dir_all(path.join("node_modules/@exact")).unwrap();
        symlink_dir(
            package.canonicalize().unwrap(),
            path.join("node_modules/@exact/store-update"),
        )
        .unwrap();
        std::fs::write(path.join("app.contract"), APP).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Grants(&'static str);
impl exact_runner::DataSource for Grants {
    fn query(
        &mut self,
        source: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(source.into()))
    }
    fn grants(&self) -> &str {
        self.0
    }
}

type Calls = Arc<Mutex<Vec<Json>>>;

/// A runner over the app on `target` with `grants`, the implementation bound
/// under the agent when `bound`, and played by the native slot as a store
/// install with nothing newer.
fn boot(
    name: &str,
    target: &str,
    grants: &'static str,
    bound: bool,
) -> (App, Runner<Source>, Calls) {
    let app = App::new(name);
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = Runner::boot(
        plan,
        boxed(Grants(grants)),
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
            let v: Json = serde_json::from_slice(&body).unwrap();
            seen.lock().unwrap().push(v.clone());
            let answer = match (v["send"].as_str(), v["source"].as_str()) {
                (Some("startStoreUpdate"), _) => json!({ "outcome": v["preferred"]["tap"] }),
                (_, Some("storeUpdateStatus")) => json!({ "available": true, "updateAvailability": "current", "apple": null, "android": null }),
                _ => Json::Null,
            };
            reply.send(Outcome::Response(Response {
                status: 200,
                headers: Vec::new(),
                body: answer.to_string().into_bytes(),
            }));
        })));
    r.bind_extensions(&[], if bound { &["store-update"] } else { &[] });
    (app, r, calls)
}

fn pump(r: &mut Runner<Source>, parked: &mut Vec<(u64, u64)>) {
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

fn run(r: &mut Runner<Source>, ticket: u64, dispatch: Dispatch) {
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

fn sent(calls: &Calls, name: &str) -> Vec<Json> {
    (calls.lock().unwrap().iter())
        .filter(|c| c["send"] == name)
        .cloned()
        .collect()
}

fn logs(r: &Runner<Source>) -> String {
    exact_runner::agent::handle(r, r#"{"op":"logs"}"#)
}

/// On Android Play's flow holds a `store-update` sheet answered by tap
/// alone, and the tap reaches the implementation as the call's `preferred`;
/// a tool's start fails `person-required` before any sheet.
#[test]
fn plays_flow_is_a_ticket_and_a_tool_cannot_start_it() {
    let (_app, mut r, calls) = boot("flow", "android", LOOKUP, true);
    let mut parked = Vec::new();
    pump(&mut r, &mut parked);
    r.act("now", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    let holds = r.device_holds().to_vec();
    assert_eq!(holds.len(), 1, "{}", logs(&r));
    assert_eq!(
        (holds[0].capability.as_str(), holds[0].choices.clone()),
        (
            "store-update",
            vec!["accepted".to_owned(), "canceled".into(), "failed".into()]
        )
    );
    let args: Json = serde_json::from_str(&holds[0].args).unwrap();
    assert_eq!(args["options"]["appUpdateType"], "immediate");
    assert!(
        sent(&calls, "startStoreUpdate").is_empty(),
        "held before the implementation"
    );

    let line = json!({ "op": "type", "ticket": holds[0].ticket, "text": "yes" }).to_string();
    let (typed, _) = exact_runner::agent::answer(&mut r, &line).expect("a module's ticket");
    assert!(typed.contains("tap"), "{typed}");
    let line = json!({ "op": "tap", "ticket": holds[0].ticket, "choice": "canceled" }).to_string();
    exact_runner::agent::answer(&mut r, &line).expect("a module's ticket");
    pump(&mut r, &mut parked);
    let asked = sent(&calls, "startStoreUpdate");
    assert_eq!(asked.len(), 1, "{}", logs(&r));
    assert_eq!(asked[0]["preferred"], json!({ "tap": "canceled" }));

    r.act_from(CallOrigin::Tool, "now", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert!(r.device_holds().is_empty(), "{}", logs(&r));
    assert!(
        logs(&r).contains("store-update.startStoreUpdate#@exact/store-update/android: refused, person-required (tool origin)"),
        "{}",
        logs(&r)
    );
}

/// The status and the store page answer only under a `net.fetch` line, the
/// App Store lookup's (D3): without one both are refused `grant-missing`
/// before the implementation is asked.
#[test]
fn the_lookup_needs_its_grant_line() {
    let (_app, mut r, calls) = boot("grant", "ios", "", true);
    let mut parked = Vec::new();
    pump(&mut r, &mut parked);
    r.act("open", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    let said = logs(&r);
    assert!(
        said.contains("store-update.storeUpdateStatus: refused, grant-missing (net.fetch)"),
        "{said}"
    );
    assert!(
        said.contains("store-update.openStorePage: refused, grant-missing (net.fetch)"),
        "{said}"
    );
    let asked = (calls.lock().unwrap().iter())
        .filter(|c| c.get("send").is_some() || c.get("source").is_some())
        .count();
    assert_eq!(asked, 0, "{:?}", calls.lock().unwrap());

    let (_app, mut r, calls) = boot("granted", "ios", LOOKUP, true);
    let mut parked = Vec::new();
    pump(&mut r, &mut parked);
    r.act("open", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert_eq!(sent(&calls, "openStorePage").len(), 1, "{}", logs(&r));
    assert!(format!("{:?}", r.resource("store")).contains("current"));
}

/// Play's flows exist on Android alone, and the store page on the stores'
/// targets: elsewhere each send fails `NotSupportedError` before any sheet,
/// and the status, answered by the declared default, is `unknown` (§7).
#[test]
fn the_targets_with_no_store_refuse_by_name() {
    for target in ["web", "linux", "windows", "ios", "macos"] {
        let (_app, mut r, calls) = boot("absent", target, LOOKUP, false);
        let mut parked = Vec::new();
        pump(&mut r, &mut parked);
        r.act("now", Vec::new()).unwrap();
        r.act("restart", Vec::new()).unwrap();
        pump(&mut r, &mut parked);
        assert!(r.device_holds().is_empty(), "{target}: {}", logs(&r));
        assert!(calls.lock().unwrap().is_empty(), "{target}");
        let said = logs(&r);
        assert!(
            said.contains("startStoreUpdate#@exact/store-update/android: NotSupportedError")
                && said.contains("completeStoreUpdate: refused: NotSupportedError"),
            "{target}: {said}"
        );
        if ["web", "linux", "windows"].contains(&target) {
            r.act("open", Vec::new()).unwrap();
            pump(&mut r, &mut parked);
            assert!(
                logs(&r).contains("openStorePage: refused: NotSupportedError"),
                "{target}: {}",
                logs(&r)
            );
            // available, updateAvailability: false, unknown.
            let status = format!("{:?}", r.resource("store"));
            assert!(
                status.contains(r#"Bool(false)"#) && status.contains(r#"Str("unknown")"#),
                "{target}: {status}"
            );
        }
    }
}
