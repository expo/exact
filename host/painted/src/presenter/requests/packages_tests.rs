//! The native slot's one handler on a desktop loop (LLP 1133.022 D10 item
//! 5): a composed package's body reaches its instance on the loop's turn, a
//! shell's goes to the shell beside it, any other is refused; a call the
//! runner asks to admit reads the instance's own status first; the session's
//! end destroys each instance once.
use super::super::*;
use crate::hatches::{install, Context as HatchContext, Element, Hatches, Ticket};
use crate::package::{json, Context, Entry, Json, Package, Reply};
use std::cell::RefCell;

/// The host work these tests hand out (LLP 1047.001 D12).
struct TestWork;
impl exact_runner::HostWork for TestWork {}

thread_local!(static HEARD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) });
thread_local!(static STATE: RefCell<&'static str> = const { RefCell::new("prompt") });
thread_local!(static NOTIFIER: RefCell<Option<crate::package::Notifier>> = const { RefCell::new(None) });
thread_local!(static BACKGROUND_STATE: RefCell<&'static str> = const { RefCell::new("prompt") });
thread_local!(static BACKGROUND_NOTIFIER: RefCell<Option<crate::package::Notifier>> = const { RefCell::new(None) });
thread_local!(static BACKGROUND_MOMENTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) });

fn heard() -> Vec<String> {
    HEARD.with(|h| std::mem::take(&mut *h.borrow_mut()))
}

/// Answers its status from `prompt`'s state and echoes every call.
struct Probe {
    context: Context,
}

struct BackgroundPackage;

impl Package for BackgroundPackage {
    fn new(context: Context) -> Self {
        BACKGROUND_NOTIFIER.with(|notifier| *notifier.borrow_mut() = Some(context.notifier()));
        Self
    }

    fn later(&mut self, name: &str, _: &Json, _: Option<&Json>, reply: Reply) {
        assert_eq!(name, "geolocationStatus");
        let state = BACKGROUND_STATE.with(|state| *state.borrow());
        reply.send(json!({"available": true, "state": state, "accuracy": "unknown"}));
    }
}

#[derive(Default)]
struct DeferredReply {
    release: Option<std::sync::mpsc::Receiver<()>>,
    started: Option<std::sync::mpsc::Sender<()>>,
    completed: Option<std::sync::mpsc::Sender<()>>,
}

impl DataSource for DeferredReply {
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
        Ok(exact_runner::Answer::Later(
            exact_runner::Request::continuation::<TestWork>(1),
        ))
    }

    fn dispatch(&mut self, _: u64, _: &exact_runner::Store) -> exact_runner::Dispatch {
        let release = self.release.take().expect("one deferred reply");
        let started = self.started.take().expect("the start signal");
        let completed = self.completed.take().expect("the completion signal");
        exact_runner::Dispatch::Run(exact_runner::Work::Later(Box::new(move |reply| {
            started.send(()).unwrap();
            release.recv().unwrap();
            reply.send(exact_runner::Outcome::Storage(b"7".to_vec()));
            completed.send(()).unwrap();
        })))
    }

    fn parse(
        &mut self,
        _: &mut exact_runner::Store,
        _: &str,
        _: &[exact_runner::Value],
        outcome: exact_runner::Outcome,
    ) -> Result<exact_runner::Answer, exact_runner::DataError> {
        assert!(matches!(outcome, exact_runner::Outcome::Storage(ref bytes) if bytes == b"7"));
        Ok(exact_runner::Answer::Now(exact_runner::Value::Number(7.0)))
    }
}

#[derive(Default)]
struct BackgroundHatch {
    frames: Option<Ticket>,
}

impl Hatches for BackgroundHatch {
    fn element(&mut self, _: &Element, context: &mut HatchContext<'_, Self>) {
        if self.frames.is_none() {
            self.frames = Some(context.frames(|_, _, context| {
                let first = BACKGROUND_MOMENTS.with(|moments| {
                    let mut moments = moments.borrow_mut();
                    moments.push("frame");
                    moments.len() == 1
                });
                if first {
                    context.after(5.0, |_, _| {
                        BACKGROUND_MOMENTS.with(|moments| moments.borrow_mut().push("after"));
                    });
                }
            }));
        }
    }
}

impl Package for Probe {
    fn new(context: Context) -> Self {
        HEARD.with(|h| {
            h.borrow_mut()
                .push(format!("made agent={}", context.agent()))
        });
        NOTIFIER.with(|n| *n.borrow_mut() = Some(context.notifier()));
        Probe { context }
    }
    fn later(&mut self, name: &str, args: &Json, _: Option<&Json>, reply: Reply) {
        let origin = self.context.origin();
        HEARD.with(|h| h.borrow_mut().push(format!("later {name} {args} {origin}")));
        match name {
            "development" => reply.send(json!(self.context.development())),
            "status" => reply.send(json!({ "state": STATE.with(|s| *s.borrow()) })),
            _ => reply.send(json!({ "name": name })),
        }
    }
    fn release(&mut self, name: &str, args: Option<&Json>) {
        HEARD.with(|h| h.borrow_mut().push(format!("release {name} {args:?}")));
    }
    fn foreground(&mut self, shown: bool) {
        let said = self.context.foreground();
        HEARD.with(|h| h.borrow_mut().push(format!("foreground {shown} {said}")));
    }
    fn destroy(&mut self) {
        HEARD.with(|h| h.borrow_mut().push("destroy".into()));
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

fn boot() -> Presenter<Nothing> {
    let plan = crate::test_plan("component App\n  view\n    text \"Hi\"\n");
    let (p, _) = Presenter::boot_with(
        &plan,
        Nothing,
        (400., 800.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    p
}

/// Hand `body` to the native slot as the runner does, run a turn, and
/// return what its reply carried.
fn call(p: &mut Presenter<Nothing>, body: Json) -> std::sync::mpsc::Receiver<Json> {
    call_status(p, body, 200)
}

fn call_status(
    p: &mut Presenter<Nothing>,
    body: Json,
    status: u16,
) -> std::sync::mpsc::Receiver<Json> {
    let (tx, rx) = std::sync::mpsc::channel();
    let reply = exact_runner::Reply::new(move |outcome| {
        if let exact_runner::Outcome::Response(r) = outcome {
            assert_eq!(r.status, status);
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
    rx
}

#[test]
fn package_reads_the_binarys_trust() {
    for trust in ["production", "development"] {
        let mut p = boot();
        p.set_delivery_from_compat(&json!({"embedded": {"trust": trust}}).to_string());
        p.serve_packages(vec![Entry::of::<Probe>("probe")]);
        let answer = call(
            &mut p,
            json!({"extension": "probe", "source": "development"}),
        );
        assert_eq!(answer.recv().unwrap(), json!(trust == "development"));
    }
}

#[test]
fn bodies_reach_the_package_the_shell_or_a_refusal() {
    let mut p = boot();
    p.serve_packages(vec![Entry::of::<Probe>("probe")]);
    let answer = call(
        &mut p,
        json!({"extension": "probe", "send": "ping", "args": {"n": 1}, "origin": "tool"}),
    );
    assert_eq!(answer.recv().unwrap(), json!({ "name": "ping" }));
    let foreground = call(&mut p, json!({"extension": "probe", "foreground": false}));
    assert_eq!(foreground.recv().unwrap(), json!({}));
    call(
        &mut p,
        json!({"extension": "probe", "release": "status", "args": {"a": 1}}),
    );
    assert_eq!(
        heard(),
        [
            "made agent=false",
            "later ping {\"n\":1} tool",
            "foreground false false",
            "release status Some(Object {\"a\": Number(1)})"
        ]
    );
    // No shell: another extension is refused, not queued.
    let refused = call(
        &mut p,
        json!({"extension": "other", "send": "go", "args": {}}),
    );
    assert_eq!(refused.recv().unwrap()["error"], "NotSupportedError");
    assert_eq!(p.take_requests(), None);
    // A shell beside the packages: its extension's body is queued for it,
    // and the package's still reaches the package.
    p.serve_requests();
    assert!(
        !p.host().runner().native_slot().hosted(),
        "the shell carries package calls without an app module"
    );
    let shells = call(
        &mut p,
        json!({"extension": "other", "send": "go", "args": {}}),
    );
    let queued = p.take_requests().expect("the shell's request");
    assert_eq!(queued[0], 11, "a module request, kind 11");
    let id = queued[1] as u32;
    assert!(p.answer_request(id, Some("{\"went\":true}")));
    assert_eq!(shells.recv().unwrap(), json!({ "went": true }));
    let rejected = call_status(&mut p, json!({"op": "ping"}), 500);
    p.set_agent_turn(Some(Box::new(|_, words| {
        let id = words.expect("the shell has a request")[1];
        serde_json::from_value(json!([{ "id": id, "error": "NotSupportedError" }])).unwrap()
    })));
    p.agent_reader_turn();
    assert_eq!(
        rejected
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap(),
        json!({"error":"NotSupportedError"})
    );
    p.set_agent_turn(None);
    let still = call(
        &mut p,
        json!({"extension": "probe", "send": "ping", "args": {}}),
    );
    assert_eq!(still.recv().unwrap(), json!({ "name": "ping" }));
    heard();
    p.end_packages();
    p.end_packages();
    assert_eq!(heard(), ["destroy"], "destroyed once");
}

#[test]
fn an_admitted_call_reads_the_status_first() {
    let mut p = boot();
    p.serve_packages(vec![Entry::of::<Probe>("probe")]);
    let admit = json!({"status": "status", "field": "state", "denied": "NotAllowedError"});
    // The status reads `prompt`: refused before the call is made.
    let refused = call(
        &mut p,
        json!({"extension": "probe", "send": "ask", "args": {}, "origin": "tool", "admit": admit}),
    );
    let answer = refused.recv().unwrap();
    assert_eq!(
        (answer["error"].as_str(), answer["message"].as_str()),
        (Some("NotAllowedError"), Some("person-required"))
    );
    assert_eq!(heard(), ["made agent=false", "later status {} tool"]);
    // Granted: the call comes back through the inbox and is made on the next turn.
    STATE.with(|s| *s.borrow_mut() = "granted");
    let made = call(
        &mut p,
        json!({"extension": "probe", "send": "ask", "args": {}, "origin": "tool", "admit": admit}),
    );
    p.run_commands(Nothing::default);
    assert_eq!(made.recv().unwrap(), json!({ "name": "ask" }));
    assert_eq!(heard(), ["later status {} tool", "later ask {} tool"]);
}

enum StatusReply {
    Send(Json),
    Error(&'static str),
    Drop,
    Hold,
}

thread_local! {
    static ADMIT_REPLY: RefCell<StatusReply> = const { RefCell::new(StatusReply::Send(Json::Null)) };
    static ADMIT_HEARD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static ADMIT_HELD: RefCell<Option<Reply>> = const { RefCell::new(None) };
}

struct AdmitProbe {
    context: Context,
}

impl Package for AdmitProbe {
    fn new(context: Context) -> Self {
        Self { context }
    }

    fn later(&mut self, name: &str, _: &Json, _: Option<&Json>, reply: Reply) {
        let req = self.context.request();
        ADMIT_HEARD.with(|h| h.borrow_mut().push(format!("later {name} req={req:?}")));
        if name == "status" {
            let mode = ADMIT_REPLY
                .with(|r| std::mem::replace(&mut *r.borrow_mut(), StatusReply::Send(Json::Null)));
            match mode {
                StatusReply::Send(v) => reply.send(v),
                StatusReply::Error(code) => reply.error(code, None),
                StatusReply::Drop => drop(reply),
                StatusReply::Hold => ADMIT_HELD.with(|h| *h.borrow_mut() = Some(reply)),
            }
            return;
        }
        reply.send(json!({ "name": name, "request": req }));
    }

    fn cancel(&mut self, request: u64) {
        ADMIT_HEARD.with(|h| h.borrow_mut().push(format!("cancel {request}")));
    }
}

fn admit_heard() -> Vec<String> {
    ADMIT_HEARD.with(|h| std::mem::take(&mut *h.borrow_mut()))
}

fn enqueue(p: &mut Presenter<Nothing>, body: Json) -> std::sync::mpsc::Receiver<Json> {
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
    rx
}

#[test]
fn admit_fails_closed_on_status_error_drop_missing_field_and_unknown() {
    let mut p = boot();
    p.serve_packages(vec![Entry::of::<AdmitProbe>("admit-probe")]);
    let admit = json!({"status": "status", "field": "state", "denied": "NotAllowedError"});

    let refused_cases = [
        StatusReply::Error("NotSupportedError"),
        StatusReply::Drop,
        StatusReply::Send(json!({})),
        StatusReply::Send(json!({ "state": 42 })),
        StatusReply::Send(json!({ "state": "unknown" })),
        StatusReply::Send(json!({ "state": "prompt" })),
        StatusReply::Send(json!({ "state": "granted", "error": "NotSupportedError" })),
    ];
    for (idx, mode) in refused_cases.into_iter().enumerate() {
        ADMIT_REPLY.with(|r| *r.borrow_mut() = mode);
        let rx = call(
            &mut p,
            json!({
                "extension": "admit-probe",
                "send": "ask",
                "request": idx as u64 + 1,
                "args": {},
                "origin": "tool",
                "admit": admit,
            }),
        );
        p.run_commands(Nothing::default);
        let answer = rx.recv().unwrap();
        assert_eq!(
            (answer["error"].as_str(), answer["message"].as_str()),
            (Some("NotAllowedError"), Some("person-required")),
            "case {idx}: {answer}"
        );
        assert_eq!(admit_heard(), ["later status req=None"], "case {idx}");
    }

    for (idx, allowed) in ["granted", "limited", "denied"].into_iter().enumerate() {
        let req = 100 + idx as u64;
        ADMIT_REPLY.with(|r| *r.borrow_mut() = StatusReply::Send(json!({ "state": allowed })));
        let rx = call(
            &mut p,
            json!({
                "extension": "admit-probe",
                "send": "ask",
                "request": req,
                "args": {},
                "origin": "tool",
                "admit": admit,
            }),
        );
        p.run_commands(Nothing::default);
        assert_eq!(
            rx.recv().unwrap(),
            json!({ "name": "ask", "request": req }),
            "state={allowed}"
        );
        assert_eq!(
            admit_heard(),
            [
                "later status req=None",
                &format!("later ask req=Some({req})")
            ],
            "state={allowed}"
        );
    }
}

#[test]
fn cancel_during_admit_probe_or_queued_retry_never_starts_the_send() {
    let mut p = boot();
    p.serve_packages(vec![Entry::of::<AdmitProbe>("admit-probe")]);
    let admit = json!({"status": "status", "field": "state", "denied": "NotAllowedError"});

    // Cancel while an async status probe is in flight, then status answers granted.
    ADMIT_REPLY.with(|r| *r.borrow_mut() = StatusReply::Hold);
    let send_rx = call(
        &mut p,
        json!({
            "extension": "admit-probe",
            "send": "ask",
            "request": 7,
            "args": {},
            "origin": "tool",
            "admit": admit,
        }),
    );
    assert_eq!(admit_heard(), ["later status req=None"]);
    let cancel_rx = call(&mut p, json!({"extension": "admit-probe", "cancel": 7}));
    assert_eq!(cancel_rx.recv().unwrap(), json!({}));
    assert_eq!(send_rx.recv().unwrap(), json!({ "error": "AbortError" }));
    assert!(
        admit_heard().is_empty(),
        "never-started send is not cancelled on package"
    );
    ADMIT_HELD
        .with(|h| h.borrow_mut().take())
        .unwrap()
        .send(json!({ "state": "granted" }));
    p.run_commands(Nothing::default);
    assert!(
        admit_heard().is_empty(),
        "cancelled send must never invoke later"
    );

    // Cancel while an async status probe is in flight, then status drops its reply.
    ADMIT_REPLY.with(|r| *r.borrow_mut() = StatusReply::Hold);
    let send_rx = call(
        &mut p,
        json!({
            "extension": "admit-probe",
            "send": "ask",
            "request": 8,
            "args": {},
            "origin": "tool",
            "admit": admit,
        }),
    );
    admit_heard();
    let _ = call(&mut p, json!({"extension": "admit-probe", "cancel": 8}));
    assert_eq!(send_rx.recv().unwrap(), json!({ "error": "AbortError" }));
    drop(ADMIT_HELD.with(|h| h.borrow_mut().take()));
    p.run_commands(Nothing::default);
    assert!(admit_heard().is_empty());

    // Cancel while the admitted send is already queued awaiting the next turn.
    ADMIT_REPLY.with(|r| *r.borrow_mut() = StatusReply::Send(json!({ "state": "granted" })));
    let send_rx = call(
        &mut p,
        json!({
            "extension": "admit-probe",
            "send": "ask",
            "request": 9,
            "args": {},
            "origin": "tool",
            "admit": admit,
        }),
    );
    assert_eq!(admit_heard(), ["later status req=None"]);
    let cancel_rx = enqueue(&mut p, json!({"extension": "admit-probe", "cancel": 9}));
    p.run_commands(Nothing::default);
    assert_eq!(cancel_rx.recv().unwrap(), json!({}));
    assert_eq!(send_rx.recv().unwrap(), json!({ "error": "AbortError" }));
    assert!(
        admit_heard().is_empty(),
        "queued admitted send must not run after cancel"
    );

    // Once the admitted send actually starts, a later cancel still reaches the package.
    ADMIT_REPLY.with(|r| *r.borrow_mut() = StatusReply::Send(json!({ "state": "granted" })));
    let send_rx = call(
        &mut p,
        json!({
            "extension": "admit-probe",
            "send": "ask",
            "request": 10,
            "args": {},
            "origin": "tool",
            "admit": admit,
        }),
    );
    p.run_commands(Nothing::default);
    assert_eq!(
        send_rx.recv().unwrap(),
        json!({ "name": "ask", "request": 10 })
    );
    admit_heard();
    let _ = call(&mut p, json!({"extension": "admit-probe", "cancel": 10}));
    assert_eq!(admit_heard(), ["cancel 10"]);
}

/// A plan that reads a module the composed package answers.
const READS_GEOLOCATION: &str = "use geolocationStatus, GeolocationStatus from \"@exact/geolocation\"\ncomponent App\n  resource status = geolocationStatus() as shape GeolocationStatus\n  view\n    text \"Hi\"\n";

const DRIVEN_BACKGROUND: &str = r#"use geolocationStatus, GeolocationStatus from "@exact/geolocation"
component App
  resource status = geolocationStatus() as shape GeolocationStatus
  state ticks = 0
  state replied = false
  mutation result as shape number then accepted
  action begin
    send result = save()
  action accepted
    replied = true
  action tick
    ticks = ticks + 1
  task beat mount
    every(10, tick)
  view
    column width=300 height=300
      button "Begin" testId="begin" press=begin height=30
      column hatch="clock" width=20 height=20
      text `${replied} ${ticks} ${status.state}`
"#;

/// Whether the runner binds the package's extension.
fn bound(p: &Presenter<Nothing>) -> bool {
    let state = (p.host().runner().extensions_json()).expect("the plan names a module");
    serde_json::from_str::<Json>(&state).unwrap()["geolocation"]["bound"] == true
}

#[test]
fn driven_page_facts_leave_painted_host_work_running() {
    BACKGROUND_STATE.with(|state| *state.borrow_mut() = "prompt");
    BACKGROUND_NOTIFIER.with(|notifier| *notifier.borrow_mut() = None);
    BACKGROUND_MOMENTS.with(|moments| moments.borrow_mut().clear());
    let (release, released) = std::sync::mpsc::channel();
    let (started, did_start) = std::sync::mpsc::channel();
    let (completed, did_complete) = std::sync::mpsc::channel();
    let data = DeferredReply {
        release: Some(released),
        started: Some(started),
        completed: Some(completed),
    };
    let app = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/native-fixture/app.contract");
    let plan = contract::compile_path_source(&app, DRIVEN_BACKGROUND)
        .unwrap()
        .encode();
    let (mut p, error) = Presenter::boot_with(
        &plan,
        data,
        (300., 300.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.set_hatches(Some(install::<BackgroundHatch>(&["clock"])));
    p.serve_packages(vec![Entry::of::<BackgroundPackage>("geolocation")]);

    let json = |reply: String| -> Json { serde_json::from_str(&reply).unwrap() };
    let data = json(crate::agent::handle(
        &mut p,
        r#"{"op":"clock","data":true}"#,
    ));
    assert_eq!(data["settled"], true, "{data}");
    let state = json(crate::agent::handle(&mut p, r#"{"op":"state"}"#));
    assert_eq!(state["resources"]["status"]["state"], "prompt", "{state}");

    let key = p.host().kernel().find_by_test_id("begin")[0];
    let begin = p.host().kernel().node_by_key(key).unwrap().id;
    let tapped = json(crate::agent::handle(
        &mut p,
        &format!(r#"{{"op":"tap","id":{begin}}}"#),
    ));
    assert!(tapped.get("error").is_none(), "{tapped}");
    did_start
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the executor started before the page transition");

    let preferred = json(crate::agent::handle(
        &mut p,
        r#"{"op":"prefer","page":{"visibility-state":"hidden","frozen":true}}"#,
    ));
    assert_eq!(
        preferred["page"]["visibility-state"], "hidden",
        "{preferred}"
    );
    assert_eq!(preferred["page"]["frozen"], true, "{preferred}");

    BACKGROUND_STATE.with(|state| *state.borrow_mut() = "granted");
    let notifier = BACKGROUND_NOTIFIER
        .with(|notifier| notifier.borrow().clone())
        .unwrap();
    notifier.changed("geolocation.geolocationStatus");
    release.send(()).unwrap();
    did_complete
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the executor reply completed while the page facts stood");
    let pump_error = p.pump(p.host().now());
    assert!(pump_error.is_none(), "{pump_error:?}");

    let clock = json(crate::agent::handle(&mut p, r#"{"op":"clock","to":40}"#));
    assert_eq!(clock["clock"], 40, "{clock}");
    assert!(clock.get("error").is_none(), "{clock}");
    let state = json(crate::agent::handle(&mut p, r#"{"op":"state"}"#));
    assert_eq!(state["slots"]["replied"], true, "{state}");
    assert_eq!(state["slots"]["ticks"], 4, "{state}");
    assert_eq!(state["resources"]["status"]["state"], "granted", "{state}");
    let moments = BACKGROUND_MOMENTS.with(|moments| moments.borrow().clone());
    assert!(moments.contains(&"frame"), "{moments:?}");
    assert!(moments.contains(&"after"), "{moments:?}");
}

/// A replacement runner, the dev loop's reload's or an activated update's,
/// is given what the last one had: the native slot's handler and the bound
/// extensions. An instance made before it announces into the new slot.
#[test]
fn a_replaced_runner_reaches_the_packages_as_the_last_did() {
    let plan = crate::test_plan(READS_GEOLOCATION);
    let (mut p, _) = Presenter::boot_with(
        &plan,
        Nothing,
        (400., 800.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    p.serve_packages(vec![Entry::of::<Probe>("geolocation")]);
    let ping = || json!({"extension": "geolocation", "send": "ping", "args": {}});
    assert_eq!(
        call(&mut p, ping()).recv().unwrap(),
        json!({ "name": "ping" })
    );
    assert!(bound(&p));
    heard();
    let reload = |p: &mut Presenter<Nothing>, plan: &[u8]| {
        p.reload(plan, Nothing).unwrap();
    };
    let update = |p: &mut Presenter<Nothing>, plan: &[u8]| {
        let staged = super::super::delivery::tests::StagedPlan(plan.into());
        p.set_updates(Some(Box::new(staged)));
        assert!(p.activate_update(Nothing).unwrap());
    };
    for (name, replace) in [
        ("reload", &reload as &dyn Fn(&mut _, &[u8])),
        ("update", &update),
    ] {
        replace(&mut p, &plan);
        assert!(
            bound(&p),
            "{name}: the extension is bound in the new runner"
        );
        let answer = call(&mut p, ping());
        assert_eq!(answer.recv().unwrap(), json!({ "name": "ping" }), "{name}");
        assert!(
            !heard().iter().any(|h| h.starts_with("made")),
            "{name}: the instance made before answers"
        );
        // Binding announced the module's topics; those are applied first.
        let now = p.host().now();
        p.pump(now);
        assert!(!p.host().has_announced());
        let notifier = NOTIFIER.with(|n| n.borrow().clone()).unwrap();
        notifier.changed("geolocation.geolocationStatus");
        assert!(
            p.host().has_announced(),
            "{name}: announced into the new runner"
        );
    }
}

#[test]
fn android_identity_admission_can_read_a_bound_package_with_or_without_an_app_module() {
    for app in [false, true] {
        let slot = exact_runner::Native::default();
        let handler: exact_runner::NativeHandler = std::sync::Arc::new(|_, _| unreachable!());
        let caps = json!({"providers":{"google":{"active":true}}});
        let read = json!({"extension":"sign-in","sync":"getIdentityCapabilitiesSync","args":{}});
        let call: exact_runner::NativeCall = std::sync::Arc::new({
            let read = read.clone();
            let caps = caps.clone();
            move |body| {
                assert_eq!(serde_json::from_slice::<Json>(body).unwrap(), read);
                Ok(caps.to_string().into_bytes())
            }
        });
        super::install_android_handlers(&slot, &handler, app, Some(call));
        slot.sync([("sign-in.getIdentityCapabilitiesSync".into(), None)].into());
        assert_eq!(slot.hosted(), app);
        let answer = slot.hosted_call().unwrap()(read.to_string().as_bytes()).unwrap();
        assert_eq!(serde_json::from_slice::<Json>(&answer).unwrap(), caps);
    }
}
