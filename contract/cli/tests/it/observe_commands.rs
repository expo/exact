//! `observe(…)`, `observeAttributes(…)`, `observeError(…)` and `observeFeedback(…)` reach the host as
//! commands, in order, with their arguments. Malformed calls fail to compile.
use crate::source::{boxed, NoData, Source};
use exact_runner::{DataError, DataSource, Runner, Value};

const APP: &str = r#"shape Checkout
  items: number
  total: number

shape Tier
  tier: string

fn five(): number = 5

component App
  state count = 0
  action checkout(items: number)
    count = count + 1
    observeAttributes(Tier(tier="pro"))
    observe("checkout.completed", Checkout(items=items, total=items * 3), "info")
    observe("checkout.seen")
  action fail
    observeError("cart unavailable", "CartError")
  action rate(stars: number)
    observeFeedback("checkout", "Too slow", stars, 1, 5, Tier(tier="pro"))
    observeFeedback("checkout", "", stars)
    observeFeedback("checkout", "Computed", count + 1, 1, five())
    observeFeedback("general", "Nice app")
  view
    column
      button press=checkout(2) testId="buy"
        text "Buy"
"#;

fn boot(source: &str) -> Runner<Source> {
    Runner::boot(
        contract::compile(source).unwrap(),
        boxed(NoData),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

#[test]
fn custom_events_are_commands_in_order() {
    let mut r = boot(APP);
    r.take_commands();
    r.act("checkout", vec![Value::Number(2.)]).unwrap();
    let commands = r.take_commands();
    let names: Vec<_> = commands
        .iter()
        .map(|c| (c.name.as_str(), c.args.len()))
        .collect();
    assert_eq!(
        names,
        [("observeAttributes", 2), ("observe", 6), ("observe", 2)]
    );
    assert_eq!(
        commands[1].args,
        vec![
            Value::str("checkout.completed"),
            Value::str("info"),
            Value::str("items"),
            Value::Number(2.),
            Value::str("total"),
            Value::Number(6.)
        ],
        "the fields go by name"
    );
    r.act("fail", vec![]).unwrap();
    let error = r.take_commands();
    assert_eq!(
        error[0].args,
        vec![Value::str("cart unavailable"), Value::str("CartError")]
    );
}

#[test]
fn feedback_goes_positionally() {
    let mut r = boot(APP);
    r.take_commands();
    r.act("rate", vec![Value::Number(4.)]).unwrap();
    let args: Vec<_> = r.take_commands().into_iter().map(|c| c.args).collect();
    let (id, none) = (Value::str, Value::NONE);
    assert_eq!(
        args,
        [
            vec![
                id("checkout"),
                id("Too slow"),
                Value::Number(4.),
                Value::Number(1.),
                Value::Number(5.),
                id("tier"),
                id("pro")
            ],
            vec![
                id("checkout"),
                id(""),
                Value::Number(4.),
                none.clone(),
                none.clone()
            ],
            vec![
                id("checkout"),
                id("Computed"),
                Value::Number(1.),
                Value::Number(1.),
                Value::Number(5.)
            ],
            vec![
                id("general"),
                id("Nice app"),
                none.clone(),
                none.clone(),
                none
            ],
        ],
        "what the call leaves out is none"
    );
}

#[test]
fn observe_arguments_are_checked() {
    for statement in [
        "observe()",
        "observe(1)",
        r#"observe("a", "b")"#,
        r#"observe("a", Tier(tier="x"), 2)"#,
        r#"observeAttributes("x")"#,
        "observeError(3)",
        r#"observeFeedback("a")"#,
        r#"observeFeedback("a", 1)"#,
        r#"observeFeedback("a", "m", 1, 5)"#,
        r#"observeFeedback("a", "m", "4")"#,
        r#"observeFeedback("a", "m", 1, 2, 3, 4)"#,
        r#"observeFeedback("a", "m", Tier(tier="x"), 4)"#,
    ] {
        let source = APP.replace(r#"observe("checkout.seen")"#, statement);
        let refused = contract::compile(&source).expect_err(statement);
        assert!(
            format!("{refused:?}").contains("type-observe"),
            "{statement}: {refused:?}"
        );
    }
}

const SCREEN: &str = r#"component App
  state loaded = false
  action load
    loaded = true
  view
    column aria-busy=(not loaded) testId="forecast"
      button press=load testId="load"
        text "Load"
"#;

/// A mounted `aria-busy` element is outstanding, by test id, until it is no
/// longer busy.
#[test]
fn aria_busy_holds_the_ledger_until_cleared() {
    let mut r = boot(SCREEN);
    assert_eq!(
        r.outstanding().busy,
        vec!["forecast".to_string()],
        "busy at boot"
    );
    assert!(!r.outstanding().is_clear());
    r.act("load", vec![]).unwrap();
    assert!(r.outstanding().busy.is_empty(), "not busy once loaded");
    assert!(r.outstanding().is_clear());
}

const FEED: &str = r#"shape Feed
  title: string

component App
  state rev = 0
  resource feed = feed(rev) as shape Feed else blank()
  mutation saved as shape Feed then afterSave
  action refresh
    rev = rev + 1
  action save
    send saved = save()
  action afterSave
    rev = rev
  view
    column
      text feed.title testId="title"
"#;

/// `blank` answers now; every other source answers later, with a title.
struct Later;
impl DataSource for Later {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::Unavailable(format!("{source} answers later")))
    }
    fn answer(
        &mut self,
        _: &mut exact_runner::Store,
        source: &str,
        _: &[Value],
    ) -> Result<exact_runner::Answer, DataError> {
        if source == "blank" {
            return Ok(exact_runner::Answer::Now(Value::record(vec![Value::str(
                "loading",
            )])));
        }
        Ok(exact_runner::Answer::Later(exact_runner::Request::get(
            &format!("https://feed.test/{source}"),
        )))
    }
    fn parse(
        &mut self,
        _: &mut exact_runner::Store,
        _: &str,
        _: &[Value],
        _: exact_runner::Outcome,
    ) -> Result<exact_runner::Answer, DataError> {
        Ok(exact_runner::Answer::Now(Value::record(vec![Value::str(
            "news",
        )])))
    }
}

fn reply() -> exact_runner::Outcome {
    exact_runner::Outcome::Response(exact_runner::Response {
        status: 200,
        headers: vec![],
        body: b"{}".to_vec(),
    })
}

/// TTI waits only for what the screen lacks: a placeholder's request holds
/// it; a refresh behind a shown answer and a mutation the app sent don't.
#[test]
fn only_a_placeholders_request_holds_the_ledger() {
    let mut r = Runner::boot(
        contract::compile(FEED).unwrap(),
        boxed(Later),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(
        r.outstanding().requests,
        vec!["feed".to_string()],
        "the placeholder is loading"
    );
    let first = r.take_requests();
    r.fulfill(first[0].ticket, reply()).unwrap();
    assert!(r.outstanding().is_clear(), "answered");

    r.act("refresh", vec![]).unwrap();
    assert_eq!(r.take_requests().len(), 1, "the refresh is in flight");
    assert!(
        r.outstanding().is_clear(),
        "a refresh behind the shown answer is not outstanding"
    );

    r.act("save", vec![]).unwrap();
    assert!(!r.take_requests().is_empty(), "the mutation is in flight");
    assert!(
        r.outstanding().is_clear(),
        "a mutation the app sent is not outstanding"
    );
}
