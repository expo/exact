//! A Rust source's two-way socket across the module seam (LLP 1016.003 D5).
use super::*;
use exact_runner::Message;

/// `feed` opens a socket; its mapper subscribes on `open`, answering with
/// the `bufferedAmount` it read, describes a binary frame and the close,
/// and on `flood` passes the bounds, ignoring the error. `bare` is a socket
/// that is not a stream.
struct Feed;
impl DataSource for Feed {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::Unit)
    }
    fn answer(&mut self, _: &mut Store, source: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(match source {
            "bare" => Request {
                socket: true,
                ..Request::get("ws://echo.test/")
            },
            _ => Request::socket("wss://echo.test/book"),
        }))
    }
    fn parse(
        &mut self,
        store: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let socket = store.socket();
        let said = match outcome {
            Outcome::Message(m) if m.event == "open" => {
                let socket = socket.expect("the mapper's handle crosses");
                socket.send_text(store, "subscribe")?;
                format!("open {}", socket.buffered_amount(store))
            }
            Outcome::Message(m) if m.data == "flood" => {
                let socket = socket.expect("the mapper's handle crosses");
                for _ in 0..257 {
                    let _ = socket.send_binary(store, &[0]);
                }
                "flooded".into()
            }
            Outcome::Message(m) => format!("{:?} {:?}", m.data, m.binary),
            Outcome::Closed {
                code,
                reason,
                was_clean,
            } => format!("close {code} {reason} {was_clean} {socket:?}"),
            other => other.summary(),
        };
        Ok(Answer::Now(Value::str(&said)))
    }
}

fn call(store: &mut Store, source: &str, outcome: Option<Outcome>) -> Result<Answer, DataError> {
    let mut module = Session::new(Feed);
    let request = call_request(None, store, source, &[], outcome.as_ref()).unwrap();
    module.dispatch(&request).unwrap();
    call_reply(module.output(), store)
}

/// A host store whose turn is socket 0's mapper on ticket 7, with 5 bytes
/// buffered.
fn mapper_turn() -> Store {
    let mut store = Store::default();
    store.set_socket_call(SocketCall::Mapper(Socket::new(0, 7)));
    store.set_socket_open(vec![(0, 7)]);
    store.set_socket_buffered(vec![(0, 5)]);
    store
}

fn said(answer: Result<Answer, DataError>) -> String {
    match answer {
        Ok(Answer::Now(v)) => v.as_str().expect("a string").to_string(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_socket_request_crosses_as_a_two_way_stream() {
    let Answer::Later(request) = call(&mut Store::default(), "feed", None).unwrap() else {
        panic!("a request");
    };
    assert!(request.stream && request.socket, "{request:?}");
    assert_eq!(request.url, "wss://echo.test/book");
    let bare = call(&mut Store::default(), "bare", None).unwrap_err();
    assert!(format!("{bare:?}").contains("cannot cross the Rust module seam"));
}

#[test]
fn the_mapper_s_turn_crosses_in_and_its_frames_come_back_staged() {
    let mut store = mapper_turn();
    let open = Outcome::Message(Message {
        event: "open".into(),
        ..Message::default()
    });
    let answer = call(&mut store, "feed", Some(open));
    assert_eq!(said(answer), "open 14", "5 buffered and the 9 it staged");
    assert_eq!(
        store.staged_sockets(),
        [(0, SocketOp::Text("subscribe".into()))]
    );
    assert!(store.end_socket_call(Ok(())).is_ok());
}

#[test]
fn a_mapper_s_overload_crosses_and_fails_its_frame() {
    let mut store = mapper_turn();
    let flood = Outcome::Message(Message {
        data: "flood".into(),
        ..Message::default()
    });
    assert_eq!(said(call(&mut store, "feed", Some(flood))), "flooded");
    assert_eq!(store.staged_sockets().len(), 256);
    let failed = store.end_socket_call(Ok(())).unwrap_err();
    assert!(format!("{failed:?}")
        .contains("QuotaExceededError: socket mapper call exceeds 256 operations"));
}

#[test]
fn a_binary_frame_and_a_close_cross_as_themselves() {
    let binary = Outcome::Message(Message {
        binary: Some(vec![1, 2, 3]),
        ..Message::default()
    });
    assert_eq!(
        said(call(&mut mapper_turn(), "feed", Some(binary))),
        r#""" Some([1, 2, 3])"#
    );
    let close = Outcome::Closed {
        code: 4001,
        reason: "gone".into(),
        was_clean: true,
    };
    assert_eq!(
        said(call(&mut Store::default(), "feed", Some(close))),
        "close 4001 gone true None"
    );
}

#[test]
fn a_socket_keeps_tag_eleven_and_never_carries_a_cache_mode() {
    let mut request = Request::socket("wss://echo.test/book");
    request.cache = CacheMode::Reload;
    let mut encoded = Writer::default();
    encode_result(&mut encoded, Ok(Answer::Later(request)));
    let bytes = encoded.into_vec();
    assert_eq!(bytes[0], 11);
    let mut reader = Reader::new(&bytes);
    let Ok(Answer::Later(request)) = read_result(&mut reader).unwrap() else {
        panic!("a socket request");
    };
    assert!(request.socket);
    assert_eq!(request.cache, CacheMode::Unspecified);
    end(&reader).unwrap();
    for tag in [0, 5, 7, 8, 11, 12, 255] {
        let mut encoded = Writer::default();
        encoded.u8(12);
        encoded.string("reload");
        encoded.u8(tag);
        let bytes = encoded.into_vec();
        assert_eq!(
            read_result(&mut Reader::new(&bytes)).unwrap_err(),
            "invalid cached HTTP result tag"
        );
    }
}
