//! A kept answer's identity read from another resource (LLP 1027.005 D3,
//! 2026-10-10): `feeds(gen, account)`, where `account` reads the store.
use super::*;
use crate::{Answer, Outcome, Request, Store};
use exact_kernel::{Kernel, NodeType};
use exact_plan::{asm::Asm, builder::PlanBuilder, Plan, TypeKind, Value};

#[derive(Default)]
struct Source {
    ready: bool,
    asks: Vec<(String, Vec<Value>)>,
}

impl DataSource for Source {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("the fixture implements answer")
    }

    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        assert!(
            self.ready,
            "a deferred source must not run before readiness"
        );
        self.asks.push((source.to_string(), args.to_vec()));
        let key = store.get("token").unwrap_or("public").to_string();
        Ok(match source {
            "account" => Answer::Now(Value::str(&key)),
            _ => Answer::Later(Request::get(&format!(
                "https://example.test/{}",
                args[1].text()
            ))),
        })
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        args: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::str(&format!(
            "feeds of {}",
            args[1].text()
        ))))
    }

    fn ready(&self) -> bool {
        self.ready
    }

    fn grants(&self) -> &str {
        "secret.keep token\nnet.fetch https://example.test"
    }
}

/// `account(0)` (placeholder "nobody") and `feeds(gen, key)`, the key read
/// straight from `account` or, with `through_derive`, through a derive.
fn plan(through_derive: bool) -> Plan {
    plan_with(through_derive, false, true)
}

/// As [`plan`]; `key_is_context` makes the key `feeds`' `with` context, and
/// `account_reads_store` false makes `account` a plain compiled resource.
fn plan_with(through_derive: bool, key_is_context: bool, account_reads_store: bool) -> Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let number = b.primitive(TypeKind::Number);
    let zero = b.constant(&Value::Number(0.));
    let gen = b.slot("gen", number, zero);
    let mut a = Asm::new();
    a.number(0.);
    let a = b.code(a);
    let account = b.resource(
        "account",
        "account",
        &[a],
        string,
        Some(&Value::str("nobody")),
    );
    b.set_resource_reader(account, account_reads_store);
    b.set_resource_initial_args(account, &[Value::Number(0.)]);
    let mut g = Asm::new();
    g.load_slot(gen);
    let g = b.code(g);
    let mut k = Asm::new();
    if through_derive {
        let mut d = Asm::new();
        d.load_resource(account);
        let d = b.code(d);
        let key = b.derive("key", string, d);
        k.load_derive(key);
    } else {
        k.load_resource(account);
    }
    let k = b.code(k);
    let feeds = b.resource(
        "feeds",
        "feeds",
        &[g, k],
        string,
        Some(&Value::str("loading")),
    );
    b.set_resource_reader(feeds, true);
    b.set_resource_context(feeds, u16::from(key_is_context));
    b.set_resource_initial_args(feeds, &[Value::Number(0.), Value::str("nobody")]);
    let mut body = Asm::new();
    body.load_param(0).store_slot(gen);
    let body = b.code(body);
    b.action("gen", &[("value", number)], &[gen], body);
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    b.finish().unwrap()
}

/// `feeds(gen, key)` with the key read from a root `state key = ""` that
/// the action `key` sets, as an app sets the signed-in user's id once its
/// session answers.
fn plan_keyed_by_state() -> Plan {
    plan_keyed(false)
}

/// As [`plan_keyed_by_state`]; `gated` adds `when open`, a root state that
/// starts false and the action `open` sets.
fn plan_keyed(gated: bool) -> Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let number = b.primitive(TypeKind::Number);
    let zero = b.constant(&Value::Number(0.));
    let gen = b.slot("gen", number, zero);
    let empty = b.constant(&Value::str(""));
    let key = b.slot("key", string, empty);
    let mut g = Asm::new();
    g.load_slot(gen);
    let g = b.code(g);
    let mut k = Asm::new();
    k.load_slot(key);
    let k = b.code(k);
    let feeds = b.resource(
        "feeds",
        "feeds",
        &[g, k],
        string,
        Some(&Value::str("loading")),
    );
    b.set_resource_reader(feeds, true);
    b.set_resource_initial_args(feeds, &[Value::Number(0.), Value::str("")]);
    if gated {
        let boolean = b.primitive(TypeKind::Bool);
        let no = b.constant(&Value::Bool(false));
        let open = b.slot("open", boolean, no);
        let mut w = Asm::new();
        w.load_slot(open);
        let w = b.code(w);
        b.set_resource_gate(feeds, w);
        let mut body = Asm::new();
        body.load_param(0).store_slot(open);
        let body = b.code(body);
        b.action("open", &[("value", boolean)], &[open], body);
    }
    let mut body = Asm::new();
    body.load_param(0).store_slot(key);
    let body = b.code(body);
    b.action("key", &[("value", string)], &[key], body);
    let mut body = Asm::new();
    body.load_param(0).store_slot(gen);
    let body = b.code(body);
    b.action("gen", &[("value", number)], &[gen], body);
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    b.finish().unwrap()
}

fn launch(through_derive: bool, snapshot: Vec<(String, Vec<u8>)>) -> Runner<Source> {
    launch_plan(plan(through_derive), snapshot)
}

fn launch_plan(plan: Plan, snapshot: Vec<(String, Vec<u8>)>) -> Runner<Source> {
    Runner::boot_stored(
        plan,
        Source::default(),
        Kernel::with_monospace(),
        snapshot,
        Default::default(),
        "/",
    )
    .unwrap()
}

fn settle_requests(r: &mut Runner<Source>) {
    for q in r.take_requests() {
        r.fulfill(q.ticket, Outcome::Storage(vec![])).unwrap();
    }
}

fn shows(r: &Runner<Source>, name: &str, value: &str) {
    assert_eq!(r.resource(name), Some(&Value::str(value)), "{name}");
}

fn feeds_kept(args: &[Value], value: &str) -> (String, Vec<u8>) {
    (
        kept::kept_name("feeds"),
        kept::encode(args, &Value::str(value)),
    )
}

#[test]
fn a_sign_in_rekeeps_both_and_the_next_launch_shows_the_signed_in_answers() {
    let mut r = launch(false, vec![]);
    r.data.ready = true;
    r.data_ready().unwrap();
    settle_requests(&mut r);
    shows(&r, "feeds", "feeds of public");
    // Signing in writes the secret; the store-reading resources answer again.
    r.store.set("token", "alice").unwrap();
    r.act("gen", vec![Value::Number(0.)]).unwrap();
    settle_requests(&mut r);
    shows(&r, "account", "alice");
    shows(&r, "feeds", "feeds of alice");
    let next = launch(false, r.store.snapshot());
    shows(&next, "account", "alice");
    shows(&next, "feeds", "feeds of alice");
    assert!(next.journal().any(
        |l| l.contains("feeds shows its kept answer from the last session, kept for [0,\"alice\"]")
    ));
}

#[test]
fn an_identity_read_from_a_resource_on_its_placeholder_does_not_admit_a_kept_answer() {
    for through_derive in [false, true] {
        // The account kept nothing (a first launch after an update, an
        // answer over the budget), so the first frame's key is the
        // placeholder's, which another launch's entry happens to match.
        let mut r = launch(
            through_derive,
            vec![feeds_kept(
                &[Value::Number(0.), Value::str("nobody")],
                "feeds of somebody else",
            )],
        );
        shows(&r, "account", "nobody");
        shows(&r, "feeds", "loading");
        assert!(!r.journal().any(|l| l.contains("shows its kept answer")));
        assert_eq!(lines_with(&r, WAITING).len(), 1);
        // At readiness both are asked for what they are now.
        r.data.ready = true;
        r.data_ready().unwrap();
        settle_requests(&mut r);
        shows(&r, "feeds", "feeds of public");
    }
}

#[test]
fn a_refused_kept_answer_equal_to_the_compiled_one_is_still_a_fallback() {
    for through_derive in [false, true] {
        // The account kept "nobody" for other arguments: refused, it shows
        // the compiled "nobody", which the dependent's identity must not
        // take for this launch's answer.
        let r = launch(
            through_derive,
            vec![
                (
                    kept::kept_name("account"),
                    kept::encode(&[Value::Number(1.)], &Value::str("nobody")),
                ),
                feeds_kept(
                    &[Value::Number(0.), Value::str("nobody")],
                    "feeds of somebody else",
                ),
            ],
        );
        shows(&r, "account", "nobody");
        shows(&r, "feeds", "loading");
        assert!(!r.journal().any(|l| l.contains("shows its kept answer")));
    }
}

#[test]
fn an_identity_read_from_a_kept_answer_still_admits_its_dependent() {
    for through_derive in [false, true] {
        let r = launch(
            through_derive,
            vec![
                (
                    kept::kept_name("account"),
                    kept::encode(&[Value::Number(0.)], &Value::str("alice")),
                ),
                feeds_kept(&[Value::Number(0.), Value::str("alice")], "feeds of alice"),
            ],
        );
        shows(&r, "account", "alice");
        shows(&r, "feeds", "feeds of alice");
        assert!(r
            .journal()
            .any(|l| l.contains("account shows its kept answer")));
    }
}

#[test]
fn a_placeholder_read_only_by_with_context_does_not_refuse_the_identity() {
    // `feeds(gen) with key`: the identity is `gen`, which is known.
    let r = launch_plan(
        plan_with(false, true, true),
        vec![feeds_kept(&[Value::Number(0.)], "feeds kept for gen 0")],
    );
    shows(&r, "account", "nobody");
    shows(&r, "feeds", "feeds kept for gen 0");
}

#[test]
fn a_compiled_value_of_a_resource_that_reads_no_store_is_a_known_identity() {
    let r = launch_plan(
        plan_with(false, false, false),
        vec![feeds_kept(
            &[Value::Number(0.), Value::str("nobody")],
            "feeds of nobody",
        )],
    );
    shows(&r, "feeds", "feeds of nobody");
}

#[test]
fn a_long_identity_is_journaled_shortened_with_a_mark() {
    let line = super::lines::kept_shown("feeds", &"x".repeat(300));
    assert!(
        line.ends_with(&format!("kept for {}…", "x".repeat(200))),
        "{line}"
    );
    let line = super::lines::kept_shown("feeds", "[0]");
    assert!(line.ends_with("kept for [0]"), "{line}");
}

#[test]
fn a_refused_kept_answer_is_journaled_with_why() {
    // The account kept for other arguments; feeds kept for a key read from
    // the account's fallback, which waits for the account's answer.
    let mut r = launch(
        false,
        vec![
            (
                kept::kept_name("account"),
                kept::encode(&[Value::Number(1.)], &Value::str("alice")),
            ),
            feeds_kept(
                &[Value::Number(0.), Value::str("nobody")],
                "feeds of nobody",
            ),
        ],
    );
    let refused = |r: &Runner<Source>| lines_with(r, "did not use its kept answer");
    let lines = refused(&r);
    assert!(
        lines.iter().any(|l| l.ends_with(
            "account did not use its kept answer: kept for [1], this launch asks for [0]: identifying argument 1 is 0"
        )),
        "{lines:?}"
    );
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines_with(&r, WAITING).len(), 1);
    // At readiness the account answers someone else: a known mismatch.
    r.data.ready = true;
    r.data_ready().unwrap();
    settle_requests(&mut r);
    let lines = refused(&r);
    assert!(
        lines.iter().any(|l| l.ends_with(
            "feeds did not use its kept answer: kept for [0,\"nobody\"], this launch asks for [0,\"public\"]: identifying argument 2 is \"public\", reading resource account = \"public\" (a store reader with no admitted device answer)"
        )),
        "{lines:?}"
    );
    assert_eq!(lines.len(), 2, "once each: {lines:?}");
}

#[test]
fn a_kept_entry_that_no_longer_decodes_is_journaled() {
    let r = launch(false, vec![(kept::kept_name("account"), vec![1, 2, 3])]);
    assert!(r
        .journal()
        .any(|l| l.ends_with("account did not use its kept answer: it no longer decodes or fits")));
}

/// A launch of [`plan_keyed_by_state`] with `feeds` kept for `"alice"`.
fn launch_keyed() -> Runner<Source> {
    launch_plan(
        plan_keyed_by_state(),
        vec![feeds_kept(
            &[Value::Number(0.), Value::str("alice")],
            "kept feeds of alice",
        )],
    )
}

fn lines_with(r: &Runner<Source>, text: &str) -> Vec<String> {
    (r.journal())
        .filter(|l| l.contains(text))
        .map(str::to_string)
        .collect()
}

const WAITING: &str = "feeds's kept answer is waiting for its identifying arguments";

#[test]
fn a_kept_answer_waits_for_an_empty_identity_and_shows_at_its_match_before_readiness() {
    let mut r = launch_keyed();
    shows(&r, "feeds", "loading");
    assert_eq!(lines_with(&r, WAITING).len(), 1);
    assert!(lines_with(&r, "did not use its kept answer").is_empty());
    // The id arrives before the data source is ready.
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "kept feeds of alice");
    let shown = "feeds shows its kept answer from the last session, kept for [0,\"alice\"]";
    assert_eq!(lines_with(&r, shown).len(), 1);
    // Readiness asks it, as it asks a seed shown at boot.
    r.data.ready = true;
    r.data_ready().unwrap();
    let asked = r.take_requests();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].request.url, "https://example.test/alice");
    r.fulfill(asked[0].ticket, Outcome::Storage(vec![]))
        .unwrap();
    shows(&r, "feeds", "feeds of alice");
}

#[test]
fn a_kept_answer_admitted_after_readiness_shows_while_its_own_request_goes_out() {
    let mut r = launch_keyed();
    r.data.ready = true;
    r.data_ready().unwrap();
    // Readiness asks for the empty id; that request is still in flight.
    let empty = r.take_requests();
    assert_eq!(empty.len(), 1);
    assert_eq!(empty[0].request.url, "https://example.test/");
    shows(&r, "feeds", "loading");
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "kept feeds of alice");
    let asked = r.take_requests();
    assert_eq!(asked.len(), 1, "the admitted seed is not the answer");
    assert_eq!(asked[0].request.url, "https://example.test/alice");
    // The empty id's reply, superseded, changes nothing.
    r.fulfill(empty[0].ticket, Outcome::Storage(vec![]))
        .unwrap();
    shows(&r, "feeds", "kept feeds of alice");
    r.fulfill(asked[0].ticket, Outcome::Storage(vec![]))
        .unwrap();
    shows(&r, "feeds", "feeds of alice");
}

#[test]
fn a_known_mismatch_consumes_a_waiting_kept_answer_and_names_the_argument() {
    let mut r = launch_keyed();
    r.act("key", vec![Value::str("bob")]).unwrap();
    let lines = lines_with(&r, "did not use its kept answer");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].ends_with(
            "feeds did not use its kept answer: kept for [0,\"alice\"], this launch asks for [0,\"bob\"]: identifying argument 2 is \"bob\", reading state key = \"bob\""
        ),
        "{lines:?}"
    );
    // Consumed: changing the id back does not readmit it.
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "loading");
    assert!(lines_with(&r, "shows its kept answer").is_empty());
}

#[test]
fn an_empty_identity_that_stays_empty_keeps_the_entry_waiting() {
    // A search field's empty query: nothing is wrong, and nothing refuses.
    let mut r = launch_keyed();
    r.act("gen", vec![Value::Number(0.)]).unwrap();
    r.act("key", vec![Value::str("")]).unwrap();
    shows(&r, "feeds", "loading");
    assert_eq!(lines_with(&r, WAITING).len(), 1, "said once");
    assert!(lines_with(&r, "did not use its kept answer").is_empty());
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "kept feeds of alice");
}

#[test]
fn a_mismatch_names_a_resource_input_and_its_kept_answer() {
    // The account's own kept answer is admitted and names another user.
    let r = launch(
        false,
        vec![
            (
                kept::kept_name("account"),
                kept::encode(&[Value::Number(0.)], &Value::str("carol")),
            ),
            feeds_kept(&[Value::Number(0.), Value::str("alice")], "feeds of alice"),
        ],
    );
    shows(&r, "account", "carol");
    shows(&r, "feeds", "loading");
    let lines = lines_with(&r, "feeds did not use its kept answer");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].ends_with(
            ": identifying argument 2 is \"carol\", reading resource account = \"carol\" (holding an admitted kept answer)"
        ),
        "{lines:?}"
    );
}

#[test]
fn a_reload_keeps_a_waiting_kept_answer() {
    let r = launch_keyed();
    let carried = r.carry();
    assert_eq!(carried.waiting.len(), 1);
    let mut next = Runner::boot_carrying(
        plan_keyed_by_state(),
        Source {
            ready: true,
            ..Default::default()
        },
        Kernel::with_monospace(),
        &carried,
        Default::default(),
        "/",
    )
    .unwrap();
    // The empty id's ask stays in flight: an answer to it would replace the entry.
    assert_eq!(next.take_requests().len(), 1);
    next.act("key", vec![Value::str("alice")]).unwrap();
    shows(&next, "feeds", "kept feeds of alice");
    let asked = next.take_requests();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].request.url, "https://example.test/alice");
}

#[test]
fn an_answer_kept_in_its_place_consumes_a_waiting_kept_answer() {
    let mut r = launch_keyed();
    r.data.ready = true;
    r.data_ready().unwrap();
    // The empty id's answer is kept, replacing the entry that waited.
    settle_requests(&mut r);
    shows(&r, "feeds", "feeds of ");
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "feeds of ");
    assert!(lines_with(&r, "shows its kept answer").is_empty());
}

/// A launch of [`plan_keyed`] gated, `feeds` kept for `"alice"`, made
/// ready while `feeds` is idle, with the key set to `key` before the gate
/// opens.
fn opened_after_readiness(key: &str) -> Runner<Source> {
    let mut r = launch_plan(
        plan_keyed(true),
        vec![feeds_kept(
            &[Value::Number(0.), Value::str("alice")],
            "kept feeds of alice",
        )],
    );
    r.data.ready = true;
    r.data_ready().unwrap();
    assert!(
        r.take_requests().is_empty(),
        "an idle resource asks nothing"
    );
    r.act("key", vec![Value::str(key)]).unwrap();
    r.act("open", vec![Value::Bool(true)]).unwrap();
    r
}

#[test]
fn a_kept_answer_idle_through_readiness_is_compared_when_its_gate_opens() {
    // The same identity: it stands while its ask goes out.
    let mut r = opened_after_readiness("alice");
    shows(&r, "feeds", "kept feeds of alice");
    let asked = r.take_requests();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].request.url, "https://example.test/alice");
    // Another: refused, named, and the placeholder shows.
    let mut r = opened_after_readiness("bob");
    shows(&r, "feeds", "");
    let lines = lines_with(&r, "feeds did not use its kept answer");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].ends_with("reading state key = \"bob\""),
        "{lines:?}"
    );
    assert_eq!(r.take_requests().len(), 1);
    // Still empty: it waits, and the id's arrival admits it.
    let mut r = opened_after_readiness("");
    shows(&r, "feeds", "loading");
    assert_eq!(lines_with(&r, WAITING).len(), 1);
    assert_eq!(r.take_requests().len(), 1, "the empty id is asked");
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "kept feeds of alice");
}
