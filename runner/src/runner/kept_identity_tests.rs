//! A kept answer's identity read from another resource (LLP 1027.005 D3,
//! 2026-10-10): `feeds(gen, account)`, where `account` reads the store.
use super::*;
use crate::{Answer, Outcome, Request, Store};
use exact_kernel::{Kernel, NodeType, PropId};
use exact_plan::{asm::Asm, builder::PlanBuilder, BindingKind, BindingsRow, Plan, TypeKind, Value};

#[derive(Default)]
struct Source {
    ready: bool,
    asks: Vec<(String, Vec<Value>)>,
    /// `account` answers by request, not at once.
    account_later: bool,
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
            "account" if self.account_later => {
                Answer::Later(Request::get("https://example.test/account"))
            }
            "account" => Answer::Now(Value::str(&key)),
            _ => Answer::Later(Request::get(&format!(
                "https://example.test/{}",
                args[1].text()
            ))),
        })
    }

    fn parse(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        if source == "account" {
            return Ok(Answer::Now(Value::str(
                store.get("token").unwrap_or("public"),
            )));
        }
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

/// `account(version)` and `feeds(account) with version`: both build-time
/// answers name `"alice"`, so returning to the account fallback keeps the
/// feed identity equal while its input is not yet known.
fn plan_with_versioned_account() -> Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let number = b.primitive(TypeKind::Number);
    let zero = b.constant(&Value::Number(0.));
    let version = b.slot("version", number, zero);
    let mut v = Asm::new();
    v.load_slot(version);
    let v = b.code(v);
    let account = b.resource(
        "account",
        "account",
        &[v],
        string,
        Some(&Value::str("alice")),
    );
    b.set_resource_reader(account, true);
    b.set_resource_initial_args(account, &[Value::Number(0.)]);
    let mut a = Asm::new();
    a.load_resource(account);
    let a = b.code(a);
    let feeds = b.resource(
        "feeds",
        "feeds",
        &[a, v],
        string,
        Some(&Value::str("loading")),
    );
    b.set_resource_reader(feeds, true);
    b.set_resource_context(feeds, 1);
    b.set_resource_initial_args(feeds, &[Value::str("alice"), Value::Number(0.)]);
    let mut body = Asm::new();
    body.load_param(0).store_slot(version);
    let body = b.code(body);
    b.action("version", &[("value", number)], &[version], body);
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
    plan_keyed_with(gated.then_some(false), true)
}

/// As [`plan_keyed`], gated when `gate` gives `open`'s initial value;
/// without `compiled`, `feeds` has no build-time answer, so its placeholder
/// stands in before readiness.
fn plan_keyed_with(gate: Option<bool>, compiled: bool) -> Plan {
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
    let loading = Value::str("loading");
    let feeds = b.resource(
        "feeds",
        "feeds",
        &[g, k],
        string,
        compiled.then_some(&loading),
    );
    b.set_resource_reader(feeds, true);
    if compiled {
        b.set_resource_initial_args(feeds, &[Value::Number(0.), Value::str("")]);
    }
    if let Some(initial) = gate {
        let boolean = b.primitive(TypeKind::Bool);
        let initial = b.constant(&Value::Bool(initial));
        let open = b.slot("open", boolean, initial);
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

fn binding(kind: BindingKind, id: u16, expr: exact_plan::Code) -> BindingsRow {
    BindingsRow {
        kind,
        id,
        expr,
        written: true,
    }
}

fn password_plan(initial: &str, mount_control: bool) -> Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let initial_value = b.constant(&Value::str(initial));
    let password = b.slot("password", string, initial_value);
    let mut identity = Asm::new();
    identity.load_slot(password);
    let identity = b.code(identity);
    let feeds = b.resource(
        "feeds",
        "feeds",
        &[identity],
        string,
        Some(&Value::str("loading")),
    );
    b.set_resource_reader(feeds, true);
    b.set_resource_initial_args(feeds, &[Value::str(initial)]);
    let mut body = Asm::new();
    body.load_param(0).store_slot(password);
    let body = b.code(body);
    b.action("password", &[("value", string)], &[password], body);
    if mount_control {
        let password_type = b.constant(&Value::str("password"));
        let mut value = Asm::new();
        value.load_slot(password);
        let value = b.code(value);
        b.node(
            NodeType::Control as u8,
            None,
            None,
            0,
            &[
                binding(BindingKind::Prop, PropId::Type as u16, password_type),
                binding(BindingKind::Prop, PropId::Value as u16, value),
            ],
            &[],
            None,
        );
    } else {
        b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    }
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
    assert!(next
        .journal()
        .any(|l| l.contains("feeds shows its kept answer from the last session")));
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
fn kept_identity_diagnostics_never_include_values() {
    use sha2::{Digest, Sha256};

    let kept = "Q7mZ4pL9vR2xN8cK6tH3sW5yF1dB0jU4eI7oP9aS2gD8";
    let current = "V6nC1qX8rM3kT9wH5fJ2zL7pA4sE0uY6iO1dG8bR3cK9mN";
    let mut launches = vec![launch_plan(
        password_plan("", false),
        vec![feeds_kept(&[Value::str(kept)], "kept result")],
    )];
    launches.push(launch_plan(
        password_plan(kept, true),
        vec![feeds_kept(&[Value::str(kept)], "kept result")],
    ));
    let mut assigned = launch_plan(
        password_plan(kept, false),
        vec![feeds_kept(&[Value::str(kept)], "kept result")],
    );
    assigned.act("password", vec![Value::str(current)]).unwrap();
    launches.push(assigned);
    let mut cleared = launch_plan(
        password_plan(kept, true),
        vec![feeds_kept(&[Value::str(kept)], "kept result")],
    );
    cleared.act("password", vec![Value::str("")]).unwrap();
    launches.push(cleared);

    let hash = |value: &str| {
        Sha256::digest(value.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let mut forbidden = Vec::new();
    for value in [kept, current] {
        forbidden.push(value.to_string());
        forbidden.extend(
            value
                .as_bytes()
                .windows(8)
                .map(|part| String::from_utf8(part.to_vec()).unwrap()),
        );
        forbidden.push(value.len().to_string());
        forbidden.push(hash(value));
    }
    for r in launches {
        let journal = r.journal().collect::<Vec<_>>().join("\n");
        let logs = crate::agent::logs(&r, 0);
        assert!(
            journal.contains("state password")
                || journal.contains("kept answer is waiting")
                || journal.contains("shows its kept answer from the last session"),
            "{journal}"
        );
        for value in &forbidden {
            assert!(
                !journal.contains(value) && !logs.contains(value),
                "forbidden identifying value or measurement {value:?}\njournal:\n{journal}\nlogs:\n{logs}"
            );
        }
    }
}

#[test]
fn a_shown_kept_answer_names_only_its_resource() {
    assert_eq!(
        super::lines::kept_shown("feeds"),
        "feeds shows its kept answer from the last session"
    );
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
            "account did not use its kept answer: identifying argument 1 differs from the one it was kept for"
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
            "feeds did not use its kept answer: identifying argument 2 differs from the one it was kept for, reading resource account (holding a fresh answer)"
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
    let shown = "feeds shows its kept answer from the last session";
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
            "feeds did not use its kept answer: identifying argument 2 differs from the one it was kept for, reading state key"
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
            "feeds did not use its kept answer: identifying argument 2 differs from the one it was kept for, reading resource account (holding an admitted kept answer)"
        ),
        "{lines:?}"
    );
}

#[test]
fn an_admitted_seed_with_an_unknown_unchanged_identity_names_its_input() {
    let mut r = launch_plan(
        plan_with_versioned_account(),
        vec![
            (
                kept::kept_name("account"),
                kept::encode(&[Value::Number(0.)], &Value::str("alice")),
            ),
            feeds_kept(&[Value::str("alice")], "feeds of alice"),
        ],
    );
    shows(&r, "account", "alice");
    shows(&r, "feeds", "feeds of alice");

    r.act("version", vec![Value::Number(1.)]).unwrap();

    shows(&r, "account", "alice");
    shows(&r, "feeds", "loading");
    let lines = lines_with(&r, "feeds did not use its kept answer");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].ends_with(
            "feeds did not use its kept answer: identifying argument 1 is not yet known, reading resource account (a store reader with no admitted device answer)"
        ),
        "{lines:?}"
    );
}

#[test]
fn an_admitted_kept_answer_under_with_context_is_current() {
    // Its entry holds only the identifying argument, the account; the
    // context, `version`, is not compared (LLP 1161.001 D1).
    let r = launch_plan(
        plan_with_versioned_account(),
        vec![
            (
                kept::kept_name("account"),
                kept::encode(&[Value::Number(0.)], &Value::str("alice")),
            ),
            feeds_kept(&[Value::str("alice")], "feeds of alice"),
        ],
    );
    shows(&r, "feeds", "feeds of alice");
    assert!(r.resources[1].current, "a kept answer to this account");
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
    assert!(lines[0].ends_with("reading state key"), "{lines:?}");
    assert_eq!(r.take_requests().len(), 1);
    // Still empty: it waits, and the id's arrival admits it.
    let mut r = opened_after_readiness("");
    shows(&r, "feeds", "loading");
    assert_eq!(lines_with(&r, WAITING).len(), 1);
    assert_eq!(r.take_requests().len(), 1, "the empty id is asked");
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "kept feeds of alice");
}

#[test]
fn a_fresh_answer_equal_to_the_build_time_one_is_a_known_identity() {
    // The account's compiled answer is "nobody"; the signed-out store
    // answers "nobody" too, freshly, and the entry kept for it is this
    // launch's.
    let mut r = launch(
        false,
        vec![feeds_kept(
            &[Value::Number(0.), Value::str("nobody")],
            "feeds of nobody, kept",
        )],
    );
    shows(&r, "feeds", "loading");
    r.store.set("token", "nobody").unwrap();
    r.data.ready = true;
    r.data_ready().unwrap();
    shows(&r, "account", "nobody");
    shows(&r, "feeds", "feeds of nobody, kept");
    let asked = r.take_requests();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].request.url, "https://example.test/nobody");
}

#[test]
fn a_kept_answer_admitted_after_its_gate_wakes_is_not_current_until_it_answers() {
    // Idle through readiness, then woken with no id: it waits behind the
    // bake's answer, and the id's arrival admits a value that slept. Neither
    // is an answer since the wake.
    let mut r = opened_after_readiness("");
    shows(&r, "feeds", "loading");
    assert!(!r.resources[0].current, "the bake's answer");
    assert_eq!(r.take_requests().len(), 1);
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "kept feeds of alice");
    assert!(!r.resources[0].current, "a value kept across idleness");
    let asked = r.take_requests();
    assert_eq!(asked.len(), 1);
    r.fulfill(asked[0].ticket, Outcome::Storage(vec![]))
        .unwrap();
    assert!(r.resources[0].current);
}

#[test]
fn an_admitted_kept_answer_that_leaves_its_identity_does_not_wait_again() {
    let mut r = launch_keyed();
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "kept feeds of alice");
    r.act("key", vec![Value::str("")]).unwrap();
    shows(&r, "feeds", "loading");
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "loading");
    assert_eq!(lines_with(&r, WAITING).len(), 1, "it waited once, at boot");
}

#[test]
fn a_kept_seed_woken_before_readiness_with_no_id_keeps_its_sleep_history() {
    // No build-time answer: woken with no id before readiness, it waits on
    // its placeholder, and the id's arrival admits a value that slept, which
    // is no answer yet.
    let mut r = launch_plan(
        plan_keyed_with(Some(false), false),
        vec![feeds_kept(
            &[Value::Number(0.), Value::str("alice")],
            "kept feeds of alice",
        )],
    );
    r.act("open", vec![Value::Bool(true)]).unwrap();
    shows(&r, "feeds", "");
    assert_eq!(lines_with(&r, WAITING).len(), 1);
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "kept feeds of alice");
    assert!(!r.resources[0].current, "a value kept across idleness");
    r.data.ready = true;
    r.data_ready().unwrap();
    settle_requests(&mut r);
    shows(&r, "feeds", "feeds of alice");
    assert!(r.resources[0].current);
}

#[test]
fn a_kept_entry_that_waits_awake_learns_a_later_sleep() {
    // Open at boot with no id: it waits awake, on its placeholder. Its gate
    // then closes and opens again before the id arrives, so the value the
    // id admits slept, and is no answer yet.
    let mut r = launch_plan(
        plan_keyed_with(Some(true), false),
        vec![feeds_kept(
            &[Value::Number(0.), Value::str("alice")],
            "kept feeds of alice",
        )],
    );
    assert_eq!(lines_with(&r, WAITING).len(), 1);
    r.act("open", vec![Value::Bool(false)]).unwrap();
    r.act("open", vec![Value::Bool(true)]).unwrap();
    assert!(r.carry().waiting[0].4, "a reload carries that it slept");
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "kept feeds of alice");
    assert!(!r.resources[0].current, "a value kept across idleness");
    r.data.ready = true;
    r.data_ready().unwrap();
    settle_requests(&mut r);
    shows(&r, "feeds", "feeds of alice");
    assert!(r.resources[0].current);
}

/// `feeds(key)` over a live module source declared `"kept": true` (LLP
/// 1133.015 D4), the key a root `state key = ""` the action `key` sets.
fn plan_kept_module() -> Plan {
    plan_kept_module_with(false)
}

/// As [`plan_kept_module`], `feeds` behind `when open`, a root state the
/// action `open` sets.
fn plan_kept_module_gated() -> Plan {
    plan_kept_module_full(false, true)
}

fn plan_kept_module_with(baked: bool) -> Plan {
    plan_kept_module_full(baked, false)
}

/// As [`plan_kept_module`]; with `baked`, `feeds` has a build-time answer,
/// `"baked"`, for the initial `""`, as a substitute that answers at bake;
/// with `gated`, it is behind `when open`.
fn plan_kept_module_full(baked: bool, gated: bool) -> Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let source = b.source("feeds", &[string], string);
    b.module(
        source,
        ("feeds", "feeds"),
        "feeds",
        false,
        true,
        "",
        "",
        "{\"kept\":true}",
    );
    let empty = b.constant(&Value::str(""));
    let key = b.slot("key", string, empty);
    let mut k = Asm::new();
    k.load_slot(key);
    let k = b.code(k);
    let feeds = b.resource(
        "feeds",
        "feeds",
        &[k],
        string,
        baked.then_some(&Value::str("baked")),
    );
    if baked {
        b.set_resource_initial_args(feeds, &[Value::str("")]);
    }
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
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    b.finish().unwrap()
}

#[test]
fn a_kept_module_ready_at_boot_waits_for_an_empty_identity() {
    let mut r = Runner::boot_stored(
        plan_kept_module(),
        Source {
            ready: true,
            ..Default::default()
        },
        Kernel::with_monospace(),
        vec![(
            kept::kept_name("feeds"),
            kept::encode(&[Value::str("alice")], &Value::str("kept feeds of alice")),
        )],
        Default::default(),
        "/",
    )
    .unwrap();
    // The seed is not this launch's empty id's answer: it waits.
    assert_ne!(
        r.resource("feeds"),
        Some(&Value::str("kept feeds of alice"))
    );
    assert_eq!(lines_with(&r, WAITING).len(), 1);
    r.act("key", vec![Value::str("alice")]).unwrap();
    shows(&r, "feeds", "kept feeds of alice");
}

#[test]
fn a_reload_keeps_a_waiting_kept_answer_s_sleep_history() {
    // Idle through readiness, woken with no id: the entry waits for a seed
    // that slept, and a reload carries that.
    let r = opened_after_readiness("");
    let carried = r.carry();
    let mut next = Runner::boot_carrying(
        plan_keyed(true),
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
    assert_eq!(next.take_requests().len(), 1, "the empty id is asked");
    next.act("key", vec![Value::str("alice")]).unwrap();
    shows(&next, "feeds", "kept feeds of alice");
    assert!(!next.resources[0].current, "a value kept across idleness");
}

#[test]
fn a_reload_to_an_unready_source_keeps_a_waiting_kept_answer_s_sleep_history() {
    // The replacement plants the stored entry as a seed again; the seed takes
    // the sleep history the carried entry waited with.
    let r = opened_after_readiness("");
    let carried = r.carry();
    assert_eq!(carried.waiting.len(), 1);
    let mut next = Runner::boot_carrying(
        plan_keyed(true),
        Source::default(),
        Kernel::with_monospace(),
        &carried,
        Default::default(),
        "/",
    )
    .unwrap();
    next.act("key", vec![Value::str("alice")]).unwrap();
    shows(&next, "feeds", "kept feeds of alice");
    assert!(!next.resources[0].current, "a value kept across idleness");
}

/// `account(0)`, a store reader whose build-time answer is `""` (signed
/// out), and `feeds(account)` over a kept module source.
fn plan_kept_module_by_account() -> Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let mut a = Asm::new();
    a.number(0.);
    let a = b.code(a);
    let account = b.resource("account", "account", &[a], string, Some(&Value::str("")));
    b.set_resource_reader(account, true);
    b.set_resource_initial_args(account, &[Value::Number(0.)]);
    let source = b.source("feeds", &[string], string);
    b.module(
        source,
        ("feeds", "feeds"),
        "feeds",
        false,
        true,
        "",
        "",
        "{\"kept\":true}",
    );
    let mut k = Asm::new();
    k.load_resource(account);
    let k = b.code(k);
    b.resource("feeds", "feeds", &[k], string, None);
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    b.finish().unwrap()
}

#[test]
fn a_build_time_identity_whose_first_answer_is_pending_does_not_admit_a_kept_module() {
    // Kept signed out; the account's first answer is still out, so its
    // build-time "" is not this launch's.
    let mut r = Runner::boot_stored(
        plan_kept_module_by_account(),
        Source {
            ready: true,
            account_later: true,
            ..Default::default()
        },
        Kernel::with_monospace(),
        vec![
            (
                kept::kept_name("feeds"),
                kept::encode(&[Value::str("")], &Value::str("kept feeds, signed out")),
            ),
            ("token".to_string(), b"alice".to_vec()),
        ],
        Default::default(),
        "/",
    )
    .unwrap();
    shows(&r, "account", "");
    assert_ne!(
        r.resource("feeds"),
        Some(&Value::str("kept feeds, signed out"))
    );
    assert_eq!(lines_with(&r, WAITING).len(), 1);
    let account = (r.take_requests().into_iter())
        .find(|q| q.request.url == "https://example.test/account")
        .expect("the account is asked");
    r.fulfill(account.ticket, Outcome::Storage(vec![])).unwrap();
    shows(&r, "account", "alice");
    assert_ne!(
        r.resource("feeds"),
        Some(&Value::str("kept feeds, signed out"))
    );
}

#[test]
fn a_kept_module_admitted_at_a_ready_boot_is_journaled_and_not_carried_after_it_fails() {
    let r = Runner::boot_stored(
        plan_kept_module(),
        Source {
            ready: true,
            ..Default::default()
        },
        Kernel::with_monospace(),
        vec![(
            kept::kept_name("feeds"),
            kept::encode(&[Value::str("")], &Value::str("kept feeds")),
        )],
        Default::default(),
        "/",
    )
    .unwrap();
    shows(&r, "feeds", "kept feeds");
    assert_eq!(lines_with(&r, "feeds shows its kept answer").len(), 1);
    // The module has no implementation here: its ask fails at once.
    assert_eq!(lines_with(&r, "resource feeds failed").len(), 1);
    let carried = r.carry();
    assert!(
        carried.resources.iter().all(|(name, ..)| name != "feeds"),
        "a kept answer is not an answer to carry: {:?}",
        carried.resources
    );
}

/// As [`plan`], `feeds` behind `when open`, a root state the action `open`
/// sets.
fn plan_gated_by_account() -> Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let boolean = b.primitive(TypeKind::Bool);
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
    b.set_resource_reader(account, true);
    b.set_resource_initial_args(account, &[Value::Number(0.)]);
    let mut g = Asm::new();
    g.number(0.);
    let g = b.code(g);
    let mut k = Asm::new();
    k.load_resource(account);
    let k = b.code(k);
    let feeds = b.resource("feeds", "feeds", &[g, k], string, None);
    b.set_resource_reader(feeds, true);
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
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    b.finish().unwrap()
}

#[test]
fn a_refusal_names_an_admitted_kept_answer_still_asked_as_one() {
    // The account shows its kept "carol" through activation while its ask
    // is out; feeds, kept for "alice", is compared when its gate opens.
    let mut r = Runner::boot_stored(
        plan_gated_by_account(),
        Source {
            account_later: true,
            ..Default::default()
        },
        Kernel::with_monospace(),
        vec![
            (
                kept::kept_name("account"),
                kept::encode(&[Value::Number(0.)], &Value::str("carol")),
            ),
            feeds_kept(
                &[Value::Number(0.), Value::str("alice")],
                "kept feeds of alice",
            ),
        ],
        Default::default(),
        "/",
    )
    .unwrap();
    shows(&r, "account", "carol");
    r.data.ready = true;
    r.data_ready().unwrap();
    shows(&r, "account", "carol");
    r.act("open", vec![Value::Bool(true)]).unwrap();
    let lines = lines_with(&r, "feeds did not use its kept answer");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].ends_with("reading resource account (holding an admitted kept answer)"),
        "{lines:?}"
    );
}

#[test]
fn a_kept_module_admitted_at_a_ready_boot_stands_over_its_build_time_answer() {
    let r = Runner::boot_stored(
        plan_kept_module_with(true),
        Source {
            ready: true,
            ..Default::default()
        },
        Kernel::with_monospace(),
        vec![(
            kept::kept_name("feeds"),
            kept::encode(&[Value::str("")], &Value::str("kept feeds")),
        )],
        Default::default(),
        "/",
    )
    .unwrap();
    shows(&r, "feeds", "kept feeds");
    assert_eq!(lines_with(&r, "feeds shows its kept answer").len(), 1);
}

fn boot_kept_module(plan: Plan, kept_for: &str) -> Runner<Source> {
    Runner::boot_stored(
        plan,
        Source {
            ready: true,
            ..Default::default()
        },
        Kernel::with_monospace(),
        vec![(
            kept::kept_name("feeds"),
            kept::encode(&[Value::str(kept_for)], &Value::str("kept feeds")),
        )],
        Default::default(),
        "/",
    )
    .unwrap()
}

fn reload(r: &Runner<Source>, plan: Plan) -> Runner<Source> {
    Runner::boot_carrying(
        plan,
        Source {
            ready: true,
            ..Default::default()
        },
        Kernel::with_monospace(),
        &r.carry(),
        Default::default(),
        "/",
    )
    .unwrap()
}

#[test]
fn a_reload_keeps_the_entry_of_a_kept_module_waiting_for_its_identity() {
    // Before readiness, woken with no identity: it waits and has asked
    // nothing, so it has read no store; its entry is still compatible.
    let mut r = Runner::boot_stored(
        plan_kept_module_gated(),
        Source::default(),
        Kernel::with_monospace(),
        vec![(
            kept::kept_name("feeds"),
            kept::encode(&[Value::str("alice")], &Value::str("kept feeds")),
        )],
        Default::default(),
        "/",
    )
    .unwrap();
    r.act("open", vec![Value::Bool(true)]).unwrap();
    assert_eq!(lines_with(&r, WAITING).len(), 1);
    assert!(r.carry().store_readers.is_empty(), "it has read no store");
    let mut next = Runner::boot_carrying(
        plan_kept_module_gated(),
        Source::default(),
        Kernel::with_monospace(),
        &r.carry(),
        Default::default(),
        "/",
    )
    .unwrap();
    next.act("key", vec![Value::str("alice")]).unwrap();
    shows(&next, "feeds", "kept feeds");
}

#[test]
fn a_reload_keeps_the_sleep_history_of_an_admitted_kept_answer() {
    // Idle through boot, woken with its identity: admitted, it slept, and
    // its ask has not answered (here it fails at once).
    let mut r = boot_kept_module(plan_kept_module_gated(), "alice");
    r.act("key", vec![Value::str("alice")]).unwrap();
    r.act("open", vec![Value::Bool(true)]).unwrap();
    shows(&r, "feeds", "kept feeds");
    assert!(!r.resources[0].current, "a value kept across idleness");
    let next = reload(&r, plan_kept_module_gated());
    shows(&next, "feeds", "kept feeds");
    assert!(!next.resources[0].current, "still no answer since the wake");
}

#[test]
fn a_reload_keeps_the_entry_of_an_admitted_kept_module_that_has_not_asked() {
    // Admitted before readiness, awake: it has neither slept nor asked, and
    // has read no store. The replacement shows the same kept answer.
    let r = Runner::boot_stored(
        plan_kept_module(),
        Source::default(),
        Kernel::with_monospace(),
        vec![(
            kept::kept_name("feeds"),
            kept::encode(&[Value::str("")], &Value::str("kept feeds")),
        )],
        Default::default(),
        "/",
    )
    .unwrap();
    shows(&r, "feeds", "kept feeds");
    let next = Runner::boot_carrying(
        plan_kept_module(),
        Source::default(),
        Kernel::with_monospace(),
        &r.carry(),
        Default::default(),
        "/",
    )
    .unwrap();
    shows(&next, "feeds", "kept feeds");
}
