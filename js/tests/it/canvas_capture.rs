#![cfg(exact_js_engine)]
use exact_js::Module;
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{Event, Outcome, Response, Runner};
const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/canvas-capture.hbc"));
const GRANTS: &str = "net.fetch https://context.test\nfs.write app:/tmp";
fn value(text: &str) -> Value {
    Value::record(vec![Value::str(text)])
}
fn response(text: &str) -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: text.as_bytes().to_vec(),
    })
}

#[test]
fn a_canvas_capture_waits_for_http_then_resumes_its_own_promise() {
    let plan = contract::compile(
            "shape Result\n  text: string\ncomponent App\n  state seed = 0\n  action go\n    seed = 1\n  resource result = captureHttp(seed) as shape Result\n  view\n    button press=go testId=\"go\"\n      text \"capture\"\n",
        ).unwrap();
    let data = Module::loaded(HBC.to_vec(), "test.context", GRANTS).unwrap();
    let mut runner = Runner::boot(
        plan,
        data,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = runner.kernel();
    let go = k.node_by_key(k.find_by_test_id("go")[0]).unwrap().id;
    runner.dispatch(go, Event::Press).unwrap();
    let requests = runner.take_requests();
    assert_eq!(
        requests.len(),
        1,
        "non-HTTP work waits for the outstanding fetch (LLP 1162 D2)"
    );
    runner
        .fulfill(requests[0].ticket, response("http"))
        .unwrap();
    let requests = runner.take_requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].request.surface.is_some());
    runner
        .fulfill(
            requests[0].ticket,
            response(r#"{"path":"app:/tmp/ink.png","width":10,"height":10,"scale":1}"#),
        )
        .unwrap();
    assert_eq!(
        runner.resource("result"),
        Some(&value("http/app:/tmp/ink.png"))
    );
    assert!(!runner.has_pending());
}
