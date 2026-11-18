//! `@exact/app-icon` under the agent (LLP 1163.000 D4), through this
//! checkout's package installed beside the app: the roster is the agent's
//! word `app-icon.icons`; a change is held under the `app-icon` ticket that
//! stands for iOS's alert; the icon already shown answers at once; a name
//! outside the roster, another target and a tool are refused with the
//! runner's names.

use exact_runner::{CallOrigin, Dispatch, Reply, Runner, Work};
use serde_json::{json, Value as Json};
#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;
use std::path::{Path, PathBuf};

const APP: &str = r#"use appIcon, setAppIcon, AppIconStatus from "@exact/app-icon"

component App
  resource shown = appIcon() as shape AppIconStatus
  mutation chosen as shape AppIconStatus refreshes shown
  derive answered = match chosen { case some(s) => s.icon, case none => none }
  action pick(name: string)
    send chosen = setAppIcon(icon=name)
  action primary
    send chosen = setAppIcon()
  view
    column
      text `${shown.available}`
"#;

struct App(PathBuf);
impl App {
    fn new(name: &str, source: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("exact-app-icon-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        std::fs::create_dir_all(path.join("node_modules/@exact")).unwrap();
        let to = packages.join("app-icon").canonicalize().unwrap();
        symlink_dir(to, path.join("node_modules/@exact/app-icon")).unwrap();
        std::fs::write(path.join("app.contract"), source).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct NoData;
impl exact_runner::DataSource for NoData {
    fn query(
        &mut self,
        source: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(source.into()))
    }
}

fn boot(name: &str, target: &str) -> (App, Runner<NoData>) {
    boot_app(name, target, APP)
}

fn boot_app(name: &str, target: &str, source: &str) -> (App, Runner<NoData>) {
    let app = App::new(name, source);
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = Runner::boot(
        plan,
        NoData,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.set_module_target(target);
    settle(&mut r);
    (app, r)
}

thread_local! {
    /// Each request's continuation token and its ticket, until it is answered.
    static PARKED: std::cell::RefCell<Vec<(u64, u64)>> = Default::default();
}

/// The host's turn: requests dispatched, released answers fulfilled, and
/// what they announced applied, until nothing moves.
fn settle(r: &mut Runner<NoData>) {
    for _ in 0..8 {
        let outs = r.take_requests();
        let released = r.release_work();
        if outs.is_empty() && released.is_empty() && r.apply_announced().0.is_empty() {
            return;
        }
        for out in outs {
            let dispatch = match out.request.continuation {
                Some(token) => {
                    PARKED.with(|p| p.borrow_mut().push((token, out.ticket)));
                    r.dispatch_work(token)
                }
                None => r.native_work(&out.request),
            };
            run(r, out.ticket, dispatch);
        }
        for (token, dispatch) in released {
            let ticket = PARKED.with(|p| p.borrow().iter().find(|(t, _)| *t == token).map(|p| p.1));
            run(r, ticket.expect("parked"), dispatch);
        }
    }
}

fn run(r: &mut Runner<NoData>, ticket: u64, dispatch: Dispatch) {
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

fn pick(r: &mut Runner<NoData>, icon: Option<&str>) {
    match icon {
        Some(icon) => r.act("pick", vec![icon.into()]),
        None => r.act("primary", vec![]),
    }
    .unwrap();
    settle(r);
}

fn logs(r: &Runner<NoData>) -> String {
    exact_runner::agent::handle(r, r#"{"op":"logs"}"#)
}

fn prefer_icons(r: &mut Runner<NoData>, icons: Json) {
    let line = json!({ "op": "prefer", "extension": { "app-icon.icons": icons } }).to_string();
    r.extension_agent(&line).unwrap().1.unwrap();
    settle(r);
}

/// The resource's value as `{available, icon, icons}`, from its debug form.
fn shown(r: &Runner<NoData>) -> String {
    format!("{:?}", r.resource("shown"))
}

fn tap(r: &mut Runner<NoData>, choice: &str) {
    let holds = r.device_holds();
    assert_eq!(holds.len(), 1, "{}", logs(r));
    assert_eq!(holds[0].capability, "app-icon");
    assert_eq!(holds[0].choices, ["ok"]);
    let line = json!({ "op": "tap", "ticket": holds[0].ticket, "choice": choice }).to_string();
    exact_runner::agent::answer(r, &line).expect("the alert's ticket");
    settle(r);
}

#[test]
fn a_change_waits_for_the_alert_and_the_icon_shown_answers_at_once() {
    let (_app, mut r) = boot("change", "ios");
    assert!(
        shown(&r).contains("Bool(false)"),
        "no roster: {}",
        shown(&r)
    );
    pick(&mut r, Some("Paper"));
    assert!(logs(&r).contains("NotSupportedError"), "{}", logs(&r));

    prefer_icons(&mut r, json!(["Paper", "Night"]));
    assert!(
        shown(&r).contains(r#"Bool(true), Option(None), List([Str("Paper"), Str("Night")])"#),
        "{}",
        shown(&r)
    );
    pick(&mut r, Some("Card"));
    assert!(logs(&r).contains("NotFoundError"), "{}", logs(&r));
    assert!(r.device_holds().is_empty());

    pick(&mut r, Some("Paper"));
    tap(&mut r, "ok");
    assert!(logs(&r).contains("setAppIcon: shows Paper"), "{}", logs(&r));
    assert!(
        shown(&r).contains(r#"Option(Some(Str("Paper")))"#),
        "{}",
        shown(&r)
    );
    let resolved = format!("{:?}", r.derive("answered"));
    assert!(
        resolved.contains(r#"Option(Some(Str("Paper")))"#),
        "the send answers the new status: {resolved}"
    );

    // The icon shown: no alert.
    pick(&mut r, Some("Paper"));
    assert!(r.device_holds().is_empty(), "{}", logs(&r));
    assert!(logs(&r).contains("setAppIcon: unchanged"), "{}", logs(&r));

    // Dismissing iOS's alert does not undo the change.
    pick(&mut r, None);
    tap(&mut r, "cancel");
    assert!(
        shown(&r).contains("Bool(true), Option(None)"),
        "{}",
        shown(&r)
    );
}

/// An update that removed every alternate leaves the one shown, which the
/// app can still put back to the primary (D3).
#[test]
fn the_primary_comes_back_when_the_roster_is_gone() {
    let (_app, mut r) = boot("removed", "ios");
    prefer_icons(&mut r, json!(["Paper"]));
    pick(&mut r, Some("Paper"));
    tap(&mut r, "ok");
    prefer_icons(&mut r, json!([]));
    assert!(
        shown(&r).contains(r#"Bool(true), Option(Some(Str("Paper"))), List([])"#),
        "{}",
        shown(&r)
    );
    pick(&mut r, Some("Paper"));
    assert!(
        r.device_holds().is_empty(),
        "the icon shown answers at once"
    );
    pick(&mut r, None);
    tap(&mut r, "ok");
    assert!(
        shown(&r).contains("Bool(false), Option(None), List([])"),
        "{}",
        shown(&r)
    );
}

/// A tool cannot change the icon, and no other target has one to change.
#[test]
fn a_tool_and_other_targets_are_refused() {
    let (_app, mut r) = boot("tool", "ios");
    prefer_icons(&mut r, json!(["Paper"]));
    r.act_from(CallOrigin::Tool, "pick", vec!["Paper".into()])
        .unwrap();
    settle(&mut r);
    assert!(r.device_holds().is_empty(), "{}", logs(&r));
    assert!(logs(&r).contains("person-required"), "{}", logs(&r));

    for target in ["web", "macos", "android", "tvos", "linux", "windows"] {
        let (_app, mut r) = boot("elsewhere", target);
        prefer_icons(&mut r, json!(["Paper"]));
        assert!(
            shown(&r).contains("Bool(false), Option(None), List([])"),
            "{target}: {}",
            shown(&r)
        );
        pick(&mut r, Some("Paper"));
        assert!(r.device_holds().is_empty(), "{target}");
        assert!(
            logs(&r).contains("NotSupportedError"),
            "{target}: {}",
            logs(&r)
        );
    }
}

/// The status is a fact with no topic (D3): after a change only a mutation
/// that `refreshes` it reads it again, as on iOS, where nothing announces it.
#[test]
fn a_change_refreshes_only_what_the_app_says() {
    let unrefreshed = APP.replace(" refreshes shown", "");
    let (_app, mut r) = boot_app("unrefreshed", "ios", &unrefreshed);
    prefer_icons(&mut r, json!(["Paper"]));
    pick(&mut r, Some("Paper"));
    tap(&mut r, "ok");
    let answered = format!("{:?}", r.derive("answered"));
    assert!(
        answered.contains(r#"Str("Paper")"#),
        "the send answers: {answered}"
    );
    assert!(
        shown(&r).contains("Bool(true), Option(None)"),
        "read before the change: {}",
        shown(&r)
    );
}
