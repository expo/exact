//! LLP 1133.002 D13: a native module's names, imported from its package's
//! `exact.json`, called with keyword arguments and a platform's dotted
//! words, compile to an ordinary source of one record. The package is the
//! real `@exact/geolocation`, linked into the app's `node_modules` as a
//! `file:` dependency installs it.

#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;
use std::path::PathBuf;

struct App(PathBuf);
impl App {
    fn new(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("exact-modules-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("node_modules/@exact")).unwrap();
        let package = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/geolocation");
        symlink_dir(
            package.canonicalize().unwrap(),
            path.join("node_modules/@exact/geolocation"),
        )
        .unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn compile_for_rust(
        &self,
        body: &str,
    ) -> Result<(exact_plan::Plan, std::collections::BTreeSet<String>), Vec<contract::CompileError>>
    {
        let path = self.0.join("app.contract");
        std::fs::write(&path, format!("{HEAD}{body}")).unwrap();
        contract::compile_path_for_rust(&path)
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

const HEAD: &str = "use getCurrentPosition, geolocationStatus, GeolocationPosition, GeolocationStatus from \"@exact/geolocation\"\n";

const APP: &str = r#"component App
  resource status = geolocationStatus() as shape GeolocationStatus
  mutation located as shape GeolocationPosition
  action locate
    send located = getCurrentPosition(enableHighAccuracy=false, maximumAge=600000, apple.desiredAccuracy="threeKilometers", android.quality="lowPower")
  view
    column
      text status.state
      match located
        case some(p)
          text `${p.coords.latitude}`
        case none
          text "none"
      button press=locate testId="locate"
        text "Use my location"
"#;

#[test]
fn a_module_send_and_source_compile_to_a_source_of_one_record() {
    let app = App::new("ok");
    let plan = app.compile(APP).unwrap_or_else(|e| panic!("{e:?}"));
    let source = |name: &str| {
        plan.sources
            .iter()
            .find(|s| plan.str(s.name) == name)
            .unwrap_or_else(|| panic!("no source {name}"))
    };
    assert_eq!(source("getCurrentPosition").params.len, 1);
    assert_eq!(source("geolocationStatus").params.len, 0);
    let rows: Vec<String> = plan
        .modules
        .iter()
        .map(|m| {
            format!(
                "{} {}.{} send={} live={} request={} sync={} substitute={:?}",
                plan.str(plan.sources[m.source.0 as usize].name),
                plan.str(m.extension),
                plan.str(m.declared),
                m.send,
                m.live,
                plan.str(m.request),
                plan.str(m.sync),
                serde_json::from_str::<serde_json::Value>(plan.str(m.substitute))
                    .unwrap()
                    .as_object()
                    .unwrap()
                    .keys()
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            "geolocationStatus geolocation.geolocationStatus send=false live=true request=never sync=geolocationStatusSync substitute=[\"default\", \"grants\", \"permission\"]",
            "getCurrentPosition geolocation.getCurrentPosition send=true live=false request=always sync=getCurrentPositionSync substitute=[\"cancel\", \"from\", \"grants\", \"permission\", \"ticket\", \"timeout\"]",
            "requestPreciseLocation geolocation.requestPreciseLocation send=true live=false request=always sync= substitute=[\"answers\", \"grants\", \"permission\", \"raise\", \"unsupported\"]",
            "watchPosition geolocation.watchPosition send=false live=true request=when-prompt sync= substitute=[\"default\", \"fill\", \"grants\", \"permission\", \"short\"]",
        ]
    );
}

fn refused(body: &str) -> Vec<(String, String)> {
    let app = App::new("refused");
    match app.compile(body) {
        Ok(_) => panic!("compiled:\n{body}"),
        Err(all) => all.into_iter().map(|e| (e.id, e.message)).collect(),
    }
}

fn locate(call: &str) -> String {
    format!(
        "component App\n  mutation located as shape GeolocationPosition\n  action locate\n    send located = {call}\n  view\n    button press=locate\n      text \"go\"\n"
    )
}

#[test]
fn a_module_call_refuses_what_its_declarations_do_not_say() {
    let one = |call: &str| {
        let all = refused(&locate(call));
        assert_eq!(all.len(), 1, "{call}: {all:?}");
        all.into_iter().next().unwrap()
    };
    let (id, message) = one("getCurrentPosition(true)");
    assert_eq!(id, "contract-module-positional");
    assert!(message.contains("`enableHighAccuracy`"), "{message}");
    let (id, message) = one("getCurrentPosition(highAccuracy=true)");
    assert_eq!(id, "contract-module-argument");
    assert!(message.contains("no option `highAccuracy`"), "{message}");
    let (id, message) = one("getCurrentPosition(apple.distanceFilter=5)");
    assert_eq!(id, "contract-module-argument");
    assert!(
        message.contains("`@exact/geolocation/apple` declares no word `distanceFilter`"),
        "{message}"
    );
    let (id, message) = one("getCurrentPosition(windows.desiredAccuracyInMeters=5)");
    assert_eq!(id, "contract-module-argument");
    assert!(message.contains("`android`, `apple`"), "{message}");
    let (id, _) = one("getCurrentPosition(timeout=1, timeout=2)");
    assert_eq!(id, "call-argument-twice");
    let (id, message) = one("geolocationStatus()");
    assert_eq!(id, "contract-module-kind");
    assert!(message.contains("a resource reads it"), "{message}");
}

#[test]
fn a_module_name_answers_its_declared_shape() {
    let all = refused(
        "shape Spot\n  latitude: number\ncomponent App\n  resource here = geolocationStatus() as shape Spot\n  view\n    text \"x\"\n",
    );
    assert_eq!(all[0].0, "contract-module-answers", "{all:?}");
    assert!(
        all[0].1.contains("write `as shape GeolocationStatus`"),
        "{all:?}"
    );
}

#[test]
fn a_platform_word_takes_its_declared_vocabulary() {
    let all = refused(&locate(
        "getCurrentPosition(apple.desiredAccuracy=\"veryClose\")",
    ));
    assert!(all.iter().any(|(_, m)| m.contains("veryClose")), "{all:?}");
}

/// No app data: every source here is the module's.
struct NoData;
impl exact_runner::DataSource for NoData {
    fn query(
        &mut self,
        source: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(source.into()))
    }
    fn grants(&self) -> &str {
        "device.geolocation-approximate locationPurpose\n"
    }
}

fn logs<D: exact_runner::DataSource>(r: &exact_runner::Runner<D>) -> String {
    exact_runner::agent::handle(r, r#"{"op":"logs"}"#)
}

/// LLP 1124 D7, D9: with no implementation bound, a module's source answers
/// its declared default, or the agent's preferred value; `clear` returns it
/// to the default; a value outside the source's shape is refused, and so is
/// the permission's state, which `prefer permission` sets (§6). Every name
/// of a used interface is in the plan, read or not.
#[test]
fn an_unbound_module_answers_its_default_or_the_preferred_value() {
    let app = App::new("runner");
    let plan = app.compile(APP).unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = exact_runner::Runner::boot(
        plan,
        NoData,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let status = |r: &exact_runner::Runner<NoData>| format!("{:?}", r.resource("status"));
    assert!(
        status(&r).contains("\"prompt\""),
        "{} {}",
        status(&r),
        logs(&r)
    );
    let prefer = r#"{"op":"prefer","extension":{"geolocation.geolocationStatus":{"available":true,"accuracy":"precise"}}}"#;
    let (_, reply) = r.extension_agent(prefer).expect("a prefer extension");
    assert!(reply.unwrap().contains("precise"));
    assert!(status(&r).contains("\"precise\""), "{}", status(&r));
    let state = r#"{"op":"prefer","extension":{"geolocation.geolocationStatus":{"available":true,"state":"granted","accuracy":"precise"}}}"#;
    let (_, reply) = r.extension_agent(state).unwrap();
    assert!(reply
        .unwrap_err()
        .contains("set by `prefer permission.geolocation`"));
    assert!(
        logs(&r).contains("prefer extension geolocation.geolocationStatus"),
        "{}",
        logs(&r)
    );
    let wrong = r#"{"op":"prefer","extension":{"geolocation.geolocationStatus":{"available":true,"accuracy":"maybe"}}}"#;
    let (_, reply) = r.extension_agent(wrong).unwrap();
    assert!(reply.unwrap_err().contains("maybe"));
    let unknown = r#"{"op":"prefer","extension":{"geolocation.where":{}}}"#;
    let (_, reply) = r.extension_agent(unknown).unwrap();
    assert!(reply
        .unwrap_err()
        .contains("its module sources are geolocation.geolocationStatus"),);
    let clear = r#"{"op":"prefer","extension":{"geolocation.geolocationStatus":"clear"}}"#;
    r.extension_agent(clear).unwrap().1.unwrap();
    assert!(status(&r).contains("\"unknown\""), "{}", status(&r));
}

/// The host's part, as a native host plays it: each request dispatched (a
/// held one parked), released work run, and each outcome fulfilled.
fn pump<D: exact_runner::DataSource>(
    r: &mut exact_runner::Runner<D>,
    parked: &mut Vec<(u64, u64)>,
) {
    for out in r.take_requests() {
        let token = out
            .request
            .continuation
            .expect("a module request is held work");
        parked.push((token, out.ticket));
        let dispatch = r.dispatch_work(token);
        run(r, out.ticket, dispatch);
    }
    for (token, dispatch) in r.release_work() {
        let ticket = parked.iter().find(|(t, _)| *t == token).expect("parked").1;
        run(r, ticket, dispatch);
    }
    r.apply_announced();
}

fn run<D: exact_runner::DataSource>(
    r: &mut exact_runner::Runner<D>,
    ticket: u64,
    dispatch: exact_runner::Dispatch,
) {
    if let exact_runner::Dispatch::Run(exact_runner::Work::Now(work)) = dispatch {
        r.fulfill(ticket, work()).unwrap();
    }
}

fn answer<D: exact_runner::DataSource>(
    r: &mut exact_runner::Runner<D>,
    op: &str,
    capability: &str,
    answer: &str,
) -> String {
    let hold = (r.device_holds().iter())
        .find(|h| h.capability == capability)
        .unwrap_or_else(|| panic!("no {capability} ticket: {:?}", r.device_holds()))
        .ticket;
    let field = if op == "tap" { "choice" } else { "text" };
    let request = format!(r#"{{"op":"{op}","ticket":{hold},"{field}":{answer:?}}}"#);
    exact_runner::agent::answer(r, &request).unwrap().0
}

/// LLP 1133.002 §6 through the agent's tickets: a send while `prompt` waits
/// on a permission ticket; `allow` grants it (approximate, under the app's
/// approximate grant) and, with no position preferred, the send waits on a
/// position ticket, whose typed answer answers it and is filled; `deny`
/// and a later send fail `PERMISSION_DENIED`.
#[test]
fn a_module_send_waits_on_the_agent_s_permission_and_position_tickets() {
    let app = App::new("tickets");
    let plan = app.compile(APP).unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = exact_runner::Runner::boot(
        plan,
        NoData,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let show = |r: &exact_runner::Runner<NoData>, name: &str| {
        format!("{:?}", r.resource(name).or(r.slot(name)))
    };
    let mut parked = Vec::new();
    r.act("locate", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert!(!r.has_pending(), "a held request is not I/O");
    let reply = answer(&mut r, "type", "permission", "x");
    assert!(reply.contains("answered by tap"), "{reply}");
    answer(&mut r, "tap", "permission", "allow");
    pump(&mut r, &mut parked);
    assert!(
        show(&r, "status").contains("\"granted\""),
        "{}",
        show(&r, "status")
    );
    assert!(
        show(&r, "status").contains("\"approximate\""),
        "{}",
        show(&r, "status")
    );
    let wrong = answer(&mut r, "type", "position", "north");
    assert!(
        wrong.contains("coords.latitude,coords.longitude,coords.accuracy"),
        "{wrong}"
    );
    answer(&mut r, "type", "position", "51.5074,-0.1278");
    pump(&mut r, &mut parked);
    let located = show(&r, "located");
    assert!(
        located.contains("51.5074") && located.contains("Number(100.0)"),
        "{located}\n{}",
        logs(&r)
    );
    let (_, reply) = r
        .extension_agent(r#"{"op":"prefer","permission":{"geolocation":"denied"}}"#)
        .unwrap();
    reply.unwrap();
    assert!(
        show(&r, "status").contains("\"denied\""),
        "{}",
        show(&r, "status")
    );
    r.act("locate", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert!(logs(&r).contains("PERMISSION_DENIED"), "{}", logs(&r));
    let state = exact_runner::agent::handle(&r, r#"{"op":"state","name":"failed"}"#);
    let _ = state;
}

/// LLP 1133.002 §6: a request's own `timeout` runs on the agent's clock. A
/// position ticket fails `TIMEOUT` once the clock passes it, and is retired;
/// `timeout=0` with no position preferred fails at once.
#[test]
fn a_position_ticket_times_out_on_the_agent_clock() {
    let app = App::new("timeout");
    let body = "component App\n  mutation slow as shape GeolocationPosition\n  mutation now as shape GeolocationPosition\n  action wait\n    send slow = getCurrentPosition(timeout=100)\n  action instant\n    send now = getCurrentPosition(timeout=0)\n  view\n    text \"x\"\n";
    let plan = app.compile(body).unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = exact_runner::Runner::boot(
        plan,
        NoData,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let mut parked = Vec::new();
    r.extension_agent(r#"{"op":"prefer","permission":{"geolocation":"granted"}}"#)
        .unwrap()
        .1
        .unwrap();
    r.act("wait", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert!(r.device_holds().iter().any(|h| h.capability == "position"));
    r.advance(99.0).unwrap();
    pump(&mut r, &mut parked);
    assert!(
        r.device_holds().iter().any(|h| h.capability == "position"),
        "not yet"
    );
    r.advance(101.0).unwrap();
    pump(&mut r, &mut parked);
    assert!(r.device_holds().is_empty(), "{:?}", r.device_holds());
    assert!(
        logs(&r).contains("geolocation.getCurrentPosition: TIMEOUT"),
        "{}",
        logs(&r)
    );
    r.act("instant", Vec::new()).unwrap();
    pump(&mut r, &mut parked);
    assert!(r.device_holds().is_empty());
    assert!(
        logs(&r).contains("send now failed: geolocation.getCurrentPosition: TIMEOUT"),
        "{}",
        logs(&r)
    );
}

struct LocationData(&'static str);
impl exact_runner::DataSource for LocationData {
    fn query(
        &mut self,
        source: &str,
        args: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, exact_runner::DataError> {
        NoData.query(source, args)
    }
    fn grants(&self) -> &str {
        self.0
    }
}

/// LLP 1133.002 D7, D10, D14: only the full grant can raise precision.
#[test]
fn a_precision_request_respects_the_grant_ceiling_and_ticket_answers() {
    let app = App::new("precision");
    let head = "use requestPreciseLocation from \"@exact/geolocation\"\n";
    let body = format!("{head}component App\n  resource status = geolocationStatus() as shape GeolocationStatus\n  mutation raised as shape GeolocationStatus\n  action raise\n    send raised = requestPreciseLocation(purposeKey=\"forecastPurpose\")\n  view\n    text status.accuracy\n");
    let plan = app.compile(&body).unwrap_or_else(|e| panic!("{e:?}"));
    for (grant, accuracy) in [
        (
            "device.geolocation-approximate locationPurpose",
            "approximate",
        ),
        ("device.geolocation locationPurpose", "precise"),
    ] {
        let mut r = exact_runner::Runner::boot(
            plan.clone(),
            LocationData(grant),
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let mut parked = Vec::new();
        let show = |r: &exact_runner::Runner<LocationData>, name: &str| {
            format!("{:?}", r.resource(name).or(r.slot(name)))
        };
        r.act("raise", Vec::new()).unwrap();
        pump(&mut r, &mut parked);
        answer(&mut r, "tap", "permission", "allow");
        pump(&mut r, &mut parked);
        assert!(
            r.device_holds().is_empty(),
            "{grant}: {:?}",
            r.device_holds()
        );
        assert!(
            show(&r, "raised").contains(&format!("\"{accuracy}\"")),
            "{}",
            show(&r, "raised")
        );
        if accuracy == "precise" {
            r.extension_agent(r#"{"op":"prefer","extension":{"geolocation.geolocationStatus":{"available":true,"accuracy":"approximate"}}}"#)
                .unwrap().1.unwrap();
            for choice in ["deny", "cancel", "allow"] {
                r.act("raise", Vec::new()).unwrap();
                pump(&mut r, &mut parked);
                answer(&mut r, "tap", "precision", choice);
                pump(&mut r, &mut parked);
                let expected = if choice == "allow" {
                    "precise"
                } else {
                    "approximate"
                };
                assert!(
                    show(&r, "raised").contains(&format!("\"{expected}\"")),
                    "{choice}: {}",
                    show(&r, "raised")
                );
                assert!(
                    show(&r, "status").contains(&format!("\"{expected}\"")),
                    "{choice}: {}",
                    show(&r, "status")
                );
            }
        }
        r.act("raise", Vec::new()).unwrap();
        pump(&mut r, &mut parked);
        assert!(r.device_holds().is_empty(), "{grant}: answered at once");
        r.set_module_target("web");
        r.act("raise", Vec::new()).unwrap();
        pump(&mut r, &mut parked);
        assert!(r.device_holds().is_empty());
        assert!(
            logs(&r).contains("geolocation.requestPreciseLocation: NotSupportedError"),
            "{}",
            logs(&r)
        );
    }
}

/// LLP 1133.002 D6, D13: an app that reads or sends a module's name writes
/// one of its interface's grant lines; with none, the compile says which.
#[test]
fn a_module_without_its_grant_line_is_named() {
    let app = App::new("grant");
    std::fs::write(
        app.0.join("app.json"),
        r#"{"name":"G","app":{"id":"test.grant","name":"G"}}"#,
    )
    .unwrap();
    std::fs::write(
        app.0.join("app.ts"),
        "export const grants = 'net.fetch https://x.test';\n",
    )
    .unwrap();
    let body = "component App\n  resource status = geolocationStatus() as shape GeolocationStatus\n  view\n    text status.state\n";
    std::fs::write(app.0.join("app.contract"), format!("{HEAD}{body}")).unwrap();
    let warned = |app: &App| {
        let (result, warnings) =
            contract::compile_path_all_with_warnings(&app.0.join("app.contract"), false);
        let errors = result.err().unwrap_or_default();
        format!(
            "{:?}",
            warnings
                .iter()
                .chain(&errors)
                .map(|e| (&e.id, &e.message))
                .collect::<Vec<_>>()
        )
    };
    let all = warned(&app);
    assert!(all.contains("grant-missing"), "{all}");
    assert!(
        all.contains("`device.geolocation` or `device.geolocation-approximate`"),
        "{all}"
    );
    std::fs::write(
        app.0.join("app.ts"),
        "export const grants = 'device.geolocation-approximate purpose.geolocation';\n",
    )
    .unwrap();
    assert!(!warned(&app).contains("grant-missing"), "{}", warned(&app));
}

/// `contract fmt` keeps a platform's word against its `=`, as it keeps a
/// named argument's (LLP 1133.002 D13).
#[test]
fn fmt_keeps_a_dotted_keyword_tight() {
    let src = format!(
        "{HEAD}{}",
        locate("getCurrentPosition(maximumAge=600000, apple.desiredAccuracy=\"threeKilometers\")")
    );
    let out = contract_syntax::fmt::format(&src).unwrap();
    assert!(
        out.contains("apple.desiredAccuracy=\"threeKilometers\""),
        "{out}"
    );
}

/// LLP 1133.001 D4: a host that binds the extension sends its names to the
/// native slot, the substitute set aside: no ticket, the module answers,
/// and a call's options travel as written.
#[test]
fn a_bound_module_answers_through_the_native_slot() {
    use std::sync::{Arc, Mutex};
    let app = App::new("bound");
    let plan = app.compile(APP).unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = exact_runner::Runner::boot(
        plan,
        NoData,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let asked: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = asked.clone();
    r.native_slot().host(Some(Arc::new(move |body: Vec<u8>, reply: exact_runner::Reply| {
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        seen.lock().unwrap().push(v.to_string());
        let answer = match (v["source"].as_str(), v["send"].as_str()) {
            (Some("geolocationStatus"), _) => serde_json::json!({ "available": true, "state": "granted", "accuracy": "precise" }),
            (_, Some("getCurrentPosition")) => serde_json::json!({ "coords": { "latitude": 35.0, "longitude": 139.0, "accuracy": 10 }, "timestamp": 1 }),
            _ => serde_json::json!({}),
        };
        reply.send(exact_runner::Outcome::Response(exact_runner::Response { status: 200, headers: Vec::new(), body: answer.to_string().into_bytes() }));
    })));
    r.bind_extensions(&["geolocation"], &[]);
    r.apply_announced();
    let pump = |r: &mut exact_runner::Runner<NoData>| {
        for out in r.take_requests() {
            let exact_runner::Dispatch::Run(exact_runner::Work::Later(work)) =
                r.native_work(&out.request)
            else {
                panic!("a bound name goes to the native slot");
            };
            let (tx, rx) = std::sync::mpsc::channel();
            work(exact_runner::Reply::new(move |o| tx.send(o).unwrap()));
            r.fulfill(out.ticket, rx.recv().unwrap()).unwrap();
        }
    };
    pump(&mut r);
    let show = |r: &exact_runner::Runner<NoData>, name: &str| {
        format!("{:?}", r.resource(name).or(r.slot(name)))
    };
    assert!(
        show(&r, "status").contains("\"granted\""),
        "{}",
        show(&r, "status")
    );
    r.act("locate", Vec::new()).unwrap();
    pump(&mut r);
    assert!(
        show(&r, "located").contains("35.0"),
        "{}",
        show(&r, "located")
    );
    assert!(r.device_holds().is_empty(), "no ticket: the module answers");
    let calls = asked.lock().unwrap().clone();
    assert!(
        calls.iter().any(|c| c.contains("\"maximumAge\":600000")
            && c.contains("\"desiredAccuracy\":\"threeKilometers\"")),
        "the call's options travel: {calls:?}"
    );
}

#[test]
fn a_synchronous_form_answers_from_the_substitute_until_its_module_is_bound() {
    use std::sync::Arc;
    let app = App::new("sync");
    let plan = app.compile(APP).unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = exact_runner::Runner::boot(
        plan,
        NoData,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.native_slot().host_call(Some(Arc::new(|_: &[u8]| {
        Ok(br#"{"state":"module"}"#.to_vec())
    })));
    r.advance(0.0).unwrap();
    let call = r
        .native_slot()
        .hosted_call()
        .expect("the host's call, behind the runner's answers");
    let now = |body: &str| String::from_utf8(call(body.as_bytes()).unwrap()).unwrap();
    let status = r#"{"extension":"geolocation","sync":"geolocationStatusSync","args":{}}"#;
    let position = r#"{"extension":"geolocation","sync":"getCurrentPositionSync","args":{"maximumAge":600000}}"#;
    assert!(now(status).contains("\"prompt\""), "{}", now(status));
    // While `prompt` a cached fix is never answered: the request would ask.
    assert_eq!(now(position), r#"{"error":"POSITION_UNAVAILABLE"}"#);
    let grant = r#"{"op":"prefer","permission":{"geolocation":"granted"}}"#;
    r.extension_agent(grant).unwrap().1.unwrap();
    let paris = r#"{"op":"prefer","extension":{"geolocation.watchPosition":{"coords":{"latitude":48.8566,"longitude":2.3522}}}}"#;
    r.extension_agent(paris).unwrap().1.unwrap();
    assert!(now(status).contains("\"granted\""), "{}", now(status));
    assert!(now(position).contains("48.8566"), "{}", now(position));
    assert_eq!(
        now(r#"{"extension":"geolocation","sync":"where","args":{}}"#),
        r#"{"error":"NotSupportedError"}"#
    );
    // Anything else is the host's module's own call.
    assert_eq!(now(r#"{"op":"level"}"#), r#"{"state":"module"}"#);
    r.bind_extensions(&["geolocation"], &[]);
    assert_eq!(now(status), r#"{"state":"module"}"#);
}

#[test]
fn rust_shapes_exclude_module_call_records() {
    let app = App::new("rust-shapes");
    for body in [APP, "component App\n  view\n    text \"unused import\"\n"] {
        let (plan, call_records) = app
            .compile_for_rust(&format!(
                "shape Local__options\n  position: GeolocationPosition\nshape getCurrentPosition__custom\n  value: number\n{body}"
            ))
            .unwrap_or_else(|e| panic!("{e:?}"));
        let plan = exact_plan::Plan::decode(&plan.encode()).unwrap();
        let rust = contract::rust(&plan, &call_records).unwrap();
        for call in [
            "getCurrentPosition",
            "watchPosition",
            "requestPreciseLocation",
        ] {
            for suffix in ["options", "apple", "android"] {
                assert!(
                    !rust.contains(&format!("/// Contract `shape {call}__{suffix}`.")),
                    "{call}__{suffix}\n{rust}"
                );
            }
        }
        for name in [
            "GeolocationPosition",
            "GeolocationCoordinates",
            "GeolocationStatus",
            "LocalOptions",
            "GetCurrentPositionCustom",
        ] {
            assert!(
                rust.contains(&format!("pub struct {name} {{")),
                "{name}\n{rust}"
            );
        }
    }
}

#[test]
fn rust_shapes_preserve_scoped_names_that_look_like_call_records() {
    let app = App::new("rust-scoped-options");
    std::fs::write(
        app.0.join("options.contract"),
        "shape geolocationStatus\n  value: number\n",
    )
    .unwrap();
    std::fs::write(
        app.0.join("web.contract"),
        "shape getCurrentPosition\n  value: number\n",
    )
    .unwrap();
    let (plan, call_records) = app
        .compile_for_rust(&format!(
            "use geolocationStatus as StatusRow from \"./options.contract\"\nuse getCurrentPosition as PositionRow from \"./web.contract\"\n{APP}"
        ))
        .unwrap_or_else(|e| panic!("{e:?}"));
    let plan = exact_plan::Plan::decode(&plan.encode()).unwrap();
    let rust = contract::rust(&plan, &call_records).unwrap();
    for (module, suffix, name) in [
        ("geolocationStatus", "options", "GeolocationStatusOptions"),
        ("getCurrentPosition", "web", "GetCurrentPositionWeb"),
    ] {
        assert!(plan.module_named(module).is_some());
        let scoped = format!("{module}__{suffix}");
        assert!(plan.types.iter().any(|row| plan.str(row.name) == scoped));
        assert!(!call_records.contains(&scoped));
        assert!(rust.contains(&format!("pub struct {name} {{")), "{rust}");
    }
    assert!(call_records.contains("getCurrentPosition__options"));
    assert!(call_records.contains("getCurrentPosition__android"));
    assert!(
        !rust.contains("pub struct GetCurrentPositionOptions {"),
        "{rust}"
    );
    assert!(
        !rust.contains("pub struct GetCurrentPositionAndroid {"),
        "{rust}"
    );
}

#[test]
fn rust_shapes_preserve_authored_module_option_records() {
    let app = App::new("rust-authored-options");
    let package = app.0.join("node_modules/@scratch/records");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"@scratch/records","exports":{".":"./exact.json"}}"#,
    )
    .unwrap();
    std::fs::write(
        package.join("exact.json"),
        r#"{
  "kind": "interface", "name": "@scratch/records", "version": "1.0.0",
  "shapes": {
    "Fetch__input": {"value": "string"},
    "Fetch__unused": {"count": "number"},
    "Fetch__android": {"quality": "option<number>"},
    "Envelope": {"input": "Fetch__input"}
  },
  "names": [{"source": "Fetch", "options": {
    "input": "option<Fetch__input>", "unused": "option<Fetch__unused>",
    "android": "option<Fetch__android>"
  }, "answers": "Envelope"}]
}"#,
    )
    .unwrap();
    std::fs::write(
        app.0.join("app.contract"),
        "use Fetch, Envelope from \"@scratch/records\"\ncomponent App\n  resource result = Fetch() as shape Envelope\n  view\n    text result.input.value\n",
    )
    .unwrap();
    let (plan, call_records) = contract::compile_path_for_rust(&app.0.join("app.contract"))
        .unwrap_or_else(|e| panic!("{e:?}"));
    let plan = exact_plan::Plan::decode(&plan.encode()).unwrap();
    let rust = contract::rust(&plan, &call_records).unwrap();
    for name in ["Envelope", "FetchInput", "FetchUnused", "FetchAndroid"] {
        assert!(
            rust.contains(&format!("pub struct {name} {{")),
            "{name}\n{rust}"
        );
    }
    assert!(rust.contains("pub input: FetchInput,"), "{rust}");
    assert!(!rust.contains("pub struct FetchOptions {"), "{rust}");
}

#[test]
fn rust_shapes_include_call_records_reached_from_authored_shapes() {
    let app = App::new("rust-reached-options");
    for ty in [
        "getCurrentPosition__options",
        "option<getCurrentPosition__options>[]",
        "option<getCurrentPosition__apple>[]",
    ] {
        let (plan, call_records) = app
            .compile_for_rust(&format!("shape Saved\n  options: {ty}\n{APP}"))
            .unwrap_or_else(|e| panic!("{e:?}"));
        let plan = exact_plan::Plan::decode(&plan.encode()).unwrap();
        let rust = contract::rust(&plan, &call_records).unwrap();
        assert!(rust.contains("pub struct Saved {"), "{rust}");
        assert!(
            rust.contains("pub struct GetCurrentPositionApple {"),
            "{rust}"
        );
        let options = ty.contains("__options");
        for name in ["GetCurrentPositionOptions", "GetCurrentPositionAndroid"] {
            assert_eq!(
                rust.contains(&format!("pub struct {name} {{")),
                options,
                "{rust}"
            );
        }
        assert!(
            !rust.contains("pub struct WatchPositionOptions {"),
            "{rust}"
        );
        let collision = if options {
            "GetCurrentPositionOptions"
        } else {
            "GetCurrentPositionApple"
        };
        let (plan, call_records) = app
            .compile_for_rust(&format!(
                "shape {collision}\n  unrelated: string\nshape Saved\n  options: {ty}\n{APP}"
            ))
            .unwrap_or_else(|e| panic!("{e:?}"));
        let error = contract::rust(&plan, &call_records).unwrap_err();
        assert!(
            error.contains(collision) && error.contains("both become Rust"),
            "{error}"
        );
    }
}

#[test]
fn rust_shapes_include_call_records_used_as_data_results() {
    let app = App::new("rust-result-options");
    let (plan, call_records) = app
        .compile_for_rust(
            "component App\n  resource options = loadOptions() as shape getCurrentPosition__options\n  view\n    text \"options\"\n",
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
    let plan = exact_plan::Plan::decode(&plan.encode()).unwrap();
    let rust = contract::rust(&plan, &call_records).unwrap();
    for expected in [
        "pub struct GetCurrentPositionOptions {",
        "pub struct GetCurrentPositionApple {",
        "pub struct GetCurrentPositionAndroid {",
        "impl ContractValue for GetCurrentPositionOptions {",
        "impl ::core::convert::From<&GetCurrentPositionOptions> for ::exact_runner::Value {",
        "impl ::core::convert::TryFrom<&::exact_runner::Value> for GetCurrentPositionOptions {",
    ] {
        assert!(rust.contains(expected), "{expected}\n{rust}");
    }
    assert!(
        !rust.contains("pub struct WatchPositionOptions {"),
        "{rust}"
    );
}

#[test]
fn rust_shapes_include_call_records_used_as_data_parameters() {
    let app = App::new("rust-parameter-options");
    let (plan, call_records) = app
        .compile_for_rust(
            "component App\n  state options: option<getCurrentPosition__options>[] = []\n  resource saved = saveOptions(options) as shape bool\n  view\n    text \"options\"\n",
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
    let plan = exact_plan::Plan::decode(&plan.encode()).unwrap();
    let rust = contract::rust(&plan, &call_records).unwrap();
    for expected in [
        "pub struct GetCurrentPositionOptions {",
        "pub struct GetCurrentPositionApple {",
        "pub struct GetCurrentPositionAndroid {",
        "pub apple: ::core::option::Option<GetCurrentPositionApple>,",
        "pub android: ::core::option::Option<GetCurrentPositionAndroid>,",
    ] {
        assert!(rust.contains(expected), "{expected}\n{rust}");
    }
    assert!(
        !rust.contains("pub struct WatchPositionOptions {"),
        "{rust}"
    );
}

#[test]
fn the_rust_binding_is_the_plans_modules_with_their_wire_names() {
    let app = App::new("rust");
    let plan = app.compile(APP).unwrap_or_else(|e| panic!("{e:?}"));
    let rust = contract::module_rust(&plan).unwrap();
    for expected in [
        "pub mod geolocation {",
        // A call's options and a platform's words, in Rust's names.
        "pub struct GetCurrentPositionOptions {",
        "pub struct GetCurrentPositionApple {",
        "pub fn get_current_position(options: &GetCurrentPositionOptions) -> ::core::result::Result<::exact_runner::Request, ::exact_data::modules::ModuleError>",
        "pub fn get_current_position_reply(outcome: &::exact_runner::Outcome) -> ::core::result::Result<GeolocationPosition, ::exact_data::modules::ModuleError>",
        // A name that takes nothing; every name of a used module, read or not.
        "pub fn geolocation_status() -> ::exact_runner::Request",
        "pub fn watch_position(",
        // The wire keeps the declared names; an enum is checked.
        "::exact_data::modules::put(&mut o, \"enableHighAccuracy\", &self.enable_high_accuracy)?;",
        "::exact_data::modules::member(x, &[\"bestForNavigation\"",
    ] {
        assert!(rust.contains(expected), "{expected}\n---\n{rust}");
    }
    // Rust has no synchronous form (LLP 1133.002 D14).
    assert!(!rust.contains("Sync"), "{rust}");
    // A plan that calls no module has no binding.
    let none = App::new("rust-none");
    let plan = none
        .compile("component App\n  view\n    text \"x\"\n")
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(contract::module_rust(&plan).unwrap().lines().count(), 1);
}

#[test]
fn a_modules_shapes_may_take_names_rust_predefines() {
    // Each record shadows the prelude's name in the generated module, so the
    // binding spells Rust's own names, and its own crate's, by their paths.
    let app = App::new("rust-prelude");
    let package = app.0.join("node_modules/@scratch/prelude");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(
        package.join("package.json"),
        r#"{"name": "@scratch/prelude", "version": "1.0.0", "exports": {".": "./exact.json"}}"#,
    )
    .unwrap();
    std::fs::write(
        package.join("exact.json"),
        r#"{
  "kind": "interface",
  "name": "@scratch/prelude",
  "version": "1.0.0",
  "shapes": {
    "Input": {"value": "string"},
    "Option": {"values": "string[]", "result": "option<Result>"},
    "Result": {"items": "Vec[]", "mode": "enum<a|b>"},
    "Vec": {"first": "option<String>", "all": "String[]"},
    "String": {"count": "number", "flag": "bool"},
    "ModuleValue": {"inner": "option<Option>"}
  },
  "names": [{"send": "fetch", "request": true, "options": {"limit": "option<number>", "input": "option<Input>"}, "answers": "ModuleValue"}]
}"#,
    )
    .unwrap();
    let source = "use fetch, ModuleValue from \"@scratch/prelude\"\ncomponent App\n  mutation got as shape ModuleValue\n  action go\n    send got = fetch(limit=3)\n  view\n    text \"x\"\n";
    std::fs::write(app.0.join("app.contract"), source).unwrap();
    let (plan, call_records) = contract::compile_path_for_rust(&app.0.join("app.contract"))
        .unwrap_or_else(|e| panic!("{e:?}"));
    let shapes = contract::rust(&plan, &call_records).unwrap();
    assert!(shapes.contains("pub struct Input {"), "{shapes}");
    let rust = contract::module_rust(&plan).unwrap();
    for name in ["Option", "Result", "Vec", "String", "ModuleValue"] {
        assert!(
            rust.contains(&format!("pub struct {name} {{")),
            "{name}\n{rust}"
        );
    }
    assert!(
        rust.contains("pub values: ::std::vec::Vec<::std::string::String>,"),
        "{rust}"
    );
    assert!(
        rust.contains("pub result: ::core::option::Option<Result>,"),
        "{rust}"
    );
    assert!(
        !rust.contains("\n    use "),
        "an import would collide with a record: {rust}"
    );
    // No record is generic and none is a trait: a bare generic or trait use
    // of these names would mean the record, and not compile.
    for bare in [
        "Option<",
        "Result<",
        "Vec<",
        "ModuleValue>",
        "ModuleValue for",
        "m::",
        "exact_runner::",
    ] {
        let unqualified = rust
            .match_indices(bare)
            .filter(|(at, _)| !rust[..*at].ends_with(|c: char| c == ':' || c.is_alphanumeric()))
            .count();
        assert_eq!(unqualified, 0, "bare `{bare}` in:\n{rust}");
    }
}

/// A package written into `app`'s `node_modules` as `@scratch/<name>`.
fn scratch_package(app: &App, name: &str, declaration: &str) {
    let package = app.0.join("node_modules/@scratch").join(name);
    std::fs::create_dir_all(&package).unwrap();
    let manifest = format!(
        r#"{{"name": "@scratch/{name}", "version": "1.0.0", "exports": {{".": "./exact.json"}}}}"#
    );
    std::fs::write(package.join("package.json"), manifest).unwrap();
    std::fs::write(package.join("exact.json"), declaration).unwrap();
}

#[test]
fn a_value_passed_whole_as_json_may_be_a_list_and_binds_in_rust_as_json() {
    // `json<T>` holds any JSON value's type a declaration may name (#316).
    let app = App::new("rust-json");
    let declaration = |records: &str| {
        format!(
            r#"{{
  "kind": "interface", "name": "@scratch/cloud", "version": "1.0.0",
  "shapes": {{ "CloudRecordWrite": {{ "key": "string" }}, "Done": {{ "ok": "bool" }} }},
  "names": [
    {{ "send": "write", "request": true, "contract": false, "answers": "json<CloudRecordWrite[]>",
       "options": {{ "records": "{records}", "extra": "option<json<CloudRecordWrite>[]>" }} }},
    {{ "send": "flush", "request": true, "answers": "Done" }}
  ]
}}"#
        )
    };
    scratch_package(&app, "cloud", &declaration("json<CloudRecordWrite[]>"));
    let source = "use flush, Done from \"@scratch/cloud\"\ncomponent App\n  mutation done as shape Done\n  action go\n    send done = flush()\n  view\n    text \"x\"\n";
    std::fs::write(app.0.join("app.contract"), source).unwrap();
    let (plan, _) = contract::compile_path_for_rust(&app.0.join("app.contract"))
        .unwrap_or_else(|e| panic!("{e:?}"));
    let rust = contract::module_rust(&plan).unwrap();
    for expected in [
        "pub records: ::exact_data::modules::serde_json::Value,",
        "pub extra: ::core::option::Option<::std::vec::Vec<::exact_data::modules::serde_json::Value>>,",
        // The body carries the value as `put` encodes it: a JSON null as null
        // (exact-data's `a_json_null_is_sent_and_an_absent_option_is_omitted`).
        "::exact_data::modules::put(&mut o, \"records\", &self.records)?;",
    ] {
        assert!(rust.contains(expected), "{expected}\n---\n{rust}");
    }
    // What JSON cannot carry is refused, naming what `json<T>` holds.
    for refused in ["json<bytes>", "json<handle<file>>", "json<json<Done>>"] {
        scratch_package(&app, "cloud", &declaration(refused));
        let errors = contract::compile_path_for_rust(&app.0.join("app.contract")).unwrap_err();
        let text = format!("{errors:?}");
        assert!(
            text.contains(&format!("`{refused}` is not a JSON value's type"))
                && text.contains("`option<T>` or `T[]` of one"),
            "{refused}: {text}"
        );
    }
}

#[test]
fn an_implementations_names_keep_their_owner_and_bind_through_the_interface() {
    let app = App::new("implementation");
    scratch_package(
        &app,
        "sensor",
        r#"{"kind":"interface","name":"@scratch/sensor","grants":["device.sensor"],
            "shapes":{"Shared":{"ok":"bool"}},"names":[{"source":"available","answers":"bool","substitute":{"default":false}}]}"#,
    );
    let package = app.0.join("node_modules/@scratch/sensor");
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"@scratch/sensor","version":"1.0.0","exports":{
            ".":"./exact.json","./apple":"./apple/exact.json"}}"#,
    )
    .unwrap();
    std::fs::create_dir(package.join("apple")).unwrap();
    std::fs::write(
        package.join("apple/exact.json"),
        r#"{"kind":"implementation","implements":"@scratch/sensor@^1.0","platform":"apple",
            "targets":["ios","tvos"],"shapes":{"Reading":{"value":"number"}},
            "names":[
              {"source":"reading","live":true,"answers":"Reading","sync":{"name":"readingSync","from":"read"},
               "substitute":{"default":{"value":0},"targets":{"ios":{"value":9}}}},
              {"send":"sample","targets":["ios"],"answers":"Reading","options":{"count":"number"},
               "substitute":{"unsupported":["web"]}},
              {"source":"sharedReading","answers":"Shared","substitute":{"default":{"ok":true}}},
              {"send":"raw","contract":false,"answers":"Reading"}]}"#,
    )
    .unwrap();
    let path = app.0.join("app.contract");
    let body = r#"component App
  resource now = reading() as shape Reading
  resource shared = sharedReading() as shape Shared
  resource supported = available() as shape bool
  mutation last as shape Reading
  action sampleNow
    send last = sample(count=2)
  view
    text `${now.value}`
"#;
    std::fs::write(
        &path,
        format!("use reading, sharedReading, sample, Reading from \"@scratch/sensor/apple\"\nuse Shared, available from \"@scratch/sensor\"\n{body}"),
    )
    .unwrap();
    let (plan, _) = contract::compile_path_for_rust(&path).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        plan.package_extensions().get("sensor"),
        Some(&"@scratch/sensor")
    );
    let row = plan
        .modules
        .iter()
        .find(|m| plan.str(m.declared) == "reading")
        .unwrap();
    let sub: serde_json::Value = serde_json::from_str(plan.str(row.substitute)).unwrap();
    assert_eq!(sub["owner"], "@scratch/sensor/apple");
    assert_eq!(sub["grants"], serde_json::json!(["device.sensor"]));
    assert_eq!(sub["targets"]["ios"]["value"], 9);
    assert_eq!(
        sub["implementationTargets"],
        serde_json::json!(["ios", "tvos"])
    );
    assert_eq!(plan.str(row.sync), "readingSync");
    let rust = contract::module_rust(&plan).unwrap();
    assert!(rust.contains("pub mod sensor"), "{rust}");
    assert!(rust.contains("pub count: ::core::primitive::f64"), "{rust}");
    let mut runner = exact_runner::Runner::boot(
        plan,
        LocationData("device.sensor"),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    runner.set_module_target("linux");
    runner.bind_extensions(&["sensor"], &[]);
    assert!(format!("{:?}", runner.resource("now")).contains("Number(0.0)"));
    runner.act("sampleNow", Vec::new()).unwrap();
    pump(&mut runner, &mut Vec::new());
    assert!(
        logs(&runner).contains("NotSupportedError"),
        "{}",
        logs(&runner)
    );
    assert!(runner.device_holds().is_empty());

    std::fs::write(
        &path,
        format!("use reading, Reading from \"@scratch/sensor\"\n{body}"),
    )
    .unwrap();
    let errors = contract::compile_path_all(&path, false)
        .err()
        .expect("interface import refused");
    assert!(
        errors.iter().any(|e| e.id == "contract-use-unknown"),
        "{errors:?}"
    );

    std::fs::write(&path, "component App\n  view\n    text \"data only\"\n").unwrap();
    std::fs::write(
        app.0.join("app.ts"),
        "import { raw } from '@scratch/sensor/apple';\n",
    )
    .unwrap();
    let (plan, _) = contract::compile_path_all(&path, false).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(plan.modules.iter().any(|m| plan.str(m.declared) == "raw"));

    scratch_package(
        &app,
        "sensor-fast",
        r#"{
        "kind":"implementation","implements":"@scratch/sensor@^1.0","targets":["ios"],
        "names":[{"source":"fast","answers":"Shared","substitute":{"default":{"ok":true}}}]
    }"#,
    );
    std::fs::remove_file(app.0.join("app.ts")).unwrap();
    std::fs::write(
        app.0.join("package.json"),
        r#"{"dependencies":{"@scratch/sensor-fast":"1.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(&path, "use fast from \"@scratch/sensor-fast\"\nuse Shared from \"@scratch/sensor\"\ncomponent App\n  resource status = fast() as shape Shared\n  view\n    text \"fast\"\n").unwrap();
    let (plan, _) = contract::compile_path_all(&path, false).unwrap_or_else(|e| panic!("{e:?}"));
    let row = plan
        .modules
        .iter()
        .find(|m| plan.str(m.declared) == "fast")
        .unwrap();
    let sub: serde_json::Value = serde_json::from_str(plan.str(row.substitute)).unwrap();
    assert_eq!(sub["owner"], "@scratch/sensor-fast");
    assert_eq!(plan.str(row.extension), "sensor");
    assert_eq!(sub["implementationTargets"], serde_json::json!(["ios"]));
}

#[test]
fn an_implementation_send_retains_primitive_interface_dependencies() {
    let app = App::new("primitive-interface");
    scratch_package(
        &app,
        "sensor",
        r#"{"kind":"interface","name":"@scratch/sensor","names":[
          {"source":"reading","answers":"number","substitute":{"default":3}}]}"#,
    );
    scratch_package(
        &app,
        "sensor-own",
        r#"{"kind":"implementation","implements":"@scratch/sensor@^1.0","targets":["linux"],
          "names":[{"send":"sample","answers":"number","substitute":{"answers":"reading"}}]}"#,
    );
    std::fs::write(
        app.0.join("package.json"),
        r#"{"dependencies":{"@scratch/sensor-own":"1.0.0"}}"#,
    )
    .unwrap();
    let plan = app
        .compile(
            r#"use sample from "@scratch/sensor-own"
component App
  mutation last as shape number
  action sampleNow
    send last = sample()
  view
    text "sensor"
"#,
        )
        .unwrap();
    let mut runner = exact_runner::Runner::boot(
        plan,
        NoData,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    runner.set_module_target("linux");
    runner.act("sampleNow", Vec::new()).unwrap();
    pump(&mut runner, &mut Vec::new());
    assert!(
        format!("{:?}", runner.slot("last")).contains("Number(3.0)"),
        "{}",
        logs(&runner)
    );
}

#[test]
fn implementation_declarations_with_the_same_name_remain_distinct() {
    let app = App::new("distinct-implementations");
    scratch_package(
        &app,
        "sensor",
        r#"{"kind":"interface","name":"@scratch/sensor","names":[{"source":"reading","answers":"number","substitute":{"default":3}}]}"#,
    );
    for (name, value) in [("first", 1), ("second", 2)] {
        scratch_package(
            &app,
            name,
            &format!(
                r#"{{"kind":"implementation","implements":"@scratch/sensor@^1.0","targets":["ios"],
            "names":[{{"source":"reading","answers":"number","sync":{{"name":"readingSync"}},"substitute":{{"default":{value}}}}}]}}"#
            ),
        );
    }
    // Both declarations are elsewhere on Linux; neither may select the other's default.
    std::fs::write(
        app.0.join("app.json"),
        r#"{"app":{"id":"test.sensor","name":"Sensor"},"implementations":{"@scratch/sensor":{"ios":"@scratch/first"}}}"#,
    )
    .unwrap();
    let path = app.0.join("app.contract");
    std::fs::write(&path, "use reading as first from \"@scratch/first\"\nuse reading as second from \"@scratch/second\"\nuse reading as common from \"@scratch/sensor\"\ncomponent App\n  resource c = common() as shape number\n  resource a = first() as shape number\n  resource b = second() as shape number\n  view\n    text `${a} ${b}`\n").unwrap();
    let (plan, _) = contract::compile_path_all(&path, false).unwrap();
    let rust = contract::module_rust(&plan).unwrap();
    let mut runner = exact_runner::Runner::boot(
        plan,
        NoData,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    fn call(runner: &mut exact_runner::Runner<NoData>, owner: Option<&str>) -> serde_json::Value {
        let request = exact_runner::Request::native(
            serde_json::json!({"extension":"sensor","owner":owner,"source":"reading","args":{}})
                .to_string()
                .into_bytes(),
        );
        let outcome = match runner.native_work(&request) {
            exact_runner::Dispatch::Run(exact_runner::Work::Now(work)) => work(),
            exact_runner::Dispatch::Run(exact_runner::Work::Later(work)) => {
                let (tx, rx) = std::sync::mpsc::channel();
                work(exact_runner::Reply::new(move |out| tx.send(out).unwrap()));
                rx.recv().unwrap()
            }
            _ => panic!("a module response"),
        };
        let exact_runner::Outcome::Response(response) = outcome else {
            panic!("a JSON response")
        };
        serde_json::from_slice(&response.body).unwrap()
    }
    runner.set_module_target("linux");
    assert_eq!(call(&mut runner, None), 3);
    for (name, expected) in [("first", 1), ("second", 2)] {
        assert_eq!(
            call(&mut runner, Some(&format!("@scratch/{name}"))),
            expected
        );
    }
    runner
        .native_slot()
        .host_call(Some(std::sync::Arc::new(|_| Ok(b"9".to_vec()))));
    runner.advance(0.0).unwrap();
    let synced = runner.native_slot().hosted_call().unwrap();
    for (name, expected) in [("first", 1), ("second", 2)] {
        let body = serde_json::json!({"extension":"sensor","owner":format!("@scratch/{name}"),"sync":"readingSync"}).to_string();
        assert_eq!(
            synced(body.as_bytes()).unwrap(),
            expected.to_string().as_bytes()
        );
    }
    runner
        .native_slot()
        .host(Some(std::sync::Arc::new(|_, reply| {
            reply.send(exact_runner::Outcome::Response(exact_runner::Response {
                status: 200,
                headers: Vec::new(),
                body: b"9".to_vec(),
            }));
        })));
    runner.set_module_target("ios");
    runner.bind_extensions(&["sensor"], &[]);
    assert_eq!(call(&mut runner, Some("@scratch/first")), 9);
    assert_eq!(call(&mut runner, Some("@scratch/second")), 2);
    let state: serde_json::Value =
        serde_json::from_str(&runner.extensions_json().unwrap()).unwrap();
    assert_eq!(
        state["sensor"]["names"]["reading#@scratch/second"]["route"],
        "substitute"
    );
    for (name, expected) in [("first", 9), ("second", 2)] {
        let body = serde_json::json!({"extension":"sensor","owner":format!("@scratch/{name}"),"sync":"readingSync"}).to_string();
        assert_eq!(
            synced(body.as_bytes()).unwrap(),
            expected.to_string().as_bytes()
        );
    }
    let functions: Vec<_> = rust
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("pub fn ")
                .and_then(|line| line.split_once('(').map(|(name, _)| name))
        })
        .collect();
    assert_eq!(functions.len(), 6, "{rust}");
    assert_eq!(
        functions
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        6,
        "{rust}"
    );
    std::fs::remove_file(app.0.join("app.json")).unwrap();
    std::fs::write(
        app.0.join("package.json"),
        r#"{"dependencies":{"@scratch/second":"1.0.0"}}"#,
    )
    .unwrap();
    let errors = contract::compile_path_all(&path, false)
        .err()
        .expect("an unbound implementation import");
    assert!(
        errors.iter().any(|e| e.id == "bake-implementation-unbound"
            && e.message.contains("binds `@scratch/second`")),
        "{errors:?}"
    );
}

#[test]
fn a_linked_packages_implementation_reads_its_own_interface() {
    let app = App::new("linked-implementation");
    let outside = app.0.with_extension("sensor");
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(outside.join("apple")).unwrap();
    std::fs::write(
        outside.join("package.json"),
        r#"{"name":"@scratch/sensor","version":"1.0.0","exports":{
            ".":"./exact.json","./apple":"./apple/exact.json"}}"#,
    )
    .unwrap();
    std::fs::write(
        outside.join("exact.json"),
        r#"{"kind":"interface","name":"@scratch/sensor","shapes":{"Shared":{"ok":"bool"}},"names":[]}"#,
    )
    .unwrap();
    std::fs::write(
        outside.join("apple/exact.json"),
        r#"{"kind":"implementation","implements":"@scratch/sensor@^1.0","platform":"apple",
            "targets":["ios"],"names":[{"source":"reading","answers":"Shared","substitute":{"default":{"ok":true}}}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(app.0.join("node_modules/@scratch")).unwrap();
    symlink_dir(&outside, app.0.join("node_modules/@scratch/sensor")).unwrap();
    let compiled = app.compile(
        r#"use reading from "@scratch/sensor/apple"
use Shared from "@scratch/sensor"
component App
  resource now = reading() as shape Shared
  view
    text "sensor"
"#,
    );
    let _ = std::fs::remove_dir_all(&outside);
    let plan = compiled.unwrap_or_else(|e| panic!("{e:?}"));
    assert!(plan
        .modules
        .iter()
        .any(|m| plan.str(m.declared) == "reading"));
}
