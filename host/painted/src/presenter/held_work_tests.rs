use super::*;
use exact_runner::{Answer, DataError, Dispatch, Outcome, Request, Store, Value};

#[derive(Default)]
struct Held {
    ready: bool,
    held: bool,
    settled: usize,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl DataSource for Held {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }

    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(Request::continuation(1)))
    }

    fn dispatch(&mut self, token: u64, _: &Store) -> Dispatch {
        assert_eq!(token, 1);
        self.held = true;
        Dispatch::Held
    }

    fn release(&mut self, _: &Store) -> Vec<(u64, Dispatch)> {
        if self.ready && self.held {
            self.held = false;
            vec![(1, Dispatch::Again)]
        } else {
            Vec::new()
        }
    }

    fn abort_wake(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        self.wake = Some(wake);
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        assert!(matches!(outcome, Outcome::Response(r) if r.status == 200));
        self.settled += 1;
        Ok(Answer::Now(Value::Bool(true)))
    }
}

#[test]
fn a_wake_without_a_reply_releases_held_work() {
    let plan = crate::test_plan(
        r#"component App
  mutation result as shape bool
  action start
    send result = work()
  view
    column width=200 height=200
      button press=start testId="start"
        text "Start"
      match result
        case some(value)
          text "Completed" testId="completed"
        case none
          text "Waiting"
"#,
    );
    let (mut p, error) = Presenter::boot_with(
        &plan,
        Held::default(),
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let k = p.host.kernel();
    let start = k.node_by_key(k.find_by_test_id("start")[0]).unwrap().id;
    p.tap(start).unwrap();
    assert!(p.host.runner().has_pending());
    assert!(p.parked.contains_key(&1));

    assert_eq!(p.pump(0.), None);
    assert!(p.parked.contains_key(&1), "still waiting on the source");

    // A secret lease becomes ready and wakes the host without a reply or topic.
    let source = p.host.runner_mut().data();
    source.ready = true;
    source.wake.as_ref().unwrap()();
    assert!(!p.host.has_announced());
    assert!(p.executor.drain().is_empty());
    assert_eq!(p.pump(0.), None);
    assert!(p.parked.is_empty(), "the wake releases the parked request");

    assert_eq!(p.pump(0.), None);
    assert!(!p.host.runner().has_pending());
    assert_eq!(p.host.kernel().find_by_test_id("completed").len(), 1);
    assert_eq!(p.pump(0.), None);
    assert_eq!(p.host.runner_mut().data().settled, 1);
}

struct Oversized;

impl DataSource for Oversized {
    fn grants(&self) -> &str {
        "net.fetch https://held.test"
    }
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        let mut request = Request::get("https://held.test/refused");
        request.body = Vec::with_capacity((4 << 20) + 1);
        Ok(Answer::Later(request))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        assert!(matches!(outcome, Outcome::Failed {
            kind: exact_runner::FailureKind::Refused, ref message,
        } if message == "native request exceeds 4 MiB"));
        Ok(Answer::Now(Value::Bool(true)))
    }
}

#[test]
#[cfg(unix)]
#[allow(unsafe_code)]
fn an_empty_pump_delivers_a_refusal_from_waiting_admission() {
    use exact_runner::{RequestClass, RequestOut, Work};
    use std::sync::mpsc::channel;
    use std::time::Duration;
    let plan = crate::test_plan(
        r#"component App
  mutation result as shape bool
  action start
    send result = work()
  view
    button press=start testId="start"
      text "Start"
"#,
    );
    let (mut p, error) = Presenter::boot_with(
        &plan,
        Oversized,
        (200., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let request = |ticket, request, class| RequestOut {
        ticket,
        request,
        class,
        target: "fixture".into(),
        forced: false,
    };
    let (sent, replies) = channel();
    for ticket in 1000..1016 {
        let sent = sent.clone();
        p.executor
            .run(
                request(ticket, Request::continuation(ticket), RequestClass::Ordered),
                Some(Work::Later(Box::new(move |reply| {
                    sent.send(reply).unwrap();
                }))),
            )
            .unwrap();
    }
    let replies: Vec<_> = (0..16)
        .map(|_| replies.recv_timeout(Duration::from_secs(10)).unwrap())
        .collect();
    p.executor
        .run(
            request(
                1016,
                Request::post_json("https://held.test/cancelled", "{}"),
                RequestClass::Read,
            ),
            None,
        )
        .unwrap();
    let k = p.host.kernel();
    let start = k.node_by_key(k.find_by_test_id("start")[0]).unwrap().id;
    p.tap(start).unwrap();
    assert!(p.host.runner().has_pending());
    assert!(!p.host.has_ordered_request_refusals());
    p.executor.cancel(&[1016]);
    for reply in replies {
        reply.send(Outcome::Storage(Vec::new()));
    }
    while p.host.runner().has_pending() {
        let mut poll = libc::pollfd {
            fd: p.executor.fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert!(
            unsafe { libc::poll(&mut poll, 1, 10_000) } > 0,
            "the refusal must wake its pump"
        );
        assert_eq!(p.pump(0.), None);
    }
    assert!(p.executor.ordered_idle());
    assert!(!p.host.has_ordered_request_refusals());
}
