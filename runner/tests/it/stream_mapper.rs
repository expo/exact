//! A stream stays pending until its mapper commits the first value (LLP 1016.000 D1).
use exact_kernel::{Kernel, PropId};
use exact_plan::Value;
use exact_runner::{
    Answer, DataError, DataSource, Message, Outcome, Request, Runner, Store, Target,
};

const APP: &str = r#"
component App
  resource feed = feed() as shape string[]
  derive waiting = pending(feed)
  resource echoed = echo(pending(feed)) as shape bool
  view
    column
      text testId="direct" pending(feed) ? "connecting" : "live"
      text testId="derived" waiting ? "connecting" : "live"
      text testId="echoed" echoed ? "connecting" : "live"
      text testId="count" `${feed.length} messages`
"#;

struct Source;

impl DataSource for Source {
    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        match source {
            "echo" => Ok(args[0].clone()),
            _ => Err(DataError::UnknownSource(source.into())),
        }
    }

    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        match source {
            "feed" => {
                let mut request = Request::get("https://example.test/feed");
                request.fetch = Some(23);
                Ok(Answer::stream(request))
            }
            _ => self.query(source, args).map(Answer::Now),
        }
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let data = match outcome {
            Outcome::Message(m) => m.data,
            Outcome::Storage(bytes) => String::from_utf8(bytes).unwrap(),
            _ => panic!("unexpected outcome: {outcome:?}"),
        };
        match data.as_str() {
            "yield" => Ok(Answer::Later(Request::continuation(17))),
            "reject" => Err(DataError::Unavailable("the mapper rejected".into())),
            _ => Ok(Answer::Now(Value::list(
                data.split_whitespace().map(Value::str).collect(),
            ))),
        }
    }

    fn parse_fetch_for(
        &mut self,
        _: Target,
        fetch: Option<u64>,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Option<Answer>, DataError> {
        let expected = matches!(outcome, Outcome::Message(_)).then_some(23);
        assert_eq!(fetch, expected, "a mapper reply names its own request");
        self.parse(store, source, args, outcome).map(Some)
    }
}

fn boot() -> (Runner<Source>, u64) {
    let mut r = Runner::boot(
        contract::compile(APP).unwrap(),
        Source,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let requests = r.take_requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].request.stream);
    (r, requests[0].ticket)
}

fn message(r: &mut Runner<Source>, ticket: u64, data: &str) {
    r.fulfill(
        ticket,
        Outcome::Message(Message {
            data: data.into(),
            ..Message::default()
        }),
    )
    .unwrap();
}

fn turn(r: &mut Runner<Source>, data: &str) {
    let requests = r.take_requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].request.continuation.is_some());
    r.fulfill(
        requests[0].ticket,
        Outcome::Storage(data.as_bytes().to_vec()),
    )
    .unwrap();
}

fn shown(r: &Runner<Source>, status: &str, count: usize, outstanding: bool) {
    let texts = ["direct", "derived", "echoed", "count"].map(|id| {
        let key = r.kernel().find_by_test_id(id)[0];
        r.kernel()
            .node_by_key(key)
            .unwrap()
            .props
            .str(PropId::Text)
            .unwrap()
            .to_string()
    });
    let work = r.outstanding();
    assert_eq!(
        (texts, work.is_clear(), work.streams),
        (
            [
                status.into(),
                status.into(),
                status.into(),
                format!("{count} messages")
            ],
            !outstanding,
            if outstanding {
                vec!["feed".into()]
            } else {
                vec![]
            },
        )
    );
}

#[test]
fn pending_ends_with_the_first_mapped_value() {
    let (mut r, ticket) = boot();
    shown(&r, "connecting", 0, true);

    message(&mut r, ticket, "yield");
    shown(&r, "connecting", 0, true);
    turn(&mut r, "yield");
    shown(&r, "connecting", 0, true);
    assert!(r.has_pending() && !r.is_open_stream(ticket));
    assert_eq!(r.streams()[0].2.messages, 0);

    turn(&mut r, "a b");
    shown(&r, "live", 2, false);
    assert!(!r.has_pending() && r.is_open_stream(ticket));
    assert_eq!(r.streams()[0].2.messages, 1);

    message(&mut r, ticket, "yield");
    shown(&r, "live", 2, false);
    assert_eq!(r.streams()[0].2.messages, 1);
    assert!(
        r.has_pending(),
        "the clock still waits for a later mapper turn"
    );
    turn(&mut r, "a b c");
    shown(&r, "live", 3, false);
    assert!(!r.has_pending() && r.is_open_stream(ticket));
    assert_eq!(r.streams()[0].2.messages, 2);
}

#[test]
fn a_rejected_first_mapper_ends_pending_without_a_value() {
    let (mut r, ticket) = boot();
    message(&mut r, ticket, "yield");
    shown(&r, "connecting", 0, true);
    turn(&mut r, "reject");
    shown(&r, "live", 0, false);
    assert!(!r.has_pending() && !r.holds(ticket));
    assert_eq!(r.outstanding().failed, ["feed"]);
    assert!(r.failed_resources()[0].1.contains("the mapper rejected"));
}

fn batch(r: &mut Runner<Source>, ticket: u64, data: &[&str]) {
    let batch = data.iter().map(|d| Message {
        data: (*d).into(),
        ..Message::default()
    });
    r.fulfill(ticket, Outcome::Messages(batch.collect()))
        .unwrap();
}

/// LLP 1016.000 D4: every message reaches the mapper in arrival order, and
/// what waited together commits once, as the last one's value.
#[test]
fn messages_that_waited_together_each_reach_the_mapper_and_commit_once() {
    let (mut r, ticket) = boot();
    batch(&mut r, ticket, &["a", "a b", "a b c"]);
    shown(&r, "live", 3, false);
    assert_eq!(r.streams()[0].2.messages, 3);
    let commits = |r: &Runner<Source>| {
        (r.journal())
            .filter(|l| l.contains(&format!(" fulfil {ticket} (")))
            .count()
    };
    assert_eq!(commits(&r), 1);
    assert!(r.journal().any(|l| l.contains("[3 messages, 9 bytes]")));

    // A message whose mapper yields holds the rest behind its turn, in order.
    batch(&mut r, ticket, &["x", "yield", "c d e f"]);
    shown(&r, "live", 3, false);
    turn(&mut r, "b b");
    shown(&r, "live", 2, false);
    turn(&mut r, "");
    shown(&r, "live", 4, false);
    assert!(r.take_requests().is_empty() && r.is_open_stream(ticket));

    // One the mapper refuses ends the stream; the batch commits nothing.
    batch(&mut r, ticket, &["p", "reject", "q"]);
    shown(&r, "live", 4, false);
    assert!(!r.holds(ticket));
    assert!(r.failed_resources()[0].1.contains("the mapper rejected"));
}

/// D4's bound behind a mapper's turns counts messages, a batch each of its
/// own, whether a later batch waits or a batch's tail does.
#[test]
fn messages_behind_a_turn_are_bounded_by_count_however_they_arrived() {
    let empty = |n: usize| vec![""; n];
    // Later batches: two of 2048 fit beside the turn, one more does not.
    let (mut r, ticket) = boot();
    batch(&mut r, ticket, &["a", "yield"]);
    batch(&mut r, ticket, &empty(2048));
    batch(&mut r, ticket, &empty(2048));
    assert!(r.holds(ticket));
    batch(&mut r, ticket, &["b"]);
    assert!(!r.holds(ticket));
    assert!(
        r.failed_resources()[0].1.contains(
            "fell behind: 4096 messages (0 bytes) waited for its turns, and 1 more arrived"
        ),
        "{:?}",
        r.failed_resources()
    );
    // A batch's own tail past the bound.
    let (mut r, ticket) = boot();
    let mut tail = vec!["yield"];
    tail.extend(empty(4097));
    batch(&mut r, ticket, &tail);
    assert!(!r.holds(ticket));
    assert!(r.failed_resources()[0]
        .1
        .contains("fell behind: 0 messages (0 bytes) waited for its turns, and 4097 more arrived"));
}
