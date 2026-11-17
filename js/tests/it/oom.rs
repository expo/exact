//! A terminal heap error fails the module's work without ending its host.
#![cfg(exact_js_engine)]

use exact_js::Module;
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    Answer, DataSource, Dispatch, Message, Outcome, Placement, RequestOut, Runner, Store, Work,
};

const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/oom.hbc"));
const GRANTS: &str = "net.fetch https://oom.test\nnet.websocket wss://oom.test\nfs.read app:/data\nfs.write app:/data\nsqlite.open app:/data/notes.db\n";
const ERROR: &str = "held more than the 64 MiB a data module's heap may hold";
const SRC: &str = r#"
component App
  resource waiting = pending() as shape string
  resource sleeping = sleep() as shape string
  resource streaming = feed() as shape string
  resource healthy = other() as shape string
  mutation output as shape string
  mutation probe as shape string
  action crash
    send output = oom()
  action refuseAfterCrash
    send output = oom()
    send probe = refuse(true)
  action timerCrash
    send output = timerOom()
  action count
    send output = counter()
  view
    text "alive"
"#;

fn module() -> Module {
    let mut m = Module::new(HBC.to_vec(), "test.oom", GRANTS);
    m.set_budget_ms(f64::INFINITY);
    m.bind(&contract::compile(SRC).unwrap());
    m.load().unwrap();
    m
}

thread_local! {
    static HELD: std::cell::RefCell<std::collections::HashMap<u64, u64>> = Default::default();
}
fn pump<D: DataSource>(r: &mut Runner<D>) -> Vec<RequestOut> {
    HELD.with(|held| {
        let mut held = held.borrow_mut();
        let mut fetches = Vec::new();
        for _ in 0..100 {
            let mut work = Vec::new();
            for request in r.take_requests() {
                let Some(token) = request.request.continuation else {
                    fetches.push(request);
                    continue;
                };
                match r.dispatch_work(token) {
                    Dispatch::Held => {
                        held.insert(token, request.ticket);
                    }
                    dispatch => work.push((request.ticket, dispatch)),
                }
            }
            for (token, dispatch) in r.release_work() {
                if let Some(ticket) = held.remove(&token) {
                    work.push((ticket, dispatch));
                }
            }
            let mut worked = false;
            for (ticket, dispatch) in work {
                let outcome = match dispatch {
                    Dispatch::Again => Dispatch::again_outcome(),
                    Dispatch::Run(Work::Now(work)) => work(),
                    Dispatch::Run(Work::Later(work)) => {
                        let (tx, rx) = std::sync::mpsc::channel();
                        work(exact_runner::Reply::new(move |outcome| {
                            tx.send(outcome).unwrap();
                        }));
                        rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap()
                    }
                    Dispatch::Missing => continue,
                    _ => panic!("released work must run"),
                };
                r.fulfill(ticket, outcome).unwrap();
                worked = true;
            }
            while let Some((ticket, outcome)) = r.take_aborted_answer() {
                r.fulfill(ticket, outcome).unwrap();
                worked = true;
            }
            held.retain(|_, ticket| r.holds(*ticket));
            if !worked {
                return fetches;
            }
        }
        panic!("the runner did not settle");
    })
}

#[derive(Default)]
struct Other;
impl DataSource for Other {
    fn app_id(&self) -> &str {
        "test.oom"
    }
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
        Ok(Value::str("other module"))
    }
}

fn runner(placement: Placement) -> Runner<exact_data::Mixed<exact_data::Placed<Module>, Other>> {
    let mut source = exact_data::Mixed::new(
        module().placed(placement),
        Other,
        &[
            "pending", "sleep", "feed", "oom", "timerOom", "counter", "refuse",
        ],
        &["other"],
    )
    .unwrap();
    source.activate().unwrap();
    Runner::boot(
        contract::compile(SRC).unwrap(),
        source,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn retired<D: DataSource>(r: &mut Runner<D>, old: &[RequestOut]) {
    pump(r);
    let failed = r.failed_resources();
    for name in ["waiting", "sleeping", "streaming"] {
        assert!(
            failed
                .iter()
                .any(|(n, error)| *n == name && error.contains(ERROR)),
            "{name}: {failed:?}"
        );
    }
    assert!(old.iter().all(|q| !r.holds(q.ticket)));
    assert_eq!(r.data_timer_due_ms(), None);
    assert_eq!(r.resource("healthy"), Some(&Value::str("other module")));
    r.act("count", vec![]).unwrap();
    pump(r);
    assert_eq!(r.slot("output"), Some(&Value::some(Value::str("1"))));
    // Even after replacement, late I/O cannot resume the old module.
    for request in old {
        r.fulfill(request.ticket, Dispatch::again_outcome())
            .unwrap();
    }
    r.act("count", vec![]).unwrap();
    pump(r);
    assert_eq!(r.slot("output"), Some(&Value::some(Value::str("2"))));
}

#[test]
#[cfg_attr(
    not(exact_js_oom),
    ignore = "requires an exception-on-OOM Hermes receipt"
)]
fn heap_oom_fails_fetch_stream_and_timer_in_both_placements() {
    for placement in [Placement::Main, Placement::Worker] {
        let mut r = runner(placement);
        let old = pump(&mut r);
        assert_eq!(old.len(), 2);
        r.act("crash", vec![]).unwrap();
        retired(&mut r, &old);
    }
}

#[test]
#[cfg_attr(
    not(exact_js_oom),
    ignore = "requires an exception-on-OOM Hermes receipt"
)]
fn heap_oom_from_a_timer_retires_every_outstanding_request() {
    let mut r = runner(Placement::Main);
    let old = pump(&mut r);
    r.act("timerCrash", vec![]).unwrap();
    pump(&mut r);
    let advanced = r.advance_until_request(10.);
    assert!(advanced.error.is_none());
    retired(&mut r, &old);
}

#[test]
#[cfg_attr(
    not(exact_js_oom),
    ignore = "requires an exception-on-OOM Hermes receipt"
)]
fn heap_oom_from_a_stream_mapper_retires_every_outstanding_request() {
    let mut r = runner(Placement::Main);
    let old = pump(&mut r);
    let stream = old
        .iter()
        .find(|q| q.request.url.starts_with("wss:"))
        .unwrap();
    r.fulfill(
        stream.ticket,
        Outcome::Message(Message {
            data: "oom".into(),
            ..Default::default()
        }),
    )
    .unwrap();
    retired(&mut r, &old);
}

#[test]
#[cfg_attr(
    not(exact_js_oom),
    ignore = "requires an exception-on-OOM Hermes receipt"
)]
fn heap_oom_cancels_a_queued_stream_mapper_turn_before_dispatch() {
    for settle_first in [false, true] {
        let mut r = runner(Placement::Main);
        let old = pump(&mut r);
        let stream = old
            .iter()
            .find(|q| q.request.url.starts_with("wss:"))
            .unwrap();
        r.fulfill(
            stream.ticket,
            Outcome::Message(Message {
                data: "wait".into(),
                ..Default::default()
            }),
        )
        .unwrap();
        let turns = r.in_flight();
        let turn = turns
            .iter()
            .find(|(name, ticket)| name == "streaming" && *ticket != stream.ticket)
            .expect("the mapper queued a turn")
            .1;
        if settle_first {
            assert!(r.act("refuseAfterCrash", vec![]).is_err());
        } else {
            r.act("crash", vec![]).unwrap();
        }
        r.collect_aborts();
        if settle_first {
            while let Some((ticket, outcome)) = r.take_aborted_answer() {
                r.fulfill(ticket, outcome).unwrap();
            }
        }
        // The Linux presenter cancels dispatched work before taking new requests.
        let aborted = r.take_explicit_aborts();
        let requests = r.take_requests();
        assert!(
            requests.iter().all(|q| q.ticket != turn),
            "retired mapper was dispatched: {requests:?}"
        );
        assert!(
            aborted.contains(&turn),
            "an already dispatched turn must be cancelled too"
        );
        retired(&mut r, &old);
        r.fulfill(turn, Dispatch::again_outcome()).unwrap();
    }
}

#[test]
#[cfg_attr(
    not(exact_js_oom),
    ignore = "requires an exception-on-OOM Hermes receipt"
)]
fn heap_oom_keeps_tool_ownership_after_unrelated_forgotten() {
    ibex2_sqlite::link();
    let root = std::env::temp_dir().join(format!("exact-oom-tool-{}", std::process::id()));
    let mut m = Module::new(HBC.to_vec(), "test.oom", GRANTS);
    m.set_budget_ms(f64::INFINITY);
    m.bind(&contract::compile(SRC).unwrap());
    m.configure_storage(root.join("data"), root.join("cache"), root.join("tmp"))
        .unwrap();
    m.load().unwrap();
    let source =
        exact_data::Mixed::new(m, Other, &["tool:h", "pending", "oom"], &["other"]).unwrap();
    let plan = contract::compile(
        &SRC.replace("  resource sleeping = sleep() as shape string\n", "")
            .replace("  resource streaming = feed() as shape string\n", ""),
    )
    .unwrap();
    let mut b = exact_plan::builder::PlanBuilder::from_plan(plan);
    let string = b.primitive(exact_plan::TypeKind::String);
    let grant = b.source("grant", &[], string);
    b.module(
        grant,
        ("test", "@fixture/test"),
        "grant",
        true,
        false,
        "never",
        "",
        r#"{"handles":{"id":"test"},"handlers":"handlers[]"}"#,
    );
    let mut r = Runner::boot(
        b.finish().unwrap(),
        source,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.take_requests();
    r.bind_extensions(&["test"], &[]);
    r.native_slot().host(Some(std::sync::Arc::new(|_, reply| {
        reply.send(Outcome::Response(exact_runner::Response {
            status: 200,
            headers: vec![],
            body: br#"{"id":"held"}"#.to_vec(),
        }));
    })));
    let request = exact_runner::Request::native(
        br#"{"extension":"test","send":"grant","args":{"handlers":["h"]}}"#.to_vec(),
    );
    let Dispatch::Run(Work::Later(grant)) = r.native_work(&request) else {
        panic!("the implementation grants its handle");
    };
    grant(exact_runner::Reply::new(|_| {}));
    let (tx, rx) = std::sync::mpsc::channel();
    r.invoke_module(
        r#"{"extension":"test","handle":"held","handler":"h","args":{}}"#,
        Box::new(move |answer| {
            let _ = tx.send(answer);
        }),
    );
    let requests = r.take_requests();
    assert_eq!(requests.len(), 1);
    let tool = &requests[0];
    assert!(tool.request.fetch.is_none());
    let token = tool.request.continuation.unwrap();
    let mut store = Store::new(GRANTS, []);
    let source = r.data();
    let target = exact_runner::Target::Resource(0);
    assert!(matches!(
        source.answer_for(target, &mut store, "pending", &[]),
        Ok(Answer::Later(_))
    ));
    let Answer::Later(new) = source
        .answer_for(target, &mut store, "pending", &[])
        .unwrap()
    else {
        panic!("refresh")
    };
    source.forgotten(
        &store,
        &[exact_runner::InFlight {
            target,
            source: "pending",
            args: &[],
            fetch: new.fetch,
            continuation: new.continuation,
        }],
    );
    let work = r.dispatch_work(token);
    assert!(matches!(work, Dispatch::Run(_)), "the tool stays resumable");
    assert!(rx.try_recv().is_err(), "the invocation is outstanding");
    assert!(r.data().answer(&mut store, "oom", &[]).is_err());
    assert!(pump(&mut r).is_empty());
    let reply: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(reply["error"], "OperationError");
    assert!(reply["message"].as_str().unwrap().contains(ERROR));
    assert!(!r.holds(tool.ticket));
    assert!(r.in_flight().is_empty());
    assert!(r.take_requests().is_empty());
    assert!(r.take_aborted_answer().is_none());
    drop(work);
    drop(r);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg_attr(
    not(exact_js_oom),
    ignore = "requires an exception-on-OOM Hermes receipt"
)]
fn heap_oom_can_repeat_without_resetting_another_runtime() {
    let mut broken = module();
    let mut healthy = module();
    let mut store = Store::new(GRANTS, []);
    for count in 1..=3 {
        let error = broken.answer(&mut store, "oom", &[]).unwrap_err();
        assert!(format!("{error:?}").contains(ERROR));
        assert!(!broken.is_loaded());
        assert_eq!(
            broken.answer(&mut store, "counter", &[]).unwrap(),
            Answer::Now(Value::str("1"))
        );
        assert_eq!(
            healthy.answer(&mut store, "counter", &[]).unwrap(),
            Answer::Now(Value::str(&count.to_string()))
        );
    }
}

fn draw(m: &mut Module, seq: u64) -> exact_runner::DrawReply {
    let request = exact_runner::DrawRequest {
        canvas: 1,
        generation: 1,
        seq,
        surface: "test",
        args: &[],
        names: &[],
        frame: Default::default(),
        current_color: None,
        rtl: false,
        p3: false,
    };
    let exact_runner::Drawn::Now(reply) =
        m.draw(&request, &exact_runner::exact_canvas::Context2d::new())
    else {
        panic!("synchronous draw")
    };
    reply
}

#[test]
#[cfg_attr(
    not(exact_js_oom),
    ignore = "requires an exception-on-OOM Hermes receipt"
)]
fn heap_oom_canvas_calls_reload_without_an_answer() {
    let mut m = module();
    assert!(draw(&mut m, 0).error.is_some());
    assert!(!m.is_loaded());
    let reply = draw(&mut m, 1);
    assert!(reply.error.is_none(), "{:?}", reply.error);
    assert_eq!(reply.notes, ["1"]);
    assert!(draw(&mut m, 0).error.is_some());
    assert!(!m.is_loaded());
    m.canvases_retired(&[(1, 1)]);
    assert!(m.is_loaded(), "retiring canvases reloads too");
    assert_eq!(draw(&mut m, 2).notes, ["2"]);
}

fn overlapping_tools(dispatch_first: bool) -> (exact_data::Mixed<Module, Other>, Store, [u64; 2]) {
    let mut source =
        exact_data::Mixed::new(module(), Other, &["tool:yielding", "oom"], &["other"]).unwrap();
    let mut store = Store::new(GRANTS, []);
    let tokens = std::array::from_fn(|index| {
        let Answer::Later(request) = source
            .answer(&mut store, "tool:yielding", &[Value::str("{}")])
            .unwrap()
        else {
            panic!("the tool yields")
        };
        let token = request.continuation.unwrap();
        if dispatch_first && index == 0 {
            assert!(matches!(source.dispatch(token, &store), Dispatch::Again));
        }
        token
    });
    (source, store, tokens)
}

#[test]
fn overlapping_same_argument_tools_both_settle() {
    let (mut source, mut store, tokens) = overlapping_tools(false);
    for token in tokens {
        assert!(
            matches!(source.dispatch(token, &store), Dispatch::Again),
            "both tools must dispatch"
        );
    }
    for call in (1..=2).rev() {
        assert_eq!(
            source
                .parse_tool(
                    None,
                    Some(tokens[call - 1]),
                    &mut store,
                    "tool:yielding",
                    &[Value::str("{}")],
                    Dispatch::again_outcome()
                )
                .unwrap(),
            Answer::Now(Value::str(&format!("{{\"call\":{call}}}")))
        );
    }
}

#[test]
#[cfg_attr(
    not(exact_js_oom),
    ignore = "requires an exception-on-OOM Hermes receipt"
)]
fn heap_oom_retires_the_other_same_argument_tool() {
    let (mut source, mut store, tokens) = overlapping_tools(true);
    source
        .parse_tool(
            None,
            Some(tokens[0]),
            &mut store,
            "tool:yielding",
            &[Value::str("{}")],
            Dispatch::again_outcome(),
        )
        .unwrap();
    let instance = source.module_identity("oom").unwrap().instance();
    assert!(source.answer(&mut store, "oom", &[]).is_err());
    assert_eq!(instance.failure(), Some(ERROR));
    assert!(matches!(
        source.dispatch(tokens[1], &store),
        Dispatch::Missing
    ));
    let error = source
        .parse_tool(
            None,
            Some(tokens[1]),
            &mut store,
            "tool:yielding",
            &[Value::str("{}")],
            Dispatch::again_outcome(),
        )
        .unwrap_err();
    assert!(error.to_string().contains(ERROR), "{error}");
}

fn finish_tool_turns<D: DataSource>(
    source: &mut D,
    store: &mut Store,
    mut answer: Answer,
) -> Answer {
    for _ in 0..16 {
        let Answer::Later(ref request) = answer else {
            return answer;
        };
        let Some(token) = request.continuation else {
            return answer;
        };
        let outcome = match source.dispatch(token, store) {
            Dispatch::Again => Dispatch::again_outcome(),
            Dispatch::Run(Work::Now(work)) => work(),
            Dispatch::Run(Work::Later(work)) => {
                let (tx, rx) = std::sync::mpsc::channel();
                work(exact_runner::Reply::new(move |o| {
                    tx.send(o).unwrap();
                }));
                rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap()
            }
            _ => panic!("the tool turn must dispatch"),
        };
        answer = source
            .parse_tool(
                request.fetch,
                request.continuation,
                store,
                "tool:fetched",
                &[Value::str("{}")],
                outcome,
            )
            .unwrap();
    }
    panic!("tool did not finish its local turns")
}

#[test]
fn equal_argument_tool_fetches_resume_their_own_replies_in_both_placements() {
    for placement in [Placement::Main, Placement::Worker] {
        let mut source = exact_data::Mixed::new(
            module().placed(placement),
            Other,
            &["tool:fetched"],
            &["other"],
        )
        .unwrap();
        source.activate().unwrap();
        let mut store = Store::new(GRANTS, []);
        for pair in 0..16 {
            let mut requests = Vec::new();
            for _ in 0..2 {
                let answer = source
                    .answer(&mut store, "tool:fetched", &[Value::str("{}")])
                    .unwrap();
                let Answer::Later(request) = finish_tool_turns(&mut source, &mut store, answer)
                else {
                    panic!("fetch")
                };
                requests.push(request);
            }
            for i in [1, 0] {
                let request = &requests[i];
                let body = format!("reply {}", pair * 2 + i + 1);
                let answer = source
                    .parse_tool(
                        request.fetch,
                        request.continuation,
                        &mut store,
                        "tool:fetched",
                        &[Value::str("{}")],
                        Outcome::Response(exact_runner::Response {
                            status: 200,
                            headers: vec![],
                            body: body.as_bytes().to_vec(),
                        }),
                    )
                    .unwrap();
                assert_eq!(
                    finish_tool_turns(&mut source, &mut store, answer),
                    Answer::Now(Value::str(&format!(
                        "{{\"call\":{},\"body\":\"{body}\"}}",
                        pair * 2 + i + 1
                    )))
                );
            }
        }
    }
}
