//! @ref LLP 1039 D9 — `exactViewport().keyboardHeight`, CSS's
//! `env(keyboard-inset-height)`: the host's overlap, answered with the
//! viewport it shrinks in one commit.
use exact_kernel::Kernel;
use exact_runner::{agent, DataError, DataSource, Runner, Value};

struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("host fact reached data: {source}")
    }
}

fn state(r: &Runner<NoData>) -> serde_json::Value {
    serde_json::from_str(&agent::state(r)).unwrap()
}

fn asked_again(r: &Runner<NoData>, since: usize) -> usize {
    agent::logs(r, since).matches("asked again)").count()
}

fn journal_end(r: &Runner<NoData>) -> usize {
    let logs: serde_json::Value = serde_json::from_str(&agent::logs(r, 0)).unwrap();
    logs["next"].as_u64().unwrap() as usize
}

/// 0 at boot and in the bake; a told overlap re-answers in one commit; a
/// row shown only for a software keyboard (a hardware keyboard's short bar
/// is under the app's threshold); the same overlap commits nothing; a
/// negative or non-finite one is refused, changing nothing.
#[test]
fn keyboard_height_is_the_hosts_overlap() {
    let src = r#"shape Keys
  keyboardHeight: number
  height: number
component App
  resource keys = exactViewport() as shape Keys
  view
    column
      if keys.keyboardHeight >= 120
        row role="toolbar" toolbarPlacement="keyboard" testId="row"
"#;
    let plan = contract::bake(contract::compile(src).unwrap(), NoData).unwrap();
    let mut r = Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let row = |r: &Runner<NoData>| !r.kernel().find_by_test_id("row").is_empty();
    assert_eq!(state(&r)["resources"]["keys"]["keyboardHeight"], 0);
    assert!(!row(&r));
    assert!(r.set_keyboard_height(336.0).unwrap().is_some());
    assert_eq!(state(&r)["resources"]["keys"]["keyboardHeight"], 336);
    assert!(row(&r));
    assert!(
        r.set_keyboard_height(336.0).unwrap().is_none(),
        "the same overlap"
    );
    r.set_keyboard_height(55.0).unwrap();
    assert!(!row(&r), "a hardware keyboard's bar");
    // The keys and the viewport they shrink: one commit.
    let since = journal_end(&r);
    r.set_view(Some((390.0, 508.0)), Some(336.0))
        .unwrap()
        .unwrap();
    assert_eq!(asked_again(&r, since), 1);
    assert_eq!(state(&r)["resources"]["keys"]["height"], 508);
    assert!(row(&r));
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(r.set_keyboard_height(bad).is_err());
    }
    assert_eq!(r.viewport().keyboard_height, 336.0);
}

#[test]
fn keyboard_height_is_a_number() {
    let wrong = "shape S\n  keyboardHeight: bool\ncomponent A\n  resource s = exactViewport() as shape S\n  view\n    text `${s.keyboardHeight}`\n";
    assert!(contract::bake(contract::compile(wrong).unwrap(), NoData).is_err());
}
