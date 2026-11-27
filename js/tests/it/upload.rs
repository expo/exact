//! `fetch(url, {exactStream, exactUploadProgress: true})` from TypeScript
//! (LLP 1016.000 D10): the prelude sends the request as an upload, the
//! native executor core (the one the Apple, Linux and render hosts share,
//! here the render host's, whose transport is the portable one, rustls, on a
//! Mac too: LLP 1048.000 D10) tells the body's bytes sent as they go, and the
//! mapper gets each as `progress` and then the reply as `load`. URLSession's
//! is `exact-apple`'s `an_upload_through_urlsession_…`.
#![cfg(all(exact_js_engine, unix))]
use super::storage::{args, call, text, Root};
use exact_runner::{Answer, DataSource, Outcome, RequestOut, Store};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

/// The upload's size: past what loopback's socket buffers take at once.
const SIZE: usize = 16 << 20;

/// A server on 127.0.0.1 that takes a request's body 256 KiB at a time,
/// 4 ms apart, and answers 201 with the bytes it took, `wait` after the
/// body. Its port.
fn server(wait: Duration) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = n.trim().parse().unwrap();
                }
            }
            let (mut took, mut chunk) = (0, vec![0; 256 << 10]);
            while took < length {
                let n = reader
                    .read(&mut chunk[..(length - took).min(256 << 10)])
                    .unwrap_or(0);
                if n == 0 {
                    break;
                }
                took += n;
                std::thread::sleep(Duration::from_millis(4));
            }
            std::thread::sleep(wait);
            let body = format!("{{\"took\":{took}}}");
            let _ = write!(
                stream,
                "HTTP/1.1 201 Created\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    port
}

/// The fixture's `op` uploads `body` (`app:/data/up.bin`) to `server(wait)`:
/// its deadline, and what the mapper answers to each outcome, until the one
/// that is no message.
fn mapped(op: &str, body: &[u8], wait: Duration) -> (Option<u32>, Vec<String>) {
    let root = Root::new();
    let mut m = root.module();
    m.activate().unwrap();
    let mut s = Store::new(super::storage::GRANTS, Vec::<(String, Vec<u8>)>::new());
    std::fs::write(root.0.join("data/up.bin"), body).unwrap();
    let port = server(wait);
    let executor = exact_render::Executor::start(&format!(
        "fs.read app:/data\nnet.fetch http://127.0.0.1:{port}"
    ));
    executor.set_app_roots(["data", "cache", "tmp"].map(|d| root.0.join(d)));

    let a = args(op, "app:/data/up.bin");
    let Answer::Later(mut request) = m.answer(&mut s, "work", &a).unwrap() else {
        panic!("an upload is a request")
    };
    assert!(request.stream && request.upload_progress && !request.bytes);
    assert_eq!(request.body_from.as_deref(), Some("app:/data/up.bin"));
    let deadline = request.timeout_ms;
    request.url = format!("http://127.0.0.1:{port}/upload");
    let out = RequestOut {
        completion: None,
        class: exact_runner::RequestClass::Ordered,
        ticket: 1,
        target: "upload".into(),
        request,
        forced: false,
    };
    executor.run(out, None).unwrap();

    let (mut said, until) = (Vec::new(), Instant::now() + wait + Duration::from_secs(30));
    loop {
        let Some((_, outcome, _)) = executor.drain().pop() else {
            assert!(Instant::now() < until, "no end after {said:?}");
            executor.wait(until);
            continue;
        };
        // Its start, which the runner takes and no mapper sees (`UPLOAD_SENDING`).
        if matches!(&outcome, Outcome::Message(m) if m.upload_started()) {
            continue;
        }
        let ended = !matches!(outcome, Outcome::Message(_));
        let Answer::Now(value) = m.parse(&mut s, "work", &a, outcome).unwrap() else {
            panic!("the mapper answers now")
        };
        said.push(text(value));
        if ended {
            return (deadline, said);
        }
    }
}

#[test]
fn an_upload_maps_its_bytes_sent_as_they_go_then_its_reply() {
    let (deadline, said) = mapped("upload-progress", &vec![7; SIZE], Duration::ZERO);
    assert_eq!(deadline, Some(30_000), "an upload takes a deadline");
    let (end, progress) = said.split_last().unwrap();
    assert_eq!(end, &format!("load 201 {{\"took\":{SIZE}}}"), "{said:?}");
    let sent: Vec<u64> = progress
        .iter()
        .map(|line| {
            let (loaded, rest) = line
                .strip_prefix("progress ")
                .and_then(|l| l.split_once('/'))
                .unwrap_or_else(|| panic!("{line}"));
            assert_eq!(rest, format!("{SIZE} true"), "{line}");
            loaded.parse().unwrap()
        })
        .collect();
    assert!(sent.windows(2).all(|w| w[0] <= w[1]), "{sent:?} went back");
    assert!(
        sent.iter().any(|&n| n < SIZE as u64),
        "nothing before the whole: {sent:?}"
    );
    assert_eq!(
        sent.last(),
        Some(&(SIZE as u64)),
        "the last progress is the whole body"
    );
}

/// An empty body has no bytes to tell (XMLHttpRequest's `upload` fires no
/// computable `progress` for one): its reply alone.
#[test]
fn an_empty_upload_maps_no_progress_then_its_reply() {
    let (_, said) = mapped("upload-progress", &[], Duration::ZERO);
    assert_eq!(said, ["load 201 {\"took\":0}"]);
}

/// A reply 32 s after the body, past the portable transport's own limits
/// (15 s for the response, 30 s for the whole), which the upload's deadline
/// of 60 s replaces.
#[test]
#[ignore = "async lane: a real socket and a reply 32 s after its body; bun scripts/async.mjs runs it"]
fn an_upload_waits_out_a_reply_past_the_portable_transports_own_limits() {
    let (deadline, said) = mapped("upload-patient", &[7; 1 << 20], Duration::from_secs(32));
    assert_eq!(deadline, Some(60_000));
    assert_eq!(
        said.last().map(String::as_str),
        Some("load 201 {\"took\":1048576}"),
        "{said:?}"
    );
}

/// LLP 1016.000 D11: an idle timeout out of range, or on anything but a
/// WebSocket, event or byte stream, is a TypeError before any request.
#[test]
fn an_idle_timeout_fetch_refuses_is_a_type_error_before_any_request() {
    let root = Root::new();
    let mut m = root.module();
    m.activate().unwrap();
    let mut s = Store::new(super::storage::GRANTS, Vec::<(String, Vec<u8>)>::new());
    let refused = call(&mut m, &mut s, "idle-refusals", "");
    let range =
        "TypeError: exactIdleTimeout must be an integer number of milliseconds from 1 to 3600000";
    let socket =
        "TypeError: exactIdleTimeout: only a WebSocket, event or byte stream takes an idle timeout";
    assert_eq!(
        refused.lines().collect::<Vec<_>>(),
        [range, range, range, socket, socket, socket, socket],
        "{refused}"
    );
    assert_eq!(m.in_flight(), 0);
}

/// A stream option the host refuses is marked at bake as storage's refusal
/// is (`code: "bake"`), so a source that lets it through keeps its
/// placeholder; in an answer it is the plain TypeError, and an option of a
/// fetch that is no stream is never marked.
#[test]
fn a_refused_stream_option_is_marked_at_bake_and_plain_in_an_answer() {
    let root = Root::new();
    let mut m = root.module();
    m.activate().unwrap();
    let mut s = Store::new(super::storage::GRANTS, Vec::<(String, Vec<u8>)>::new());
    let plain = "TypeError:undefined:undefined:exactTimeout: a stream has no timeout";
    assert_eq!(call(&mut m, &mut s, "stream-option-refused", ""), plain);
    // The bake's module (`Module::inspect`).
    let mut bake = exact_js::Module::inspect(super::storage::HBC.to_vec()).unwrap();
    bake.set_budget_ms(f64::INFINITY);
    bake.bind(&super::storage::plan());
    let baked = bake
        .query("work", &super::storage::args("stream-option-refused", ""))
        .unwrap();
    assert_eq!(
        super::storage::text(baked),
        "TypeError:Unavailable:bake:exactTimeout: a stream has no timeout"
    );
    let own = bake
        .query("work", &super::storage::args("own-error", ""))
        .unwrap();
    assert!(
        super::storage::text(own).starts_with("TypeError:undefined:undefined:exactTimeout must be"),
    );
    // Let through, it is the bake's deferral, which keeps the resource's
    // placeholder; the source's own TypeError fails the bake as before.
    let deferred = bake.query("work", &super::storage::args("stream-option-uncaught", ""));
    assert!(
        matches!(&deferred, Err(exact_runner::DataError::DeferredAtBake { message: m, warning: true }) if m.contains("a stream has no timeout")),
        "{deferred:?}"
    );
    let fatal = bake.query("work", &super::storage::args("own-uncaught", ""));
    assert!(
        matches!(&fatal, Err(exact_runner::DataError::Unavailable(m)) if m.contains("the source's own")),
        "{fatal:?}"
    );
}

/// What `fetch` refuses itself, before any request: each a TypeError.
#[test]
fn an_upload_fetch_refuses_is_a_type_error_before_any_request() {
    let root = Root::new();
    let mut m = root.module();
    m.activate().unwrap();
    let mut s = Store::new(super::storage::GRANTS, Vec::<(String, Vec<u8>)>::new());
    let refused = call(&mut m, &mut s, "upload-progress-refusals", "");
    assert_eq!(
        refused.lines().collect::<Vec<_>>(),
        [
            "TypeError: exactUploadProgress is true or false",
            "TypeError: exactUploadProgress reports an upload's progress to its stream: give exactStream too",
            "TypeError: exactUploadProgress: a GET or HEAD request sends no body",
            "TypeError: exactUploadProgress: a WebSocket sends no body",
            "TypeError: exactUploadProgress: a long poll sends no body",
            "TypeError: exactUploadProgress: an upload follows its redirects",
            "TypeError: exactTimeout: a stream has no timeout",
            "TypeError: exactUploadProgress: an upload is an http: or https: request",
        ],
        "{refused}"
    );
    assert_eq!(m.in_flight(), 0);
}

/// LLP 1016.000 D12: `exactStreamOpen` refusals are TypeErrors before any
/// request, and marked for bake deferral at bake time.
#[test]
fn stream_open_refusals_and_bake_mark() {
    let root = Root::new();
    let mut m = root.module();
    m.activate().unwrap();
    let mut s = Store::new(super::storage::GRANTS, Vec::<(String, Vec<u8>)>::new());
    let refused = call(&mut m, &mut s, "stream-open-refusals", "");
    let only =
        "TypeError: exactStreamOpen: only an HTTP event or byte stream takes exactStreamOpen";
    assert_eq!(
        refused.lines().collect::<Vec<_>>(),
        [
            "TypeError: exactStreamOpen is true or false",
            "TypeError: exactStreamOpen reports an HTTP stream's response head on open: give exactStream too",
            only,
            only,
            only,
            only,
        ],
        "{refused}"
    );
    assert_eq!(m.in_flight(), 0);

    assert_eq!(
        call(&mut m, &mut s, "stream-open-bake-mark", ""),
        "TypeError:undefined:undefined:exactStreamOpen: only an HTTP event or byte stream takes exactStreamOpen"
    );
    let mut bake = exact_js::Module::inspect(super::storage::HBC.to_vec()).unwrap();
    bake.set_budget_ms(f64::INFINITY);
    bake.bind(&super::storage::plan());
    let marked = bake
        .query("work", &super::storage::args("stream-open-bake-mark", ""))
        .unwrap();
    assert_eq!(
        super::storage::text(marked),
        "TypeError:Unavailable:bake:exactStreamOpen: only an HTTP event or byte stream takes exactStreamOpen"
    );
    let deferred = bake.query("work", &super::storage::args("stream-open-uncaught", ""));
    assert!(
        matches!(&deferred, Err(exact_runner::DataError::DeferredAtBake { message: msg, warning: true }) if msg.contains("exactStreamOpen")),
        "{deferred:?}"
    );
}

/// LLP 1016.000 D12 end-to-end on Hermes + portable executor: `exactStreamOpen: true`
/// delivers status and immutable `Headers` before SSE events or byte chunks without
/// colliding with a server-sent `event: open`, and a non-2xx terminal `Response` error
/// carries immutable `Headers` even when `exactStreamOpen` is omitted.
#[test]
fn stream_open_and_terminal_response_headers_through_hermes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for _ in 0..4 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            if req.starts_with("GET /sse ") {
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nMcp-Session-Id: mcp-99\r\nX-Dup: a\r\nX-Dup: b\r\nSet-Cookie: sid=secret\r\nConnection: close\r\n\r\nevent: open\nid: srv-id-1\ndata: payload\n\ndata: next\n\n"
                );
            } else if req.starts_with("GET /bin ") {
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nMcp-Session-Id: bin-7\r\nX-Dup: x\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(&[1, 2, 3, 4]);
            } else {
                let _ = write!(
                    stream,
                    "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Bearer realm=\"mcp\"\r\nMcp-Session-Id: err-5\r\nSet-Cookie: sid=secret\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnope"
                );
            }
            let _ = stream.flush();
        }
    });

    let run = |op: &str, path: &str, expect_open: bool| -> Vec<String> {
        let root = Root::new();
        let mut m = root.module();
        m.activate().unwrap();
        let mut s = Store::new(super::storage::GRANTS, Vec::<(String, Vec<u8>)>::new());
        let url = format!("http://127.0.0.1:{port}{path}");
        let executor = exact_render::Executor::start(&format!("net.fetch http://127.0.0.1:{port}"));
        let a = args(op, &url);
        let Answer::Later(request) = m.answer(&mut s, "work", &a).unwrap() else {
            panic!("stream is a request");
        };
        assert_eq!(request.stream_open, expect_open);
        executor
            .run(
                RequestOut {
                    completion: None,
                    class: exact_runner::RequestClass::Ordered,
                    ticket: 1,
                    target: op.into(),
                    request,
                    forced: false,
                },
                None,
            )
            .unwrap();
        let (mut said, until) = (Vec::new(), Instant::now() + Duration::from_secs(10));
        loop {
            let drained = executor.drain();
            if drained.is_empty() {
                assert!(Instant::now() < until, "no end after {said:?}");
                executor.wait(until);
                continue;
            }
            for (_, outcome, _) in drained {
                let items = match outcome {
                    Outcome::Messages(ms) => ms.into_iter().map(Outcome::Message).collect(),
                    other => vec![other],
                };
                for item in items {
                    let ended = !matches!(item, Outcome::Message(_));
                    let Answer::Now(value) = m.parse(&mut s, "work", &a, item).unwrap() else {
                        panic!("mapper answers now");
                    };
                    said.push(text(value));
                    if ended {
                        return said;
                    }
                }
            }
        }
    };

    let imm = "set:TypeError,append:TypeError,delete:TypeError";
    let sse = run("stream-open-sse", "/sse", true);
    assert_eq!(
        sse[0],
        format!("head 200 sid=mcp-99 dup=a, b cookie=null imm={imm}"),
        "{sse:?}"
    );
    assert!(
        sse.iter()
            .any(|l| l == "server-open payload id=srv-id-1 hasHeaders=false"),
        "{sse:?}"
    );
    assert!(sse.iter().any(|l| l == "msg next"), "{sse:?}");

    let bin = run("stream-open-bytes", "/bin", true);
    assert_eq!(
        bin[0],
        format!("head 200 sid=bin-7 dup=x cookie=null imm={imm}"),
        "{bin:?}"
    );
    assert!(bin.iter().any(|l| l == "bytes 4"), "{bin:?}");

    let err = run("stream-open-error", "/err", false);
    assert_eq!(
        err,
        [format!(
            "err Response 401 www=Bearer realm=\"mcp\" sid=err-5 cookie=null imm={imm} body=nope"
        )]
    );

    let err_optin = run("stream-open-error-optin", "/err", true);
    assert_eq!(
        err_optin,
        [format!(
            "err Response 401 www=Bearer realm=\"mcp\" sid=err-5 cookie=null imm={imm} body=nope"
        )]
    );

    // Direct Message::http_open with duplicate header pairs combines them via Headers.get()
    // and strips forbidden Set-Cookie response headers.
    let root = Root::new();
    let mut m = root.module();
    m.activate().unwrap();
    let mut s = Store::new(super::storage::GRANTS, Vec::<(String, Vec<u8>)>::new());
    let a = args("stream-open-sse", "https://example.test/sse");
    let Answer::Later(_) = m.answer(&mut s, "work", &a).unwrap() else {
        panic!("stream is a request");
    };
    let dup = exact_runner::Message::http_open(
        200,
        vec![
            ("Mcp-Session-Id".into(), "dup-1".into()),
            ("X-Dup".into(), "first".into()),
            ("x-dup".into(), "second".into()),
            ("Set-Cookie".into(), "sid=secret".into()),
            ("Set-Cookie2".into(), "old=secret".into()),
        ],
    );
    let Answer::Now(v) = m.parse(&mut s, "work", &a, Outcome::Message(dup)).unwrap() else {
        panic!("mapper answers now");
    };
    assert_eq!(
        text(v),
        format!("head 200 sid=dup-1 dup=first, second cookie=null imm={imm}")
    );
}
