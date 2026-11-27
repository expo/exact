//! The harness's values are what `shapes.contract` declares: a real
//! runner boots against it, takes `session()` and `animation(...)` through
//! the shape check, and asks `session()` again when the source announces.

use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{DataSource, Runner};
use harness_data::shapes::{ContractValue, Session};
use harness_data::{Harness, Options};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const APP: &str = r#"
component App
  resource session = session() as shape Session
  resource frame = animation("fire", 12, 20, 4) as shape Frame
  view
    text `${session.model} ${session.phase} ${frame.title}`
"#;

fn entries(v: &Value) -> usize {
    Session::from_value(v).expect("a Session").entries.len()
}

#[test]
fn a_runner_takes_the_shapes_and_the_push() {
    let source = format!("{}\n{APP}", include_str!("../../../shapes.contract"));
    let plan = contract::compile(&source).expect("the app compiles");
    let harness = Harness::with_options(Options::offline(None));
    let mut handle = harness.clone();
    let mut r = Runner::boot(
        plan,
        harness,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .expect("boots: the values conform");
    let woke = Arc::new(AtomicUsize::new(0));
    let count = woke.clone();
    r.listen(Arc::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
    }));
    r.data_ready().unwrap();
    let session = r.resource("session").expect("session").clone();
    assert_eq!(entries(&session), 1, "the banner");
    let Some(Value::Record(frame)) = r.resource("frame") else {
        panic!("no frame")
    };
    assert_eq!(frame[0], Value::str("fire"));

    // A send on another handle changes the session and announces it.
    handle.query("submit", &[Value::str("/help")]).unwrap();
    assert!(woke.load(Ordering::SeqCst) >= 1);
    let (_, error) = r.apply_announced();
    assert!(error.is_none(), "{error:?}");
    let session = r.resource("session").expect("session");
    assert_eq!(entries(session), 2, "the banner and the help");
}

#[test]
fn a_session_read_as_the_wrong_shape_fails_its_resource() {
    let wrong = r#"
shape Wrong
  model: number
component App
  resource session = session() as shape Wrong
  derive bad = failed(session)
  derive code = match failure(session) { case some(f) => f.code, case none => "" }
  view
    text `${session.model} ${code}`
"#;
    for (declaration, placeholder) in [
        ("as shape Wrong", 0.),
        ("as shape Wrong else empty(model=-1)", -1.),
    ] {
        let source = wrong.replace("as shape Wrong", declaration);
        let plan = contract::compile(&source).expect("compiles");
        let runner = Runner::boot(
            plan,
            Harness::with_options(Options::offline(None)),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        assert_eq!(runner.derive("bad"), Some(&Value::Bool(true)));
        assert!(runner.resource_is_placeholder("session"));
        assert_eq!(runner.derive("code"), Some(&Value::str("shape")));
        assert_eq!(
            runner.resource("session"),
            Some(&Value::record(vec![Value::Number(placeholder)]))
        );
        let failures = runner.failed_resources();
        let [("session", why)] = failures.as_slice() else {
            panic!("expected the session's shape failure: {failures:?}");
        };
        let (message, detail) = why
            .split_once(": ")
            .expect("shape failure has a field diagnostic");
        assert!(message.starts_with("`session` "), "{why}");
        assert!(
            detail.starts_with("Wrong.model: a Number was declared"),
            "{why}"
        );
    }
}
