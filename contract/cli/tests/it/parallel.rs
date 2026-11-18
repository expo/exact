//! LLP 1161.000 end to end: a `parallel` mutation asks every send when it
//! is made, each its own request in flight; replies land in any order, and
//! the slot and the failure follow the latest send that has concluded.

use exact_kernel::{Kernel, PropId};
use exact_runner::{
    failure::FailureCode, Answer, DataError, DataSource, FailureKind, Outcome, Overlaid, Request,
    Response, Runner, RunnerError, Store, Value, Write,
};

const APP: &str = r#"
shape Ack
  op: string

shape Item
  text: string

component App
  mutation renamed as shape Ack parallel refreshes items
  resource items = items() as shape Item[]
  derive last = match renamed { case some(a) => a.op, case none => "-" }
  derive why = match failure(renamed) { case some(f) => f.reason, case none => "-" }
  derive shown = items.map(i => i.text).join(",")
  action rename(t: string)
    send renamed = rename(t)
  action twice(t: string)
    send renamed = rename(t)
    send renamed = rename(t)
  action clear
    renamed = none
  view
    column
      text shown testId="items"
      text pending(renamed) ? "busy" : "idle" testId="busy"
"#;

/// A server's list of names, read by `items` and written by `rename`,
/// which answers later. Its overlay lays each write over the answer, and
/// records the order it was given them in.
#[derive(Default)]
struct Names {
    server: Vec<String>,
    /// The ops whose reply fails to parse: the server declined them.
    decline: Vec<String>,
    overlays: Vec<Vec<(String, bool)>>,
}

fn ok() -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: b"{}".to_vec(),
    })
}

fn list(texts: &[String]) -> Value {
    Value::list(
        (texts.iter())
            .map(|t| Value::record(vec![Value::str(t)]))
            .collect(),
    )
}

fn op(args: &[Value]) -> String {
    args[0].as_str().unwrap_or("").to_string()
}

impl DataSource for Names {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        match source {
            "items" => Ok(list(&self.server)),
            other => Err(DataError::UnknownSource(other.into())),
        }
    }
    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        match source {
            "items" => Ok(Answer::Later(Request::get("https://names.test/items"))),
            "rename" => Ok(Answer::Later(Request::post_json(
                "https://names.test/rename",
                &op(args),
            ))),
            other => Err(DataError::UnknownSource(other.into())),
        }
    }
    fn parse(
        &mut self,
        _: &mut Store,
        source: &str,
        args: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        match source {
            "items" => Ok(Answer::Now(list(&self.server))),
            "rename" if self.decline.contains(&op(args)) => Err(DataError::Failed(
                FailureCode::Error,
                "declined".into(),
                op(args),
            )),
            "rename" => {
                self.server.push(op(args));
                Ok(Answer::Now(Value::record(vec![Value::str(&op(args))])))
            }
            other => Err(DataError::UnknownSource(other.into())),
        }
    }
    fn overlay(
        &mut self,
        _: &str,
        _: &[Value],
        answer: &Value,
        writes: &[Write<'_>],
    ) -> Result<Option<Overlaid>, DataError> {
        self.overlays.push(
            (writes.iter())
                .map(|w| (op(w.args), w.reply.is_some()))
                .collect(),
        );
        let mut shown: Vec<String> = match answer {
            Value::List(items) => (items.iter())
                .map(|i| match i {
                    Value::Record(f) => f[0].as_str().unwrap_or("").to_string(),
                    _ => String::new(),
                })
                .collect(),
            _ => Vec::new(),
        };
        for w in writes {
            if !shown.contains(&op(w.args)) {
                shown.push(op(w.args));
            }
        }
        Ok(Some(Overlaid {
            value: list(&shown),
            keep: Vec::new(),
        }))
    }
}

fn boot_with(src: &str) -> Runner<Names> {
    let baked = contract::bake(contract::compile(src).unwrap(), Names::default()).unwrap();
    let mut r = Runner::boot(
        baked,
        Names::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.data_ready().unwrap();
    for q in r.take_requests() {
        r.fulfill(q.ticket, ok()).unwrap();
    }
    r
}

fn boot() -> Runner<Names> {
    boot_with(APP)
}

/// The tickets handed out since the last take, by target name.
fn sent(r: &mut Runner<Names>, name: &str) -> Vec<u64> {
    (r.take_requests().into_iter())
        .filter(|q| q.target == name)
        .map(|q| q.ticket)
        .collect()
}

fn derive(r: &Runner<Names>, name: &str) -> String {
    r.derive(name)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn busy(r: &Runner<Names>) -> bool {
    let k = r.kernel();
    let key = k.find_by_test_id("busy")[0];
    k.node_by_key(key).unwrap().props.str(PropId::Text) == Some("busy")
}

fn failed() -> Outcome {
    Outcome::Failed {
        kind: FailureKind::Network,
        message: "declined".into(),
    }
}

#[test]
fn every_send_is_asked_at_once_and_each_reply_lands() {
    let mut r = boot();
    r.act("rename", vec![Value::str("a")]).unwrap();
    r.act("rename", vec![Value::str("b")]).unwrap();
    let [a, b] = sent(&mut r, "renamed")[..] else {
        panic!("both sends are asked when made")
    };
    assert!(r.holds(a) && r.holds(b));
    assert!(busy(&r));
    // In send order: the later reply is the newer outcome.
    r.fulfill(a, ok()).unwrap();
    assert_eq!(derive(&r, "last"), "a");
    assert!(busy(&r), "pending while any send is out");
    r.fulfill(b, ok()).unwrap();
    assert_eq!(derive(&r, "last"), "b");
    assert!(!busy(&r));
    assert_eq!(r.mutation_requests(), 2);
}

#[test]
fn a_reply_landing_after_a_later_one_leaves_the_slot() {
    let mut r = boot();
    r.act("rename", vec![Value::str("a")]).unwrap();
    r.act("rename", vec![Value::str("b")]).unwrap();
    let [a, b] = sent(&mut r, "renamed")[..] else {
        panic!()
    };
    r.fulfill(b, ok()).unwrap();
    assert_eq!(derive(&r, "last"), "b");
    assert!(r.holds(a), "the earlier send is still wanted");
    // Its write retires and its refreshes are asked; the slot stays.
    r.fulfill(a, ok()).unwrap();
    assert_eq!(derive(&r, "last"), "b");
    assert!(!busy(&r));
    assert_eq!(r.data().server, ["b", "a"]);
}

#[test]
fn an_earlier_failure_while_the_later_is_out_shows_until_the_later_lands() {
    let mut r = boot();
    r.data().decline.push("a".into());
    r.act("rename", vec![Value::str("a")]).unwrap();
    r.act("rename", vec![Value::str("b")]).unwrap();
    let [a, b] = sent(&mut r, "renamed")[..] else {
        panic!()
    };
    let _ = r.fulfill(a, failed());
    assert_eq!(derive(&r, "why"), "a", "no later send has concluded");
    assert!(busy(&r) && r.holds(b));
    r.fulfill(b, ok()).unwrap();
    assert_eq!(
        (derive(&r, "last").as_str(), derive(&r, "why").as_str()),
        ("b", "-")
    );
}

#[test]
fn an_earlier_failure_after_the_later_landed_leaves_the_later() {
    let mut r = boot();
    r.data().decline.push("a".into());
    r.act("rename", vec![Value::str("a")]).unwrap();
    r.act("rename", vec![Value::str("b")]).unwrap();
    let [a, b] = sent(&mut r, "renamed")[..] else {
        panic!()
    };
    r.fulfill(b, ok()).unwrap();
    let _ = r.fulfill(a, failed());
    assert_eq!(
        (derive(&r, "last").as_str(), derive(&r, "why").as_str()),
        ("b", "-")
    );
    assert!(r
        .journal()
        .any(|l| l.contains("failed and is no longer pending")));
}

#[test]
fn a_later_failure_stands_through_an_earlier_success() {
    let mut r = boot();
    r.data().decline.push("b".into());
    r.act("rename", vec![Value::str("a")]).unwrap();
    r.act("rename", vec![Value::str("b")]).unwrap();
    let [a, b] = sent(&mut r, "renamed")[..] else {
        panic!()
    };
    let _ = r.fulfill(b, failed());
    assert_eq!(derive(&r, "why"), "b");
    r.fulfill(a, ok()).unwrap();
    assert_eq!(
        derive(&r, "why"),
        "b",
        "an earlier success does not land in the slot"
    );
    assert_eq!(derive(&r, "last"), "-");
    // A new send clears it.
    r.act("rename", vec![Value::str("c")]).unwrap();
    assert_eq!(derive(&r, "why"), "-");
}

#[test]
fn an_assignment_forgets_no_request_and_a_reply_lands_after_it() {
    let mut r = boot();
    r.data().decline.push("a".into());
    r.act("rename", vec![Value::str("a")]).unwrap();
    let [a] = sent(&mut r, "renamed")[..] else {
        panic!()
    };
    let _ = r.fulfill(a, failed());
    r.act("rename", vec![Value::str("b")]).unwrap();
    r.act("rename", vec![Value::str("c")]).unwrap();
    let [b, c] = sent(&mut r, "renamed")[..] else {
        panic!()
    };
    r.act("clear", vec![]).unwrap();
    assert_eq!(
        (derive(&r, "last").as_str(), derive(&r, "why").as_str()),
        ("-", "-")
    );
    assert!(r.holds(b) && r.holds(c) && busy(&r), "nothing is forgotten");
    r.fulfill(b, ok()).unwrap();
    assert_eq!(derive(&r, "last"), "b");
    r.fulfill(c, ok()).unwrap();
    assert_eq!(derive(&r, "last"), "c");
}

#[test]
fn one_action_may_send_twice_with_equal_arguments_and_both_land() {
    let mut r = boot();
    r.act("twice", vec![Value::str("a")]).unwrap();
    let tickets = sent(&mut r, "renamed");
    assert_eq!(tickets.len(), 2);
    r.fulfill(tickets[1], ok()).unwrap();
    assert!(r.holds(tickets[0]));
    r.fulfill(tickets[0], ok()).unwrap();
    assert_eq!(r.data().server, ["a", "a"]);
    assert!(!busy(&r));
}

#[test]
fn the_sixty_fifth_send_in_flight_refuses_its_action() {
    let mut r = boot();
    for i in 0..64 {
        r.act("rename", vec![Value::str(&i.to_string())]).unwrap();
    }
    assert_eq!(sent(&mut r, "renamed").len(), 64);
    let e = r.act("rename", vec![Value::str("over")]).unwrap_err();
    assert!(
        matches!(&e, RunnerError::QueueFull { mutation } if mutation == "renamed"),
        "{e:?}"
    );
    assert!(sent(&mut r, "renamed").is_empty(), "nothing was asked");
    assert_eq!(r.mutation_requests(), 64);
}

#[test]
fn a_parallel_mutation_takes_no_then() {
    let src = APP.replace(
        "parallel refreshes items",
        "parallel refreshes items then clear",
    );
    let e = contract::compile(&src).unwrap_err();
    assert_eq!(e.id, "type-parallel-then", "{}", e.message);
    assert!(
        e.message.contains("`renamed` is `parallel`"),
        "{}",
        e.message
    );
}

#[test]
fn each_write_shows_on_its_own_and_the_overlay_sees_landed_writes_in_landing_order() {
    let mut r = boot();
    r.act("rename", vec![Value::str("a")]).unwrap();
    r.act("rename", vec![Value::str("b")]).unwrap();
    r.act("rename", vec![Value::str("c")]).unwrap();
    let [a, b, _c] = sent(&mut r, "renamed")[..] else {
        panic!()
    };
    assert_eq!(
        derive(&r, "shown"),
        "a,b,c",
        "every write shows from its send"
    );
    assert_eq!(
        r.data().overlays.last().unwrap(),
        &[
            ("a".into(), false),
            ("b".into(), false),
            ("c".into(), false)
        ]
    );
    // `b` lands first, then `a`: landed writes in landing order, then the
    // one still out.
    r.fulfill(b, ok()).unwrap();
    r.fulfill(a, ok()).unwrap();
    assert_eq!(
        r.data().overlays.last().unwrap(),
        &[("b".into(), true), ("a".into(), true), ("c".into(), false)]
    );
    assert_eq!(r.writes().len(), 3, "landed writes show until answered");
}

#[test]
fn a_declined_write_stops_showing_alone() {
    let mut r = boot();
    r.data().decline.push("a".into());
    r.act("rename", vec![Value::str("a")]).unwrap();
    r.act("rename", vec![Value::str("b")]).unwrap();
    let [a, b] = sent(&mut r, "renamed")[..] else {
        panic!()
    };
    let _ = r.fulfill(a, failed());
    assert_eq!(derive(&r, "shown"), "b", "the other write still shows");
    assert_eq!(r.writes().len(), 1);
    r.fulfill(b, ok()).unwrap();
    // `items` answers what the server holds, and the write retires.
    for q in r.take_requests() {
        r.fulfill(q.ticket, ok()).unwrap();
    }
    assert_eq!(derive(&r, "shown"), "b");
    assert!(r.writes().is_empty());
}

const KEPT: &str = r#"
shape Ack
  op: string

source remote(op: string) as shape Ack
  url "https://names.test/remote"
  method "POST"
  body json

component App
  mutation renamed as shape Ack parallel
  derive last = match renamed { case some(a) => a.op, case none => "-" }
  derive why = match failure(renamed) { case some(f) => f.reason, case none => "-" }
  action near(t: string)
    send renamed = quick(t)
  action far(t: string)
    send renamed = remote(t)
  view
    text last
"#;

/// Answers `quick` at once once it is ready, failing an op named `fail`.
struct Quick(bool);
impl DataSource for Quick {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        match source {
            "quick" if op(args) == "fail" => Err(DataError::Failed(
                FailureCode::Error,
                "failed".into(),
                op(args),
            )),
            "quick" => Ok(Answer::Now(Value::record(vec![Value::str(&op(args))]))),
            other => Err(DataError::UnknownSource(other.into())),
        }
    }
    fn ready(&self) -> bool {
        self.0
    }
}

/// A runner over [`KEPT`] whose data source is still loading.
fn loading() -> Runner<Quick> {
    let plan = contract::compile(KEPT).unwrap();
    Runner::boot(
        plan,
        Quick(false),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

/// A send kept until the data source is ready lands its reply only if no
/// later send has concluded meanwhile (LLP 1161.000 D4): here a later send
/// to a source declared in Contract, which needs no data source, lands
/// first, and the kept one's synchronous answer leaves the slot.
#[test]
fn a_kept_send_answering_after_a_later_one_landed_leaves_the_slot() {
    let mut r = loading();
    r.act("near", vec![Value::str("a")]).unwrap();
    assert!(r.take_requests().is_empty(), "kept for the data source");
    r.act("far", vec![Value::str("b")]).unwrap();
    let [b] = &r.take_requests()[..] else {
        panic!("the declared source's request")
    };
    let reply = Outcome::Response(Response {
        status: 200,
        headers: vec![("content-type".into(), "application/json".into())],
        body: br#"{"op":"b"}"#.to_vec(),
    });
    r.fulfill(b.ticket, reply).unwrap();
    assert_eq!(r.derive("last"), Some(&Value::str("b")));
    r.data().0 = true;
    r.data_ready().unwrap();
    assert_eq!(
        r.derive("last"),
        Some(&Value::str("b")),
        "the older send leaves it"
    );
}

/// Two kept sends, the first failing: the second's answer lands and clears
/// the failure, as a reply landing does.
#[test]
fn a_later_kept_success_clears_an_earlier_kept_failure() {
    let mut r = loading();
    r.act("near", vec![Value::str("fail")]).unwrap();
    r.act("near", vec![Value::str("ok")]).unwrap();
    r.data().0 = true;
    r.data_ready().unwrap();
    assert_eq!(r.derive("last"), Some(&Value::str("ok")));
    assert_eq!(r.derive("why"), Some(&Value::str("-")));
}
