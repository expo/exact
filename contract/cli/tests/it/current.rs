//! LLP 1161.001: `current(x)`, whether a resource shows an answer to its
//! current call arguments, through each row of D1's table.

use crate::source::{boxed, Read, Source};
use exact_kernel::{Kernel, PropId};
use exact_runner::{
    Answer, DataError, DataSource, FailureKind, Outcome, Request, Response, Runner, Store, Value,
};

/// `search(q)` answers later, one request per query; `fail` makes the next
/// replies failures.
#[derive(Default)]
struct Search {
    fail: bool,
}

impl DataSource for Search {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!()
    }
    fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
        let q = args[0].as_str().unwrap_or("");
        Ok(Answer::Later(Request::get(&format!(
            "https://search.test/?q={q}"
        ))))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        args: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        if self.fail {
            return Err(DataError::Unavailable("offline".into()));
        }
        Ok(Answer::Now(Value::str(&format!(
            "for {}",
            args[0].as_str().unwrap_or("")
        ))))
    }
}

const SRC: &str = "component App
  state query = \"a\"
  state tick = 0
  resource results = search(query) with tick as shape string else empty()
  derive shown = current(results)
  action search(q: string)
    query = q
  action again
    refresh results
  action later
    tick = tick + 1
  view
    text (current(results) ? results : \"loading\") testId=\"out\"
";

fn boot() -> Runner<Source> {
    Runner::boot(
        contract::compile(SRC).unwrap(),
        boxed(Search::default()),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn ok() -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: b"{}".to_vec(),
    })
}

fn offline() -> Outcome {
    Outcome::Failed {
        kind: FailureKind::Network,
        message: "offline".into(),
    }
}

fn current<D: DataSource>(r: &Runner<D>) -> bool {
    r.derive("shown") == Some(&Value::Bool(true))
}

fn text<D: DataSource>(r: &Runner<D>) -> String {
    let k = r.kernel();
    let key = k.find_by_test_id("out").into_iter().next().unwrap();
    k.node_by_key(key)
        .unwrap()
        .props
        .str(PropId::Text)
        .unwrap()
        .to_string()
}

/// Lands the one request out.
fn land(r: &mut Runner<Source>, outcome: Outcome) {
    let requests = r.take_requests();
    assert_eq!(requests.len(), 1, "one request out");
    r.fulfill(requests[0].ticket, outcome).unwrap();
}

#[test]
fn the_placeholder_is_not_current_and_the_answer_is() {
    let mut r = boot();
    assert!(!current(&r), "boot shows the placeholder");
    assert_eq!(text(&r), "loading");
    land(&mut r, ok());
    assert!(current(&r));
    assert_eq!(text(&r), "for a");
}

#[test]
fn new_arguments_are_not_current_until_their_answer_lands() {
    let mut r = boot();
    land(&mut r, ok());
    r.act("search", vec![Value::str("b")]).unwrap();
    assert!(!current(&r), "the shown answer is a's");
    assert_eq!(r.resource("results"), Some(&Value::str("for a")));
    assert_eq!(text(&r), "loading");
    land(&mut r, ok());
    assert!(current(&r));
    assert_eq!(text(&r), "for b");
}

#[test]
fn a_refresh_and_a_with_value_leave_it_current() {
    let mut r = boot();
    land(&mut r, ok());
    r.act("again", vec![]).unwrap();
    assert!(current(&r), "a refresh of the same arguments");
    assert_eq!(text(&r), "for a");
    land(&mut r, ok());
    r.act("later", vec![]).unwrap();
    assert!(current(&r), "a `with` value is context, not identity");
    land(&mut r, ok());
    assert!(current(&r));
}

#[test]
fn a_failure_for_new_arguments_is_not_current_and_a_failed_refresh_is() {
    let mut r = boot();
    land(&mut r, ok());
    r.source::<Search>().fail = true;
    r.act("again", vec![]).unwrap();
    land(&mut r, offline());
    assert!(current(&r), "a's answer still answers a");
    r.act("search", vec![Value::str("b")]).unwrap();
    land(&mut r, offline());
    assert!(!current(&r), "a's answer kept, b failed");
    assert_eq!(text(&r), "loading");
    r.source::<Search>().fail = false;
    r.act("search", vec![Value::str("a")]).unwrap();
    assert!(current(&r), "back to the arguments the kept answer answers");
}

#[test]
fn a_mutation_or_a_derive_is_refused() {
    for (decl, arg) in [
        ("mutation sent as shape string", "sent"),
        ("derive d = 1", "d"),
    ] {
        let src =
            format!("component App\n  {decl}\n  view\n    text (current({arg}) ? \"y\" : \"n\")\n");
        let errors = contract::compile(&src).unwrap_err();
        assert!(
            format!("{errors:?}").contains("type-current-argument"),
            "{arg}: {errors:?}"
        );
    }
}

/// The server snapshots /state at the start of each read. The test delivers
/// that snapshot 200 ms later, after any intervening events.
#[derive(Default)]
struct Snapshots {
    refuse: bool,
    change_prop: bool,
    post: bool,
    refuse_failure: bool,
    asks: Vec<Vec<Value>>,
}

impl DataSource for Snapshots {
    fn grants(&self) -> &str {
        "secret.keep revision\nsecret.keep attempted"
    }

    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(
        &mut self,
        store: &mut Store,
        name: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        if name == "revision" {
            return Ok(Answer::Now(Value::Number(
                if store.get("revision").is_some() {
                    2.
                } else {
                    0.
                },
            )));
        }
        if name == "guard" {
            if self.refuse_failure && args == [Value::Bool(true)] {
                return Err(DataError::Unavailable("failure refused".into()));
            }
            return Ok(Answer::Now(Value::Number(0.)));
        }
        self.asks.push(args.to_vec());
        if self.refuse {
            store.set("attempted", "yes").unwrap();
            return Err(DataError::Unavailable("offline".into()));
        }
        let url = "https://example.test/state";
        Ok(Answer::Later(if self.post {
            Request::post_json(url, "{}")
        } else {
            Request::get(url)
        }))
    }

    fn parse(
        &mut self,
        store: &mut Store,
        _: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let Outcome::Response(response) = outcome else {
            return Err(DataError::Unavailable("offline".into()));
        };
        if self.change_prop {
            store.set("revision", "2").unwrap();
        }
        Ok(Answer::Now(Value::str(&format!(
            "{}@{}/{}",
            response.body[0],
            args[0].as_str().unwrap(),
            args[1].as_number().unwrap()
        ))))
    }
}

fn live_view() -> Runner<Source> {
    let src = r#"
component App
  state seq = 0
  state topic = "a"
  state active = true
  state mounted = true
  resource revision = revision() as shape number
  action event
    seq = seq + 1
  action change
    topic = "b"
  action both
    topic = "c"
    seq = seq + 1
  action idle
    active = false
  action remove
    mounted = false
  view
    column
      if mounted
        Readout(seq=seq, topic=topic, active=active, revision=revision)
component Readout
  props
    seq: number
    topic: string
    active: bool
    revision: number
  resource snapshot = load(topic) with seq, revision when active as shape string else empty()
  resource guarded = guard(failed(snapshot)) as shape number
  view
    text `${snapshot} ${current(snapshot)} ${pending(snapshot)} ${failed(snapshot)}` testId="out"
"#;
    Runner::boot(
        contract::compile(src).unwrap(),
        boxed(Snapshots::default()),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn read(r: &mut Runner<Source>) -> u64 {
    let reads = r.take_requests();
    assert_eq!(reads.len(), 1, "one GET /state");
    assert_eq!(reads[0].request.url, "https://example.test/state");
    assert!(!reads[0].forced);
    reads[0].ticket
}

fn snapshot(value: u8) -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: vec![value],
    })
}

#[test]
fn with_events_during_a_kept_read_land_then_ask_once_at_the_latest_arguments() {
    let mut r = live_view();
    let first = read(&mut r);
    r.advance(100.).unwrap();
    for action in ["event", "both", "event"] {
        r.act(action, vec![]).unwrap();
    }
    assert!(r.take_requests().is_empty());
    assert!(r.holds(first));
    r.advance(200.).unwrap();
    r.fulfill(first, snapshot(0)).unwrap();
    assert_eq!(text(&r), "0@c/3 true true false");
    let second = read(&mut r);
    r.advance(300.).unwrap();
    r.act("event", vec![]).unwrap();
    assert_eq!(text(&r), "0@c/3 true true false");
    assert!(r.holds(second));
    r.advance(400.).unwrap();
    r.act("event", vec![]).unwrap();
    r.fulfill(second, snapshot(3)).unwrap();
    assert_eq!(text(&r), "3@c/5 true true false");
    let third = read(&mut r);
    r.advance(500.).unwrap();
    r.act("event", vec![]).unwrap();
    r.advance(600.).unwrap();
    r.fulfill(third, snapshot(5)).unwrap();
    assert_eq!(text(&r), "5@c/6 true true false");
    let last = read(&mut r);
    r.advance(800.).unwrap();
    r.fulfill(last, snapshot(6)).unwrap();
    assert_eq!(text(&r), "6@c/6 true false false");
    assert!(r.take_requests().is_empty());
    assert!(r.pending().is_empty());
}

#[test]
fn a_call_argument_alone_earns_no_follow_up_and_a_failed_kept_read_spends_one() {
    let mut r = live_view();
    let first = read(&mut r);
    r.act("change", vec![]).unwrap();
    r.fulfill(first, snapshot(0)).unwrap();
    assert_eq!(text(&r), "0@b/0 true false false");
    assert!(r.take_requests().is_empty());
    r.act("event", vec![]).unwrap();
    let held = read(&mut r);
    r.act("both", vec![]).unwrap();
    r.fulfill(held, offline()).unwrap();
    assert_eq!(text(&r), "0@b/0 false true false");
    let last = read(&mut r);
    r.fulfill(last, offline()).unwrap();
    assert_eq!(text(&r), "0@b/0 false false true");
    assert!(r.take_requests().is_empty());
}

#[test]
fn a_constant_post_read_is_sent_twice_when_its_with_value_changes_in_flight() {
    let mut r = live_view();
    let first = read(&mut r);
    r.fulfill(first, snapshot(0)).unwrap();
    r.source::<Snapshots>().post = true;
    r.act("change", vec![]).unwrap();
    let post = r.take_requests().remove(0);
    r.act("event", vec![]).unwrap();
    assert!(r.take_requests().is_empty());
    r.fulfill(post.ticket, snapshot(0)).unwrap();
    let second = r.take_requests();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].request, post.request);
    assert_eq!(post.request.method, "POST");
    r.fulfill(second[0].ticket, snapshot(1)).unwrap();
    assert!(r.take_requests().is_empty());
}

#[test]
fn an_idle_or_unmounted_resource_spends_its_kept_read_without_a_follow_up() {
    for action in ["idle", "remove"] {
        let mut r = live_view();
        let held = read(&mut r);
        r.act("event", vec![]).unwrap();
        assert!(r.holds(held));
        r.act(action, vec![]).unwrap();
        assert!(!r.holds(held));
        assert!(r.fulfill(held, snapshot(0)).unwrap().is_none());
        assert!(r.take_requests().is_empty());
        assert!(r.pending().is_empty());
    }
}

#[test]
fn a_refused_follow_up_keeps_the_reply_and_its_failure_stands() {
    let mut r = live_view();
    let held = read(&mut r);
    r.act("event", vec![]).unwrap();
    r.source::<Snapshots>().refuse = true;
    r.fulfill(held, snapshot(0)).unwrap();
    assert_eq!(text(&r), "0@a/1 true false true");
    assert!(!r
        .store()
        .snapshot()
        .iter()
        .any(|(key, _)| key == "attempted"));
    r.advance(200.).unwrap();
    assert!(r.take_requests().is_empty());
}

#[test]
fn the_follow_up_uses_the_props_committed_by_the_parser() {
    let mut r = live_view();
    let first = read(&mut r);
    r.act("event", vec![]).unwrap();
    r.source::<Snapshots>().change_prop = true;
    r.source::<Snapshots>().asks.clear();
    r.fulfill(first, snapshot(0)).unwrap();
    let next = read(&mut r);
    r.fulfill(next, snapshot(1)).unwrap();
    assert_eq!(
        r.source::<Snapshots>().asks,
        [vec![Value::str("a"), Value::Number(1.), Value::Number(2.)]]
    );
    assert!(r.take_requests().is_empty());
}

#[test]
fn refusing_to_publish_a_failure_leaves_the_last_receipt_and_allows_later_asks() {
    for parse_fails in [false, true] {
        let mut r = live_view();
        let first = read(&mut r);
        r.act("event", vec![]).unwrap();
        r.source::<Snapshots>().refuse = true;
        r.source::<Snapshots>().refuse_failure = true;
        if parse_fails {
            assert!(r.fulfill(first, offline()).is_err());
        } else {
            assert!(r.fulfill(first, snapshot(5)).unwrap().is_some());
            assert!(text(&r).starts_with("5@a/1"));
        }
        r.source::<Snapshots>().refuse = false;
        r.source::<Snapshots>().refuse_failure = false;
        r.act("event", vec![]).unwrap();
        let next = read(&mut r);
        r.fulfill(next, snapshot(6)).unwrap();
        assert_eq!(text(&r), "6@a/2 true false false");
        assert!(r.take_requests().is_empty());
    }
}
