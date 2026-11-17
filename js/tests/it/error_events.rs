//! LLP 1175: a data module hears its global's error events natively, as a
//! page does. The shared fixture (`tests/fixtures/error-events.ts`) runs here
//! against what `error-events.json` expects of every host, and of this one's
//! journal and platform log (`system`); host/web-js/error-events.test.mjs
//! runs it on the web JS target.
#![cfg(exact_js_engine)]

use exact_js::Module;
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{DataSource, Outcome, Response, Runner};

const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/error-events.hbc"));
const EXPECTED: &str = include_str!("../fixtures/error-events.json");

const SRC: &str = r#"
component App
  mutation result as shape string
  action go(kind: string)
    send result = run(kind)
  view
    text "errors"
"#;

fn runner() -> Runner<Module> {
    let plan = contract::compile(SRC).expect("the fixture's plan compiles");
    let mut module =
        Module::loaded(HBC.to_vec(), "test.error-events", "").expect("the fixture loads");
    module.set_budget_ms(f64::INFINITY);
    module.bind(&plan);
    Runner::boot(
        plan,
        module,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

/// Where a listener's report goes in these tests.
const URL: &str = "https://example.test/report";

/// Run the source on `kind`, its timers included: its answer, or `None` for one that failed.
fn go<D: DataSource>(r: &mut Runner<D>, kind: &str) -> Option<String> {
    go_reporting(r, kind).0
}

/// `go`, with the bodies of the reports made, each answered 204.
fn go_reporting<D: DataSource>(r: &mut Runner<D>, kind: &str) -> (Option<String>, Vec<String>) {
    r.act("go", vec![Value::str(kind)]).unwrap();
    let mut reports = pump(r);
    let to = r.now_ms() + 1.;
    r.advance_until_request(to);
    reports.extend(pump(r));
    (answer(r), reports)
}

fn answer<D: DataSource>(r: &Runner<D>) -> Option<String> {
    match r.slot("result") {
        Some(Value::Option(Some(v))) => v.as_str().map(str::to_owned),
        _ => None,
    }
}

fn response(status: u16) -> Outcome {
    Outcome::Response(Response {
        status,
        headers: vec![],
        body: Vec::new(),
    })
}

/// Run what the runner hands out until it hands out nothing: each
/// continuation now, each report (module work) answered 204. The reports' bodies.
fn pump<D: DataSource>(r: &mut Runner<D>) -> Vec<String> {
    let mut reports = Vec::new();
    loop {
        let requests = r.take_requests();
        if requests.is_empty() {
            return reports;
        }
        for request in requests {
            match request.request.continuation {
                Some(token) => {
                    if let exact_runner::Dispatch::Run(exact_runner::Work::Now(run)) =
                        crate::runnable(r.dispatch_work(token))
                    {
                        r.fulfill(request.ticket, run()).unwrap();
                    }
                }
                None => {
                    assert_eq!(request.target, "module work", "{request:?}");
                    reports.push(String::from_utf8(request.request.body.clone()).unwrap());
                    r.fulfill(request.ticket, response(204)).unwrap();
                }
            }
        }
    }
}

/// The journal's lines since `from`, without their time.
fn journal_since<D: DataSource>(r: &Runner<D>, from: usize) -> Vec<String> {
    r.journal()
        .skip(from)
        .filter_map(|l| l.split_once(' ').map(|(_, l)| l.to_string()))
        .collect()
}

/// A sink for the host's platform log that keeps the first line of each
/// `console.error` line handed to it, not the stack frames under it.
fn system_log<D: DataSource>(r: &mut Runner<D>) -> std::rc::Rc<std::cell::RefCell<Vec<String>>> {
    let written = std::rc::Rc::<std::cell::RefCell<Vec<String>>>::default();
    let into = written.clone();
    r.set_console(move |method, line| {
        if method == exact_runner::Console::Error && !line.starts_with(char::is_whitespace) {
            into.borrow_mut().push(line.to_string());
        }
    });
    written
}

#[test]
fn the_shared_fixture_hears_what_a_page_hears() {
    let expected: serde_json::Value = serde_json::from_str(EXPECTED).unwrap();
    let mut r = runner();
    let system = system_log(&mut r);
    for scenario in expected["scenarios"].as_array().unwrap() {
        let kind = scenario["run"].as_str().unwrap();
        let from = r.journal().count();
        system.borrow_mut().clear();
        let run = match scenario["url"].as_bool() {
            Some(true) => format!("{kind} {URL}"),
            _ => kind.to_owned(),
        };
        let (answered, reports) = go_reporting(&mut r, &run);
        let expected = scenario["reports"]
            .as_array()
            .map_or(vec![], |r| r.to_vec());
        assert_eq!(
            reports,
            (expected.iter())
                .map(|r| r.as_str().unwrap())
                .collect::<Vec<_>>(),
            "{kind}'s reports"
        );
        if scenario["fails"].as_bool() == Some(true) {
            assert_ne!(answered.as_deref(), Some(kind), "{kind} fails");
        } else {
            assert_eq!(answered.as_deref(), Some(kind), "{kind} answers");
        }
        let journal = journal_since(&r, from);
        let logged = system.take();
        assert_eq!(
            logged,
            (scenario["system"].as_array().unwrap().iter())
                .map(|l| l.as_str().unwrap())
                .collect::<Vec<_>>(),
            "{kind}: the platform's log"
        );
        let heard: serde_json::Value =
            serde_json::from_str(&go(&mut r, "log").expect("the log answers")).unwrap();
        let expected = &scenario["heard"];
        let expected = expected.get("native").unwrap_or(expected);
        assert_eq!(&heard, expected, "what the listeners heard of {kind}");
        for line in scenario["journal"]["native"].as_array().unwrap() {
            let line = line.as_str().unwrap();
            assert!(
                journal.iter().any(|l| l == line),
                "{kind}: {line:?} in {journal:#?}"
            );
        }
        let reports = |l: &&String| l.contains("Uncaught") || l.contains("unhandled rejection");
        assert_eq!(
            journal.iter().filter(reports).count(),
            scenario["journal"]["native"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|l| {
                    let l = l.as_str().unwrap();
                    l.contains("Uncaught") || l.contains("unhandled rejection")
                })
                .count(),
            "{kind}: one journal line per report: {journal:#?}"
        );
    }
}

/// D5: a rejection whose listener cancels it is journaled, and the
/// platform's log has nothing, as a page's console then prints nothing.
#[test]
fn a_cancelled_rejection_reaches_only_the_journal() {
    let mut r = runner();
    let system = system_log(&mut r);
    let from = r.journal().count();
    assert_eq!(go(&mut r, "quiet").as_deref(), Some("quiet"));
    let journal = journal_since(&r, from);
    assert!(
        journal
            .iter()
            .any(|l| l == "data: unhandled rejection: quieted"),
        "{journal:#?}"
    );
    assert_eq!(system.take(), Vec::<String>::new());
}

/// D1: what else the events binding gives a data module, and what it does
/// not: no `self` or `navigator`, no clock in `timeStamp`, an `AbortSignal`
/// that is an `EventTarget`, `onerror` assignable after hardening, and
/// Exact's rejection tracker the only one.
#[test]
fn the_events_binding_adds_the_web_s_names_and_no_clock() {
    let mut r = runner();
    assert_eq!(
        go(&mut r, "probe").as_deref(),
        Some("object,function,function,object"),
        "the reporter's probe (#519) answers as on the web"
    );
    let names: serde_json::Value =
        serde_json::from_str(&go(&mut r, "names").expect("names answers")).unwrap();
    assert_eq!(
        names.as_array().unwrap()[..5],
        serde_json::json!(["undefined", "undefined", 0, true, true])
            .as_array()
            .unwrap()[..]
    );
    assert!(
        names[5].as_str().unwrap().starts_with("TypeError: "),
        "the tracker cannot be replaced: {names}"
    );
    assert_eq!(go(&mut r, "rejection").as_deref(), Some("rejection"));
    assert_eq!(
        go(&mut r, "log").as_deref(),
        Some(
            r#"[["unhandledrejection","Error: lost",0,true],["onunhandledrejection","Error: lost"]]"#
        ),
        "Exact's tracker still dispatches"
    );
}

/// D2: what the module's evaluation throws is reported, an `error` event its
/// listeners hear, before the load fails as it did. D6: the listener's fetch
/// is refused, as any fetch in an evaluation is.
#[test]
fn an_evaluation_s_throw_is_heard_before_the_load_fails() {
    let mut module = Module::new(
        include_bytes!(concat!(env!("OUT_DIR"), "/error-events-load.hbc")).to_vec(),
        "test.error-events-load",
        "",
    );
    let error = module.load().expect_err("the evaluation throws");
    assert!(error.contains("at load"), "{error}");
    let logs = module.take_logs();
    let heard = logs.iter().position(|l| l == "heard at load");
    let reported = logs.iter().position(|l| l == "Uncaught Error: at load");
    assert!(heard.is_some() && heard < reported, "{logs:#?}");
    assert!(
        (logs.iter()).any(|l| l.contains("during the module's evaluation: it was never run")),
        "{logs:#?}"
    );
    assert!(module.take_reports().is_empty());
}

fn report_runner() -> Runner<Module> {
    let mut r = runner();
    assert_eq!(go(&mut r, "log").as_deref(), Some("[]"));
    r
}

/// D3: a listener's fetch goes out as module work, with the app's reach
/// here. No answer waits for it: the answer that called `reportError` has
/// its value first, and the report is in flight (so `clock settle` waits for
/// it) until its reply reaches the listener's continuation.
#[test]
fn a_listener_s_fetch_goes_out_as_module_work_that_no_answer_waits_for() {
    let mut r = report_runner();
    r.act("go", vec![Value::str(&format!("send {URL}"))])
        .unwrap();
    let out = r.take_requests();
    assert_eq!(out.len(), 1, "{out:?}");
    let report = &out[0];
    assert_eq!(report.target, "module work");
    assert_eq!(report.request.url, URL);
    assert_eq!(report.request.method, "POST");
    assert_eq!(report.request.body, b"Error: send");
    assert_eq!(report.request.reach.origin, exact_runner::CallOrigin::App);
    assert_eq!(
        answer(&r).as_deref(),
        Some("send"),
        "the answer replied first"
    );
    assert!(
        r.in_flight().iter().any(|(_, t)| *t == report.ticket),
        "{:?}",
        r.in_flight()
    );
    r.fulfill(report.ticket, response(204)).unwrap();
    assert!(r.take_requests().is_empty());
    assert!(r.in_flight().is_empty(), "{:?}", r.in_flight());
    let heard = go(&mut r, "log").unwrap();
    assert!(heard.ends_with(r#"["reply",204]]"#), "{heard}");
}

/// D3: a report is one round. A fetch after the dispatch (after an await, in
/// a microtask, in the reply's continuation) is refused with a line, and its
/// refusal is not dispatched again (D4).
#[test]
fn a_fetch_after_the_dispatch_is_refused() {
    let mut r = report_runner();
    for (kind, said) in [
        ("continuation", "or in an error event's listener itself"),
        ("deferred", "or in an error event's listener itself"),
        ("again", "after an error event's listener returned"),
    ] {
        let from = r.journal().count();
        let (answered, reports) = go_reporting(&mut r, &format!("{kind} {URL}"));
        assert_eq!(answered.as_deref(), Some(kind));
        assert_eq!(reports.len(), usize::from(kind == "again"), "{kind}");
        let heard = go(&mut r, "log").unwrap();
        assert!(heard.contains(said), "{kind}: {heard}");
        assert!(!heard.contains("unhandledrejection"), "{kind}: {heard}");
        let journal = journal_since(&r, from);
        assert!(
            journal.iter().any(|l| l.contains(said)),
            "{kind}: {journal:#?}"
        );
    }
}

/// D3: the dispatch ends when `reportError` returns, and the answer's own
/// fetch after it is the answer's, as before.
#[test]
fn a_fetch_after_report_error_returns_is_the_answer_s() {
    let mut r = report_runner();
    r.act("go", vec![Value::str(&format!("answer-fetch {URL}"))])
        .unwrap();
    let out = r.take_requests();
    assert_eq!(out.len(), 1, "{out:?}");
    assert_ne!(out[0].target, "module work");
    assert_eq!(
        answer(&r).as_deref(),
        Some("[]"),
        "the answer waits for its fetch"
    );
    r.fulfill(out[0].ticket, response(200)).unwrap();
    pump(&mut r);
    assert_eq!(answer(&r).as_deref(), Some("answer-fetch 200"));
}

/// D3: at most 16 reports are in flight. The next is refused with a line,
/// and that refusal is not dispatched again (D4).
#[test]
fn the_cap_s_refusal_is_not_dispatched_again() {
    let mut r = report_runner();
    let from = r.journal().count();
    r.act("go", vec![Value::str(&format!("cap {URL}"))])
        .unwrap();
    let out = r.take_requests();
    assert_eq!(out.len(), 16, "{out:?}");
    assert!(out.iter().all(|o| o.target == "module work"));
    for o in &out {
        r.fulfill(o.ticket, response(204)).unwrap();
    }
    pump(&mut r);
    let heard = go(&mut r, "log").unwrap();
    assert_eq!(
        heard.matches("16 reports are in flight").count(),
        1,
        "{heard}"
    );
    assert!(!heard.contains("unhandledrejection"), "{heard}");
    let journal = journal_since(&r, from);
    assert!(
        journal
            .iter()
            .any(|l| l.contains("16 reports are in flight")),
        "{journal:#?}"
    );
}

/// D4: a report's refusal that no code catches is journaled, never
/// dispatched again; and its options are read once, so a getter cannot slip
/// a refused option past the check.
#[test]
fn a_report_s_refusal_is_journaled_and_its_options_read_once() {
    let mut r = report_runner();
    let from = r.journal().count();
    assert_eq!(
        go(&mut r, &format!("process {URL}")).as_deref(),
        Some("process")
    );
    let heard = go(&mut r, "log").unwrap();
    assert!(!heard.contains("unhandledrejection"), "{heard}");
    let journal = journal_since(&r, from);
    assert!(
        journal.iter().any(|l| l.starts_with("data: unhandled rejection: fetch() in an error event's listener sends a plain HTTP request")),
        "{journal:#?}"
    );
    r.act("go", vec![Value::str(&format!("flip {URL}"))])
        .unwrap();
    let out = r.take_requests();
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].target, "module work");
    assert_eq!(out[0].request.body_from, None);
    r.fulfill(out[0].ticket, response(204)).unwrap();
}
