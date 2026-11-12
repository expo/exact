//! A string holding U+0000 crosses the host door and the console whole: the
//! door carries each string with its length, not to its first zero byte.

#![cfg(exact_js_engine)]

use exact_js::Module;
use exact_plan::Plan;
use exact_runner::{Answer, DataSource, Store};

const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nul-byte.hbc"));
const GRANTS: &str = "secret.keep k2\n";

#[test]
fn a_nul_in_a_stored_value_or_a_log_line_is_kept() {
    let plan: Plan = contract::compile(
        "component App\n  resource nul = nul() as shape string\n  view\n    text nul\n",
    )
    .expect("the fixture's Contract compiles");
    let mut m = Module::new(HBC.to_vec(), "test.nul", GRANTS);
    m.set_budget_ms(f64::INFINITY);
    m.bind(&plan);
    m.load().expect("the fixture loads");
    let mut store = Store::new(GRANTS, []);
    let answer = match m.answer(&mut store, "nul", &[]).expect("answered") {
        Answer::Now(value) => value.as_str().expect("a string").to_string(),
        Answer::Later(_) => panic!("not answered at once"),
    };
    assert_eq!(answer, r#"{"direct":3,"back":3,"same":true}"#);
    assert_eq!(store.get("k2"), Some("a\0b"));
    let logs = DataSource::take_logs(&mut m);
    assert!(logs.iter().any(|l| l.contains("x\0y")), "{logs:?}");
    assert!(logs.iter().any(|l| l.contains("second line")), "{logs:?}");
}
