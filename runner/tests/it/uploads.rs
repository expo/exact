//! Upload marks follow the reply through mapper turns (LLP 1016.000 D10).
use exact_kernel::Kernel;
use exact_plan::Value;

/// The host work these tests hand out (LLP 1047.001 D12).
struct TestWork;
impl exact_runner::HostWork for TestWork {}
use exact_runner::{
    Answer, DataError, DataSource, FailureKind, Message, Outcome, Request, Response, Runner, Store,
    UploadEnd,
};

const APP: &str = r#"
component App
  state wave = 1
  resource progress = progress(wave) as shape number
  action go(n: number)
    wave = n
  action retry
    refresh progress
  view
    text `${progress}`
"#;

#[derive(Default)]
struct Source {
    progress_turns: u8,
    reply_turns: u8,
    same_request: bool,
    waiting: Option<(u8, f64)>,
}

impl Source {
    fn mapped(&mut self, turns: u8, value: f64) -> Answer {
        if turns == 0 {
            Answer::Now(Value::Number(value))
        } else {
            self.waiting = Some((turns - 1, value));
            Answer::Later(Request::continuation::<TestWork>(17))
        }
    }
}

impl DataSource for Source {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
        let url = if self.same_request {
            "https://example.test/upload".into()
        } else {
            format!("https://example.test/upload/{:?}", args[0])
        };
        Ok(Answer::stream(
            Request::post_json(&url, "{}").upload_progress(),
        ))
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(match outcome {
            Outcome::Message(m) => {
                let (loaded, _) = m.sent().expect("upload progress");
                self.mapped(self.progress_turns, loaded as f64)
            }
            Outcome::Response(r) => self.mapped(self.reply_turns, f64::from(r.status)),
            Outcome::Storage(bytes) => {
                if bytes == b"reject" {
                    return Err(DataError::Unavailable("the mapper rejected".into()));
                }
                let (turns, value) = self.waiting.take().expect("a mapper is waiting");
                self.mapped(turns, value)
            }
            Outcome::Failed { .. } => self.mapped(self.reply_turns, -1.0),
            _ => panic!("unexpected outcome: {outcome:?}"),
        })
    }
}

fn boot(source: Source) -> (Runner<Source>, u64) {
    let mut r = Runner::boot(
        contract::compile(APP).unwrap(),
        source,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.data_ready().unwrap();
    let out = r.take_requests();
    assert_eq!(out.len(), 1);
    assert!(out[0].request.upload_progress);
    (r, out[0].ticket)
}

fn reply() -> Outcome {
    Outcome::Response(Response {
        status: 201,
        headers: Vec::new(),
        body: Vec::new(),
    })
}

fn turns(r: &mut Runner<Source>, count: u8) {
    for _ in 0..count {
        let out = r.take_requests();
        assert_eq!(out.len(), 1, "one mapper turn");
        assert!(out[0].request.continuation.is_some());
        r.fulfill(out[0].ticket, Outcome::Storage(Vec::new()))
            .unwrap();
    }
}

fn mark(r: &Runner<Source>, wave: f64) -> UploadEnd {
    r.carry()
        .uploads
        .into_iter()
        .find(|m| m.args == [Value::Number(wave)])
        .expect("upload mark")
        .end
}

fn value(r: &Runner<Source>) -> f64 {
    r.resource("progress").and_then(Value::as_number).unwrap()
}

fn completed(progress_turns: u8, reply_turns: u8) {
    let (mut r, ticket) = boot(Source {
        progress_turns,
        reply_turns,
        ..Default::default()
    });
    r.fulfill(
        ticket,
        Outcome::Message(Message::upload_progress(1024, 2048)),
    )
    .unwrap();
    turns(&mut r, progress_turns);
    assert_eq!(value(&r), 1024.0);
    assert!(r.has_pending());
    r.fulfill(ticket, reply()).unwrap();
    turns(&mut r, reply_turns);
    assert_eq!(value(&r), 201.0);
    assert!(!r.has_pending());
    assert_eq!(mark(&r, 1.0), UploadEnd::Answered(Value::Number(201.0)));

    r.act("go", vec![Value::Number(2.0)]).unwrap();
    assert_eq!(r.take_requests().len(), 1);
    r.act("go", vec![Value::Number(1.0)]).unwrap();
    assert!(
        r.take_requests().is_empty(),
        "an upload is never sent twice"
    );
    assert_eq!(value(&r), 201.0);
}

#[test]
fn a_synchronous_upload_keeps_its_reply() {
    completed(0, 0);
}

#[test]
fn an_upload_with_progress_mapper_turns_keeps_its_reply() {
    completed(2, 0);
}

#[test]
fn an_upload_with_reply_mapper_turns_keeps_its_reply() {
    completed(0, 2);
}

#[test]
fn an_upload_reply_held_behind_progress_turns_ends_every_kept_arguments_mark() {
    let (mut r, ticket) = boot(Source {
        progress_turns: 2,
        reply_turns: 2,
        same_request: true,
        ..Default::default()
    });
    r.act("go", vec![Value::Number(2.0)]).unwrap();
    assert!(
        r.take_requests().is_empty(),
        "the same upload stays in flight"
    );
    r.fulfill(
        ticket,
        Outcome::Message(Message::upload_progress(1024, 2048)),
    )
    .unwrap();
    assert!(r.fulfill(ticket, reply()).unwrap().is_none());
    turns(&mut r, 2);
    assert_eq!(value(&r), 1024.0);
    turns(&mut r, 3); // The held reply, then its two mapper turns.
    assert_eq!(value(&r), 201.0);
    assert!(!r.has_pending());
    for wave in [1.0, 2.0] {
        assert_eq!(mark(&r, wave), UploadEnd::Answered(Value::Number(201.0)));
        r.act("go", vec![Value::Number(wave)]).unwrap();
        assert!(r.take_requests().is_empty());
        assert_eq!(value(&r), 201.0);
    }
}

#[test]
fn an_upload_let_go_after_progress_turns_or_during_reply_turns_is_interrupted() {
    for reply_waiting in [false, true] {
        let (mut r, ticket) = boot(Source {
            progress_turns: 1,
            reply_turns: 2,
            ..Default::default()
        });
        r.fulfill(
            ticket,
            Outcome::Message(Message::upload_progress(1024, 2048)),
        )
        .unwrap();
        turns(&mut r, 1);
        if reply_waiting {
            r.fulfill(ticket, reply()).unwrap();
            turns(&mut r, 1);
        }
        r.act("go", vec![Value::Number(2.0)]).unwrap();
        assert!(matches!(mark(&r, 1.0), UploadEnd::Failed(f) if f.reason == "interrupted"));
        r.take_requests();
        r.act("go", vec![Value::Number(1.0)]).unwrap();
        assert!(r.take_requests().is_empty());
        let state = exact_runner::agent::handle(&r, r#"{"op":"state"}"#);
        assert!(state.contains("the upload was cut off before its reply"));
    }
}

#[test]
fn an_upload_reply_without_progress_keeps_its_mapper_rejection() {
    let (mut r, ticket) = boot(Source {
        reply_turns: 2,
        ..Default::default()
    });
    r.fulfill(ticket, reply()).unwrap();
    turns(&mut r, 1);
    let out = r.take_requests();
    assert_eq!(out.len(), 1);
    r.fulfill(out[0].ticket, Outcome::Storage(b"reject".to_vec()))
        .unwrap();
    assert!(matches!(mark(&r, 1.0), UploadEnd::Failed(f) if f.message == "the mapper rejected"));
    assert!(!r.has_pending());
    r.act("retry", vec![]).unwrap();
    assert!(r.take_requests().is_empty());
}

#[test]
fn an_upload_failure_mapped_in_later_turns_is_remembered_only_if_it_was_sent() {
    for started in [false, true] {
        let (mut r, ticket) = boot(Source {
            reply_turns: 2,
            ..Default::default()
        });
        if started {
            r.fulfill(ticket, Outcome::Message(Message::upload_start()))
                .unwrap();
        }
        r.fulfill(
            ticket,
            Outcome::Failed {
                kind: FailureKind::Network,
                message: "connection failed".into(),
            },
        )
        .unwrap();
        turns(&mut r, 2);
        assert_eq!(value(&r), -1.0);
        assert!(!r.has_pending());
        if started {
            assert_eq!(mark(&r, 1.0), UploadEnd::Answered(Value::Number(-1.0)));
        } else {
            assert!(r.carry().uploads.is_empty());
        }
        r.act("retry", vec![]).unwrap();
        assert_eq!(r.take_requests().len(), usize::from(!started));
    }
}
