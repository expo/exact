use super::*;
use crate::stdlib::abort::AbortController;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn server(handler: impl FnOnce(TcpStream) + Send + 'static) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        handler(stream);
    });
    (url, worker)
}

#[test]
fn headers_arrive_before_body_completes_and_abort_interrupts_read() {
    let (release, wait) = mpsc::channel();
    let (url, server) = server(move |mut stream| {
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 5\r\n\r\nx")
            .unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut byte = [0];
        assert_eq!(
            stream.read(&mut byte).unwrap(),
            0,
            "abort did not close task connection"
        );
    });
    let transport = DarwinTransport::new();
    let controller = AbortController::new();
    let mut response = transport
        .open(&Request::get(&url), &controller.signal())
        .unwrap();
    assert_eq!(response.status, 200);
    // CFNetwork waits for an initial byte before its header callback, even
    // with an explicit Content-Type and nosniff. The rest is still streaming.
    assert_eq!(response.body.read(&mut [0; 1]).unwrap(), 1);
    let (done, result) = mpsc::channel();
    let reader = thread::spawn(move || {
        done.send(response.body.read(&mut [0; 5])).unwrap();
    });
    controller.abort();
    assert!(result
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("AbortError"));
    release.send(()).unwrap();
    reader.join().unwrap();
    server.join().unwrap();
}

#[test]
fn abort_interrupts_wait_for_headers() {
    let (accepted, request) = mpsc::channel();
    let (url, server) = server(move |mut stream| {
        accepted.send(()).unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let controller = AbortController::new();
    let signal = controller.signal();
    let (done, result) = mpsc::channel();
    let worker = thread::spawn(move || {
        done.send(DarwinTransport::new().open(&Request::get(&url), &signal))
            .unwrap();
    });
    request.recv_timeout(Duration::from_secs(2)).unwrap();
    controller.abort();
    assert!(result
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("AbortError"));
    worker.join().unwrap();
    server.join().unwrap();
}

#[test]
fn dropping_unread_body_closes_task() {
    let (url, server) = server(move |mut stream| {
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 999999\r\n\r\nx")
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let response = DarwinTransport::new()
        .open(&Request::get(&url), &AbortSignal::default())
        .unwrap();
    drop(response);
    server.join().unwrap();
}

#[test]
fn slow_large_body_does_not_block_another_response() {
    const SIZE: usize = 8 * 1024 * 1024;
    let (sent, sending) = mpsc::channel();
    let (url, server) = server(move |mut stream| {
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {SIZE}\r\n\r\n").unwrap();
        sent.send(()).unwrap();
        for _ in 0..SIZE / 16384 {
            stream.write_all(&[42; 16384]).unwrap();
        }
    });
    let transport = DarwinTransport::new();
    let mut response = transport
        .open(&Request::get(&url), &AbortSignal::default())
        .unwrap();
    sending.recv_timeout(Duration::from_secs(2)).unwrap();
    // Give the producer enough time to fill transport and socket buffers while
    // this response is unread. A second exchange must still make progress.
    thread::sleep(Duration::from_millis(100));
    let (other, second) = server_fn_small();
    assert_eq!(transport.send(&Request::get(&other)).unwrap().body, b"ok");
    let mut total = 0;
    let mut bytes = [0; 4096];
    loop {
        let n = response.body.read(&mut bytes).unwrap();
        if n == 0 {
            break;
        }
        assert!(bytes[..n].iter().all(|&v| v == 42));
        total += n;
        if total % (256 * 1024) == 0 {
            thread::sleep(Duration::from_millis(1));
        }
    }
    assert_eq!(total, SIZE);
    server.join().unwrap();
    second.join().unwrap();
}
fn server_fn_small() -> (String, thread::JoinHandle<()>) {
    server(|mut stream| {
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 2\r\n\r\nok")
            .unwrap();
    })
}

#[test]
fn declared_length_over_limit_is_rejected_before_body_is_exposed() {
    let (url, server) = server(|mut stream| {
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 1000\r\n\r\nx")
            .unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).unwrap(), 0);
    });
    let mut request = Request::get(&url);
    request.max_body = Some(10);
    let error = DarwinTransport::new()
        .open(&request, &AbortSignal::default())
        .unwrap_err();
    assert!(error.to_string().contains("10-byte limit"));
    server.join().unwrap();
}

/// A request read for as long as its caller holds it: one that leases a
/// session of its own (Exact patch 49).
fn open_ended(url: &str) -> Request {
    let mut request = Request::get(url);
    request.open_ended = true;
    request
}

/// A request its caller reads whole as it arrives: one that may share the
/// transport's session (Exact patch 49).
fn collected(url: &str) -> Request {
    let mut request = Request::get(url);
    request.collected = true;
    request
}

#[test]
fn fifth_session_wait_is_cancellable_and_drop_returns_a_lease() {
    let transport = Arc::new(DarwinTransport::new());
    let mut responses = Vec::new();
    let mut servers = Vec::new();
    for _ in 0..4 {
        let (url, worker) = server(|mut stream| {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 5\r\n\r\nx")
                .unwrap();
            assert_eq!(stream.read(&mut [0]).unwrap(), 0);
        });
        responses.push(
            transport
                .open(&open_ended(&url), &AbortSignal::default())
                .unwrap(),
        );
        servers.push(worker);
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let controller = AbortController::new();
    let signal = controller.signal();
    let clone = transport.clone();
    let (send, receive) = mpsc::channel();
    let worker = thread::spawn(move || {
        send.send(clone.open(&open_ended(&url), &signal)).unwrap();
    });
    thread::sleep(Duration::from_millis(50));
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    controller.abort();
    assert!(receive
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("AbortError"));
    worker.join().unwrap();
    drop(responses.pop());
    let (url, small) = server_fn_small();
    assert_eq!(transport.send(&open_ended(&url)).unwrap().body, b"ok");
    drop(responses);
    for server in servers {
        server.join().unwrap();
    }
    small.join().unwrap();
}

#[test]
fn completed_body_returns_connection_and_unregisters_old_abort() {
    let (url, server) = server(|mut stream| {
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 2\r\n\r\nok")
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 2\r\n\r\nok")
            .unwrap();
    });
    let transport = DarwinTransport::new();
    let controller = AbortController::new();
    let response = transport
        .open(&Request::get(&url), &controller.signal())
        .unwrap();
    assert_eq!(response.collect().unwrap().body, b"ok");
    controller.abort();
    assert_eq!(transport.send(&Request::get(&url)).unwrap().body, b"ok");
    assert_eq!(transport.last_connection_was_reused(), Some(true));
    server.join().unwrap();
}

#[test]
fn completed_unread_bodies_do_not_exhaust_session_pool() {
    let kinds: [fn(&str) -> Request; 3] = [|url| Request::get(url), open_ended, collected];
    for kind in kinds {
        let transport = DarwinTransport::new();
        let old = AbortController::new();
        let mut responses = Vec::new();
        let mut servers = Vec::new();
        for _ in 0..8 {
            let (url, worker) = server_fn_small();
            responses.push(transport.open(&kind(&url), &old.signal()).unwrap());
            servers.push(worker);
        }
        let (url, worker) = server_fn_small();
        let final_response = transport
            .open(&kind(&url), &AbortSignal::default())
            .unwrap();
        // Cancelling completed old exchanges cannot cancel the session's next task.
        old.abort();
        assert_eq!(final_response.collect().unwrap().body, b"ok");
        for response in responses {
            assert!(response.collect().is_err());
        }
        for server in servers {
            server.join().unwrap();
        }
        worker.join().unwrap();
    }
}

/// Collected exchanges share one session (Exact patch 49): with four
/// open-ended ones holding every session of their own, eight at once each
/// have their heads, which the peer sends only once all eight requests are
/// in (more than URLSession's six connections a host would let through),
/// and their 4 MiB replies, more than the session holds ahead of a reader,
/// arrive whole; eight more after them open no connection.
#[test]
fn collected_exchanges_share_one_session_and_none_waits() {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::sync::{Condvar, Mutex};
    const AT_ONCE: usize = 8;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let accepted = Arc::new(AtomicUsize::new(0));
    let arrived = Arc::new((Mutex::new(0), Condvar::new()));
    let (counted, gate) = (accepted.clone(), arrived.clone());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            counted.fetch_add(1, SeqCst);
            let gate = gate.clone();
            thread::spawn(move || {
                let mut head = Vec::new();
                let mut byte = [0];
                while stream.read_exact(&mut byte).is_ok() {
                    head.push(byte[0]);
                    if !head.ends_with(b"\r\n\r\n") {
                        continue;
                    }
                    head.clear();
                    let (count, all) = &*gate;
                    let mut count = count.lock().unwrap();
                    *count += 1;
                    all.notify_all();
                    let (count, _) = all
                        .wait_timeout_while(count, Duration::from_secs(10), |n| *n < AT_ONCE)
                        .unwrap();
                    drop(count);
                    let reply = [
                        &b"HTTP/1.1 200 OK\r\nContent-Length: 4194304\r\n\r\n"[..],
                        &[7; 4 << 20],
                    ]
                    .concat();
                    if stream.write_all(&reply).is_err() {
                        return;
                    }
                }
            });
        }
    });
    let transport = Arc::new(DarwinTransport::new());
    let mut streams = Vec::new();
    for _ in 0..4 {
        let (url, server) = server(|mut stream| {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n: open\n")
                .unwrap();
            let _ = stream.read(&mut [0]);
        });
        let response = transport
            .open(&open_ended(&url), &AbortSignal::default())
            .unwrap();
        streams.push((response, server));
    }
    let opened: Vec<_> = (0..AT_ONCE)
        .map(|_| {
            let (transport, url) = (transport.clone(), url.clone());
            thread::spawn(move || {
                let started = std::time::Instant::now();
                let response = transport.open(&collected(&url), &AbortSignal::default());
                (started.elapsed(), response)
            })
        })
        .collect();
    for response in opened {
        let (took, response) = response.join().unwrap();
        assert!(
            took < Duration::from_secs(8),
            "a head waited {took:?} for the others"
        );
        assert_eq!(response.unwrap().collect().unwrap().body.len(), 4 << 20);
    }
    let before = accepted.load(SeqCst);
    assert_eq!(before, AT_ONCE, "connections for eight exchanges at once");
    for _ in 0..AT_ONCE {
        let body = transport.send(&collected(&url)).unwrap().body;
        assert_eq!(body.len(), 4 << 20);
    }
    assert_eq!(
        accepted.load(SeqCst),
        before,
        "a later exchange opened a connection"
    );
    for (response, server) in streams {
        drop(response);
        server.join().unwrap();
    }
}

#[test]
fn head_representation_length_does_not_count_against_body_limit() {
    let (url, server) = server(|mut stream| {
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n")
            .unwrap();
    });
    let mut request = Request::get(&url);
    request.method = "HEAD".into();
    request.max_body = Some(0);
    let response = DarwinTransport::new().send(&request).unwrap();
    assert_eq!(response.status, 200);
    assert!(response.body.is_empty());
    assert_eq!(response.headers.get("content-length"), Some("100"));
    server.join().unwrap();
}

#[test]
fn not_modified_representation_length_does_not_count_against_body_limit() {
    let (url, server) = server(|mut stream| {
        stream
            .write_all(
                b"HTTP/1.1 304 Not Modified\r\nContent-Length: 100\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
    });
    let mut request = Request::get(&url);
    request.max_body = Some(0);
    let response = DarwinTransport::new().send(&request).unwrap();
    assert_eq!(response.status, 304);
    assert!(response.body.is_empty());
    assert_eq!(response.headers.get("content-length"), Some("100"));
    server.join().unwrap();
}

/// An upload's progress (Exact patch 23): URLSession's `didSendBodyData`
/// reaches the request's hook while the body goes out, before the headers,
/// rising to the whole.
#[test]
fn an_uploads_bytes_sent_are_told_before_its_headers() {
    use crate::stdlib::fetch::Sent;
    const SIZE: usize = 16 << 20;
    let (url, server) = server(move |mut stream| {
        // Slowly, so the body is still going out while the hook is told.
        let mut chunk = vec![0; 256 << 10];
        let mut took = 0;
        while took < SIZE {
            let n = stream
                .read(&mut chunk[..(SIZE - took).min(256 << 10)])
                .unwrap();
            assert!(n > 0, "the body ended at {took}");
            took += n;
            thread::sleep(Duration::from_millis(4));
        }
        stream
            .write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .unwrap();
    });
    let told = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = told.clone();
    let mut request = Request::get(&url);
    request.method = "POST".into();
    request.body = Some(vec![7; SIZE]);
    request.sent = Some(Sent(
        std::sync::Arc::new(move |loaded, total| seen.lock().unwrap().push((loaded, total))),
        None,
    ));
    let response = DarwinTransport::new().send(&request).unwrap();
    assert_eq!(response.status, 201);
    server.join().unwrap();
    let told = told.lock().unwrap();
    let total = SIZE as u64;
    assert!(
        told.iter().any(|&(n, _)| n < total),
        "nothing before the whole: {told:?}"
    );
    assert!(told.windows(2).all(|w| w[0].0 <= w[1].0), "{told:?}");
    assert!(told.iter().all(|&(_, t)| t == total), "{told:?}");
    assert_eq!(told.last(), Some(&(total, total)));
}

/// A file body held in memory, read from its first byte for each attempt.
struct MemoryFile(Vec<u8>);
impl crate::stdlib::fetch::FileBody for MemoryFile {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }
    fn reader(&self) -> std::io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(std::io::Cursor::new(self.0.clone())))
    }
}

type Told = std::sync::Arc<std::sync::Mutex<Vec<(u64, u64)>>>;

fn file_upload(url: &str, bytes: Vec<u8>) -> (Request, Told) {
    use crate::stdlib::fetch::{BodyFile, Sent};
    let told = Told::default();
    let seen = told.clone();
    let mut request = Request::get(url);
    request.method = "POST".into();
    request.body_file = Some(BodyFile(std::sync::Arc::new(MemoryFile(bytes))));
    request.sent = Some(Sent(
        std::sync::Arc::new(move |loaded, total| seen.lock().unwrap().push((loaded, total))),
        None,
    ));
    (request, told)
}

/// A file body (Exact patch 46) is a streamed upload task framed by its
/// length, its bytes arriving whole, its progress rising to the whole.
#[test]
fn a_file_body_is_streamed_by_its_length() {
    const SIZE: usize = 6 << 20;
    let bytes: Vec<u8> = (0..SIZE as u32)
        .map(|i| (i * 13 + i / 4093) as u8)
        .collect();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut head = Vec::new();
        let mut byte = [0];
        while !head.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
        let mut body = vec![0; SIZE];
        stream.read_exact(&mut body).unwrap();
        stream
            .write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .unwrap();
        (String::from_utf8(head).unwrap().to_ascii_lowercase(), body)
    });
    let (request, told) = file_upload(&url, bytes.clone());
    let response = DarwinTransport::new().send(&request).unwrap();
    assert_eq!(response.status, 201);
    let (head, body) = peer.join().unwrap();
    assert!(
        head.contains(&format!("content-length: {SIZE}\r\n"))
            && !head.contains("transfer-encoding"),
        "{head}"
    );
    assert!(body == bytes, "the body differs from the file");
    let told = told.lock().unwrap();
    assert!(told.windows(2).all(|w| w[0].0 <= w[1].0), "{told:?}");
    assert_eq!(told.last(), Some(&(SIZE as u64, SIZE as u64)));
}

/// A 413 before the body has gone is the reply, and progress stops at the
/// bytes written (LLP 1133.008.000 D4).
#[test]
fn an_early_reply_is_the_reply_and_progress_stops_at_the_bytes_written() {
    const SIZE: u64 = 64 << 20;
    let (url, server) = server(move |mut stream| {
        let mut some = vec![0; 64 << 10];
        stream.read_exact(&mut some).unwrap();
        let _ = stream.write_all(
            b"HTTP/1.1 413 Content Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        thread::sleep(Duration::from_millis(200));
    });
    let (request, told) = file_upload(&url, vec![1; SIZE as usize]);
    let started = std::time::Instant::now();
    let response = DarwinTransport::new().send(&request).unwrap();
    server.join().unwrap();
    assert_eq!(response.status, 413);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    let last = told.lock().unwrap().last().map_or(0, |&(n, _)| n);
    assert!(last < SIZE, "told {last} of {SIZE}");
}

/// A file body that fails part way (a file cut shorter while it is sent)
/// fails the exchange with the reader's words.
#[test]
fn a_file_body_that_fails_fails_the_exchange_with_its_words() {
    struct Short;
    impl crate::stdlib::fetch::FileBody for Short {
        fn len(&self) -> u64 {
            1 << 20
        }
        fn reader(&self) -> std::io::Result<Box<dyn Read + Send>> {
            let cut = std::io::Cursor::new(vec![1; 4096]).chain(std::io::repeat(0).take(0));
            struct Fails<R>(R);
            impl<R: Read> Read for Fails<R> {
                fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                    match self.0.read(out)? {
                        0 => Err(std::io::Error::other(
                            "exactBodyFrom app:/tmp/x: the file shrank while it was sent",
                        )),
                        n => Ok(n),
                    }
                }
            }
            Ok(Box::new(Fails(cut)))
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut sink = [0; 4096];
        while stream.read(&mut sink).is_ok_and(|n| n > 0) {}
    });
    let mut request = Request::get(&format!("http://{address}/"));
    request.method = "POST".into();
    request.body_file = Some(crate::stdlib::fetch::BodyFile(std::sync::Arc::new(Short)));
    let error = DarwinTransport::new().send(&request).unwrap_err();
    // The read may fail before URLSession connects: release the accept.
    let _ = TcpStream::connect(address);
    peer.join().unwrap();
    assert!(
        format!("{error:?}").contains("the file shrank while it was sent"),
        "{error:?}"
    );
}
