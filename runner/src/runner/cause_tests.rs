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
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::Number(0.0)))
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
