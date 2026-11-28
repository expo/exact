//! LLP 1103: a driver fault fails a fetch by its URL's prefix, where a host
//! would hand the request to its transport, as a refused connection does.

use crate::source::{boxed, Source};
use exact_kernel::Kernel;
use exact_runner::failure::FailureCode;
use exact_runner::{
    agent, Answer, DataError, DataSource, Dispatch, FailureKind, Outcome, Request, Response,
    Runner, Store, Value, Work,
};

/// A source that fetches its URL and lands the body, or `error: <message>`.
struct Recipes;
impl DataSource for Recipes {
    fn grants(&self) -> &str {
        "net.fetch https://api.test"
    }
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::Unavailable("answers with a request".into()))
    }
    fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
        let url = args[0].as_str().unwrap_or_default().to_string();
        Ok(Answer::Later(Request::get(&url)))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let text = match outcome {
            Outcome::Response(r) => String::from_utf8_lossy(&r.body).into_owned(),
            Outcome::Failed { kind, message } => format!("error: {kind:?} {message}"),
            other => format!("other: {other:?}"),
        };
        Ok(Answer::Now(Value::record(vec![Value::str(&text)])))
    }
}

const APP: &str = "shape Line\n  text: string\ncomponent App\n  resource a = recipes(\"https://api.test/recipes/1\") as shape Line\n  resource b = recipes(\"https://api.test/users/1\") as shape Line\n  view\n    column\n      text a.text testId=\"a\"\n      text b.text testId=\"b\"\n";

#[test]
fn an_http_fixture_runs_the_source_parser_and_survives_replacement() {
    let mut r = Runner::boot(
        contract::compile(APP).unwrap(),
        boxed(Recipes),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let reply = agent::handle(
        &r,
        r#"{"op":"prefer","faults":{"answer":"https://api.test/recipes","response":{"status":201,"headers":{"content-type":"application/json"},"body":"Stubbed greeting"}}}"#,
    );
    assert!(!reply.contains("error"), "{reply}");
    let out = r
        .take_requests()
        .into_iter()
        .find(|r| r.target == "a")
        .unwrap();
    let Some(Dispatch::Run(Work::HttpResponse(work))) = r.fault_dispatch(&out) else {
        panic!("fixture must replace the transport")
    };
    let outcome = work();
    let Outcome::Response(response) = &outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(response.status, 201);
    assert_eq!(
        response.headers,
        vec![("content-type".into(), "application/json".into())]
    );
    r.fulfill(out.ticket, outcome).unwrap();
    assert_eq!(text(&r, "a"), "Stubbed greeting");
    let carried = r.carry();
    let next = Runner::boot_carrying(
        contract::compile(APP).unwrap(),
        boxed(Recipes),
        Kernel::with_monospace(),
        &carried,
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(next.faults().json(), r.faults().json());
    assert!(
        next.fetch_fault()("https://api.test/recipes").is_none(),
        "long polls do not consume response fixtures"
    );
    let mut denied = out.clone();
    denied.request.grants = Some(String::new());
    assert!(
        r.fault_dispatch(&denied).is_none(),
        "an ungranted fixture reaches the executor's refusal"
    );
    r.faults().pass("https://api.test/recipes");
    assert!(r.fault_dispatch(&out).is_none());
}

fn text(r: &Runner<Source>, id: &str) -> String {
    let key = r.kernel().find_by_test_id(id)[0];
    let node = r.kernel().node_by_key(key).unwrap();
    node.props
        .str(exact_kernel::PropId::Text)
        .unwrap_or("")
        .to_string()
}

/// Every request out, as a host runs it: a fault's work, else a 200 whose
/// body is the URL.
fn run(r: &mut Runner<Source>) {
    for out in r.take_requests() {
        let outcome = match r.fault_dispatch(&out) {
            Some(Dispatch::Run(Work::Now(work))) => work(),
            Some(_) => unreachable!("a fault is work now"),
            None => Outcome::Response(Response {
                status: 200,
                headers: vec![],
                body: out.request.url.clone().into_bytes(),
            }),
        };
        r.fulfill(out.ticket, outcome).unwrap();
    }
}

#[test]
fn a_fault_fails_the_fetches_its_prefix_matches_as_a_network_failure() {
    let mut r = Runner::boot(
        contract::compile(APP).unwrap(),
        boxed(Recipes),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let reply = agent::handle(
        &r,
        r#"{"op":"prefer","faults":{"fail":"https://api.test/recipes","times":1}}"#,
    );
    assert!(reply.contains("\"left\":1"), "{reply}");
    run(&mut r);
    assert!(
        text(&r, "a")
            .starts_with("error: Network fetch failed (driver fault): https://api.test/recipes/1"),
        "{}",
        text(&r, "a")
    );
    assert_eq!(
        text(&r, "b"),
        "https://api.test/users/1",
        "another prefix goes out"
    );
    let logs = agent::handle(&r, r#"{"op":"logs"}"#);
    assert!(
        logs.contains("fetch failed (driver fault): https://api.test/recipes/1"),
        "{logs}"
    );
    let state = agent::handle(&r, r#"{"op":"state"}"#);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&state).unwrap()["faults"],
        serde_json::json!([{"prefix": "https://api.test/recipes", "times": 1,
            "left": 0, "hits": 1, "armed": true, "failureHits": 1}])
    );
    // Spent: a refresh goes out.
    let spent = agent::handle(
        &r,
        r#"{"op":"prefer","faults":{"pass":"https://nothing.test/"}}"#,
    );
    assert!(spent.contains("no fault was armed"), "{spent}");
    let refused = agent::handle(
        &r,
        r#"{"op":"prefer","faults":{"fail":"https://x.test/","times":0}}"#,
    );
    assert!(refused.contains("positive integer"), "{refused}");
}

#[test]
fn a_stream_is_not_matched() {
    let mut request = Request::get("https://api.test/recipes/live");
    request.stream = true;
    let r = Runner::boot(
        contract::compile(APP).unwrap(),
        boxed(Recipes),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.faults().arm("https://api.test/", None).unwrap();
    let mut r = r;
    let out = exact_runner::RequestOut {
        completion: None,
        class: exact_runner::RequestClass::Ordered,
        ticket: 99,
        target: "a".into(),
        request,
        forced: false,
    };
    assert!(
        r.fault_dispatch(&out).is_none(),
        "streams are deferred (D5)"
    );
    let plain = exact_runner::RequestOut {
        completion: None,
        class: exact_runner::RequestClass::Ordered,
        request: Request::get("https://api.test/recipes/2"),
        ..out
    };
    match r.fault_dispatch(&plain) {
        Some(Dispatch::Run(Work::Now(work))) => assert!(matches!(
            work(),
            Outcome::Failed {
                kind: FailureKind::Network,
                ..
            }
        )),
        _ => panic!("a plain fetch is failed"),
    }
}

#[test]
fn a_replacement_runner_carries_the_table_as_it_is() {
    let boot = || {
        Runner::boot(
            contract::compile(APP).unwrap(),
            boxed(Recipes),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap()
    };
    let mut r = boot();
    r.faults().arm("https://api.test/recipes", Some(1)).unwrap();
    r.faults().arm("https://api.test/users", None).unwrap();
    r.faults().pass("https://api.test/users");
    run(&mut r);
    let carried = r.carry();
    let mut next = Runner::boot_carrying(
        contract::compile(APP).unwrap(),
        boxed(Recipes),
        Kernel::with_monospace(),
        &carried,
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(next.faults().json(), r.faults().json());
    run(&mut next);
    assert!(
        !next.faults().take("https://api.test/recipes/2"),
        "the spent count stays spent"
    );
    assert!(
        !next.faults().take("https://api.test/users/2"),
        "the passed prefix stays passed"
    );
    assert!(boot().carry().faults.is_none(), "no table, nothing carried");
}

/// Answers a record wider than its shape, as a backend row with one more field.
struct Wide;
impl DataSource for Wide {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::Unavailable("answers with a request".into()))
    }
    fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(Request::get(
            args[0].as_str().unwrap_or_default(),
        )))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::record(vec![
            Value::str("x"),
            Value::str("extra"),
        ])))
    }
}

#[test]
fn state_says_why_a_resource_failed_until_it_answers() {
    // App farm round 1: a shape refusal was only in the journal.
    let mut r = Runner::boot(
        contract::compile(APP).unwrap(),
        boxed(Wide),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert!(!agent::handle(&r, r#"{"op":"state"}"#).contains("\"failed\""));
    for out in r.take_requests() {
        r.fulfill(out.ticket, Outcome::Storage(vec![])).unwrap();
    }
    let state = agent::handle(&r, r#"{"op":"state"}"#);
    let failed = state.split("\"failed\":{").nth(1).unwrap_or_default();
    assert!(
        failed.starts_with("\"a\":\"") && failed.contains("outside its shape"),
        "{state}"
    );
}

/// A source that lets a failed fetch through, as a TypeScript module that
/// does not catch it (js/src/reply.js `fail`), throws on `/boom`, throws
/// a Snapback refusal naming its reason on `/full` (the snapback4 driver's
/// `refused`), and answers wider than its shape on `/wide`.
struct Through;
impl DataSource for Through {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::Unavailable("answers with a request".into()))
    }
    fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(Request::get(
            args[0].as_str().unwrap_or_default(),
        )))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let body = match outcome {
            Outcome::Failed { kind, message } => {
                return Err(DataError::Failed(
                    FailureCode::of_kind(kind),
                    message,
                    String::new(),
                ))
            }
            Outcome::Response(r) => String::from_utf8_lossy(&r.body).into_owned(),
            other => format!("{other:?}"),
        };
        if body.ends_with("/boom") {
            return Err(DataError::Unavailable("the source threw".into()));
        }
        if body.ends_with("/full") {
            return Err(FailureCode::seam_error(
                Some("error"),
                "SLOT_FULL: the slot is taken".into(),
                Some("SLOT_FULL"),
            ));
        }
        let mut fields = vec![Value::str(&body)];
        if body.ends_with("/wide") {
            fields.push(Value::str("extra"));
        }
        Ok(Answer::Now(Value::record(fields)))
    }
}

const WHY: &str = "shape Line\n  text: string\ncomponent App\n  resource a = through(\"https://api.test/a\") as shape Line\n  resource b = through(\"https://api.test/boom\") as shape Line\n  resource c = through(\"https://api.test/wide\") as shape Line\n  resource d = through(\"https://api.test/full\") as shape Line\n  derive why = match failure(a) { case some(f) => `${f.code}: ${f.message} [${f.reason}]`, case none => \"ok\" }\n  view\n    column\n      text why testId=\"a\"\n      text (match failure(b) { case some(f) => f.code, case none => \"ok\" }) testId=\"b\"\n      text (match failure(c) { case some(f) => f.code, case none => \"ok\" }) testId=\"c\"\n      text (match failure(d) { case some(f) => f.reason == \"SLOT_FULL\" ? `full ${f.code}` : f.reason, case none => \"ok\" }) testId=\"d\"\n";

#[test]
fn failure_names_each_class_and_says_why() {
    // LLP 1109 D3: `failure(x)` is `none` until `x` fails, then its code,
    // the message `state.failed` shows and the reason its error named
    // (empty unless it named one: the author's ruling of 2026-10-12).
    let mut r = Runner::boot(
        contract::compile(WHY).unwrap(),
        boxed(Through),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let text = |r: &Runner<Source>, id: &str| {
        let key = r.kernel().find_by_test_id(id)[0];
        let node = r.kernel().node_by_key(key).unwrap();
        node.props
            .str(exact_kernel::PropId::Text)
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(text(&r, "a"), "ok", "none while in flight");
    agent::handle(
        &r,
        r#"{"op":"prefer","faults":{"fail":"https://api.test/a","times":1}}"#,
    );
    for out in r.take_requests() {
        let outcome = match r.fault_dispatch(&out) {
            Some(Dispatch::Run(Work::Now(work))) => work(),
            _ => Outcome::Response(Response {
                status: 200,
                headers: vec![],
                body: out.request.url.clone().into_bytes(),
            }),
        };
        r.fulfill(out.ticket, outcome).unwrap();
    }
    assert_eq!(
        text(&r, "a"),
        "offline: fetch failed (driver fault): https://api.test/a []"
    );
    assert_eq!(text(&r, "b"), "error");
    assert_eq!(text(&r, "c"), "shape");
    assert_eq!(
        text(&r, "d"),
        "full SLOT_FULL",
        "a domain refusal is the exposed failure code"
    );
    let state = agent::handle(&r, r#"{"op":"state"}"#);
    assert!(
        state.contains("\"a\":\"fetch failed (driver fault): https://api.test/a\""),
        "the message is state.failed's: {state}"
    );
    assert_eq!(
        FailureCode::from_name("offline"),
        Some(FailureCode::Offline)
    );
}

#[test]
fn answer_stream_parks_events_off_settle_and_coexists_with_answer_fetch() {
    use exact_runner::{DataError, Request, Store};

    const STREAM_APP: &str = r#"
shape State
  log: string
  status: string

component App
  state active = true
  resource stream = openStream(active) as shape State
  action stop
    active = false
  view
    column
      if failed(stream)
        text "failed" testId="log"
        text "failed" testId="status"
      else if pending(stream)
        text "waiting" testId="log"
        text "loading" testId="status"
      else
        text stream.log testId="log"
        text stream.status testId="status"
      button "Stop" press=stop testId="stop"
"#;

    struct StreamSource {
        log: String,
    }
    impl DataSource for StreamSource {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::Unavailable("request".into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
            if args[0] == Value::Bool(false) {
                return Ok(Answer::Now(Value::record(vec![
                    Value::str(&self.log),
                    Value::str("stopped"),
                ])));
            }
            let mut req = Request::get("https://stream-fixture.invalid/chat");
            req.stream = true;
            req.method = "POST".into();
            req.stream_open = true;
            req.idle_timeout_ms = Some(200);
            Ok(Answer::Later(req))
        }
        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            outcome: Outcome,
        ) -> Result<Answer, DataError> {
            match outcome {
                Outcome::Message(m) => {
                    if let Some(head) = m.head {
                        let req_id = head
                            .headers
                            .iter()
                            .find(|(k, _)| k == "x-stream-id")
                            .map(|(_, v)| v.as_str())
                            .unwrap_or("");
                        self.log
                            .push_str(&format!("[open:{}:{req_id}]", head.status));
                    } else {
                        self.log.push_str(&format!("[{}:{}]", m.event, m.data));
                    }
                    Ok(Answer::Now(Value::record(vec![
                        Value::str(&self.log),
                        Value::str("streaming"),
                    ])))
                }
                Outcome::Messages(ms) => {
                    for m in ms {
                        self.log.push_str(&format!("[{}:{}]", m.event, m.data));
                    }
                    Ok(Answer::Now(Value::record(vec![
                        Value::str(&self.log),
                        Value::str("streaming"),
                    ])))
                }
                Outcome::Failed { kind, message } => {
                    self.log.push_str(&format!("[end:{kind:?}:{message}]"));
                    Ok(Answer::Now(Value::record(vec![
                        Value::str(&self.log),
                        Value::str("ended"),
                    ])))
                }
                Outcome::Response(r) => Ok(Answer::Now(Value::record(vec![
                    Value::str(String::from_utf8_lossy(&r.body).as_ref()),
                    Value::str(&format!("http:{}", r.status)),
                ]))),
                _ => unreachable!(),
            }
        }
    }

    let mut r = Runner::boot(
        contract::compile(STREAM_APP).unwrap(),
        boxed(StreamSource { log: String::new() }),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();

    // Arm BOTH `answer fetch` and `answer stream` on the exact same prefix: they must stay isolated.
    agent::handle(
        &r,
        r#"{"op":"prefer","faults":{"answer":"https://stream-fixture.invalid/chat","response":{"status":200,"headers":{},"body":"fetch-only"}}}"#,
    );
    let armed = agent::handle(
        &r,
        r#"{"op":"prefer","faults":{"answerStream":"https://stream-fixture.invalid/chat","method":"POST","stream":{"status":200,"headers":{"x-stream-id":"s-1"},"mode":"events","steps":[{"kind":"event","name":"chunk","data":"Hello","afterMs":0},{"kind":"event","name":"chunk","data":" world","afterMs":100},{"kind":"end","afterMs":150}]}}}"#,
    );
    assert!(!armed.contains("\"error\""), "{armed}");

    // Dispatch boot stream request through `fault_dispatch` (with a grants stub).
    let reqs = r.take_requests();
    assert_eq!(reqs.len(), 1);
    let ticket = reqs[0].ticket;
    // Exercise Web Wasm handoff directly (same schedule mechanics as native `fault_dispatch`).
    let handoff = serde_json::json!({
        "prefix": "https://stream-fixture.invalid/chat",
        "method": "POST",
        "url": "https://stream-fixture.invalid/chat",
        "streamOpen": true,
        "idleTimeoutMs": 200,
        "maxResponseBytes": 1048576,
        "stream": {
            "status": 200,
            "headers": { "x-stream-id": "s-1" },
            "mode": "events",
            "steps": [
                { "kind": "event", "name": "chunk", "data": "Hello", "afterMs": 0 },
                { "kind": "event", "name": "chunk", "data": " world", "afterMs": 100 },
                { "kind": "end", "afterMs": 150 }
            ]
        }
    });
    let receipts = exact_runner::agent::open_web_stream_fixture(
        &mut r,
        ticket,
        serde_json::to_vec(&handoff).unwrap().as_slice(),
    )
    .unwrap();
    assert_eq!(
        receipts.len(),
        2,
        "open head and after-0 chunk land as distinct turns"
    );

    let text = |r: &Runner<Source>, id: &str| {
        let key = r.kernel().find_by_test_id(id)[0];
        r.kernel()
            .node_by_key(key)
            .unwrap()
            .props
            .str(exact_kernel::PropId::Text)
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(text(&r, "log"), "[open:200:s-1][chunk:Hello]");
    assert_eq!(text(&r, "status"), "streaming");
    assert!(
        !r.has_pending(),
        "clock data must not block on parked stream events"
    );
    assert_eq!(
        r.data_timer_due_ms(),
        None,
        "clock settle must NOT advance past parked stream events"
    );

    // Explicit `clock +100` delivers the second chunk; stream remains open before `end` at 150 ms.
    r.advance(100.0).unwrap();
    assert_eq!(text(&r, "log"), "[open:200:s-1][chunk:Hello][chunk: world]");
    assert_eq!(text(&r, "status"), "streaming");

    // Explicit `clock +150` delivers clean EOF (`end`).
    r.advance(150.0).unwrap();
    assert_eq!(
        text(&r, "log"),
        "[open:200:s-1][chunk:Hello][chunk: world][end:Network:the event stream ended]"
    );
    assert_eq!(text(&r, "status"), "ended");

    // `pass stream` disarms only the stream rule, leaving `answer fetch` intact.
    let passed = agent::handle(
        &r,
        r#"{"op":"prefer","faults":{"passStream":"https://stream-fixture.invalid/chat"}}"#,
    );
    assert!(!passed.contains("\"error\""), "{passed}");
    let state = agent::handle(&r, r#"{"op":"state"}"#);
    assert!(state.contains("\"family\":\"stream\""), "{state}");
    assert!(state.contains("\"armed\":false"), "{state}");
    assert!(state.contains("\"body\":\"fetch-only\""), "{state}");
}

#[test]
fn answer_stream_virtual_idle_timeout_and_fail_step_record_failure_hits() {
    use exact_runner::{DataError, Request, Store};

    const APP: &str = r#"
shape State
  text: string

component App
  resource s = openStream() as shape State
  view
    column
      if failed(s)
        text "failed" testId="out"
      else if pending(s)
        text "loading" testId="out"
      else
        text s.text testId="out"
"#;

    struct S;
    impl DataSource for S {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::Unavailable("req".into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            let mut req = Request::get("https://stream-fixture.invalid/idle");
            req.stream = true;
            req.idle_timeout_ms = Some(100);
            Ok(Answer::Later(req))
        }
        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            outcome: Outcome,
        ) -> Result<Answer, DataError> {
            match outcome {
                Outcome::Message(m) => Ok(Answer::Now(Value::record(vec![Value::str(&m.data)]))),
                Outcome::Failed { kind, message } => {
                    Ok(Answer::Now(Value::record(vec![Value::str(&format!(
                        "{kind:?}:{message}"
                    ))])))
                }
                _ => unreachable!(),
            }
        }
    }

    let mut r = Runner::boot(
        contract::compile(APP).unwrap(),
        boxed(S),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.faults()
        .answer_stream_method(
            "https://stream-fixture.invalid/idle",
            None,
            &serde_json::json!({
                "status": 200,
                "headers": {},
                "mode": "events",
                "steps": [
                    { "kind": "event", "data": "first", "afterMs": 0 },
                    { "kind": "event", "data": "too-late", "afterMs": 250 }
                ]
            }),
        )
        .unwrap();
    let req = r.take_requests().pop().unwrap();
    // Gap between after 0 and after 250 exceeds idleTimeoutMs=100 -> virtual Timeout fires at 100 ms.
    let handoff = serde_json::json!({
        "prefix": "https://stream-fixture.invalid/idle",
        "stream": {
            "status": 200,
            "headers": {},
            "mode": "events",
            "steps": [
                { "kind": "event", "data": "first", "afterMs": 0 },
                { "kind": "event", "data": "too-late", "afterMs": 250 }
            ]
        }
    });
    exact_runner::agent::open_web_stream_fixture(
        &mut r,
        req.ticket,
        serde_json::to_vec(&handoff).unwrap().as_slice(),
    )
    .unwrap();
    let read_out = |r: &Runner<Source>| {
        let key = r.kernel().find_by_test_id("out")[0];
        r.kernel()
            .node_by_key(key)
            .unwrap()
            .props
            .str(exact_kernel::PropId::Text)
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(read_out(&r), "first");
    r.advance(100.0).unwrap();
    assert_eq!(read_out(&r), "Timeout:nothing arrived for 100 ms");
    // Virtual idle timeout MUST record a failure hit in both native faults and Wasm failure drain.
    let faults_json: serde_json::Value = serde_json::from_str(&r.faults().json()).unwrap();
    assert_eq!(faults_json[0]["failureHits"].as_u64(), Some(1));
    assert_eq!(r.take_stream_fixture_failures().len(), 1);
}

#[test]
fn answer_stream_rearm_credits_exact_key_and_buffered_bytes_honors_fail() {
    const APP: &str = r#"shape State
  text: string
component App
  resource s = openStream() as shape State
  view
    column
      if failed(s)
        text "failed" testId="out"
      else if pending(s)
        text "loading" testId="out"
      else
        text s.text testId="out"
"#;

    struct S;
    impl DataSource for S {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::Unavailable("req".into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            let mut req = Request::get("https://stream-fixture.invalid/bytes");
            req.stream = true;
            req.bytes = false;
            Ok(Answer::Later(req))
        }
        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            outcome: Outcome,
        ) -> Result<Answer, DataError> {
            match outcome {
                Outcome::Response(resp) => {
                    Ok(Answer::Now(Value::record(vec![Value::str(&format!(
                        "resp:{}:{}",
                        resp.status,
                        String::from_utf8_lossy(&resp.body)
                    ))])))
                }
                Outcome::Failed { kind, message } => {
                    Ok(Answer::Now(Value::record(vec![Value::str(&format!(
                        "{kind:?}:{message}"
                    ))])))
                }
                _ => unreachable!(),
            }
        }
    }

    let mut r = Runner::boot(
        contract::compile(APP).unwrap(),
        boxed(S),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let schedule = serde_json::json!({
        "status": 200,
        "headers": {},
        "mode": "bytes",
        "steps": [
            { "kind": "bytes", "base64": "YQ==", "afterMs": 50 },
            { "kind": "fail", "message": "truncated", "afterMs": 120 }
        ]
    });
    r.faults()
        .answer_stream_method(
            "https://stream-fixture.invalid/bytes",
            Some("GET"),
            &schedule,
        )
        .unwrap();
    let req = r.take_requests().pop().unwrap();
    let handoff = serde_json::json!({
        "prefix": "https://stream-fixture.invalid/bytes",
        "method": "GET",
        "stream": schedule,
    });
    exact_runner::agent::open_web_stream_fixture(
        &mut r,
        req.ticket,
        serde_json::to_vec(&handoff).unwrap().as_slice(),
    )
    .unwrap();
    let read_out = |r: &Runner<Source>| {
        let key = r.kernel().find_by_test_id("out")[0];
        r.kernel()
            .node_by_key(key)
            .unwrap()
            .props
            .str(exact_kernel::PropId::Text)
            .unwrap_or("")
            .to_string()
    };
    // Must stay loading (NOT return an immediate 200) while chunks buffer on the virtual clock.
    assert_eq!(read_out(&r), "loading");
    // Re-arm the same (prefix, method) while in flight AND add a different method on same prefix.
    r.faults()
        .answer_stream_method(
            "https://stream-fixture.invalid/bytes",
            Some("GET"),
            &serde_json::json!({ "status": 200, "headers": {}, "mode": "bytes", "steps": [{ "kind": "end", "afterMs": 500 }] }),
        )
        .unwrap();
    r.faults()
        .answer_stream_method(
            "https://stream-fixture.invalid/bytes",
            Some("POST"),
            &serde_json::json!({ "status": 200, "headers": {}, "mode": "bytes", "steps": [{ "kind": "end", "afterMs": 500 }] }),
        )
        .unwrap();
    r.advance(50.0).unwrap();
    assert_eq!(read_out(&r), "loading");
    r.advance(120.0).unwrap();
    assert_eq!(read_out(&r), "Network:truncated");
    let faults: serde_json::Value = serde_json::from_str(&r.faults().json()).unwrap();
    let arr = faults.as_array().unwrap();
    assert_eq!(
        arr.iter()
            .find(|f| f["method"].as_str() == Some("GET"))
            .unwrap()["failureHits"]
            .as_u64(),
        Some(1)
    );
    assert_eq!(
        arr.iter()
            .find(|f| f["method"].as_str() == Some("POST"))
            .unwrap()["failureHits"]
            .as_u64(),
        Some(0)
    );
}

#[test]
fn native_buffered_bytes_and_delayed_429_and_live_upload_waited() {
    use exact_runner::{DataError, Request, Store, Work};
    const APP: &str = r#"
shape Out
  text: string

component App
  state mode = "bytes"
  resource res = load(mode) as shape Out
  action switch
    mode = "delay"
  action to_upload
    mode = "upload"
  view
    column
      if failed(res)
        text "failed" testId="out"
      else if pending(res)
        text "loading" testId="out"
      else
        text res.text testId="out"
      button "Switch" press=switch testId="sw"
      button "Upload" press=to_upload testId="up"
"#;

    struct S;
    impl DataSource for S {
        fn grants(&self) -> &str {
            "net.fetch https://stream-fixture.invalid"
        }
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::Unavailable("request".into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
            let mode = args[0].as_str().unwrap_or("");
            let mut req = match mode {
                "bytes" => Request::get("https://stream-fixture.invalid/buf-bytes"),
                "delay" => Request::get("https://stream-fixture.invalid/delay-429"),
                _ => {
                    let mut u = Request::get("https://stream-fixture.invalid/upload");
                    u.method = "POST".into();
                    u.body = b"hi".to_vec();
                    u.upload_progress = true;
                    u
                }
            };
            req.stream = true;
            req.bytes = false;
            req.http = exact_runner::HttpScheduling::Independent {
                max_response_bytes: 1024 * 1024,
            };
            Ok(Answer::Later(req))
        }
        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            outcome: Outcome,
        ) -> Result<Answer, DataError> {
            match outcome {
                Outcome::Response(resp) => Ok(Answer::Now(Value::record(vec![Value::str(
                    &format!("{}:{}", resp.status, String::from_utf8_lossy(&resp.body)),
                )]))),
                Outcome::Failed { kind, message } => {
                    Ok(Answer::Now(Value::record(vec![Value::str(&format!(
                        "{kind:?}:{message}"
                    ))])))
                }
                _ => Ok(Answer::Now(Value::record(vec![Value::str("msg")]))),
            }
        }
    }

    let mut r = Runner::boot(
        contract::compile(APP).unwrap(),
        boxed(S),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.faults()
        .answer_stream_method(
            "https://stream-fixture.invalid/buf-bytes",
            Some("GET"),
            &serde_json::json!({
                "status": 200,
                "headers": {},
                "mode": "bytes",
                "steps": [
                    { "kind": "bytes", "base64": "SGVs", "afterMs": 50 },
                    { "kind": "bytes", "base64": "bG8=", "afterMs": 80 },
                    { "kind": "end", "afterMs": 100 }
                ]
            }),
        )
        .unwrap();
    r.faults()
        .answer_stream_method(
            "https://stream-fixture.invalid/delay-429",
            Some("GET"),
            &serde_json::json!({ "status": 429, "headers": {}, "body": "wait", "afterMs": 200 }),
        )
        .unwrap();

    // 1. Native `buffered_mode` byte schedule via `fault_dispatch` + `Work::Later`:
    // must NOT abort on `Reply::drop`, must clear `has_pending()` for `clock data`,
    // and must deliver the concatenated 200 response at 100 ms.
    let req1 = r.take_requests().remove(0);
    let work1 = match r.fault_dispatch(&req1) {
        Some(exact_runner::Dispatch::Run(Work::Later(w))) => w,
        _ => panic!("expected Work::Later"),
    };
    let outcomes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let o_c = outcomes.clone();
    work1(exact_runner::Reply::stream(move |o| {
        o_c.lock().unwrap().push(o);
    }));
    for o in outcomes.lock().unwrap().drain(..) {
        r.fulfill(req1.ticket, o).unwrap();
    }
    assert!(
        !r.has_pending(),
        "parked buffered byte fixture unblocks clock data"
    );
    assert!(
        r.holds(req1.ticket),
        "buffered byte fixture must not abort on Reply drop"
    );
    r.advance(100.0).unwrap();
    let read_out = |r: &Runner<Source>| {
        let key = r.kernel().find_by_test_id("out")[0];
        r.kernel()
            .node_by_key(key)
            .unwrap()
            .props
            .str(exact_kernel::PropId::Text)
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(read_out(&r), "200:Hello");
    assert!(
        r.take_stream_fixture_failures().is_empty(),
        "native sessions must never accumulate Wasm failure notifications"
    );

    // 2. Native delayed buffered 429 aborted before deadline leaves hits=0 and failureHits=0.
    let sw_key = r.kernel().find_by_test_id("sw")[0];
    let sw = r.kernel().node_by_key(sw_key).unwrap().id;
    r.dispatch(sw, exact_runner::Event::Press).unwrap();
    let req2 = r.take_requests().remove(0);
    let work2 = match r.fault_dispatch(&req2) {
        Some(exact_runner::Dispatch::Run(Work::HttpResponse(w))) => w,
        _ => panic!("expected Work::HttpResponse"),
    };
    r.fulfill(req2.ticket, work2()).unwrap();
    assert!(r.holds(req2.ticket), "delayed 429 must park until 300 ms");
    // Switch to upload before 300 ms -> drops req2 before delivery!
    let up_key = r.kernel().find_by_test_id("up")[0];
    let up = r.kernel().node_by_key(up_key).unwrap().id;
    r.dispatch(up, exact_runner::Event::Press).unwrap();
    r.advance(400.0).unwrap();
    let faults: serde_json::Value = serde_json::from_str(&r.faults().json()).unwrap();
    let delay_entry = faults
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["prefix"].as_str() == Some("https://stream-fixture.invalid/delay-429"))
        .unwrap();
    assert_eq!(delay_entry["hits"].as_u64(), Some(0));
    assert_eq!(delay_entry["failureHits"].as_u64(), Some(0));

    // 3. Live upload receiving `exact:sending` remains waited on by clocks (no regression).
    let req3 = r.take_requests().remove(0);
    assert!(r.fault_dispatch(&req3).is_none());
    assert!(r.has_pending());
    r.fulfill(
        req3.ticket,
        Outcome::Message(exact_runner::Message {
            event: exact_runner::UPLOAD_SENDING.into(),
            ..Default::default()
        }),
    )
    .unwrap();
    assert!(
        r.has_pending(),
        "live upload must remain waited after exact:sending"
    );
}
