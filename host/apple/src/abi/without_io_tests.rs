use super::super::*;
use exact_plan::Value;
use exact_runner::{Answer, DataError, Store};
use std::sync::{Arc, Mutex};

/// The host work these tests hand out (LLP 1047.001 D12).
struct TestWork;
impl exact_runner::HostWork for TestWork {}

/// Answers through a continuation, as Markdown Stress's document does, and
/// keeps the outcome it is handed back.
#[derive(Clone, Default)]
struct Continuing(Arc<Mutex<Option<exact_runner::Outcome>>>);

impl DataSource for Continuing {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(
            exact_runner::Request::continuation::<TestWork>(1),
        ))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: exact_runner::Outcome,
    ) -> Result<Answer, DataError> {
        *self.0.lock().unwrap() = Some(outcome);
        Ok(Answer::Now(Value::Number(0.0)))
    }
}

/// Production's composition without `io` has no executor: a source's
/// continuation is refused by name and settled at the next pump, which the
/// session's wake asks for, where it once waited for ever.
#[test]
fn a_request_without_io_is_refused_by_name_not_left_pending() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static WOKEN: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn wake(_: *mut c_void) {
        WOKEN.fetch_add(1, Ordering::SeqCst);
    }
    let plan = contract::compile(
        "component App\n  resource doc = doc(0) as shape number\n  view\n    text `${doc}`\n",
    )
    .unwrap()
    .encode();
    let source = Continuing::default();
    let mut bridge = Bridge::with_links(crate::link::Links::of(crate::link::Uses::NONE));
    let mut hooks = Hooks::none();
    hooks.wake = Some(wake);
    bridge.boot(&plan, source.clone(), hooks, 390.0, 844.0);
    bridge.data_ready();
    assert!(bridge.executor.is_none());
    assert!(
        WOKEN.load(Ordering::SeqCst) > 0,
        "the session is woken to settle it"
    );
    bridge.pump(0.0);
    let outcome = source.0.lock().unwrap().take();
    let message = format!("{outcome:?}");
    assert!(message.contains("Unlinked(\\\"io\\\")"), "{message}");
    let host = bridge.host.as_ref().unwrap();
    assert!(host.runner().pending().is_empty(), "nothing waits");
    let refused = host
        .runner()
        .journal()
        .filter(|line| line.contains("refused: Unlinked(\"io\")"))
        .count();
    assert_eq!(refused, 1, "the journal says it with the request");
}
