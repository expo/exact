//! Long polls through the bridge (LLP 1016.000 D8).
use super::*;

/// The host work these tests hand out (LLP 1047.001 D12).
struct TestWork;
impl exact_runner::HostWork for TestWork {}

/// LLP 1016.000 D8 through Apple's bridge: a long poll attached to work that
/// is no HTTP request (here a continuation) is refused before its work runs,
/// and the source parses the refusal.
#[derive(Clone, Default)]
struct PollsOtherWork {
    ran: Arc<Mutex<bool>>,
    parsed: Arc<Mutex<Option<String>>>,
}

impl DataSource for PollsOtherWork {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
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
            *ran.lock().unwrap() = true;
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
        Ok(Answer::Now(Value::str("refused")))
    }
}

#[test]
fn a_long_poll_on_other_work_is_refused_before_its_work_runs() {
    let data = PollsOtherWork::default();
    let (ran, parsed) = (data.ran.clone(), data.parsed.clone());
    let initial = plan(Some("polled"));
    let mut bridge = Bridge::new();
    bridge.input_write(&initial);
    bridge.prepare_plan(initial.len(), data, Hooks::none(), 390.0, 844.0);
    bridge.commit_plan();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while parsed.lock().unwrap().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "the refusal never settled"
        );
        bridge.pump(0.0);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let parsed = parsed.lock().unwrap().clone().unwrap();
    assert!(
        parsed.contains("Refused") && parsed.contains("exactLongPoll"),
        "{parsed}"
    );
    assert!(!*ran.lock().unwrap(), "its work never ran");
}

#[test]
fn agent_logs_drain_faults_without_a_pump_or_a_batch() {
    let mut bridge = Bridge::new();
    bridge.boot(&plan(None), (), Hooks::none(), 200., 200.);
    let runner = bridge.host.as_ref().unwrap().runner();
    let url = "https://example.test/changes?since=0";
    runner.faults().arm(url, None).unwrap();
    let fault = runner.fetch_fault();
    let seq = runner.seq();
    assert!(fault(url).is_some());
    let len = bridge.input_write(br#"{"op":"logs"}"#);
    let len = bridge.agent(len);
    let logs: serde_json::Value =
        serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap();
    let message = format!("fetch failed (driver fault): {url}");
    assert!(
        logs["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line.as_str().unwrap().ends_with(&message)),
        "{logs}"
    );
    let len = bridge.input_write(br#"{"op":"logs"}"#);
    let len = bridge.agent(len);
    let logs: serde_json::Value =
        serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap();
    assert_eq!(
        logs["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|line| line
                .as_str()
                .unwrap()
                .contains("fetch failed (driver fault)"))
            .count(),
        1,
        "drained once"
    );
    assert_eq!(bridge.host.as_ref().unwrap().runner().seq(), seq);
}

/// The old executor's long poll takes from the live table until the commit
/// lets it go: a fault its background retry takes after the candidate
/// carried the table neither holds the commit back nor fails a third attempt
/// in the session that replaces it.
#[test]
fn a_commit_keeps_the_faults_a_retry_took_while_it_prepared() {
    let mut bridge = Bridge::new();
    bridge.boot(&plan(None), (), Hooks::none(), 200., 200.);
    let url = "https://example.test/changes?since=0";
    let runner = bridge.host.as_ref().unwrap().runner();
    runner.faults().arm(url, Some(2)).unwrap();
    let retry = runner.fetch_fault();
    assert!(retry(url).is_some(), "the first attempt");
    let next = plan(None);
    bridge.input_write(&next);
    bridge.prepare_plan(next.len(), (), Hooks::none(), 200., 200.);
    assert!(bridge.prepared.is_some());
    assert!(retry(url).is_some(), "the retry takes the second");
    let len = bridge.commit_plan();
    let reply = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(!reply.contains("\"pending\""), "{reply}");
    let runner = bridge.host.as_ref().unwrap().runner();
    let faults = runner.faults().json();
    assert!(faults.contains("\"left\":0,\"hits\":2"), "{faults}");
    assert!(runner.fetch_fault()(url).is_none(), "no third failure");
}
