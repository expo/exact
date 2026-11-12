//! A Rust source's two-way socket (LLP 1016.003), through the public
//! surface a data crate uses: `Request::socket`, the mapper's
//! `store.socket()` and the `Socket` handle, under LLP 1016.000 D7's rules.
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    Answer, DataError, DataSource, Message, Outcome, Request, Runner, Socket, SocketOp,
    SocketOrigin, SocketSend, Store,
};
use std::collections::HashMap;

const SRC: &str = r#"
component App
  state id = "a"
  state q = ""
  resource feed = feed(id) as shape string
  resource side = side() as shape string
  resource reply = ask(q) as shape string
  mutation said as shape string
  action say(what: string)
    send said = say(what)
  action sayOn(which: string, what: string)
    send said = sayOn(which, what)
  action flood
    send said = flood()
  action closeWith(code: number, reason: string)
    send said = closeWith(code, reason)
  action sayThenFetch
    send said = sayThenFetch()
  action ask(what: string)
    q = what
  action switch
    id = "b"
  view
    text feed testId="feed"
"#;

/// An order book's feed. `feed(id)` and `side()` open sockets, `side` at
/// the same URL as `feed("a")`. Each mapper subscribes on `open`, keeping its
/// handle by source, and answers a frame: `frame N` with `Ack N`, `binary`
/// with three bytes, `other` on `side`'s socket, `flood` with 257 frames,
/// `misshapen` with an `Ack` and a value outside the shape. The mutations
/// send on the kept handles; what a handle refused is kept in `refused`.
#[derive(Default)]
struct Book {
    kept: HashMap<String, Socket>,
    refused: Vec<String>,
    at_close: Vec<Option<Socket>>,
}

fn why(e: DataError) -> String {
    match e {
        DataError::Unavailable(why) => why,
        other => format!("{other:?}"),
    }
}

impl Book {
    fn note(&mut self, sent: Result<(), DataError>) {
        if let Err(e) = sent {
            self.refused.push(why(e));
        }
    }
}

impl DataSource for Book {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::Unavailable("asked at run time".into()))
    }

    fn grants(&self) -> &str {
        "net.websocket ws://echo.test"
    }

    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        let arg = |n: usize| args[n].as_str().unwrap_or("").to_string();
        let socket = |name: &str| Request::socket(&format!("ws://echo.test/{name}"));
        let reply = |what: &str| Ok(Answer::Now(Value::str(what)));
        match source {
            "feed" => Ok(Answer::Later(socket(&arg(0)))),
            "side" => Ok(Answer::Later(socket("a"))),
            "ask" => {
                if let Some(s) = self.kept.get("feed").copied() {
                    let sent = s.send_text(store, "from a read");
                    self.note(sent);
                }
                reply("asked")
            }
            "say" | "sayOn" => {
                let (which, what) = match source {
                    "say" => ("feed".to_string(), arg(0)),
                    _ => (arg(0), arg(1)),
                };
                let s = self.kept[&which];
                let sent = s.send_text(store, &what);
                self.note(sent);
                reply(&what)
            }
            "flood" => {
                let s = self.kept["feed"];
                for n in 0..257 {
                    let sent = s.send_text(store, &format!("{n}"));
                    self.note(sent);
                }
                reply("flooded")
            }
            "closeWith" => {
                let s = self.kept["feed"];
                let code = args[0].as_number().unwrap_or(0.) as u16;
                let closed = s.close(store, code, &arg(1));
                self.note(closed);
                let after = s.send_text(store, "after the close");
                self.note(after);
                reply("closed")
            }
            "sayThenFetch" => {
                self.kept["feed"].send_text(store, "and fetch")?;
                Ok(Answer::Later(Request::get("ws://echo.test/fetch")))
            }
            _ => Err(DataError::UnknownSource(source.into())),
        }
    }

    fn parse(
        &mut self,
        store: &mut Store,
        source: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let m = match outcome {
            Outcome::Message(m) => m,
            Outcome::Closed { code, reason, .. } => {
                self.at_close.push(store.socket());
                let text = format!("close {code} {reason}");
                return Ok(Answer::Now(Value::str(&text)));
            }
            other => return Ok(Answer::Now(Value::str(&other.summary()))),
        };
        let socket = store.socket().expect("a socket's mapper has its handle");
        if m.event == "open" {
            self.kept.insert(source.into(), socket);
            socket.send_text(store, "subscribe")?;
            return Ok(Answer::Now(Value::str("open")));
        }
        match m.data.as_str() {
            "binary" => socket.send_binary(store, &[1, 2, 3])?,
            "other" => self.kept["side"].send_text(store, "stray")?,
            "flood" => (0..257).for_each(|n| {
                let _ = socket.send_text(store, &format!("{n}"));
            }),
            "misshapen" => {
                socket.send_text(store, "Ack misshapen")?;
                return Ok(Answer::Now(Value::Number(1.)));
            }
            data => {
                let ack = data.strip_prefix("frame ").unwrap_or(data);
                socket.send_text(store, &format!("Ack {ack}"))?;
            }
        }
        Ok(Answer::Now(Value::str(&format!("message {}", m.data))))
    }
}

/// The app booted, with `feed`'s and `side`'s tickets.
fn boot() -> (Runner<Book>, u64, u64) {
    let plan = contract::compile(SRC).expect("the fixture compiles");
    let boot = Runner::boot(
        plan,
        Book::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    );
    let mut r = boot.expect("boots");
    let out = r.take_requests();
    let socket = |n: usize| out[n].request.socket && out[n].request.stream;
    assert!(
        socket(0) && socket(1),
        "the host is told both sockets are two-way"
    );
    assert_eq!(out[0].request.url, out[1].request.url, "both at one URL");
    (r, out[0].ticket, out[1].ticket)
}

fn message(event: &str, data: &str) -> Outcome {
    Outcome::Message(Message {
        event: event.into(),
        data: data.into(),
        ..Message::default()
    })
}

fn open(r: &mut Runner<Book>, ticket: u64) {
    r.fulfill(ticket, message("open", "")).unwrap();
}

fn frame(r: &mut Runner<Book>, ticket: u64, data: &str) {
    r.fulfill(ticket, message("", data)).unwrap();
}

/// Each batch taken: who staged it, and each operation with its ticket.
fn batches(r: &mut Runner<Book>) -> Vec<(SocketOrigin, Vec<(u64, SocketOp)>)> {
    let sends: Vec<SocketSend> = r.take_socket_sends();
    (sends.into_iter())
        .map(|s| {
            (
                s.origin,
                s.ops.into_iter().map(|(t, _, op)| (t, op)).collect(),
            )
        })
        .collect()
}

fn text(ticket: u64, t: &str) -> (u64, SocketOp) {
    (ticket, SocketOp::Text(t.into()))
}

fn mapper(name: &str) -> SocketOrigin {
    SocketOrigin::Mapper(name.into())
}

fn said() -> SocketOrigin {
    SocketOrigin::Mutation("said".into())
}

fn journal(r: &Runner<Book>) -> String {
    r.journal().collect::<Vec<_>>().join("\n")
}

fn act(r: &mut Runner<Book>, action: &str, args: &[&str]) {
    r.act(action, args.iter().map(|a| Value::str(a)).collect())
        .unwrap();
}

#[test]
fn the_mapper_subscribes_on_open_answers_frames_and_a_mutation_sends() {
    let (mut r, feed, _) = boot();
    open(&mut r, feed);
    assert_eq!(
        batches(&mut r),
        [(mapper("feed"), vec![text(feed, "subscribe")])]
    );
    frame(&mut r, feed, "frame 1");
    frame(&mut r, feed, "binary");
    assert_eq!(r.resource("feed"), Some(&Value::str("message binary")));
    assert_eq!(
        batches(&mut r),
        [
            (mapper("feed"), vec![text(feed, "Ack 1")]),
            (
                mapper("feed"),
                vec![(feed, SocketOp::Binary(vec![1, 2, 3]))]
            ),
        ]
    );
    act(&mut r, "say", &["hi"]);
    let sends = r.take_socket_sends();
    assert_eq!(sends.len(), 1);
    assert_eq!(
        (sends[0].origin.clone(), sends[0].ops[0].2.clone()),
        (said(), SocketOp::Text("hi".into()))
    );
    r.socket_published(&sends[0], Ok(vec![(feed, 1, 2)]));
    let state = exact_runner::agent::state(&r);
    assert!(
        state.contains(r#""readyState":1,"bufferedAmount":2,"sent":1"#),
        "{state}"
    );
    assert!(journal(&r).contains("socket send: 2 bytes (feed)"));
}

#[test]
fn a_frame_or_mutation_that_fails_sends_nothing_it_staged() {
    let (mut r, feed, _) = boot();
    open(&mut r, feed);
    r.take_socket_sends();
    // A mutation that sends must answer now: one that fetches too is refused.
    act(&mut r, "sayThenFetch", &[]);
    assert!(r.take_socket_sends().is_empty());
    assert!(r
        .take_requests()
        .iter()
        .all(|o| !o.request.url.ends_with("/fetch")));
    assert!(journal(&r).contains("a socket mutation must answer synchronously"));
    // A frame whose value is outside the shape fails, and ends the stream.
    let _ = r.fulfill(feed, message("", "misshapen"));
    assert!(r.take_socket_sends().is_empty());
    assert!(
        journal(&r).contains("mapper feed failed:"),
        "{}",
        journal(&r)
    );
}

#[test]
fn two_resources_at_one_url_keep_their_own_sockets() {
    let (mut r, feed, side) = boot();
    open(&mut r, feed);
    open(&mut r, side);
    assert_ne!(feed, side);
    assert_eq!(
        batches(&mut r),
        [
            (mapper("feed"), vec![text(feed, "subscribe")]),
            (mapper("side"), vec![text(side, "subscribe")]),
        ]
    );
    act(&mut r, "sayOn", &["side", "to side"]);
    assert_eq!(batches(&mut r), [(said(), vec![text(side, "to side")])]);
    // A mapper sends on its own socket alone: the frame fails.
    let _ = r.fulfill(feed, message("", "other"));
    assert!(r.take_socket_sends().is_empty());
    assert!(
        journal(&r).contains("InvalidStateError: a mapper sends only on its own socket"),
        "{}",
        journal(&r)
    );
}

#[test]
fn a_handle_from_before_a_refresh_is_stale() {
    let (mut r, feed, _) = boot();
    open(&mut r, feed);
    r.take_socket_sends();
    act(&mut r, "switch", &[]);
    let again = r.take_requests();
    let next = again
        .iter()
        .find(|o| o.request.url.ends_with("/b"))
        .expect("feed reopens");
    let ticket = next.ticket;
    open(&mut r, ticket);
    assert_eq!(
        batches(&mut r),
        [(mapper("feed"), vec![text(ticket, "subscribe")])]
    );
    // `kept` now holds the new handle; the old one is refused.
    let old = Socket::new(r.data().kept["feed"].resource(), feed);
    r.data().kept.insert("feed".into(), old);
    act(&mut r, "say", &["late"]);
    assert!(r.take_socket_sends().is_empty());
    assert_eq!(
        r.data().refused,
        ["InvalidStateError: the socket is not open"]
    );
}

#[test]
fn a_mapper_past_the_bounds_fails_its_frame_even_when_it_ignores_the_error() {
    let (mut r, feed, _) = boot();
    open(&mut r, feed);
    r.take_socket_sends();
    // A mutation's operation past them is refused alone; the rest go.
    act(&mut r, "flood", &[]);
    let sends = r.take_socket_sends();
    assert_eq!(sends.len(), 1);
    assert_eq!(sends[0].ops.len(), 256);
    assert_eq!(
        r.data().refused,
        ["QuotaExceededError: socket mutation exceeds 256 operations"]
    );
    let _ = r.fulfill(feed, message("", "flood"));
    assert!(r.take_socket_sends().is_empty());
    assert!(journal(&r).contains("QuotaExceededError: socket mapper call exceeds 256 operations"));
}

#[test]
fn the_transport_s_buffer_bounds_a_send() {
    let (mut r, feed, _) = boot();
    r.set_socket_reader(std::sync::Arc::new(|_| Some((1 << 20) - 1)));
    open(&mut r, feed);
    assert!(
        r.take_socket_sends().is_empty(),
        "subscribe passes the buffer"
    );
    assert!(journal(&r).contains("QuotaExceededError: socket buffer exceeds 1 MiB"));
}

#[test]
fn a_close_is_checked_and_ends_the_call_s_sends() {
    let (mut r, feed, _) = boot();
    open(&mut r, feed);
    r.take_socket_sends();
    let close = |r: &mut Runner<Book>, code: f64, reason: &str| {
        r.act("closeWith", vec![Value::Number(code), Value::str(reason)])
            .unwrap();
        std::mem::take(&mut r.data().refused)
    };
    let after = || (said(), vec![text(feed, "after the close")]);
    let refused = close(&mut r, 999., "");
    assert_eq!(refused, ["InvalidAccessError: invalid socket close code"]);
    assert_eq!(batches(&mut r), [after()], "a refused close stages nothing");
    let refused = close(&mut r, 4000., &"x".repeat(124));
    assert_eq!(
        refused,
        ["SyntaxError: socket close reason exceeds 123 UTF-8 bytes"]
    );
    assert_eq!(batches(&mut r), [after()]);
    let refused = close(&mut r, 4000., "done");
    assert_eq!(
        refused,
        ["InvalidStateError: the socket is not open"],
        "a send after the close"
    );
    let close = SocketOp::Close {
        code: 4000,
        reason: "done".into(),
    };
    assert_eq!(batches(&mut r), [(said(), vec![(feed, close)])]);
}

#[test]
fn a_read_and_a_socket_s_end_send_nothing() {
    let (mut r, feed, _) = boot();
    open(&mut r, feed);
    r.take_socket_sends();
    act(&mut r, "ask", &["now"]);
    assert_eq!(r.resource("reply"), Some(&Value::str("asked")));
    assert_eq!(
        r.data().refused,
        ["InvalidStateError: a socket sends only from a mutation's answer or its own mapper"]
    );
    let end = Outcome::Closed {
        code: 1000,
        reason: "bye".into(),
        was_clean: true,
    };
    r.fulfill(feed, end).unwrap();
    assert_eq!(r.resource("feed"), Some(&Value::str("close 1000 bye")));
    assert_eq!(
        r.data().at_close,
        [None],
        "the end hands the mapper no handle"
    );
    assert!(r.take_socket_sends().is_empty());
}
