//! An open stream through the renderer (LLP 1016.000 D5): once its first message lands it is open, not
//! in flight, so the render completes with that message shown, and the checkpoint lists the stream as
//! open for the client to subscribe again (LLP 1048.000 D6).
use super::{warm_transport, SITE};
use exact_plan::{ActivatePolicy, Value};
use exact_render::{render_as, Ids, Settled};
use exact_runner::{Answer, DataError, DataSource, Outcome, Request, Store};
use exact_web::document::read_checkpoint;
use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// A stream of server-sent events from a server on this machine, and the
/// messages it has parsed.
struct Feed {
    url: String,
    grants: String,
    parsed: Arc<AtomicUsize>,
}
impl DataSource for Feed {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::stream(Request::get(&self.url)))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        source: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        // Take long enough with each message for the next to arrive, so one always waits
        std::thread::sleep(Duration::from_millis(5));
        self.parsed.fetch_add(1, Ordering::Relaxed);
        match outcome {
            Outcome::Message(message) => Ok(Answer::Now(Value::str(&message.data))),
            _ => Err(DataError::UnknownSource(source.into())),
        }
    }
    fn grants(&self) -> &str {
        &self.grants
    }
}

#[test]
fn a_stream_completes_the_render_at_its_first_message_and_is_listed_open() {
    // A hang bound, not a deadline: on a loaded Mac the platform transport
    // has taken over ten seconds to open a loopback connection.
    let bound = Duration::from_secs(60);
    warm_transport();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // The server sends events without a pause until the client closes the
    // stream, so a message is always waiting: the render completes at the
    // first all the same, and lets the stream go. It answers how its writing
    // ended.
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(bound)).unwrap();
        stream.set_write_timeout(Some(bound)).unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let head = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n";
        let mut written = stream.write_all(head);
        while written.is_ok() {
            written = stream.write_all(b"data: first\n\n");
        }
        written.unwrap_err().kind()
    });
    let plan = contract::compile(
        "component App\n  resource feed = feed() as shape string\n  view\n    text feed\n",
    )
    .unwrap();
    let url = format!("http://127.0.0.1:{port}/feed");
    let parsed = Arc::new(AtomicUsize::new(0));
    let feed = || Feed {
        url: url.clone(),
        grants: format!("net.fetch {url}\n"),
        parsed: parsed.clone(),
    };
    // Render a JavaScript page, whose activation the render infers
    let rendered = render_as(&plan, feed, Default::default(), "/", &SITE, bound, Ids::Any).unwrap();
    assert_eq!(rendered.settled, Settled::Complete, "not its deadline");
    assert_eq!(
        parsed.load(Ordering::Relaxed),
        1,
        "the first message completes it"
    );
    assert_eq!(rendered.status(false), 200);
    assert!(
        rendered.document.root.contains(">first<"),
        "{}",
        rendered.document.root
    );
    assert_eq!(rendered.state.open, [0], "`feed`'s answer is still open");
    assert!(
        rendered.state.pending.is_empty(),
        "{:?}",
        rendered.state.pending
    );
    assert_eq!(
        read_checkpoint(&rendered.checkpoint).unwrap(),
        rendered.state
    );
    // The runtime has a stream to subscribe again, so the page doesn't wait for the reader
    assert_eq!(rendered.activate, ActivatePolicy::Inferred);
    let ended = server.join().unwrap();
    assert!(
        matches!(ended, ErrorKind::BrokenPipe | ErrorKind::ConnectionReset),
        "the render let the stream go: {ended:?}"
    );
}
