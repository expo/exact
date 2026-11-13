//! `@exact/notifications`' actions (LLP 1133.004 stage 2, as amended
//! 2026-11-12): a post declares its actions as a literal, which the plan
//! keeps as the app's categories; a chosen action is `notificationclick`
//! with the action's name and a typed reply; a background action
//! (`foreground=false`) runs its handler as an invocation of origin
//! `action`, which owns its work until that work ends or the platform's
//! time does.

use exact_runner::{agent, Runner, Value, WakeHow};
use std::path::PathBuf;

struct App(PathBuf);
impl App {
    fn new(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("exact-actions-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("node_modules/@exact")).unwrap();
        let package =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/notifications");
        std::os::unix::fs::symlink(
            package.canonicalize().unwrap(),
            path.join("node_modules/@exact/notifications"),
        )
        .unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn compile(&self, body: &str) -> Result<exact_plan::Plan, Vec<contract::CompileError>> {
        let path = self.0.join("app.contract");
        std::fs::write(&path, format!("{HEAD}{body}")).unwrap();
        contract::compile_path_all(&path, false).map(|(plan, _)| plan)
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const HEAD: &str = "use showNotification, requestNotificationPermission, notificationclick, NotificationEvent, NotificationAction, NotificationStatus from \"@exact/notifications\"\n";

const APPROVE: &str = r#"NotificationAction(action="approve", title="Approve", foreground=false, destructive=false, authenticationRequired=true, placeholder=none)"#;
const NOTE: &str = r#"NotificationAction(action="note", title="Add a note", foreground=true, destructive=false, authenticationRequired=false, placeholder=some("Note"))"#;

fn app_text() -> String {
    format!(
        r#"shape Approved
  n: number

component App
  state acted = ""
  state origin = ""
  mutation approved as shape Approved
  mutation asked as shape NotificationStatus
  action clicked(tag: string, e: NotificationEvent)
    match e.action
      case some(a)
        match e.reply
          case some(r)
            acted = `${{tag}} ${{a}} ${{r}}`
          case none
            acted = `${{tag}} ${{a}}`
            send approved = approve(tag)
            send asked = requestNotificationPermission()
            copyText(tag)
      case none
        acted = tag
  action review
    showNotification(title="Coffee, $4.50", tag="p1", actions=[{APPROVE}, {NOTE}])
  action plain
    showNotification(title="Delayed", tag="d1")
  view
    column notificationclick=clicked
      text acted
"#
    )
}

/// `approve` answers later, as a server would: the action's owned work
/// until the test fulfills it.
struct Later;
impl exact_runner::DataSource for Later {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(source.into()))
    }
    fn answer(
        &mut self,
        _: &mut exact_runner::Store,
        _: &str,
        _: &[Value],
    ) -> Result<exact_runner::Answer, exact_runner::DataError> {
        Ok(exact_runner::Answer::Later(exact_runner::Request::native(
            b"{}".to_vec(),
        )))
    }
    fn parse(
        &mut self,
        _: &mut exact_runner::Store,
        _: &str,
        _: &[Value],
        outcome: exact_runner::Outcome,
    ) -> Result<exact_runner::Answer, exact_runner::DataError> {
        match outcome {
            exact_runner::Outcome::Response(_) => {
                Ok(exact_runner::Answer::Now(Value::record(vec![
                    Value::Number(7.0),
                ])))
            }
            other => Err(exact_runner::DataError::Unavailable(format!("{other:?}"))),
        }
    }
    fn grants(&self) -> &str {
        "device.notifications purpose.notifications\n"
    }
}

fn boot(app: &App) -> Runner<Later> {
    let plan = app.compile(&app_text()).unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = Runner::boot(
        plan,
        Later,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.set_module_target("ios");
    by(&mut r, 0.0);
    r.apply_announced();
    r
}

fn by(r: &mut Runner<Later>, ms: f64) {
    let at = r.now_ms() + ms;
    r.advance(at).unwrap();
}

fn state(r: &Runner<Later>) -> serde_json::Value {
    serde_json::from_str(&agent::handle(r, r#"{"op":"state"}"#)).unwrap()
}

fn logs(r: &Runner<Later>) -> String {
    agent::handle(r, r#"{"op":"logs"}"#)
}

fn tap(r: &mut Runner<Later>, ticket: u64, choice: Option<&str>) -> String {
    let mut request = serde_json::json!({ "op": "tap", "ticket": ticket });
    if let Some(c) = choice {
        request["choice"] = c.into();
    }
    agent::answer(r, &request.to_string()).unwrap().0
}

/// A post, allowed at the permission's ticket, and its listed entry.
fn posted(r: &mut Runner<Later>) -> serde_json::Value {
    r.act("review", Vec::new()).unwrap();
    let ticket = (r.device_holds().iter())
        .find(|h| h.capability == "permission")
        .unwrap_or_else(|| panic!("no permission ticket: {}", logs(r)))
        .ticket;
    tap(r, ticket, Some("allow"));
    state(r)["notifications"][0].clone()
}

fn acted(r: &Runner<Later>) -> String {
    format!("{:?}", r.slot("acted"))
}

#[test]
fn a_posts_literal_actions_are_the_apps_declared_categories() {
    let app = App::new("declared");
    let plan = app.compile(&app_text()).unwrap_or_else(|e| panic!("{e:?}"));
    let row = (plan.modules.iter())
        .find(|m| plan.str(m.declared) == "showNotification")
        .unwrap();
    let declared: serde_json::Value = serde_json::from_str(plan.str(row.substitute)).unwrap();
    let lists = declared["declared"]["actions"].clone();
    assert_eq!(lists.as_array().map(Vec::len), Some(1), "{declared}");
    assert_eq!(lists[0][0]["action"], "approve", "{lists}");
    assert_eq!(lists[0][0]["foreground"], false);
    assert_eq!(lists[0][1]["placeholder"], "Note");
    // An app with no actions declares none.
    let plain = App::new("plain");
    let plan = plain
        .compile("component App\n  action go\n    showNotification(title=\"x\")\n  view\n    button press=go\n      text \"go\"\n")
        .unwrap_or_else(|e| panic!("{e:?}"));
    let row = (plan.modules.iter())
        .find(|m| plan.str(m.declared) == "showNotification")
        .unwrap();
    assert!(!plan.str(row.substitute).contains("declared"));
}

#[test]
fn actions_are_written_out_and_an_action_means_one_thing() {
    let refused = |body: String| -> Vec<String> {
        let app = App::new("refused");
        match app.compile(&body) {
            Ok(_) => panic!("compiled:\n{body}"),
            Err(all) => all
                .into_iter()
                .map(|e| format!("{} {}", e.id, e.message))
                .collect(),
        }
    };
    let go = |call: &str, state: &str| {
        format!("component App\n{state}  action go\n    {call}\n  view\n    button press=go\n      text \"go\"\n")
    };
    let computed = refused(go(
        "showNotification(title=\"x\", actions=[NotificationAction(action=name, title=\"Approve\", foreground=false, destructive=false, authenticationRequired=false, placeholder=none)])",
        "  state name = \"approve\"\n",
    ));
    assert!(
        computed
            .iter()
            .any(|e| e.contains("contract-module-literal")),
        "{computed:?}"
    );
    let twice = refused(go(
        &format!(
            "showNotification(title=\"x\", actions=[{APPROVE}])\n    showNotification(title=\"y\", actions=[NotificationAction(action=\"approve\", title=\"OK\", foreground=true, destructive=false, authenticationRequired=false, placeholder=none)])"
        ),
        "",
    ));
    assert!(
        twice
            .iter()
            .any(|e| e.contains("contract-module-declared") && e.contains("approve")),
        "{twice:?}"
    );
}

/// §7 under the agent: the listed entry carries its actions; `tap @ref
/// note=…` is a foreground action, `notificationclick` with the action and
/// the reply, a person's; the plain click carries neither.
#[test]
fn a_foreground_action_is_the_click_with_the_actions_name_and_reply() {
    let app = App::new("foreground");
    let mut r = boot(&app);
    let entry = posted(&mut r);
    assert_eq!(entry["actions"][1]["action"], "note", "{entry}");
    let reference = entry["ref"].as_u64().unwrap();
    let unknown = tap(&mut r, reference, Some("share"));
    assert!(unknown.contains("no action `share`"), "{unknown}");
    let reply = tap(&mut r, reference, Some("note=On my way"));
    assert!(reply.contains("notificationclick"), "{reply}");
    r.apply_announced();
    assert!(acted(&r).contains("p1 note On my way"), "{}", logs(&r));
    assert!(
        logs(&r).contains("notifications.notificationclick: heard by clicked"),
        "{}",
        logs(&r)
    );
}

/// The background action (`foreground=false`): the handler runs as an
/// invocation of origin `action`, the page hidden; it owns the request it
/// sends until the reply lands; what opens UI is refused (a permission's
/// prompt, `copyText`); then it completes and the page shows again.
#[test]
fn a_background_action_owns_its_work_until_it_lands() {
    let app = App::new("background");
    let mut r = boot(&app);
    let entry = posted(&mut r);
    let reference = entry["ref"].as_u64().unwrap();
    // The person changed the permission back meanwhile: a request now
    // would prompt, which a background action cannot show.
    let prompt = r#"{"op":"prefer","permission":{"notifications":"prompt"}}"#;
    r.extension_agent(prompt).unwrap().1.unwrap();
    let reply = tap(&mut r, reference, Some("approve"));
    assert!(reply.contains(r#""as":"action""#), "{reply}");
    by(&mut r, 0.0);
    assert!(acted(&r).contains("p1 approve"), "{}", logs(&r));
    let running = state(&r)["wake"].clone();
    assert_eq!(running["invocations"][0]["state"], "running", "{running}");
    assert_eq!(running["origin"], "action", "{running}");
    assert!(r.page().hidden, "no view sees a background action");
    let journal = logs(&r);
    for line in [
        "action notificationclick p1: heard by clicked",
        "requestNotificationPermission: refused, person-required (action origin)",
        "copyText: refused: during a background action",
    ] {
        assert!(journal.contains(line), "{line} in {journal}");
    }
    let out = r.take_requests();
    assert_eq!(out.len(), 1, "the approval in flight: {}", logs(&r));
    let ok = exact_runner::Outcome::Response(exact_runner::Response {
        status: 200,
        headers: Vec::new(),
        body: b"7".to_vec(),
    });
    r.fulfill(out[0].ticket, ok).unwrap();
    by(&mut r, 0.0);
    assert!(
        logs(&r).contains("action notificationclick p1: completed"),
        "{}",
        logs(&r)
    );
    assert!(format!("{:?}", r.slot("approved")).contains('7'));
    assert!(!r.page().hidden, "the page the action hid is shown again");
}

/// The host raises a chosen background action (`raise_action`), with its
/// own id for it; the runner says when it ended, and the host may expire it
/// first, when the platform's time is up: its request then fails.
#[test]
fn the_host_raises_a_background_action_and_reads_its_end() {
    let app = App::new("host");
    let mut r = boot(&app);
    let args = r#"["p1",{"tag":"p1","data":null,"title":"Coffee","body":null,"action":"approve","reply":null}]"#;
    let how = WakeHow {
        id: 0,
        cold: true,
        hidden: true,
    };
    r.raise_action("notifications", "notificationclick", args, how, "n-1")
        .unwrap();
    by(&mut r, 0.0);
    assert_eq!(r.take_requests().len(), 1, "{}", logs(&r));
    let ended = |r: &Runner<Later>| -> serde_json::Value {
        serde_json::from_str(&agent::handle(r, r#"{"op":"actions"}"#)).unwrap()
    };
    assert_eq!(ended(&r)["ended"], serde_json::json!([]));
    r.expire_action("n-1");
    by(&mut r, 0.0);
    assert_eq!(
        ended(&r)["ended"],
        serde_json::json!([{ "id": "n-1", "outcome": "expired" }]),
        "{}",
        logs(&r)
    );
    assert!(!format!("{:?}", r.slot("approved")).contains('7'));
    // A plain click's event is not an action's: raised as one, it is refused.
    let refused = r
        .raise_action("notifications", "nothing", "[]", how, "n-2")
        .unwrap_err();
    assert!(refused.contains("no event"), "{refused}");
}

/// Data code's post whose actions no literal declared is refused
/// `TypeError`: the platform shows only the categories declared at build.
/// One the Contract declared goes out.
#[test]
fn data_codes_post_naming_undeclared_actions_is_refused() {
    let app = App::new("undeclared");
    let mut r = boot(&app);
    let granted = r#"{"op":"prefer","permission":{"notifications":"granted"}}"#;
    r.extension_agent(granted).unwrap().1.unwrap();
    let mut post = |actions: serde_json::Value| -> String {
        let body = serde_json::json!({ "extension": "notifications", "send": "showNotification",
            "args": { "title": "Coffee", "tag": "p2", "actions": actions } });
        let request = exact_runner::Request::native(body.to_string().into_bytes());
        let exact_runner::Dispatch::Run(work) = r.native_work(&request) else {
            panic!("a module call is work");
        };
        let outcome = match work {
            exact_runner::Work::Now(work) => work(),
            exact_runner::Work::Later(work) => {
                let (tx, rx) = std::sync::mpsc::channel();
                work(exact_runner::Reply::new(move |o| tx.send(o).unwrap()));
                rx.recv().unwrap()
            }
        };
        match outcome {
            exact_runner::Outcome::Response(r) => String::from_utf8_lossy(&r.body).into_owned(),
            other => format!("{other:?}"),
        }
    };
    let undeclared = post(serde_json::json!([{ "action": "delete", "title": "Delete",
        "foreground": false, "destructive": true, "authenticationRequired": true, "placeholder": null }]));
    assert!(undeclared.contains("TypeError"), "{undeclared}");
    let declared = post(serde_json::json!([
        { "action": "approve", "title": "Approve", "foreground": false, "destructive": false,
          "authenticationRequired": true, "placeholder": null },
        { "action": "note", "title": "Add a note", "foreground": true, "destructive": false,
          "authenticationRequired": false, "placeholder": "Note" } ]));
    assert!(!declared.contains("error"), "{declared}: {}", logs(&r));
    assert!(
        logs(&r).contains("showNotification: refused, actions not declared"),
        "{}",
        logs(&r)
    );
}

#[test]
fn notify_action_names_and_list_lengths_are_unambiguous() {
    let app = App::new("names");
    let mut accepted = Vec::new();
    for names in [
        vec![""],
        vec!["approve,note"],
        vec!["note=later"],
        vec!["approve", "approve"],
        vec!["a", "b", "c", "d", "e"],
    ] {
        let actions = names
            .iter()
            .map(|name| APPROVE.replace("approve", name))
            .collect::<Vec<_>>()
            .join(",");
        let body = format!("component App\n  action go\n    showNotification(title=\"x\", actions=[{actions}])\n  view\n    button press=go\n      text \"go\"\n");
        if app.compile(&body).is_ok() {
            accepted.push(names);
        }
    }
    assert!(accepted.is_empty(), "invalid lists compiled: {accepted:?}");
    let four = ["a", "b", "c", "d"]
        .map(|name| APPROVE.replace("approve", name))
        .join(",");
    app.compile(&format!("component App\n  action go\n    showNotification(title=\"x\", actions=[{four}])\n  view\n    text \"x\"\n")).unwrap();
}

#[test]
fn notify_generated_rust_binding_posts_a_declared_list_with_no_placeholder() {
    let app = App::new("rust-binding");
    let mut r = boot(&app);
    let plan = app.compile(&app_text()).unwrap();
    let mut source = contract::module_rust(&plan).unwrap();
    source.push_str(r#"
fn main() {
    use exact_data::modules::ModuleValue;
    let options = notifications::ShowNotificationOptions::decode(&exact_data::modules::serde_json::json!({
        "title":"Coffee", "actions":[
            {"action":"approve","title":"Approve","foreground":false,"destructive":false,"authenticationRequired":true},
            {"action":"note","title":"Add a note","foreground":true,"destructive":false,"authenticationRequired":false,"placeholder":"Note"}
        ]
    })).unwrap();
    assert!(options.actions.as_ref().unwrap()[0].placeholder.is_none());
    let request = notifications::show_notification(&options).unwrap();
    println!("{}", String::from_utf8(request.body).unwrap());
}
"#);
    let src = app.0.join("binding.rs");
    let exe = app.0.join("binding");
    std::fs::write(&src, source).unwrap();
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let mut command = std::process::Command::new("rustc");
    command
        .arg("--edition=2021")
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .arg("-L")
        .arg(format!("dependency={}", deps.display()));
    // Match the libraries this test linked; Apple builds may leave another
    // copy with different rustflags beside them.
    let exe_name = std::env::current_exe()
        .unwrap()
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let fingerprint = deps.parent().unwrap().join(".fingerprint");
    let test: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            fingerprint
                .join(exe_name.replacen("it-", "contract-", 1))
                .join("test-integration-test-it.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for name in ["exact_data", "exact_runner"] {
        let hash = test["deps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d[1] == name)
            .unwrap()[3]
            .as_u64()
            .unwrap();
        let expected = hash.to_le_bytes().map(|b| format!("{b:02x}")).concat();
        let prefix = format!("{}-", name.replace('_', "-"));
        let dir = std::fs::read_dir(&fingerprint)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&prefix)
                    && std::fs::read_to_string(p.join(format!("lib-{name}")))
                        .is_ok_and(|s| s == expected)
            })
            .unwrap();
        let suffix = dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .strip_prefix(&prefix)
            .unwrap()
            .to_owned();
        let lib = deps.join(format!("lib{name}-{suffix}.rlib"));
        command
            .arg("--extern")
            .arg(format!("{name}={}", lib.display()));
    }
    let compiled = command.output().unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(&exe).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = exact_runner::Request::native(output.stdout);
    r.extension_agent(r#"{"op":"prefer","permission":{"notifications":"granted"}}"#)
        .unwrap()
        .1
        .unwrap();
    let exact_runner::Dispatch::Run(exact_runner::Work::Now(work)) = r.native_work(&request) else {
        panic!("substitute answers the post now")
    };
    let exact_runner::Outcome::Response(response) = work() else {
        panic!("the post answers")
    };
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert!(body.get("error").is_none(), "{body}");
}

#[test]
fn simulated_actions_keep_only_bounded_diagnostics_without_a_host_drain() {
    let app = App::new("simulated-endings");
    let mut r = boot(&app);
    r.extension_agent(r#"{"op":"prefer","permission":{"notifications":"granted"}}"#)
        .unwrap()
        .1
        .unwrap();
    for i in 0..256 {
        r.act("review", Vec::new()).unwrap();
        let reference = state(&r)["notifications"][0]["ref"].as_u64().unwrap();
        let reply = tap(&mut r, reference, Some("approve"));
        assert!(reply.contains(r#""as":"action""#), "{reply}");
        by(&mut r, 0.0);
        let requests = r.take_requests();
        assert_eq!(requests.len(), 1);
        r.fulfill(
            requests[0].ticket,
            exact_runner::Outcome::Response(exact_runner::Response {
                status: 200,
                headers: Vec::new(),
                body: b"7".to_vec(),
            }),
        )
        .unwrap();
        by(&mut r, 0.0);
        let diagnostic = r.ended_actions();
        assert_eq!(diagnostic.as_array().unwrap().len(), (i + 1).min(32));
        assert_eq!(
            diagnostic.as_array().unwrap().last().unwrap()["outcome"],
            "completed"
        );
    }
    assert_eq!(
        r.take_ended_actions(),
        serde_json::json!([]),
        "no host held these simulated actions"
    );
}

/// A notification handler either signs in or leaves a storage write in flight.
struct ActionWork {
    callback: Option<&'static str>,
    storage: bool,
    storage_sent: bool,
}

impl exact_runner::DataSource for ActionWork {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
        unreachable!("the fixture implements answer")
    }
    fn answer(
        &mut self,
        _: &mut exact_runner::Store,
        _: &str,
        _: &[Value],
    ) -> Result<exact_runner::Answer, exact_runner::DataError> {
        if let Some(callback) = self.callback {
            return Ok(exact_runner::Answer::Later(
                exact_data::auth::open_auth_session(
                    "https://as.test/authorize",
                    callback,
                    "s1",
                    false,
                ),
            ));
        }
        self.storage = true;
        Ok(exact_runner::Answer::Now(Value::record(vec![
            Value::Number(200.0),
        ])))
    }
    fn parse(
        &mut self,
        _: &mut exact_runner::Store,
        _: &str,
        _: &[Value],
        outcome: exact_runner::Outcome,
    ) -> Result<exact_runner::Answer, exact_runner::DataError> {
        let exact_runner::Outcome::Response(response) = outcome else {
            panic!("expected an auth response")
        };
        assert_eq!(response.status, 403);
        Ok(exact_runner::Answer::Now(Value::record(vec![
            Value::Number(response.status as f64),
        ])))
    }
    fn grants(&self) -> &str {
        "device.notifications purpose.notifications\nauth.session https://as.test\nauth.callback app.test:/cb\nauth.callback http://127.0.0.1/cb"
    }
    fn background(&mut self, _: &exact_runner::Store) -> Option<exact_runner::Request> {
        if !self.storage || self.storage_sent {
            return None;
        }
        self.storage_sent = true;
        Some(exact_runner::Request::continuation(
            exact_runner::BACKGROUND,
        ))
    }
    fn background_state(&self) -> Option<exact_runner::BackgroundState> {
        Some(exact_runner::BackgroundState {
            in_flight: u64::from(self.storage),
            ..Default::default()
        })
    }
}

fn action_work(app: &App, callback: Option<&'static str>, hidden: bool) -> Runner<ActionWork> {
    let plan = app.compile(&app_text()).unwrap();
    let mut r = Runner::boot(
        plan,
        ActionWork {
            callback,
            storage: false,
            storage_sent: false,
        },
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.set_module_target("ios");
    r.advance(0.0).unwrap();
    r.apply_announced();
    r.raise_action(
        "notifications", "notificationclick",
        r#"["p1",{"tag":"p1","data":null,"title":"Coffee","body":null,"action":"approve","reply":null}]"#,
        WakeHow { id: 0, cold: hidden, hidden }, "n-work",
    ).unwrap();
    r.advance(r.now_ms()).unwrap();
    r
}

#[test]
fn a_background_notification_action_cannot_open_or_hold_auth() {
    use exact_runner::auth::{self, Arm, Browser};
    let app = App::new("auth");
    for hidden in [false, true] {
        for callback in ["app.test:/cb", "http://127.0.0.1:4401/cb"] {
            for agent in [false, true] {
                let mut r = action_work(&app, Some(callback), hidden);
                assert_eq!(r.page().hidden, hidden);
                let out = r
                    .take_requests()
                    .into_iter()
                    .find(|o| o.request.is_auth())
                    .unwrap();
                assert_eq!(out.request.reach.origin, exact_runner::CallOrigin::Action);
                assert_eq!(
                    auth::arm(&mut r, &out, agent, Browser::Native, None),
                    Arm::Settled
                );
                let (ticket, outcome) = r.take_request_refusal(true).unwrap();
                r.fulfill(ticket, outcome).unwrap();
                r.advance(r.now_ms()).unwrap();
                assert!(format!("{:?}", r.slot("approved")).contains("403"));
                assert!(r.device_holds().is_empty());
                assert_eq!(
                    r.take_ended_actions(),
                    serde_json::json!([{"id":"n-work","outcome":"completed"}])
                );
            }
        }
    }
}

#[test]
fn a_completed_notification_handler_keeps_its_deadline_while_storage_drains() {
    let app = App::new("storage-deadline");
    let mut r = action_work(&app, None, true);
    let out = r.take_requests();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].target, "background");
    r.advance(r.now_ms()).unwrap();
    let state: serde_json::Value =
        serde_json::from_str(&agent::handle(&r, r#"{"op":"state"}"#)).unwrap();
    assert_eq!(state["wake"]["invocations"][0]["state"], "completed");
    assert_eq!(r.background_operations(), 1);
    assert_eq!(r.take_ended_actions(), serde_json::json!([]));
    r.advance(r.now_ms() + 30_001.0).unwrap();
    assert_eq!(
        r.take_ended_actions(),
        serde_json::json!([{"id":"n-work","outcome":"expired"}])
    );
    assert!(!r.page().hidden);
    assert_eq!(
        r.background_operations(),
        1,
        "expiry does not roll back storage"
    );
}
