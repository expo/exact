//! A queued send's `then` runs as the commit that queued it (LLP 1133.002
//! D12), whether its source answers it at once in its `next` commit or
//! later: a tool's or a wake's continuation asks as the tool or the wake,
//! and a tool call owns that continuation until it settles (LLP 1133.023
//! D7).
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    agent, Answer, CallHow, CallOrigin, DataError, DataSource, Outcome, Request, RequestOut,
    Response, Runner, Store,
};

const SRC: &str = r#"
component App
  mutation q as shape number queue then follow
  mutation work as shape number parallel
  tool second writes "Queue a send answered at once." = second
  action first
    send q = save(1)
  action second
    send q = save(2)
  action third
    send q = save(3)
  action follow
    send work = work()
  view
    text "queue origin"
"#;

/// `save(2)` answers at once; every other send answers later.
struct Src;

impl DataSource for Src {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("answers through answer")
    }

    fn grants(&self) -> &str {
        ""
    }

    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        Ok(match (source, args.first()) {
            ("save", Some(Value::Number(n))) if *n == 2.0 => Answer::Now(Value::Number(2.0)),
            ("save" | "work", _) => Answer::Later(Request::native(b"{}".to_vec())),
            (other, _) => return Err(DataError::UnknownSource(other.into())),
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
}

fn boot() -> Runner<Src> {
    let plan = contract::compile(SRC).unwrap_or_else(|e| panic!("{e:?}"));
    Runner::boot(plan, Src, Kernel::with_monospace(), Default::default(), "/").unwrap()
}

fn ok() -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: Vec::new(),
        body: b"{}".to_vec(),
    })
}

/// Answer each `q` request in flight, then run what is due, until none is.
fn drain(r: &mut Runner<Src>, out: &mut Vec<RequestOut>) {
    loop {
        out.extend(r.take_requests());
        let Some(i) = out.iter().position(|o| o.target == "q") else {
            return;
        };
        let q = out.remove(i);
        r.fulfill(q.ticket, ok()).unwrap();
        r.land_then();
    }
}

#[test]
fn a_queued_sends_then_asks_as_the_origin_that_queued_it() {
    for origin in [CallOrigin::Tool, CallOrigin::Wake] {
        let mut r = boot();
        r.take_requests();
        // `first` is in flight; `second`, answered at once, and `third`,
        // answered later, wait their turn.
        for action in ["first", "second", "third"] {
            r.act_from(origin, action, vec![]).unwrap();
        }
        let mut out = Vec::new();
        drain(&mut r, &mut out);
        out.extend(r.take_requests());
        let work: Vec<CallOrigin> = (out.iter())
            .filter(|o| o.target == "work")
            .map(|o| o.request.reach.origin)
            .collect();
        assert_eq!(work, [origin; 3], "{origin:?}");
    }
}

#[test]
fn a_tool_owns_the_then_of_its_send_answered_at_once_from_the_queue() {
    let mut r = boot();
    r.take_requests();
    r.act("first", vec![]).unwrap();
    let how = CallHow {
        door: "agent".into(),
        ..Default::default()
    };
    r.call_tool("second", "{}", how).unwrap();
    let at = r.now_ms();
    r.advance(at).unwrap();
    // The app's send lands: the tool's waits no more, is answered at once
    // in its `next`, and its `then` asks for `work` as the tool's.
    let mut out = r.take_requests();
    let first = out.iter().position(|o| o.target == "q").unwrap();
    let first = out.remove(first);
    r.fulfill(first.ticket, ok()).unwrap();
    r.land_then();
    out.extend(r.take_requests());
    let work: Vec<(CallOrigin, u64)> = (out.iter())
        .filter(|o| o.target == "work")
        .map(|o| (o.request.reach.origin, o.ticket))
        .collect();
    assert_eq!(work.len(), 2, "{}", agent::handle(&r, r#"{"op":"logs"}"#));
    assert_eq!([work[0].0, work[1].0], [CallOrigin::App, CallOrigin::Tool]);
    // The call owns the `work` its send's `then` asked: it ends once that
    // request's reply lands, not before.
    let calls = |r: &Runner<Src>| {
        let state: serde_json::Value =
            serde_json::from_str(&agent::handle(r, r#"{"op":"state"}"#)).unwrap();
        state["intents"]["calls"].clone()
    };
    let running = |r: &Runner<Src>| calls(r)[0]["state"] == "running";
    assert!(running(&r), "{}", calls(&r));
    r.fulfill(work[1].1, ok()).unwrap();
    let at = r.now_ms();
    r.advance(at).unwrap();
    assert!(!running(&r), "{}", calls(&r));
}
