//! Long polls through the presenter (LLP 1016.000 D8).
use super::*;
use exact_runner::{Answer, DataError, Store, Value};

/// The host work these tests hand out (LLP 1047.001 D12).
struct TestWork;
impl exact_runner::HostWork for TestWork {}

/// LLP 1016.000 D8 through the Linux presenter: a long poll attached to work
/// that is no HTTP request (a continuation) is refused before its work runs,
/// and the source parses the refusal.
#[derive(Default)]
struct PollsOtherWork {
    ran: Arc<std::sync::atomic::AtomicBool>,
    parsed: Arc<std::sync::Mutex<Option<String>>>,
}
impl DataSource for PollsOtherWork {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(
            exact_runner::Request::continuation::<TestWork>(1).long_poll("since", "seq"),
        ))
    }
    fn continuation(
        &mut self,
        _: u64,
    ) -> Option<Box<dyn FnOnce() -> exact_runner::Outcome + Send>> {
        let ran = self.ran.clone();
        Some(Box::new(move || {
            ran.store(true, std::sync::atomic::Ordering::SeqCst);
            exact_runner::Outcome::Storage(vec![])
        }))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: exact_runner::Outcome,
    ) -> Result<Answer, DataError> {
        *self.parsed.lock().unwrap() = Some(format!("{outcome:?}"));
        Ok(Answer::Now(Value::Number(0.)))
    }
}

#[test]
fn a_long_poll_on_other_work_is_refused_before_its_work_runs() {
    let source = PollsOtherWork::default();
    let (ran, parsed) = (source.ran.clone(), source.parsed.clone());
    let (mut p, error) = Presenter::boot_with(
        &contract::compile(
            "component App\n  resource polled = polled() as shape number\n  view\n    text \"x\"\n",
        )
        .unwrap()
        .encode(),
        source,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while parsed.lock().unwrap().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "the refusal never settled"
        );
        p.pump(p.host().now());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let parsed = parsed.lock().unwrap().clone().unwrap();
    assert!(
        parsed.contains("Refused") && parsed.contains("exactLongPoll"),
        "{parsed}"
    );
    assert!(
        !ran.load(std::sync::atomic::Ordering::SeqCst),
        "its work never ran"
    );
}

#[derive(Default)]
struct Poll;
impl DataSource for Poll {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn grants(&self) -> &str {
        "net.fetch https://example.test"
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::stream(
            exact_runner::Request::get("https://example.test/changes?since=0")
                .long_poll("since", "seq"),
        ))
    }
}

#[test]
fn idle_long_poll_failures_reach_agent_logs_without_a_completion() {
    let plan = crate::test_plan(
        r#"component App
  mutation live as shape string
  action start
    send live = changes()
  view
    button press=start testId="start"
      text "Start"
"#,
    );
    let (mut p, error) = Presenter::boot_with(
        &plan,
        Poll,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let _ = p.frame();
    p.first_pixel();
    let url = "https://example.test/changes?since=0";
    p.host.runner().faults().arm(url, None).unwrap();
    let k = p.host.kernel();
    let start = k.node_by_key(k.find_by_test_id("start")[0]).unwrap().id;
    p.tap(start).unwrap();
    let seq = p.host.runner().seq();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let reply = crate::agent::handle(&mut p, r#"{"op":"logs"}"#);
        let logs: serde_json::Value = serde_json::from_str(&reply).unwrap();
        let failures = logs["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|line| {
                line.as_str()
                    .unwrap()
                    .contains("fetch failed (driver fault)")
            })
            .count();
        if failures >= 2 {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{reply}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let state: serde_json::Value =
        serde_json::from_str(&crate::agent::handle(&mut p, r#"{"op":"state"}"#)).unwrap();
    assert!(state["faults"][0]["hits"].as_u64().unwrap() >= 2);
    assert_eq!(p.host.runner().seq(), seq, "no completion committed");
}

/// A source whose boot, as a replacement, stands for the old executor's
/// background retry taking a counted fault after `carry` read the table and
/// before commit.
struct TakesDuringBoot(Option<exact_runner::runner::faults::FetchFault>);
impl DataSource for TakesDuringBoot {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn bind(&mut self, _: &Plan) {
        if let Some(retry) = self.0.take() {
            assert!(retry(LONG_POLL).is_some(), "the retry takes the second");
        }
    }
}

const LONG_POLL: &str = "https://example.test/changes?since=0";

#[test]
fn a_reload_keeps_the_faults_a_retry_took_while_it_prepared() {
    for update in [false, true] {
        let plan = crate::test_plan("component App\n  view\n    text \"x\"\n");
        let (mut p, error) = Presenter::boot_with(
            &plan,
            TakesDuringBoot(None),
            (200., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.host.runner().faults().arm(LONG_POLL, Some(2)).unwrap();
        let retry = p.host.runner().fetch_fault();
        assert!(retry(LONG_POLL).is_some(), "the first attempt");
        let replacement = TakesDuringBoot(Some(retry));
        if update {
            p.set_updates(Some(Box::new(super::delivery::tests::StagedPlan(
                plan.into(),
            ))));
            assert!(p.activate_update(replacement).unwrap());
        } else {
            assert!(
                p.reload(&plan, replacement).unwrap()
                    == crate::presenter::ReloadResult::Applied(None)
            );
        }
        let runner = p.host.runner();
        let faults = runner.faults().json();
        assert!(faults.contains("\"left\":0,\"hits\":2"), "{faults}");
        assert!(
            runner.fetch_fault()(LONG_POLL).is_none(),
            "no third failure"
        );
    }
}
