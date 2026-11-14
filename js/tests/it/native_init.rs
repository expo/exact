//! A synchronous native call from a module's top level is refused. The Apple
//! host evaluates the module from the first draw, before it prepares the
//! app's native module (LLP 1027.008 D1), so no such call may reach it then.
#![cfg(exact_js_engine)]
use exact_js::Module;
use exact_runner::{Answer, DataError, DataSource, Store};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/native-init.hbc"));

const SRC: &str = r#"
component App
  resource refused = refused() as shape string
  resource inside = inside() as shape string
  view
    text `${refused} ${inside}`
"#;

fn text(answer: Result<Answer, DataError>) -> String {
    match answer {
        Ok(Answer::Now(value)) if value.as_str().is_some() => value.as_str().unwrap().into(),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn a_native_call_at_the_top_level_is_refused_and_reaches_no_module() {
    let mut m = Module::new(HBC.to_vec(), "test.native-init", "");
    m.set_budget_ms(f64::INFINITY);
    m.bind(&contract::compile(SRC).unwrap());
    // The host's app module, installed as the Apple host installs it when
    // the session is made (`exact_set_app_module`).
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let native = m.native().expect("a module offers its native handle");
    native.host(Some(Arc::new(|_, _| {})));
    native.host_call(Some(Arc::new(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(b"{\"ok\":true}".to_vec())
    })));
    m.activate().expect("the module catches the refusal");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no call reached it");
    let mut s = Store::new("", Vec::<(String, Vec<u8>)>::new());
    let refused = text(m.answer(&mut s, "refused", &[]));
    assert_eq!(refused, "native call outside an answer");
    assert_eq!(text(m.answer(&mut s, "inside", &[])), "{\"ok\":true}");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "made inside an answer");
}
