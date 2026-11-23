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
