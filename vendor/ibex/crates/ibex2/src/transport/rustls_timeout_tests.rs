use super::*;
use crate::transport::stream_tests::{request_head, server};
use ureq::unversioned::transport::time::Duration as WireDuration;

#[test]
fn a_plain_request_through_an_https_proxy_uses_the_tls_agent() {
    let https_proxy = ureq::Proxy::new("https://proxy.corp:443").unwrap();
    let http_proxy = ureq::Proxy::new("http://proxy.corp:3128").unwrap();
    assert!(needs_tls(false, Some(&https_proxy)));
    assert!(!needs_tls(false, Some(&http_proxy)));
    assert!(!needs_tls(false, None));
    assert!(needs_tls(true, None));
    assert!(needs_tls(true, Some(&http_proxy)));
}

#[derive(Debug)]
struct Probe {
    buffers: LazyBuffers,
    timeout: Option<NextTimeout>,
}
impl WireTransport for Probe {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }
    fn transmit_output(&mut self, _: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.timeout = Some(timeout);
        Ok(())
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.timeout = Some(timeout);
        Ok(true)
    }
    fn is_open(&mut self) -> bool {
        true
    }
}

fn assert_deadlines(context: &Context, exempt: bool) {
    for response in [true, false] {
        let mut observed = Observed {
            inner: Probe {
                buffers: LazyBuffers::new(1024, 1024),
                timeout: None,
            },
            context: context.clone(),
            response,
        };
        // Probe the body limit, the global limit, and reads after both expired.
        for (reason, seconds) in [
            (ureq::Timeout::RecvBody, 15),
            (ureq::Timeout::Global, 30),
            (ureq::Timeout::RecvBody, 0),
            (ureq::Timeout::Global, 0),
        ] {
            let timeout = NextTimeout {
                after: WireDuration::from_secs(seconds),
                reason,
            };
            observed.await_input(timeout).unwrap();
            let mut expected = timeout;
            if exempt && response {
                expected.after = WireDuration::NotHappening;
            }
            assert_eq!(observed.inner.timeout, Some(expected));
            observed.transmit_output(0, timeout).unwrap();
            assert_eq!(observed.inner.timeout, Some(timeout));
        }
    }
}

fn transport_with_context() -> (RustlsHttpTransport, Context) {
    let mut transport = RustlsHttpTransport::new();
    transport.proxy = None;
    let lease = transport.lease(&AbortSignal::default(), false, true);
    let context = lease.slot.as_ref().unwrap().context.clone();
    drop(lease);
    (transport, context)
}

fn check_response(status: u16, types: &[&str], timeout: Option<Duration>, exempt: bool) {
    let mut head = format!("HTTP/1.1 {status} Test\r\nContent-Length: 0\r\n");
    for content_type in types {
        head.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    head.push_str("\r\n");
    let (url, peer) = server(move |mut socket| socket.write_all(head.as_bytes()).unwrap());
    let (transport, context) = transport_with_context();
    let mut request = Request::get(&url);
    request.headers.set("accept", "text/event-stream");
    request.timeout = timeout;
    let response = transport.open(&request, &AbortSignal::default()).unwrap();
    assert_eq!(response.headers.get("content-type"), types.last().copied());
    assert_deadlines(&context, exempt);
    assert!(response.body.collect().unwrap().is_empty());
    peer.join().unwrap();
}

#[test]
fn a_last_plain_content_type_keeps_body_deadlines() {
    check_response(200, &["text/event-stream", "text/plain"], None, false);
}

#[test]
fn a_last_event_stream_content_type_removes_body_deadlines() {
    check_response(200, &["text/plain", "text/event-stream"], None, true);
}

#[test]
fn repeated_locations_and_cookies_keep_the_last_value() {
    let (url, peer) = server(move |mut socket| {
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nLocation: /first\r\nLocation: /last\r\nSet-Cookie: session=abc\r\nSet-Cookie: theme=dark\r\n\r\n").unwrap();
    });
    let (transport, _) = transport_with_context();
    let response = transport.send(&Request::get(&url)).unwrap();
    assert_eq!(response.headers.get("location"), Some("/last"));
    assert_eq!(response.headers.get("set-cookie"), Some("theme=dark"));
    peer.join().unwrap();
}

#[test]
fn only_successful_event_streams_without_an_explicit_timeout_remove_deadlines() {
    check_response(200, &["Text/Event-Stream; charset=utf-8"], None, true);
    check_response(206, &["text/event-stream"], None, true);
    check_response(400, &["text/event-stream"], None, false);
    check_response(
        200,
        &["text/event-stream"],
        Some(Duration::from_secs(5)),
        false,
    );
    check_response(200, &["text/plain"], None, false);
    check_response(200, &[], None, false);
}

#[test]
fn a_pooled_event_stream_restores_deadlines_before_the_next_headers() {
    let (arrived, arrival) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let (url, peer) = server(move |mut socket| {
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 0\r\n\r\n",
            )
            .unwrap();
        request_head(&mut socket);
        arrived.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(3)).unwrap();
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\nok",
            )
            .unwrap();
    });
    let (transport, context) = transport_with_context();
    assert!(transport.send(&Request::get(&url)).unwrap().body.is_empty());
    assert_deadlines(&context, true);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| transport.send(&Request::get(&url)));
        arrival.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_deadlines(&context, false);
        release.send(()).unwrap();
        assert_eq!(worker.join().unwrap().unwrap().body, b"ok");
    });
    assert_deadlines(&context, false);
    let idle = transport.pool.plain_idle.lock().unwrap();
    assert_eq!(idle.len(), 1);
    assert!(Arc::ptr_eq(&context, &idle[0].context));
    peer.join().unwrap();
}
