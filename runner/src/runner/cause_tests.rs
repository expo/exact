//! LLP 1133.023 D7a: what the host observes of the page and a drag handle's
//! release are the app's, and a world's message under a tool's mark and a
//! remote Play keep D4a's command rules.

use crate::request::Request;
use crate::{Answer, CallOrigin, DataError, DataSource, Event, Outcome, Runner, Store, Value};
use exact_kernel::Kernel;

/// Every send answers later.
struct Later;

impl DataSource for Later {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("answers through answer")
    }

    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(Request::native(b"{}".to_vec())))
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let value = match outcome {
            Outcome::Response(reply) => serde_json::from_slice(&reply.body).unwrap_or(0.0),
            _ => 0.0,
        };
        Ok(Answer::Now(Value::Number(value)))
    }
}

const FRAME: &str = "component App\n  mutation m as shape number\n  tool ping writes \"Ping.\" = loaded\n  action loaded\n    send m = save(1)\n  action heard(text: string)\n    focus(\"x\")\n  view\n    iframe \"/deck/index.html\" load=loaded message=heard testId=\"deck\"\n";

fn boot() -> (Runner<Later>, exact_kernel::ViewId) {
    boot_with(FRAME, "deck")
}

fn boot_with(source: &str, test_id: &str) -> (Runner<Later>, exact_kernel::ViewId) {
    let plan = contract::compile(source).unwrap();
    let r = Runner::boot(
        plan,
        Later,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let key = r.kernel().find_by_test_id(test_id)[0];
    let id = r.kernel().node_by_key(key).unwrap().id;
    (r, id)
}

/// A camera preview's `load` may start the camera.
#[test]
fn a_load_the_host_observes_is_the_apps() {
    let (mut r, frame) = boot();
    r.take_requests();
    r.dispatch(frame, Event::Load).unwrap();
    let sent = r.take_requests();
    assert_eq!(sent[0].request.reach.origin, CallOrigin::App);
}

/// A sheet's or a photo's drag handle reports the geometry it reads and the
/// release of the person's drag.
#[test]
fn a_drag_handles_release_is_the_persons() {
    const HANDLES: &str = "component App\n  mutation m as shape number\n  action geometry(bw: number, bh: number, pw: number, ph: number)\n    send m = save(1)\n  action release(x: number, y: number, scale: number, vx: number, vy: number, vs: number)\n    send m = save(2)\n  action snap(height: number, velocity: number)\n    send m = save(3)\n  view\n    column\n      column id=\"sheet\" height=360 box-sizing=\"border-box\"\n        column testId=\"sheet-handle\" heightDragFor=\"sheet\" heightrelease=snap\n      column overflow=\"hidden\"\n        column id=\"photo\" width=\"100%\" height=\"100%\" box-sizing=\"border-box\"\n          column testId=\"photo-handle\" transformDragFor=\"photo\" transformgeometry=geometry transformrelease=release\n";
    let origin = |id: &str, event: Event| {
        let (mut r, handle) = boot_with(HANDLES, id);
        r.take_requests();
        r.dispatch(handle, event).unwrap();
        r.take_requests()[0].request.reach.origin
    };
    let snap = Event::HeightRelease {
        height: 180.0,
        velocity: 0.0,
    };
    assert_eq!(origin("sheet-handle", snap), CallOrigin::App);
    let release = Event::TransformRelease {
        x: 0.,
        y: 0.,
        scale: 1.,
        vx: 0.,
        vy: 0.,
        vscale: 0.,
    };
    assert_eq!(origin("photo-handle", release), CallOrigin::App);
    let geometry = Event::TransformGeometry {
        box_width: 100.,
        box_height: 100.,
        port_width: 100.,
        port_height: 100.,
    };
    assert_eq!(origin("photo-handle", geometry), CallOrigin::App);
}

#[test]
fn a_worlds_message_under_a_tools_mark_keeps_the_calls_command_rules() {
    // In an app with no tool too: the filter is the core's, not the tool seam's.
    let toolless = FRAME.replace("  tool ping writes \"Ping.\" = loaded\n", "");
    for source in [FRAME, &toolless] {
        let (mut r, frame) = boot_with(source, "deck");
        r.dispatch(frame, Event::Message("\u{1}origin:tool\u{1}hi".into()))
            .unwrap();
        let lines: Vec<String> = r.journal().map(str::to_owned).collect();
        assert!(
            lines
                .iter()
                .any(|l| l.contains("focus: skipped in a tool call")),
            "{lines:?}"
        );
        // The app's own message runs it.
        r.dispatch(frame, Event::Message("hi".into())).unwrap();
        let skipped = r.journal().filter(|l| l.contains("focus: skipped")).count();
        assert_eq!(skipped, 1);
    }
}

#[test]
fn a_window_focus_carries_the_reach_of_its_issuing_work() {
    let (mut r, _) = boot_with(
        "component App\n  state armed = false\n  action arm\n    armed = true\n  task later when armed\n    after(5, bring)\n  action bring\n    focus()\n    focus(\"x\")\n  view\n    text \"here\" testId=\"x\"\n",
        "x",
    );
    r.act("arm", vec![]).unwrap();
    r.act_from(CallOrigin::Device, "bring", vec![]).unwrap();
    r.advance(r.now_ms() + 5.).unwrap();
    r.act_named("bring", vec![]).unwrap();
    let origins: Vec<_> = r
        .take_commands()
        .into_iter()
        .map(|cmd| cmd.window_reach.map(|reach| reach.origin))
        .collect();
    assert_eq!(
        origins,
        [
            Some(CallOrigin::Device),
            None,
            Some(CallOrigin::App),
            None,
            Some(CallOrigin::Device),
            None
        ]
    );
}

fn hide(r: &mut Runner<Later>) {
    r.set_page(crate::Page {
        hidden: true,
        ..r.page()
    })
    .unwrap();
}

fn call(r: &mut Runner<Later>, tool: &str) {
    r.call_tool(
        tool,
        "{}",
        crate::CallHow {
            door: "agent".into(),
            ..Default::default()
        },
    )
    .unwrap();
    r.advance(r.now_ms()).unwrap();
}

fn hidden_refusals(r: &Runner<Later>, reason: &str) -> usize {
    let line = format!("focus: refused: hidden page ({reason})");
    r.journal().filter(|l| l.contains(&line)).count()
}

#[test]
fn only_a_tool_or_background_action_contributes_a_hidden_show_refusal() {
    use super::Cause;
    let (mut r, _) = boot_with(
        "component App\n  action bring\n    focus()\n    focus(\"x\")\n    relaunch()\n    reload()\n    reload(\"x\")\n    openDevTools(\"x\")\n  view\n    iframe \"/deck/index.html\" id=\"x\" testId=\"x\"\n", "x");
    let joined = r.join_causes(Cause::device(), Cause::model_tool());
    // A model handler has a tool contributor without D4a's call flag.
    let bring = |r: &mut Runner<Later>, cause| {
        r.as_cause(cause, |r| r.act_named("bring", vec![])).unwrap();
        r.take_commands()
            .into_iter()
            .map(|c| (c.name, c.args.len()))
            .collect::<Vec<_>>()
    };
    let all = vec![
        ("focus".into(), 0),
        ("focus".into(), 1),
        ("relaunch".into(), 0),
        ("reload".into(), 0),
        ("reload".into(), 2),
        ("openDevTools".into(), 1),
    ];
    assert_eq!(bring(&mut r, joined), all, "the page shows");
    hide(&mut r);
    for cause in [Cause::device(), Cause::person()] {
        assert_eq!(bring(&mut r, cause), all);
    }
    r.act_named("bring", vec![]).unwrap();
    assert_eq!(r.take_commands().len(), all.len(), "no cause installed");
    for cause in [Cause::remote_control(), Cause::control(1, true)] {
        assert!(
            bring(&mut r, cause).is_empty(),
            "D4a still skips the press's focus"
        );
        assert_eq!(hidden_refusals(&r, "background action"), 0);
    }
    for (cause, reason) in [
        (joined, "tool call"),
        (Cause::background_action(0), "background action"),
    ] {
        assert_eq!(
            bring(&mut r, cause),
            [("focus".into(), 1), ("reload".into(), 2)]
        );
        assert_eq!(hidden_refusals(&r, reason), 1);
        assert!(r
            .journal()
            .any(|l| l.contains(&format!("relaunch: refused: hidden page ({reason})"))));
    }
}

#[test]
fn a_cold_tool_call_in_a_hidden_session_issues_no_window_command() {
    let (mut r, _) = boot_with(
        "component App\n  tool show writes \"Show.\" = bring\n  action bring\n    focus()\n  view\n    text \"window\" testId=\"x\"\n", "x");
    assert!(r.take_commands().is_empty(), "boot issued no show");
    let launched: serde_json::Value = serde_json::from_str(
        &r.call_agent("launchEntry", r#"{"launch":"call","entry":"show {}"}"#)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    r.advance(r.now_ms()).unwrap();
    let receipt: serde_json::Value = serde_json::from_str(
        &r.call_agent(
            "called",
            &serde_json::json!({"id": launched["call"]}).to_string(),
        )
        .unwrap()
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        receipt["skipped"],
        serde_json::json!(["focus"]),
        "{receipt}"
    );
    assert_eq!(hidden_refusals(&r, "tool call"), 1);
    assert!(r.take_commands().is_empty());
}

#[test]
fn a_hidden_page_checks_the_generated_panel_command() {
    let (mut r, _) = boot_with(
        "component App\n  view\n    link href=\"/tools\" target=\"tools\" windowKind=\"panel\" testId=\"panel\"\n      text \"Tools\"\n", "panel");
    hide(&mut r);
    for origin in [
        CallOrigin::Tool,
        CallOrigin::Action,
        CallOrigin::Device,
        CallOrigin::App,
    ] {
        r.act_from(
            origin,
            "#panel",
            vec![Value::str("/tools"), Value::str("tools")],
        )
        .unwrap();
        let allowed = matches!(origin, CallOrigin::App | CallOrigin::Device);
        assert_eq!(
            r.take_commands().iter().any(|c| c.name == "panel"),
            allowed,
            "{origin:?}"
        );
    }
}

#[test]
fn a_tools_armed_task_and_its_worlds_reply_keep_the_tool_after_it_ends() {
    let (mut r, view) = boot_with(
        r#"component App
  state armed = false
  tool start writes "Start." = start
  action start
    armed = true
    postMessage("go", "world")
  task later when armed
    after(5, bring)
  action bring
    focus()
  action reply(data: string)
    focus()
  view
    canvas surface=world() width=100 height=100 message=reply testId="world"
"#,
        "world",
    );
    call(&mut r, "start");
    let posted = r.take_commands();
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0].name, "postMessage");
    let message = posted[0].args[0].as_str().unwrap().to_owned();
    hide(&mut r);
    r.advance(r.now_ms() + 5.).unwrap();
    assert_eq!(hidden_refusals(&r, "tool call"), 1);
    r.dispatch(view, Event::Message(message)).unwrap();
    assert_eq!(hidden_refusals(&r, "tool call"), 2);
    assert!(r.take_commands().is_empty());
}

#[test]
fn a_tools_resource_answer_arms_a_hidden_timer_with_the_same_cause() {
    let (mut r, _) = boot_with(
        r#"component App
  state armed = false
  state edition = 0
  resource level = readLevel(edition) when armed as shape number else empty()
  tool start writes "Start." = start
  action start
    armed = true
  action edit
    edition = edition + 1
  task loaded when armed and current(level) key=level
    after(1, bring)
  action bring
    focus()
  view
    text testId="level" `${level}`
"#,
        "level",
    );
    call(&mut r, "start");
    let sent = r.take_requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].request.reach.origin, CallOrigin::Tool);
    r.act("edit", vec![]).unwrap();
    assert!(
        r.take_requests().is_empty(),
        "the new arguments kept the request"
    );
    hide(&mut r);
    assert_eq!(hidden_refusals(&r, "tool call"), 0);
    let answer = |value: u8| {
        Outcome::Response(crate::Response {
            status: 200,
            headers: vec![],
            body: vec![value],
        })
    };
    r.fulfill(sent[0].ticket, answer(b'1')).unwrap();
    r.advance(r.now_ms() + 1.).unwrap();
    assert_eq!(hidden_refusals(&r, "tool call"), 1);
    r.changed(crate::ResourceChange::Source("readLevel"))
        .unwrap();
    let again = r.take_requests();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].request.reach.origin, CallOrigin::Tool);
    r.fulfill(again[0].ticket, answer(b'2')).unwrap();
    r.advance(r.now_ms() + 1.).unwrap();
    assert_eq!(hidden_refusals(&r, "tool call"), 2);
    assert!(r.take_commands().is_empty());
}

#[test]
fn a_persons_reask_after_a_kept_device_request_uses_the_last_ask() {
    let (mut r, _) = boot_with(
        "component App\n  state key = 0\n  resource level = readLevel(key) as shape number\n  action edit\n    key = key + 1\n  view\n    text testId=\"level\" `${level}`\n",
        "level",
    );
    let sent = r.take_requests();
    r.act_from(CallOrigin::Device, "edit", vec![]).unwrap();
    r.act("edit", vec![]).unwrap();
    assert!(r.take_requests().is_empty());
    r.fulfill(
        sent[0].ticket,
        Outcome::Response(crate::Response {
            status: 200,
            headers: vec![],
            body: b"1".to_vec(),
        }),
    )
    .unwrap();
    r.changed(crate::ResourceChange::Source("readLevel"))
        .unwrap();
    assert_eq!(r.take_requests()[0].request.reach.origin, CallOrigin::App);
}

/// expo/exact#1424: a `keepRunning` window the person shows again by
/// launching the app reaches the app only as its page becoming visible, and
/// what that starts is the person's, as at a cold launch.
#[test]
fn a_page_shown_again_starts_work_as_the_apps() {
    const SHOWN: &str = "shape Page\n  visibilityState: string\n  persisted: bool\n\ncomponent App\n  resource page = exactPage() as shape Page\n  mutation frozen as shape number\n  task arrive when page.visibilityState == \"visible\" and not page.persisted\n    after(1, begin)\n  action begin\n    send frozen = save(1)\n  view\n    text \"x\" testId=\"x\"\n";
    let (mut r, _) = boot_with(SHOWN, "x");
    r.advance(1.0).unwrap();
    assert_eq!(r.take_requests().len(), 1, "the cold launch's capture");
    let away = crate::Page {
        hidden: true,
        persisted: true,
        ..r.page()
    };
    r.set_page(away).unwrap();
    let shown = crate::Page {
        hidden: false,
        persisted: false,
        restores: 1,
        ..r.page()
    };
    r.set_page(shown).unwrap();
    r.advance(10.0).unwrap();
    let sent = r.take_requests();
    assert_eq!(sent.len(), 1, "the warm launch's capture");
    assert_eq!(sent[0].request.reach.origin, CallOrigin::App);
}

/// A source that is not ready until the test says: `prep` answers at
/// once, every other send later.
struct Unready(bool);

impl DataSource for Unready {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("answers through answer")
    }

    fn answer(&mut self, _: &mut Store, source: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(match source {
            "prep" => Answer::Now(Value::Number(1.0)),
            _ => Answer::Later(Request::native(b"{}".to_vec())),
        })
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::Number(0.0)))
    }

    fn ready(&self) -> bool {
        self.0
    }
}

fn boot_unready(source: &str) -> Runner<Unready> {
    let plan = contract::compile(source).unwrap();
    Runner::boot(
        plan,
        Unready(false),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

/// What `data_ready`'s answers open (a root task, a component's mounted
/// task) is the work of the sends that waited for it: a person's press is
/// the app's, as it would be had the source been ready; joined by a send
/// the device made, it is the device's.
#[test]
fn what_a_waiting_sends_answer_opens_is_the_join_of_the_waiting_causes() {
    const DEFERRED: &str = "component App\n  mutation prepared as shape number\n  mutation seen as shape number\n  mutation m as shape number\n  derive ready = match prepared { case some(p) => p == 1, case none => false }\n  action start\n    send prepared = prep(1)\n  action observed\n    send seen = prep(2)\n  action fire\n    send m = save(1)\n  task armed when ready\n    after(1, fire)\n  view\n    column\n      button press=start testId=\"start\"\n        text \"Start\"\n      if ready\n        Child()\ncomponent Child\n  mutation c as shape number\n  action go\n    send c = save(2)\n  task beat mount\n    after(1, go)\n  view\n    text \"child\"\n";
    for (device_waits, expected) in [(false, CallOrigin::App), (true, CallOrigin::Device)] {
        let mut r = boot_unready(DEFERRED);
        if device_waits {
            r.act_from(CallOrigin::Device, "observed", vec![]).unwrap();
        }
        let key = r.kernel().find_by_test_id("start")[0];
        let start = r.kernel().node_by_key(key).unwrap().id;
        r.dispatch(start, Event::Press).unwrap();
        r.data().0 = true;
        r.data_ready().unwrap();
        let at = r.now_ms() + 1.0;
        r.advance(at).unwrap();
        let sent = r.take_requests();
        assert_eq!(sent.len(), 2, "{:?}", r.journal().collect::<Vec<_>>());
        for s in sent {
            assert_eq!(
                s.request.reach.origin, expected,
                "device waits {device_waits}"
            );
        }
    }
}

/// A resource the boot asked before the data source was ready answers at
/// `data_ready`: a task its answer opens is the boot's, the app's.
#[test]
fn a_task_a_boot_asks_answer_opens_is_the_apps() {
    const ASKED: &str = "component App\n  resource level = prep(1) as shape number\n  mutation m as shape number\n  action fire\n    send m = save(1)\n  task armed when level == 1\n    after(1, fire)\n  view\n    text \"level\"\n";
    let mut r = boot_unready(ASKED);
    r.data().0 = true;
    r.data_ready().unwrap();
    let at = r.now_ms() + 1.0;
    r.advance(at).unwrap();
    let sent = r.take_requests();
    assert_eq!(sent.len(), 1, "{:?}", r.journal().collect::<Vec<_>>());
    assert_eq!(sent[0].request.reach.origin, CallOrigin::App);
}

/// A page the host hides (a wake's headless boot, a closed window) is not the
/// person's act, and a load it renders hidden is not either.
#[test]
fn what_a_hidden_page_arms_or_loads_is_no_ones() {
    const AWAY: &str = "shape Page\n  visibilityState: string\n\ncomponent App\n  resource page = exactPage() as shape Page\n  mutation m as shape number\n  task away when page.visibilityState == \"hidden\"\n    after(1, gone)\n  action gone\n    send m = save(1)\n  action loaded\n    send m = save(2)\n  view\n    iframe \"/deck/index.html\" load=loaded testId=\"deck\"\n";
    let (mut r, deck) = boot_with(AWAY, "deck");
    r.take_requests();
    let hidden = crate::Page {
        hidden: true,
        ..r.page()
    };
    r.set_page(hidden).unwrap();
    r.advance(1.0).unwrap();
    r.dispatch(deck, Event::Load).unwrap();
    let sent = r.take_requests();
    assert_eq!(sent.len(), 2);
    for s in sent {
        assert_eq!(s.request.reach.origin, CallOrigin::Device);
    }
}

/// A remote Play or Pause is the person's act on one declared control
/// outside the app's UI: an `action`, as a control's press is, whose
/// command rules hold. The element's own `play` while the page shows stays
/// the app's, and a report that carries anything else runs nothing.
#[test]
fn a_remote_play_or_pause_is_an_action() {
    const PLAYER: &str = "component App\n  mutation m as shape number\n  action moved\n    send m = save(1)\n    focus()\n  view\n    audio \"https://example.com/a.mp3\" play=moved pause=moved testId=\"v\"\n";
    let (mut r, audio) = boot_with(PLAYER, "v");
    r.take_requests();
    r.take_commands();
    let wire = |w: &str| Event::media_payload(w).unwrap_or_else(|| panic!("{w:?} refused"));
    let focused = |r: &mut Runner<Later>| r.take_commands().iter().any(|c| c.name == "focus");
    for remote in ["play\nremote", "pause\nremote"] {
        r.dispatch(audio, wire(remote)).unwrap();
        assert_eq!(
            r.take_requests()[0].request.reach.origin,
            CallOrigin::Action,
            "{remote:?}"
        );
        assert!(!focused(&mut r), "{remote:?}: a call's work does not focus");
    }
    let skipped = |r: &Runner<Later>| {
        r.journal()
            .filter(|l| l.contains("focus: skipped in a tool call"))
            .count()
    };
    assert_eq!(skipped(&r), 2, "{:?}", r.journal().collect::<Vec<_>>());
    r.dispatch(audio, wire("play\n")).unwrap();
    assert_eq!(r.take_requests()[0].request.reach.origin, CallOrigin::App);
    assert!(focused(&mut r));
    assert!(Event::media_payload("play\nremote ").is_none());
    let odd = Event::Media(exact_plan::EventKind::Pause, "remote ".into());
    assert!(r.dispatch(audio, odd).is_err());
    assert!(r.take_requests().is_empty());
    let last = r.journal().last().unwrap_or_default().to_owned();
    assert!(
        last.contains("pause") && last.contains("InvalidEvent"),
        "{last}"
    );
}

/// A call's work never changes the app's language or mode: D4a's filter
/// refuses `setLocale` and `setMode`, and neither applies before the filter
/// runs, in an app with no tool too. The app's own `play` applies both.
#[test]
fn a_calls_work_sets_no_locale_or_mode() {
    const PLAYER: &str = "component App\n  action started\n    setLocale(\"fr\")\n    setMode(\"dark-style\", \"dim\")\n  view\n    column\n      text t(\"title\")\n      audio \"https://example.com/a.mp3\" play=started testId=\"v\"\n";
    const MODES: &str = "forma t\n\naxes\n  dark-style = black | dim  under scheme=dark\n\ntheme\n  ink = #111111\n";
    let dir = crate::TempDir(
        std::env::temp_dir().join(format!("exact2-call-locale-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("strings")).unwrap();
    std::fs::write(dir.join("strings/en.json"), r#"{"title": "Title"}"#).unwrap();
    std::fs::write(dir.join("strings/fr.json"), r#"{"title": "Titre"}"#).unwrap();
    std::fs::write(dir.join("app.forma"), MODES).unwrap();
    std::fs::write(dir.join("app.contract"), PLAYER).unwrap();
    let plan = contract::compile_path(&dir.join("app.contract")).unwrap();
    let mut r = Runner::boot(
        plan,
        Later,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let key = r.kernel().find_by_test_id("v")[0];
    let audio = r.kernel().node_by_key(key).unwrap().id;
    let (locale, modes) = (r.resolved_locale().to_owned(), r.modes());
    let play = |how: &str| Event::Media(exact_plan::EventKind::Play, how.into());
    r.dispatch(audio, play("remote")).unwrap();
    assert_eq!(
        (r.resolved_locale(), r.modes()),
        (&locale[..], modes.clone())
    );
    let refused = |r: &Runner<Later>| {
        (r.journal())
            .filter(|l| l.contains(": refused: in a tool call"))
            .count()
    };
    assert_eq!(refused(&r), 2, "{:?}", r.journal().collect::<Vec<_>>());
    r.dispatch(audio, play("")).unwrap();
    assert_eq!(r.resolved_locale(), "fr");
    assert_ne!(r.modes(), modes);
}
