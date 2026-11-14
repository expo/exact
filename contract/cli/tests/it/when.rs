//! LLP 1161.002: a resource that asks nothing while its `when` is false, and
//! what `current(x)` says of it (LLP 1161.001 D1).

use exact_kernel::Kernel;
use exact_runner::{
    Answer, DataError, DataSource, Outcome, Request, Response, Runner, Store, Value,
};

/// `search(q)` answers later, one request per query.
#[derive(Default)]
struct Search {
    asks: usize,
}

impl DataSource for Search {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!()
    }
    fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
        self.asks += 1;
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
        Ok(Answer::Now(Value::str(&format!(
            "for {}",
            args[0].as_str().unwrap_or("")
        ))))
    }
}

const SRC: &str = "component App
  state open = false
  state query = \"a\"
  resource results = search(query) when open as shape string else empty()
  derive shown = current(results)
  derive busy = pending(results)
  action show
    open = true
  action hide
    open = false
  action search(q: string)
    query = q
  action again
    refresh results
  view
    text results
";

fn boot() -> Runner<Search> {
    Runner::boot(
        contract::compile(SRC).unwrap(),
        Search::default(),
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

fn flag<D: DataSource>(r: &Runner<D>, name: &str) -> bool {
    r.derive(name) == Some(&Value::Bool(true))
}

/// Lands the one request out.
fn land(r: &mut Runner<Search>) {
    let requests = r.take_requests();
    assert_eq!(requests.len(), 1, "one request out");
    r.fulfill(requests[0].ticket, ok()).unwrap();
}

#[test]
fn a_closed_gate_asks_nothing_at_boot() {
    let r = boot();
    assert_eq!(r.data_ref().asks, 0);
    assert_eq!(r.resource("results"), Some(&Value::str("")));
    assert!(!flag(&r, "busy"));
    assert!(!flag(&r, "shown"));
}

#[test]
fn opening_asks_and_its_answer_is_current() {
    let mut r = boot();
    r.act("show", vec![]).unwrap();
    assert!(flag(&r, "busy"));
    land(&mut r);
    assert!(flag(&r, "shown"));
    assert_eq!(r.resource("results"), Some(&Value::str("for a")));
}

#[test]
fn closing_lets_go_of_the_request_and_keeps_the_value() {
    let mut r = boot();
    r.act("show", vec![]).unwrap();
    land(&mut r);
    r.act("search", vec![Value::str("b")]).unwrap();
    let out = r.take_requests();
    assert_eq!(out.len(), 1);
    r.act("hide", vec![]).unwrap();
    assert!(!flag(&r, "busy"), "nothing in flight while idle");
    assert!(!flag(&r, "shown"), "an idle value is not current");
    assert_eq!(
        r.resource("results"),
        Some(&Value::str("for a")),
        "the value stays"
    );
    // The reply of the request let go changes nothing.
    let _ = r.fulfill(out[0].ticket, ok());
    assert_eq!(r.resource("results"), Some(&Value::str("for a")));
}

#[test]
fn waking_asks_again_and_is_not_current_until_it_answers() {
    let mut r = boot();
    r.act("show", vec![]).unwrap();
    land(&mut r);
    r.act("hide", vec![]).unwrap();
    let asks = r.data_ref().asks;
    r.act("show", vec![]).unwrap();
    assert_eq!(
        r.data_ref().asks,
        asks + 1,
        "the same arguments, asked again"
    );
    assert!(
        !flag(&r, "shown"),
        "the value kept across idleness is not current"
    );
    assert!(flag(&r, "busy"));
    land(&mut r);
    assert!(flag(&r, "shown"));
}

#[test]
fn a_refresh_while_idle_asks_nothing() {
    let mut r = boot();
    r.act("show", vec![]).unwrap();
    land(&mut r);
    r.act("hide", vec![]).unwrap();
    let asks = r.data_ref().asks;
    r.act("again", vec![]).unwrap();
    assert_eq!(r.data_ref().asks, asks);
    assert!(r.take_requests().is_empty());
}

#[test]
fn a_gate_is_a_bool_and_a_family_takes_none() {
    let not_bool = "component App\n  resource r = src() when 1 as shape string else empty()\n  view\n    text r\n";
    let e = contract::compile(not_bool).unwrap_err();
    assert!(format!("{e:?}").contains("type-when-not-bool"), "{e:?}");
    let family = "component App\n  state ids = [\"a\"]\n  resource r = src(i) for i in ids when true as shape string\n  view\n    text \"x\"\n";
    let e = contract::compile(family).unwrap_err();
    assert!(
        format!("{e:?}").contains("syntax-resource-for-when"),
        "{e:?}"
    );
}

#[test]
fn lean_carries_the_gate_and_current() {
    let emitted = contract::lean::lean(SRC, "gate").unwrap();
    assert!(
        emitted.contains(
            "{ name := \"results\", ty := .string, source := \"search\", args := [(.var \"query\")], gate := .some ((.var \"open\")) }"
        ),
        "{emitted}"
    );
    assert!(
        emitted.contains("(.call \"current\" [(.var \"results\")])"),
        "{emitted}"
    );
}

/// `profile()` answers the store's `token`, or a number outside its shape for
/// `bad`; `signIn()` writes the token.
struct Profile;

impl DataSource for Profile {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!()
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        _: &[Value],
    ) -> Result<Answer, DataError> {
        if source == "signIn" {
            store.set("token", "ada").unwrap();
        }
        Ok(Answer::Now(match store.get("token") {
            Some("bad") => Value::Number(1.0),
            token => Value::str(token.unwrap_or("guest")),
        }))
    }
    fn grants(&self) -> &str {
        "secret.keep token\n"
    }
}

/// A store reader asleep behind a closed gate asks nothing, so a store write,
/// the app's own or a peer's, settles without it and is not remembered:
/// waking asks as with no write before, reads the store as it is then, and an
/// answer outside its shape refuses the action that opens the gate.
#[test]
fn a_store_write_settles_while_a_store_reader_sleeps() {
    let src = "component App
  state open = true
  resource me = profile() when open as shape string else empty()
  mutation signedIn as shape string
  action show
    open = true
  action hide
    open = false
  action signIn
    send signedIn = signIn()
  view
    text me
";
    let mut r = Runner::boot(
        contract::compile(src).unwrap(),
        Profile,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let peer = |token: &str| vec![("token".to_string(), token.as_bytes().to_vec())];
    r.act("hide", vec![]).unwrap();
    r.act("signIn", vec![]).unwrap();
    r.store_changed(peer("bob")).unwrap();
    assert_eq!(r.resource("me"), Some(&Value::str("guest")));
    r.act("show", vec![]).unwrap();
    assert_eq!(r.resource("me"), Some(&Value::str("bob")));
    r.act("hide", vec![]).unwrap();
    r.store_changed(peer("bad")).unwrap();
    assert!(r.act("show", vec![]).is_err());
}
