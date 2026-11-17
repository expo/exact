//! A resource over a list through the renderer (LLP 1017.000): the checkpoint holds each element's answer, so the
//! runtime's boot from it shows the list as the document does and asks nothing (LLP 1048.000 D6).
use super::{warm_transport, SITE};
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_render::{render_as, Ids, Settled};
use exact_runner::{Answer, DataError, DataSource, Outcome, Request, Runner, Store};
use std::io::{Read, Write};
use std::time::Duration;

/// `echo(u)` asks `<base>/<u>`, and its reply's body is the answer.
struct Echo {
    base: String,
    grants: String,
}
impl DataSource for Echo {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
        let u = args[0].as_str().unwrap();
        Ok(Answer::Later(Request::get(&format!("{}/{u}", self.base))))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let Outcome::Response(r) = outcome else {
            return Err(DataError::Unavailable("no reply".into()));
        };
        Ok(Answer::Now(Value::str(&String::from_utf8(r.body).unwrap())))
    }
    fn grants(&self) -> &str {
        &self.grants
    }
}

#[test]
fn a_list_s_document_shows_its_elements_answers_and_its_boot_asks_nothing() {
    // A hang bound, not a deadline, as the stream test's
    let bound = Duration::from_secs(60);
    warm_transport();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // Answer each request for `/<u>` with `got-<u>`
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            stream.set_read_timeout(Some(bound)).unwrap();
            stream.set_write_timeout(Some(bound)).unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let line = String::from_utf8_lossy(&request)
                .lines()
                .next()
                .unwrap()
                .to_string();
            let body = format!(
                "got-{}",
                line.split(' ').nth(1).unwrap().trim_start_matches('/')
            );
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all((head + &body).as_bytes());
        }
    });
    let base = format!("http://127.0.0.1:{port}");
    let echo = || Echo {
        base: base.clone(),
        grants: format!("net.fetch {base}/\n"),
    };
    let plan = contract::compile(
        "component App\n  state urls: string[] = [\"a\", \"b\"]\n  resource heard = echo(u) for u in urls as shape string\n  view\n    text heard.join(\",\")\n",
    )
    .unwrap();
    // The wasm runtime's page: its document is projected from a boot from its checkpoint, as the runtime boots
    let rendered = render_as(
        &plan,
        echo,
        Default::default(),
        "/",
        &SITE,
        bound,
        Ids::Runtime,
    )
    .unwrap();
    assert_eq!(rendered.settled, Settled::Complete);
    assert!(
        rendered.document.root.contains(">got-a,got-b<"),
        "{}",
        rendered.document.root
    );
    let answers: Vec<(&str, &Value, &Value)> = (rendered.state.answers.iter())
        .map(|(name, _, args, value)| (name.as_str(), &args[0], value))
        .collect();
    assert_eq!(
        answers,
        [
            ("heard", &Value::str("a"), &Value::str("got-a")),
            ("heard", &Value::str("b"), &Value::str("got-b"))
        ]
    );
    let mut booted = Runner::boot_checkpoint(
        plan,
        echo(),
        Kernel::with_monospace(),
        &rendered.state,
        Vec::new(),
        Default::default(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert!(booted.take_requests().is_empty());
}
