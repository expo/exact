//! LLP 1133.025 stage 0 in the runner, through this checkout's
//! `@exact/login-item`, `@exact/global-shortcuts` and `@exact/dock`
//! installed beside the app and answered by their substitutes, as under the
//! agent on macOS: the login item's registration and a launch the login item
//! made; a global shortcut listed while read, pressed by `tap @ref`, moved
//! to new keys and released with its last reader; the absent cells; and the
//! status item and Dock menu the agent reads from the tree.

use crate::source::{boxed, Source};
use exact_runner::{agent, CallOrigin, Runner};
use serde_json::{json, Value as Json};
#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;
use std::path::{Path, PathBuf};

const APP: &str = r#"use loginItemStatus, registerLoginItem, unregisterLoginItem, LoginItemStatus from "@exact/login-item"
use globalShortcut, checkGlobalShortcut, globalShortcutStatus, globalshortcut, globalshortcutup, GlobalShortcut, GlobalShortcutCheck, GlobalShortcutStatus from "@exact/global-shortcuts"
use requestAttention from "@exact/dock"

component App
  state chord = "Meta+Shift+2"
  state lifts = 0
  state pressed = 0
  state heard = ""
  resource login = loginItemStatus() as shape LoginItemStatus
  resource snap = globalShortcut(id="snap", keys=chord) as shape GlobalShortcut
  resource capabilities = globalShortcutStatus() as shape GlobalShortcutStatus
  mutation registered as shape LoginItemStatus
  mutation checked as shape GlobalShortcutCheck
  action enable
    send registered = registerLoginItem(hidden=true)
  action disable
    send registered = unregisterLoginItem()
  action rebind
    chord = "Control+Alt+K"
  action check
    send checked = checkGlobalShortcut(keys="Meta+Shift+3")
  action attend
    requestAttention(critical=true)
  action press(id: string)
    pressed = pressed + 1
    heard = id
  action lift(id: string)
    lifts = lifts + 1
  action open
    focus()
  view
    column globalshortcut=press globalshortcutup=lift
      text testId="releases" `${capabilities.releaseEvents}`
      text testId="lifts" `${lifts}`
      text testId="pressed" `${pressed}`
      text testId="snap" `${snap.registered}`
      button testId="tray" -exact-menu="status" aria-label="Snap" popovertarget="traymenu"
        text "S"
      column id="traymenu" popover="auto"
        button testId="open" press=open
          text "Open Snap"
        hr
        button testId="pause" disabled=true
          text "Pause"
      button testId="new" -exact-menu="file dock" press=open
        text "New Conversation"
"#;

struct App(PathBuf);
impl App {
    fn new(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("exact-desktop-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        std::fs::create_dir_all(path.join("node_modules/@exact")).unwrap();
        for package in ["login-item", "global-shortcuts", "dock"] {
            let to = packages.join(package).canonicalize().unwrap();
            symlink_dir(to, path.join("node_modules/@exact").join(package)).unwrap();
        }
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
    fn answer(
        &mut self,
        store: &mut exact_runner::Store,
        source: &str,
        args: &[exact_runner::Value],
    ) -> Result<exact_runner::Answer, exact_runner::DataError> {
        if source == "yieldPointer" {
            return Ok(exact_runner::Answer::Later(exact_runner::Request::get(
                "https://fixture.invalid/",
            )));
        }
        if source == "chordFromData" {
            return Ok(exact_runner::Answer::Later(exact_runner::Request::native(
                br#"{"extension":"global-shortcuts","source":"globalShortcut","args":{"id":"record","keys":"Control+Alt+Meta+F"}}"#.to_vec(),
            )));
        }
        if source == "pointerFromData" && args.first() == Some(&exact_runner::Value::Bool(true)) {
            store.observe_topic("global-shortcuts.globalPointer");
            return Ok(exact_runner::Answer::Now(exact_runner::Value::record(
                vec![exact_runner::Value::Bool(false)],
            )));
        }
        if matches!(source, "pointerFromData" | "ungrantedPointer") {
            let mut request = exact_runner::Request::native(
                br#"{"extension":"global-shortcuts","source":"globalPointer","args":{}}"#.to_vec(),
            );
            if source == "ungrantedPointer" {
                request.grants = Some("device.global-shortcuts".into());
            }
            return Ok(exact_runner::Answer::Later(request));
        }
        if source == "topicOnly" {
            store.observe_topic("global-shortcuts.globalPointer");
            return Ok(exact_runner::Answer::Now(exact_runner::Value::record(
                vec![exact_runner::Value::Bool(false)],
            )));
        }
        self.query(source, args).map(exact_runner::Answer::Now)
    }
    fn parse(
        &mut self,
        store: &mut exact_runner::Store,
        source: &str,
        _: &[exact_runner::Value],
        outcome: exact_runner::Outcome,
    ) -> Result<exact_runner::Answer, exact_runner::DataError> {
        let exact_runner::Outcome::Response(response) = outcome else {
            panic!("expected a module reply")
        };
        let value: Json = serde_json::from_slice(&response.body).unwrap();
        if source == "yieldPointer" && value == "continue" {
            return Ok(exact_runner::Answer::Later(exact_runner::Request::native(
                br#"{"extension":"global-shortcuts","source":"globalPointer","args":{}}"#.to_vec(),
            )));
        }
        if source == "chordFromData" && value["id"] == "record" {
            store.observe_topic("global-shortcuts.globalShortcut");
            return Ok(exact_runner::Answer::Later(exact_runner::Request::native(
                br#"{"extension":"global-shortcuts","source":"globalShortcut","args":{"id":"other","keys":"Control+Alt+Meta+G"}}"#.to_vec(),
            )));
        }
        if source == "chordFromData" {
            store.observe_topic("global-shortcuts.globalShortcut");
            return Ok(exact_runner::Answer::Now(exact_runner::Value::record(
                vec![exact_runner::Value::Bool(value["registered"] == true)],
            )));
        }
        store.observe_topic("global-shortcuts.globalPointer");
        Ok(exact_runner::Answer::Now(exact_runner::Value::record(
            vec![exact_runner::Value::Bool(value["listening"] == true)],
        )))
    }
    fn grants(&self) -> &str {
        self.0
    }
}

fn boot(app: &App, target: &str) -> Runner<Source> {
    boot_with_grants(app, target, "device.login-item\ndevice.global-shortcuts")
}

fn boot_with_grants(app: &App, target: &str, grants: &'static str) -> Runner<Source> {
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
    settle(&mut r);
    r
}

/// Time to now, and every topic the substitutes announced asked again.
fn settle(r: &mut Runner<Source>) {
    for _ in 0..8 {
        let at = r.now_ms();
        r.advance(at).unwrap();
        if r.apply_announced().0.is_empty() {
            break;
        }
    }
}

fn logs(r: &Runner<Source>) -> String {
    agent::handle(r, r#"{"op":"logs"}"#)
}

fn state(r: &Runner<Source>) -> Json {
    serde_json::from_str(&agent::handle(r, r#"{"op":"state"}"#)).unwrap()
}

fn show(r: &Runner<Source>, name: &str) -> String {
    format!("{:?}", r.resource(name).or(r.slot(name)))
}

/// The text of the node with `test_id`.
fn text(r: &Runner<Source>, test_id: &str) -> String {
    let tree: Json = serde_json::from_str(&agent::handle(r, r#"{"op":"tree"}"#)).unwrap();
    let nodes = tree["nodes"].as_array().unwrap();
    let node = (nodes.iter())
        .find(|n| n["props"]["testId"] == test_id)
        .unwrap_or_else(|| panic!("no {test_id}: {}", logs(r)));
    let child = node["children"][0].clone();
    let text = nodes.iter().find(|n| n["id"] == child).unwrap_or(node);
    text["props"]["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn op(r: &mut Runner<Source>, request: Json) -> Result<String, String> {
    let reply = agent::answer(r, &request.to_string())
        .expect("an agent op")
        .0;
    settle(r);
    let parsed: Json = serde_json::from_str(&reply).unwrap_or_default();
    match parsed["error"].as_str() {
        Some(error) => Err(error.to_owned()),
        None => Ok(reply),
    }
}

fn act(r: &mut Runner<Source>, action: &str) {
    r.act(action, Vec::new()).unwrap();
    settle(r);
}

/// The registration's entries in `state.globalShortcuts`.
fn shortcuts(r: &Runner<Source>) -> Vec<Json> {
    let all = state(r)["globalShortcuts"].clone();
    all.as_array().cloned().unwrap_or_default()
}

/// D2: a registration is the app's, kept across its sessions, `hidden` as
/// asked; a status the platform reports as needing approval stays so; a
/// launch the login item made says so, with its `hidden`.
#[test]
fn a_login_item_registers_and_a_login_launch_says_so() {
    let app = App::new("login");
    let mut r = boot(&app, "macos");
    let login = show(&r, "login");
    assert!(login.contains("\"disabled\""), "{login}");
    act(&mut r, "enable");
    let login = show(&r, "login");
    assert!(login.contains("\"enabled\""), "{login}\n{}", logs(&r));
    assert!(
        logs(&r).contains("registerLoginItem: enabled"),
        "{}",
        logs(&r)
    );
    act(&mut r, "disable");
    assert!(
        show(&r, "login").contains("\"disabled\""),
        "{}",
        show(&r, "login")
    );

    let approval = json!({ "op": "prefer", "extension": { "login-item.loginItemStatus": { "state": "requires-approval" } } });
    r.extension_agent(&approval.to_string()).unwrap().1.unwrap();
    settle(&mut r);
    act(&mut r, "enable");
    let registered = show(&r, "registered");
    assert!(registered.contains("\"requires-approval\""), "{registered}");

    let mut again = boot(&app, "macos");
    op(
        &mut again,
        json!({ "op": "launchEntry", "launch": "login", "entry": "hidden" }),
    )
    .unwrap();
    // The status's fields in order: apple, available, hidden,
    // launchedAtLogin, state, windows.
    let login = show(&again, "login");
    let launched = "Bool(true), Bool(false), Bool(true), Str(\"disabled\")";
    assert!(login.contains(launched), "{login}");
    assert!(
        logs(&again).contains("launch --login hidden"),
        "{}",
        logs(&again)
    );
}

/// D2, D3: the changes a registration makes need a person's act.
#[test]
fn a_tool_cannot_register_a_login_item_or_check_a_chord() {
    let app = App::new("origin");
    let mut r = boot(&app, "macos");
    r.act_from(CallOrigin::Tool, "enable", Vec::new()).unwrap();
    settle(&mut r);
    assert!(
        logs(&r).contains("registerLoginItem: refused, person-required"),
        "{}",
        logs(&r)
    );
    assert!(show(&r, "login").contains("\"disabled\""));
}

/// D3: a read registers its chord and lists it; `tap @ref` presses it,
/// raising `globalshortcut` with its id; new keys list the new entry and
/// unregister the old.
#[test]
fn a_shortcut_is_listed_while_read_and_pressed_by_its_ref() {
    let app = App::new("shortcut");
    let mut r = boot(&app, "macos");
    let listed = shortcuts(&r);
    assert_eq!(listed.len(), 1, "{listed:?}\n{}", logs(&r));
    assert_eq!(
        (&listed[0]["id"], &listed[0]["keys"]),
        (&json!("snap"), &json!("Meta+Shift+2"))
    );
    assert_eq!(text(&r, "snap"), "true");
    let reference = listed[0]["ref"].as_u64().expect("a ref");
    op(&mut r, json!({ "op": "tap", "ticket": reference })).unwrap();
    assert_eq!(text(&r, "pressed"), "1", "{}", logs(&r));
    assert_eq!(shortcuts(&r).len(), 1, "a press keeps the registration");

    assert_eq!(text(&r, "releases"), "true");
    assert_eq!(text(&r, "lifts"), "1");
    op(
        &mut r,
        json!({"op":"tap","ticket":reference,"choice":"down"}),
    )
    .unwrap();
    assert_eq!(text(&r, "pressed"), "2");
    assert_eq!(text(&r, "lifts"), "1");
    op(&mut r, json!({"op":"tap","ticket":reference,"choice":"up"})).unwrap();
    assert_eq!(text(&r, "pressed"), "2");
    assert_eq!(text(&r, "lifts"), "2");
    op(
        &mut r,
        json!({"op":"tap","ticket":reference,"choice":"down"}),
    )
    .unwrap();
    act(&mut r, "rebind");
    let listed = shortcuts(&r);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["keys"], "Control+Alt+K");
    assert!(op(&mut r, json!({ "op": "tap", "ticket": reference })).is_err());
    assert!(logs(&r).contains("unregistered"), "{}", logs(&r));
    assert_eq!(text(&r, "lifts"), "3");

    let mut windows = boot(&app, "windows");
    let reference = shortcuts(&windows)[0]["ref"].as_u64().unwrap();
    assert!(op(
        &mut windows,
        json!({"op":"tap","ticket":reference,"choice":"down"})
    )
    .is_err());
    op(&mut windows, json!({"op":"tap","ticket":reference})).unwrap();
    assert_eq!(text(&windows, "pressed"), "1");
    assert_eq!(text(&windows, "lifts"), "0");
    assert_eq!(text(&windows, "releases"), "false");
}

#[test]
fn single_side_shortcuts_are_standalone_and_macos_only() {
    let app = App::new("sides");
    let path = app.0.join("app.contract");
    for keys in ["MetaRight", "AltRight"] {
        std::fs::write(
            &path,
            APP.replace("Meta+Shift+2", keys)
                .replace("Meta+Shift+3", keys),
        )
        .unwrap();
        for (target, registered) in [("macos", true), ("windows", false)] {
            let mut r = boot(&app, target);
            let entry = &shortcuts(&r)[0];
            assert_eq!(entry["registered"], registered);
            act(&mut r, "check");
            if registered {
                assert!(show(&r, "checked").contains("true"));
                let ticket = entry["ref"].as_u64().unwrap();
                op(&mut r, json!({"op":"tap", "ticket":ticket})).unwrap();
                assert_eq!(
                    (text(&r, "pressed"), text(&r, "lifts")),
                    ("1".into(), "1".into())
                );
            } else {
                assert_eq!(entry["reason"], "unsupported");
                assert!(show(&r, "checked").contains("unsupported"));
            }
        }
    }
    for keys in [
        "MetaLeft",
        "AltLeft",
        "ShiftRight",
        "ControlLeft",
        "RightMeta",
        "Shift+AltRight",
    ] {
        std::fs::write(
            &path,
            APP.replace("keys=chord", &format!("keys=\"{keys}\""))
                .replace("Meta+Shift+3", keys),
        )
        .unwrap();
        let errors = contract::compile_path_all(&path, false)
            .err()
            .expect("invalid side");
        assert_eq!(
            errors
                .iter()
                .filter(|e| e.id == "contract-module-shortcut"
                    && e.message.contains(keys)
                    && e.message.contains("MetaRight")
                    && e.message.contains("AltRight"))
                .count(),
            2
        );
        std::fs::write(&path, APP.replace("Meta+Shift+2", keys)).unwrap();
        assert_eq!(shortcuts(&boot(&app, "macos"))[0]["reason"], "invalid");
    }
}

#[test]
fn a_shortcut_keeps_the_cause_that_registered_it() {
    let app = App::new("registrar");
    let source = r#"use globalShortcut, globalshortcut, GlobalShortcut from "@exact/global-shortcuts"

component App
  state enabled = false
  state other = false
  resource shortcut = globalShortcut(id="snap", keys="Control+Alt+K") when enabled as shape GlobalShortcut
  resource sibling = globalShortcut(id="snap", keys="Control+Alt+K") when other as shape GlobalShortcut
  tool enable writes "Enable the shortcut." = enable
  action enable
    enabled = true
  action readAgain
    refresh shortcut
  action share
    other = true
  action unshare
    other = false
  action disable
    enabled = false
  action bring
    focus()
  view
    column globalshortcut=bring
      button testId="install" press=enable
        text "Install"
      text `${shortcut.registered} ${sibling.registered}`
"#;
    std::fs::write(app.0.join("app.contract"), source).unwrap();
    let manifest = json!({"app":{"id":"com.exact.registrar","name":"Registrar"}, "controls":[
        {"kind":"enable","title":"Enable","symbol":"plus","tool":"enable"}
    ]});
    std::fs::write(app.0.join("app.json"), manifest.to_string()).unwrap();
    let mut r = boot(&app, "macos");
    r.set_page(exact_runner::Page {
        hidden: true,
        ..r.page()
    })
    .unwrap();

    r.call_tool(
        "enable",
        "{}",
        exact_runner::CallHow {
            door: "agent".into(),
            ..Default::default()
        },
    )
    .unwrap();
    settle(&mut r);
    assert!(shortcuts(&r).is_empty());
    assert!(
        logs(&r).contains("globalShortcut: refused, person-required"),
        "{}",
        logs(&r)
    );

    act(&mut r, "disable");
    let digest = state(&r)["controls"][0]["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    r.press_control("enable", &digest, Default::default())
        .unwrap();
    settle(&mut r);
    assert_eq!(
        state(&r)["intents"]["calls"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["outcome"],
        "ok"
    );
    let tap = |r: &mut Runner<Source>| {
        let entries = shortcuts(r);
        assert_eq!(entries.len(), 1, "{}", logs(r));
        op(r, json!({"op":"tap", "ticket":entries[0]["ref"]})).unwrap();
    };
    let native_press = |r: &mut Runner<Source>| {
        r.raise_module_event(
            "global-shortcuts",
            "globalshortcut",
            r#"["snap",{"id":"snap","keys":"Control+Alt+K"}]"#,
            CallOrigin::App,
        )
        .unwrap();
        settle(r);
    };
    native_press(&mut r);
    let reference = shortcuts(&r)[0]["ref"].clone();
    act(&mut r, "readAgain");
    assert_eq!(shortcuts(&r)[0]["ref"], reference);
    tap(&mut r);
    assert_eq!(
        logs(&r)
            .matches("focus: refused: hidden page (background action)")
            .count(),
        2,
        "{}",
        logs(&r)
    );
    act(&mut r, "share");
    tap(&mut r);
    assert_eq!(
        logs(&r)
            .matches("focus: refused: hidden page (background action)")
            .count(),
        3
    );
    assert!(r.take_commands().iter().all(|c| c.name != "focus"));
    native_press(&mut r);
    act(&mut r, "disable");
    native_press(&mut r);
    tap(&mut r);
    assert_eq!(
        logs(&r)
            .matches("focus: refused: hidden page (background action)")
            .count(),
        6
    );
    assert!(r.take_commands().iter().all(|c| c.name != "focus"));
    act(&mut r, "unshare");
    assert!(shortcuts(&r).is_empty());
    let key = r.kernel().find_by_test_id("install")[0];
    let install = r.kernel().node_by_key(key).unwrap().id;
    r.dispatch(install, exact_runner::Event::Press).unwrap();
    settle(&mut r);
    assert_ne!(shortcuts(&r)[0]["ref"], reference);
    tap(&mut r);
    native_press(&mut r);
    assert_eq!(
        r.take_commands()
            .iter()
            .filter(|c| c.name == "focus")
            .count(),
        2
    );

    let mut bound = boot(&app, "macos");
    bound.bind_extensions(&["global-shortcuts"], &[]);
    settle(&mut bound);
    bound
        .set_page(exact_runner::Page {
            hidden: true,
            ..bound.page()
        })
        .unwrap();
    bound
        .press_control("enable", &digest, Default::default())
        .unwrap();
    settle(&mut bound);
    let sent = bound.take_requests();
    assert_eq!(sent.len(), 1, "{}", logs(&bound));
    let registration = json!({"id":"snap","keys":"Control+Alt+K","registered":true,"reason":null});
    bound
        .fulfill(
            sent[0].ticket,
            exact_runner::Outcome::Response(exact_runner::Response {
                status: 200,
                headers: Vec::new(),
                body: registration.to_string().into_bytes(),
            }),
        )
        .unwrap();
    settle(&mut bound);
    native_press(&mut bound);
    assert!(
        logs(&bound).contains("focus: refused: hidden page (background action)"),
        "{}",
        logs(&bound)
    );
    assert!(!bound.take_commands().iter().any(|cmd| cmd.name == "focus"));
    act(&mut bound, "disable");
    native_press(&mut bound);
    assert_eq!(
        bound
            .take_commands()
            .iter()
            .filter(|c| c.name == "focus")
            .count(),
        1,
        "a released registration cannot identify a native press: {}",
        logs(&bound)
    );
}

/// D3: a chord another app holds is preferred as the platform's answer:
/// the registration is listed unregistered, no press reaches it, and a
/// check answers the same reason.
#[test]
fn a_chord_in_use_is_not_registered_and_a_check_says_why() {
    let app = App::new("in-use");
    let mut r = boot(&app, "macos");
    let held = json!({ "op": "prefer", "extension": { "global-shortcuts.globalShortcut": { "registered": false, "reason": "in-use" } } });
    r.extension_agent(&held.to_string()).unwrap().1.unwrap();
    settle(&mut r);
    act(&mut r, "rebind");
    let listed = shortcuts(&r);
    assert_eq!(
        (&listed[0]["registered"], &listed[0]["reason"]),
        (&json!(false), &json!("in-use")),
        "{listed:?}"
    );
    let reference = listed[0]["ref"].as_u64().unwrap();
    let refused = op(&mut r, json!({ "op": "tap", "ticket": reference })).unwrap_err();
    assert!(refused.contains("in-use"), "{refused}");
    act(&mut r, "check");
    let checked = show(&r, "checked");
    assert!(checked.contains("\"in-use\""), "{checked}");
}

/// D14: outside macOS and Windows each module answers that it is absent,
/// and the status and Dock buttons stay ordinary buttons the agent lists.
#[test]
fn the_desktop_modules_are_absent_on_the_other_targets() {
    let app = App::new("absent");
    for target in ["web", "linux", "ios", "android"] {
        let mut r = boot(&app, target);
        assert!(shortcuts(&r).is_empty(), "{target}");
        act(&mut r, "enable");
        assert!(
            logs(&r).contains("NotSupportedError")
                || show(&r, "registered").contains("NotSupported"),
            "{target}: {}\n{}",
            show(&r, "registered"),
            logs(&r)
        );
    }
}

/// D4, D5, §11: `state.statusItems` and `state.dockMenu` from the tree, the
/// status button marked as presented elsewhere and reached by `tap`.
#[test]
fn the_agent_lists_the_status_item_and_the_dock_menu() {
    let app = App::new("presented");
    for target in ["macos", "windows"] {
        let r = boot(&app, target);
        let tree = agent::handle(&r, r#"{"op":"tree"}"#);
        assert!(
            tree.contains("\"presented\":\"status\""),
            "{target}: {tree}"
        );
    }
    let r = boot(&app, "macos");
    let s = state(&r);
    let items = s["statusItems"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{s}");
    assert_eq!(
        (&items[0]["label"], &items[0]["testId"]),
        (&json!("Snap"), &json!("tray"))
    );
    let menu = items[0]["menu"].as_array().unwrap();
    let tree = agent::handle(&r, r#"{"op":"tree"}"#);
    assert_eq!(menu.len(), 3, "{menu:?}\n{tree}");
    assert_eq!(
        (&menu[0]["label"], &menu[1], &menu[2]["disabled"]),
        (&json!("Open Snap"), &json!("—"), &json!(true))
    );
    let dock = s["dockMenu"].as_array().unwrap();
    assert_eq!(dock.len(), 1);
    assert_eq!(dock[0]["label"], "New Conversation");
    assert!(tree.contains("\"presented\":\"status\""), "{tree}");
    // Where no host places it, the button is ordinary; the list stays.
    let r = boot(&app, "web");
    let tree = agent::handle(&r, r#"{"op":"tree"}"#);
    assert!(!tree.contains("\"presented\""), "{tree}");
    let items = state(&r)["statusItems"].as_array().map(Vec::len);
    assert_eq!(items, Some(1));
}

/// D5: a request for attention is the agent's to read, critical or not.
#[test]
fn a_request_for_attention_is_kept_for_the_agent() {
    let app = App::new("attention");
    let mut r = boot(&app, "macos");
    act(&mut r, "attend");
    let s = state(&r);
    let attention = s.to_string();
    assert!(
        attention.contains("\"attention\":true"),
        "{attention}\n{}",
        logs(&r)
    );
}

const RECORDER: &str = r#"use globalPointer, globalpointerdown, globalpointerup, GlobalPointer, GlobalPointerEvent, globalShortcut, GlobalShortcut, globalshortcut, globalshortcutup from "@exact/global-shortcuts"

component App
  state recording = false
  state attempted = false
  state mounted = false
  state edges = 0
  state x = 0
  state y = 0
  resource chord = globalShortcut(id="record", keys="Control+Alt+Meta+F") as shape GlobalShortcut
  resource listener = globalPointer() when recording as shape GlobalPointer
  resource another = globalPointer() when attempted as shape GlobalPointer
  tool record writes "Start recording." = start
  action start
    recording = true
  action stop
    recording = false
    attempted = false
  action refreshListener
    refresh listener
  action toggle
    recording = !recording
  action extra
    attempted = true
  action mount
    mounted = true
  action unmount
    mounted = false
  action edge(e: GlobalPointerEvent)
    edges = edges + 1
    x = e.screenX
    y = e.screenY
    attempted = true
  view
    column globalshortcut=toggle globalshortcutup=extra globalpointerdown=edge globalpointerup=edge
      button testId="start" press=start
        text "Start"
      button testId="mount" press=mount
        text "Mount"
      if mounted
        Recorder()
      text testId="chord" `${chord.registered}`
      text testId="listening" `${listener.listening}`
      text testId="count" `${edges}`
      text testId="point" `${x}, ${y}`

component Recorder
  resource owned = globalPointer() as shape GlobalPointer
  view
    text testId="owned" `${owned.listening}`
"#;

const BOTH: &str = "device.global-pointer\ndevice.global-shortcuts";

fn recording_app() -> App {
    let app = App::new("recorder");
    std::fs::write(app.0.join("app.contract"), RECORDER).unwrap();
    app
}

fn press(r: &mut Runner<Source>, id: &str) {
    let key = r.kernel().find_by_test_id(id)[0];
    let button = r.kernel().node_by_key(key).unwrap().id;
    r.dispatch(button, exact_runner::Event::Press).unwrap();
    settle(r);
}

fn pointer_edge(r: &mut Runner<Source>) {
    let detail = json!({"screenX":-120.5,"screenY":40,"button":1,"timeStamp":483.2,
        "ctrlKey":false,"altKey":false,"metaKey":true,"shiftKey":false});
    r.extension_agent(
        &json!({"op":"prefer","extension":{"global-shortcuts.globalpointerdown":detail}})
            .to_string(),
    )
    .unwrap()
    .1
    .unwrap();
    settle(r);
}

fn shortcut_edge(r: &mut Runner<Source>, event: &str) {
    r.raise_module_event(
        "global-shortcuts",
        event,
        r#"["record",{"id":"record","keys":"Control+Alt+Meta+F"}]"#,
        CallOrigin::Device,
    )
    .unwrap();
    settle(r);
}

#[test]
fn only_input_and_the_registered_shortcut_press_open_the_recorder() {
    let app = recording_app();
    std::fs::write(
        app.0.join("app.contract"),
        RECORDER.replace("state recording = false", "state recording = true"),
    )
    .unwrap();
    let mut launched = boot_with_grants(&app, "macos", BOTH);
    pointer_edge(&mut launched);
    assert!(logs(&launched).contains("globalPointer: refused, person-required (app origin)"));
    assert_eq!(text(&launched, "count"), "0");
    std::fs::write(app.0.join("app.contract"), RECORDER).unwrap();
    let mut r = boot_with_grants(&app, "macos", BOTH);
    for origin in [CallOrigin::App, CallOrigin::Wake, CallOrigin::Device] {
        r.act_from(origin, "start", vec![]).unwrap();
        settle(&mut r);
        assert!(
            logs(&r).contains(&format!(
                "globalPointer: refused, person-required ({} origin)",
                origin.word()
            )),
            "{}",
            logs(&r)
        );
        act(&mut r, "stop");
    }
    r.call_tool(
        "record",
        "{}",
        exact_runner::CallHow {
            door: "agent".into(),
            ..Default::default()
        },
    )
    .unwrap();
    settle(&mut r);
    assert!(logs(&r).contains("globalPointer: refused, person-required (tool origin)"));
    act(&mut r, "stop");
    r.set_page(exact_runner::Page {
        hidden: true,
        ..r.page()
    })
    .unwrap();
    shortcut_edge(&mut r, "globalshortcutup");
    pointer_edge(&mut r);
    assert_eq!(text(&r, "count"), "0");
    act(&mut r, "stop");
    r.raise_module_event(
        "global-shortcuts",
        "globalshortcut",
        r#"["other",{"id":"other","keys":"F12"}]"#,
        CallOrigin::Device,
    )
    .unwrap();
    settle(&mut r);
    pointer_edge(&mut r);
    assert_eq!(text(&r, "count"), "0");
    act(&mut r, "stop");
    let reference = shortcuts(&r)[0]["ref"].clone();
    op(
        &mut r,
        json!({"op":"tap","ticket":reference,"choice":"down"}),
    )
    .unwrap();
    assert_eq!(text(&r, "listening"), "true", "{}", logs(&r));
    pointer_edge(&mut r);
    assert_eq!(text(&r, "count"), "1");
    assert!(
        logs(&r).contains("another failed: global-shortcuts.globalPointer: NotAllowedError"),
        "{}",
        logs(&r)
    );
    r.act_from(CallOrigin::Device, "refreshListener", vec![])
        .unwrap();
    settle(&mut r);
    pointer_edge(&mut r);
    assert_eq!(text(&r, "count"), "2");
    assert_eq!(text(&r, "point"), "-120.5, 40");
    assert_eq!(text(&r, "listening"), "true");
    op(&mut r, json!({"op":"tap","ticket":reference,"choice":"up"})).unwrap();
    op(
        &mut r,
        json!({"op":"tap","ticket":reference,"choice":"down"}),
    )
    .unwrap();
    pointer_edge(&mut r);
    assert_eq!(
        text(&r, "count"),
        "2",
        "a refused reader cannot keep observation alive"
    );
}

#[test]
fn the_last_live_recorder_read_owns_observation() {
    let app = recording_app();
    // A separate data resource can watch the same topic without reading the device.
    std::fs::write(
        app.0.join("app.contract"),
        RECORDER.replace(
            "  resource chord =",
            "  resource passive = topicOnly() as shape GlobalPointer\n  resource chord =",
        ),
    )
    .unwrap();
    let mut a = boot_with_grants(&app, "macos", BOTH);
    let mut b = boot_with_grants(&app, "macos", BOTH);
    for r in [&mut a, &mut b] {
        press(r, "start");
        pointer_edge(r);
        assert_eq!(text(r, "count"), "1", "{}", logs(r));
    }
    press(&mut a, "mount");
    assert_eq!(text(&a, "owned"), "true", "{}", logs(&a));
    act(&mut a, "stop");
    pointer_edge(&mut a);
    assert_eq!(text(&a, "count"), "2");
    act(&mut a, "unmount");
    pointer_edge(&mut a);
    assert_eq!(text(&a, "count"), "2", "{}", logs(&a));
    drop(a);
    pointer_edge(&mut b);
    assert_eq!(text(&b, "count"), "2");
    act(&mut b, "stop");
    pointer_edge(&mut b);
    assert_eq!(text(&b, "count"), "2");
}

#[test]
fn recording_needs_its_own_grant_and_a_supported_host() {
    let app = recording_app();
    for (host, grants, why) in [
        (
            "macos",
            "device.global-shortcuts",
            "grant-missing (device.global-pointer)",
        ),
        ("windows", BOTH, "globalPointer: NotSupportedError"),
    ] {
        let mut r = boot_with_grants(&app, host, grants);
        press(&mut r, "start");
        assert!(logs(&r).contains(why), "{}", logs(&r));
        pointer_edge(&mut r);
        assert_eq!(text(&r, "count"), "0");
    }
}

#[test]
fn another_packages_button_event_still_reaches_its_handler() {
    let app = recording_app();
    let packages = app.0.join("node_modules");
    let fixture = packages.join("@fixture/global-shortcuts");
    std::fs::create_dir_all(&fixture).unwrap();
    std::fs::write(
        fixture.join("package.json"),
        json!({
            "name":"@fixture/global-shortcuts","version":"1.0.0","exports":{".":"./exact.json"}
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        fixture.join("exact.json"),
        json!({
            "kind":"interface","name":"@fixture/global-shortcuts","version":"1.0.0",
            "names":[{"event":"globalpointerdown","on":"root","raised":"emit","args":[],"hold":1}, {"event":"globalshortcut","on":"root","raised":"emit","args":["string","string","number"],"hold":1}]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        app.0.join("app.contract"),
        r#"use globalpointerdown, globalshortcut as fixturePress from "@fixture/global-shortcuts"
use globalPointer, GlobalPointer, globalshortcut as exactPress from "@exact/global-shortcuts"
component App
  state edges = 0
  resource listener = globalPointer() when false as shape GlobalPointer
  action fixture(id: string, label: string, n: number)
    edges = n
  action edge
    edges = edges + 1
  view
    column globalpointerdown=edge fixturePress=fixture exactPress=edge
      text testId="count" `${edges}`
"#,
    )
    .unwrap();
    let mut r = boot(&app, "macos");
    r.raise_module_event(
        "global-shortcuts",
        "globalpointerdown",
        "[]",
        CallOrigin::Device,
    )
    .unwrap();
    settle(&mut r);
    assert_eq!(text(&r, "count"), "1");
    r.raise_module_event(
        "global-shortcuts",
        "globalshortcut",
        r#"["fixture","event",17]"#,
        CallOrigin::Device,
    )
    .unwrap();
    settle(&mut r);
    assert_eq!(text(&r, "count"), "17");
}

fn shared_chord_app(app: &App) {
    let fixture = RECORDER.replace("  state recording", "  state shared = false\n  state recording")
        .replace("  tool record", r#"  resource sharedChord = globalShortcut(id="record", keys="Control+Alt+Meta+F") when shared as shape GlobalShortcut
  tool share writes "Share the chord." = share
  action share
    shared = true
  tool record"#);
    let fixture = fixture.replace("  state shared", "  state installed = true\n  state shared")
        .replacen(r#"keys="Control+Alt+Meta+F") as shape GlobalShortcut"#, r#"keys="Control+Alt+Meta+F") when installed as shape GlobalShortcut"#, 1)
        .replace("  action mount", "  action retire\n    installed = false\n    shared = false\n    recording = false\n  action install\n    installed = true\n  action mount")
        .replace(r#"      button testId="mount""#, "      button testId=\"install\" press=install\n        text \"Install\"\n      button testId=\"mount\"");
    let fixture = fixture.replace(
        "  action retire",
        "  action unshare\n    recording = false\n    shared = false\n  action retire",
    );
    std::fs::write(app.0.join("app.contract"), fixture).unwrap();
    let manifest = json!({"app":{"id":"com.exact.recorder","name":"Recorder"}, "controls":[
        {"kind":"share","title":"Share","symbol":"plus","tool":"share"}
    ]});
    std::fs::write(app.0.join("app.json"), manifest.to_string()).unwrap();
}

#[test]
fn a_background_control_cannot_install_an_opener_beside_the_apps_chord() {
    let app = recording_app();
    shared_chord_app(&app);
    let mut r = boot_with_grants(&app, "macos", BOTH);
    r.set_page(exact_runner::Page {
        hidden: true,
        ..r.page()
    })
    .unwrap();
    let digest = state(&r)["controls"][0]["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    r.press_control("share", &digest, Default::default())
        .unwrap();
    settle(&mut r);
    let entry = shortcuts(&r)[0]["ref"].clone();
    op(&mut r, json!({"op":"tap", "ticket":entry,"choice":"down"})).unwrap();
    settle(&mut r);
    pointer_edge(&mut r);
    assert_eq!(text(&r, "count"), "0", "{}", logs(&r));
    assert!(logs(&r).contains("listener failed: global-shortcuts.globalPointer: NotAllowedError"));
    op(&mut r, json!({"op":"tap","ticket":entry,"choice":"up"})).unwrap();
    act(&mut r, "unshare");
    op(&mut r, json!({"op":"tap","ticket":entry,"choice":"down"})).unwrap();
    assert_eq!(text(&r, "listening"), "true", "{}", logs(&r));
    act(&mut r, "retire");
    assert!(shortcuts(&r).is_empty());
    r.set_page(exact_runner::Page {
        hidden: false,
        ..r.page()
    })
    .unwrap();
    press(&mut r, "install");
    r.set_page(exact_runner::Page {
        hidden: true,
        ..r.page()
    })
    .unwrap();
    let current = shortcuts(&r)[0]["ref"].clone();
    assert_ne!(entry, current);
    op(&mut r, json!({"op":"tap","ticket":current,"choice":"down"})).unwrap();
    assert_eq!(text(&r, "listening"), "true", "{}", logs(&r));
}

#[test]
fn a_rolled_back_start_leaves_no_substitute_listening() {
    let app = recording_app();
    let source = RECORDER.replacen(
        "  action toggle",
        "  task reject key=recording ? 0 / 0 : 1\n    after(100000, stop)\n  action toggle",
        1,
    );
    std::fs::write(app.0.join("app.contract"), source).unwrap();
    let mut r = boot_with_grants(&app, "macos", BOTH);
    let key = r.kernel().find_by_test_id("start")[0];
    let button = r.kernel().node_by_key(key).unwrap().id;
    assert!(r.dispatch(button, exact_runner::Event::Press).is_err());
    pointer_edge(&mut r);
    assert_eq!(text(&r, "count"), "0");
}

#[test]
fn a_data_resource_owns_only_the_pointer_read_it_was_admitted_to() {
    use exact_runner::{Dispatch, Work};
    let app = recording_app();
    for (source, origin, listens) in [
        ("pointerFromData", None, true),
        ("pointerFromData", Some(CallOrigin::Wake), false),
        ("ungrantedPointer", None, false),
    ] {
        let wrapped = RECORDER.replace(
            "listener = globalPointer()",
            &format!("listener = {source}(attempted)"),
        );
        std::fs::write(app.0.join("app.contract"), wrapped).unwrap();
        let mut r = boot_with_grants(&app, "macos", BOTH);
        if let Some(origin) = origin {
            r.act_from(origin, "start", vec![]).unwrap();
            settle(&mut r);
        } else {
            press(&mut r, "start");
        }
        let ask = r.take_requests().pop().unwrap();
        let Dispatch::Run(Work::Now(work)) = r.native_work(&ask.request) else {
            panic!("expected a substituted read")
        };
        r.fulfill(ask.ticket, work()).unwrap();
        settle(&mut r);
        pointer_edge(&mut r);
        let count = if listens { "1" } else { "0" };
        assert_eq!(
            text(&r, "count"),
            count,
            "{source} {origin:?}: {}",
            logs(&r)
        );
        if listens {
            assert_eq!(
                text(&r, "listening"),
                "false",
                "the data answer was overwritten"
            );
            pointer_edge(&mut r);
            assert_eq!(
                text(&r, "count"),
                count,
                "watching the old topic cannot extend the read"
            );
        }
        act(&mut r, "stop");
        pointer_edge(&mut r);
        assert_eq!(text(&r, "count"), count);
    }
}

#[path = "recorder.rs"]
mod recorder;
