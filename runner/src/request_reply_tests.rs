use super::*;
use std::sync::{Arc, Mutex};

type Seen = Arc<Mutex<Vec<String>>>;

fn recorder() -> (Seen, impl FnMut(Outcome) + Send + 'static) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let into = seen.clone();
    (seen, move |o: Outcome| {
        into.lock().unwrap().push(o.summary())
    })
}

#[test]
fn a_stream_reply_sends_many_then_ends_once() {
    let (seen, deliver) = recorder();
    let mut reply = Reply::stream(deliver);
    for data in ["a", "bb"] {
        reply.message(Message {
            data: data.into(),
            ..Message::default()
        });
    }
    reply.send(Outcome::Failed {
        kind: FailureKind::Network,
        message: "closed".into(),
    });
    assert_eq!(
        *seen.lock().unwrap(),
        ["message, 1 bytes", "message, 2 bytes", "network: closed"]
    );
}

#[test]
fn a_stream_reply_dropped_without_an_end_aborts_once() {
    let (seen, deliver) = recorder();
    let mut reply = Reply::stream(deliver);
    reply.message(Message::default());
    drop(reply);
    assert_eq!(
        *seen.lock().unwrap(),
        [
            "message, 0 bytes",
            "aborted: the owner ended without a reply"
        ]
    );
}

#[test]
fn a_message_on_a_single_reply_is_its_one_answer() {
    let (seen, deliver) = recorder();
    let mut reply = Reply::new(deliver);
    reply.message(Message::default());
    reply.message(Message::default());
    drop(reply);
    assert_eq!(*seen.lock().unwrap(), ["message, 0 bytes"]);
}

#[test]
fn a_stream_answer_is_independent_and_marked() {
    let Answer::Later(request) = Answer::stream(Request::get("https://example.test/e")) else {
        panic!("a stream is a request")
    };
    assert!(request.stream);
    assert_eq!(
        request.http,
        HttpScheduling::Independent {
            max_response_bytes: 1 << 20
        }
    );
    let Answer::Later(kept) =
        Answer::stream(Request::get("https://example.test/e").independent_http(4096))
    else {
        panic!("a stream is a request")
    };
    assert_eq!(
        kept.http,
        HttpScheduling::Independent {
            max_response_bytes: 4096
        }
    );
}
