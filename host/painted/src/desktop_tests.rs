//! LLP 1133.025's Windows implementations (`packages/*/windows/lib.rs`)
//! on this loop, over a fake of the host's services: a chord registered,
//! refused and released by its reader, a check that keeps nothing, the
//! login item set and read with the launch, attention asked, and a press
//! the host heard raised on the next turn. Compiled here, on every host,
//! because no app in the tree composes all three for Windows.
use crate::desktop::{Login, Refusal, Services};
use crate::package::{json, Entry, Json};
use crate::presenter::PainterChoice;
use crate::Presenter;
use exact_runner::DataSource;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[path = "../../../packages/dock/windows/lib.rs"]
mod dock;
#[path = "../../../packages/global-shortcuts/windows/lib.rs"]
mod global_shortcuts;
#[path = "../../../packages/login-item/windows/lib.rs"]
mod login_item;
#[path = "../../../packages/window-management/linux/lib.rs"]
mod window_management;

/// A session label and the callback `watch_reserved` was given for it.
type Watcher = (String, Box<dyn Fn() + Send>);

#[derive(Default)]
struct Fake {
    said: RefCell<Vec<String>>,
    login: RefCell<Option<bool>>,
    /// Whether the system has given bare Print Screen back.
    print_screen: Cell<bool>,
    watchers: RefCell<Vec<Watcher>>,
}

impl Services for Fake {
    fn register_hotkey(
        &self,
        session: &str,
        id: &str,
        keys: &str,
        repeat: bool,
        extension: &'static str,
        event: &'static str,
    ) -> Result<(), Refusal> {
        self.said.borrow_mut().push(format!(
            "register {session} {id} {keys} repeat={repeat} {extension}.{event}"
        ));
        if keys == "Control+Alt+Delete" || (keys == "PrintScreen" && !self.print_screen.get()) {
            Err("reserved")
        } else {
            Ok(())
        }
    }
    fn unregister_hotkey(&self, session: &str, id: &str, keys: &str) {
        self.said
            .borrow_mut()
            .push(format!("unregister {session} {id} {keys}"));
    }
    fn check_hotkey(&self, keys: &str) -> Result<(), Refusal> {
        self.said.borrow_mut().push(format!("check {keys}"));
        Err("in-use")
    }
    fn attention(&self, critical: bool, count: Option<u32>) -> Result<(), String> {
        self.said
            .borrow_mut()
            .push(format!("attention {critical} {count:?}"));
        Err("the window is in front".into())
    }
    fn login(&self) -> Login {
        match *self.login.borrow() {
            Some(hidden) => Login {
                state: "enabled",
                hidden,
            },
            None => Login {
                state: "disabled",
                hidden: false,
            },
        }
    }
    fn set_login(&self, register: Option<bool>) -> Result<Login, String> {
        *self.login.borrow_mut() = register;
        Ok(self.login())
    }
    fn open_login_settings(&self) -> Result<(), String> {
        self.said.borrow_mut().push("settings".into());
        Ok(())
    }
    fn login_launch(&self) -> Option<bool> {
        Some(true)
    }
    fn watch_reserved(&self, session: &str, changed: Box<dyn Fn() + Send>) {
        self.said.borrow_mut().push(format!("watch {session}"));
        self.watchers.borrow_mut().push((session.into(), changed));
    }
}

#[derive(Default)]
struct Nothing;
impl DataSource for Nothing {
    fn query(
        &mut self,
        name: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(name.into()))
    }
}

fn call(p: &mut Presenter<Nothing>, body: Json) -> Json {
    let (tx, rx) = std::sync::mpsc::channel();
    let reply = exact_runner::Reply::new(move |outcome| {
        if let exact_runner::Outcome::Response(r) = outcome {
            let _ = tx.send(serde_json::from_slice(&r.body).unwrap_or_default());
        }
    });
    let handler = p
        .host()
        .runner()
        .native_slot()
        .handler()
        .expect("a handler");
    handler(body.to_string().into_bytes(), reply);
    p.run_commands(Nothing::default);
    rx.try_recv().unwrap_or_default()
}

#[test]
fn the_windows_implementations_use_the_hosts_services() {
    let fake = Rc::new(Fake::default());
    crate::desktop::install(fake.clone());
    let plan = crate::test_plan("component App\n  view\n    text \"Hi\"\n");
    let (mut p, _) = Presenter::boot_with(
        &plan,
        Nothing,
        (400., 800.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    p.serve_packages(vec![
        Entry::of::<global_shortcuts::GlobalShortcuts>("global-shortcuts"),
        Entry::of::<login_item::LoginItem>("login-item"),
        Entry::of::<dock::Dock>("dock"),
    ]);

    let read = |keys: &str| json!({"extension": "global-shortcuts", "source": "globalShortcut", "args": {"id": "snap", "keys": keys}});
    let held = call(&mut p, read("Meta+Shift+2"));
    assert_eq!(
        (&held["registered"], &held["id"]),
        (&json!(true), &json!("snap")),
        "{held}"
    );
    let refused = call(&mut p, read("Control+Alt+Delete"));
    assert_eq!(
        (&refused["registered"], &refused["reason"]),
        (&json!(false), &json!("reserved"))
    );
    let checked = call(
        &mut p,
        json!({"extension": "global-shortcuts", "send": "checkGlobalShortcut", "args": {"keys": "Meta+K"}}),
    );
    assert_eq!(checked["reason"], "in-use");
    call(
        &mut p,
        json!({"extension": "global-shortcuts", "release": "globalShortcut", "args": {"id": "snap", "keys": "Meta+Shift+2"}}),
    );
    // `windows.repeat` reaches the host; omitted, it is false.
    for repeat in [json!(true), json!(false)] {
        let args = json!({"id": "volume", "keys": "Control+K", "windows": {"repeat": repeat}});
        let read =
            json!({"extension": "global-shortcuts", "source": "globalShortcut", "args": args});
        assert_eq!(call(&mut p, read)["registered"], true);
    }

    let status = call(
        &mut p,
        json!({"extension": "login-item", "source": "loginItemStatus", "args": {}}),
    );
    assert_eq!(
        (&status["state"], &status["launchedAtLogin"]),
        (&json!("disabled"), &json!(true)),
        "{status}"
    );
    let on = call(
        &mut p,
        json!({"extension": "login-item", "send": "registerLoginItem", "args": {"hidden": true}}),
    );
    assert_eq!(
        (&on["state"], &on["hidden"]),
        (&json!("enabled"), &json!(true)),
        "{on}"
    );
    assert!(
        on["windows"].is_object(),
        "a Windows status says it is Windows's"
    );
    let off = call(
        &mut p,
        json!({"extension": "login-item", "send": "unregisterLoginItem", "args": {}}),
    );
    assert_eq!(off["state"], "disabled");
    call(
        &mut p,
        json!({"extension": "login-item", "send": "openLoginItemSettings", "args": {}}),
    );
    let attended = call(
        &mut p,
        json!({"extension": "dock", "send": "requestAttention", "args": {"critical": true}}),
    );
    assert_eq!(attended, json!({}));
    let attend =
        |args: Json| json!({"extension": "dock", "send": "requestAttention", "args": args});
    for args in [
        json!({"windows": {"count": 1}}),
        json!({"windows": {"count": 7.0}}),
        json!({"critical": true, "windows": {"count": 7}}),
    ] {
        assert_eq!(call(&mut p, attend(args)), json!({}));
    }
    for count in [json!(0), json!(2.5), json!(-1), json!("3")] {
        let refused = call(&mut p, attend(json!({"windows": {"count": count}})));
        assert_eq!(refused["error"], "TypeError", "{count}: {refused}");
    }

    assert_eq!(
        *fake.said.borrow(),
        [
            "register 1 snap Meta+Shift+2 repeat=false global-shortcuts.globalshortcut",
            "register 1 snap Control+Alt+Delete repeat=false global-shortcuts.globalshortcut",
            "check Meta+K",
            "unregister 1 snap Meta+Shift+2",
            "register 1 volume Control+K repeat=true global-shortcuts.globalshortcut",
            "register 1 volume Control+K repeat=false global-shortcuts.globalshortcut",
            "settings",
            "attention true None",
            "attention false Some(1)",
            "attention false Some(7)",
            "attention true Some(7)",
        ]
    );

    // A press the host heard is raised on the next turn, as a person's act.
    crate::desktop::activate(
        "global-shortcuts",
        "globalshortcut",
        json!(["snap", {"id": "snap", "keys": "Meta+Shift+2"}]).to_string(),
    );
    p.run_commands(Nothing::default);
    let journal = p.host().runner().journal().collect::<Vec<_>>().join("\n");
    assert!(
        journal.contains("global-shortcuts.globalshortcut: no handler hears it"),
        "{journal}"
    );
    assert!(
        journal.contains("requestAttention: the window is in front"),
        "{journal}"
    );
}

/// #503: a reader of bare Print Screen has the host watch for Snipping
/// Tool's setting, once per session, and a reading asked again after the
/// system took the chord back is no longer one the session releases.
#[test]
fn a_print_screen_reader_is_asked_again_as_the_system_takes_it() {
    let fake = Rc::new(Fake::default());
    crate::desktop::install(fake.clone());
    let plan = crate::test_plan("component App\n  view\n    text \"Hi\"\n");
    let (mut p, _) = Presenter::boot_with(
        &plan,
        Nothing,
        (400., 800.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    p.serve_packages(vec![Entry::of::<global_shortcuts::GlobalShortcuts>(
        "global-shortcuts",
    )]);
    let read = |keys: &str| json!({"extension": "global-shortcuts", "source": "globalShortcut", "args": {"id": "snap", "keys": keys}});
    let release = json!({"extension": "global-shortcuts", "release": "globalShortcut", "args": {"id": "snap", "keys": "PrintScreen"}});

    assert_eq!(call(&mut p, read("PrintScreen"))["reason"], "reserved");
    fake.print_screen.set(true);
    // The host's watcher asks the session's readings again.
    for (_, changed) in fake.watchers.borrow().iter() {
        changed();
    }
    assert_eq!(call(&mut p, read("PrintScreen"))["registered"], true);
    fake.print_screen.set(false);
    assert_eq!(call(&mut p, read("PrintScreen"))["reason"], "reserved");
    call(&mut p, release.clone());
    fake.print_screen.set(true);
    assert_eq!(call(&mut p, read("PrintScreen"))["registered"], true);
    call(&mut p, release);
    assert_eq!(
        *fake.said.borrow(),
        [
            "watch 1",
            "register 1 snap PrintScreen repeat=false global-shortcuts.globalshortcut",
            "register 1 snap PrintScreen repeat=false global-shortcuts.globalshortcut",
            "register 1 snap PrintScreen repeat=false global-shortcuts.globalshortcut",
            "register 1 snap PrintScreen repeat=false global-shortcuts.globalshortcut",
            "unregister 1 snap PrintScreen",
        ]
    );
}

/// LLP 1178 D6: a host that fills its one display reports the presenter's
/// viewport and scale as that display, and calls its watchers when either
/// changes, not when the pointer moves.
#[test]
fn a_full_screen_display_is_the_viewport_and_tells_its_watchers_of_a_resize() {
    use crate::desktop::{surface, Displays, FullScreen};
    let screen = FullScreen {
        label: "eDP-1".into(),
        internal: true,
    };
    surface((800., 600.), 2., None);
    let told = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = told.clone();
    screen.watch_displays(
        "1",
        Box::new(move || {
            counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }),
    );
    let s = screen.screens();
    assert_eq!(
        (s.screens[0].rect, s.screens[0].scale),
        ([0., 0., 800., 600.], 2.)
    );
    assert!(s.screens[0].internal && s.screens[0].primary);
    assert!(screen.pointer().is_none(), "no pointer drawn yet");
    surface((800., 600.), 2., Some((10., 20.)));
    assert_eq!(screen.pointer().map(|p| p.client), Some(Some([10., 20.])));
    assert_eq!(told.load(std::sync::atomic::Ordering::Relaxed), 0);
    surface((1024., 600.), 2., Some((10., 20.)));
    assert_eq!(told.load(std::sync::atomic::Ordering::Relaxed), 1);
    screen.unwatch_displays("1");
    surface((800., 600.), 2., None);
    assert_eq!(told.load(std::sync::atomic::Ordering::Relaxed), 1);
}
