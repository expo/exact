use super::tests::{accept_within, request_head, tls_test_lock};
use super::*;
use crate::stdlib::abort::AbortController;
use std::net::TcpListener;
use std::sync::mpsc;

thread_local! {
    static BEFORE_SOCKET_READ: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

pub(super) fn before_socket_read() -> bool {
    if let Some(run) = BEFORE_SOCKET_READ.with(|hook| hook.take()) {
        run();
        true
    } else {
        false
    }
}

#[test]
fn a_cancelled_socket_returns_a_terminal_error_to_std_read_and_write() {
    use ureq::unversioned::transport::TransportAdapter;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    peer.write_all(b"ready").unwrap();
    let control = AbortController::new();
    let mut io = TransportAdapter::new(CancellableSocket {
        socket: stream.try_clone().unwrap(),
        stream,
        buffers: LazyBuffers::new(1024, 1024),
        context: Arc::new(Mutex::new(Current {
            signal: Some(control.signal()),
            ..Current::default()
        })),
        generation: None,
        registration: None,
    });
    control.abort();
    let read = io.read(&mut [0]).unwrap_err();
    let write = io.write(b"x").unwrap_err();
    assert_eq!(
        [read.kind(), write.kind()],
        [std::io::ErrorKind::ConnectionAborted; 2]
    );
}

#[test]
fn abort_during_tls_handshake_terminates_worker() {
    {
        let _guard = tls_test_lock();
        tls_config();
    }
    let wait = Duration::from_secs(5);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let controller = AbortController::new();
    let signal = controller.signal();
    let (reading, at_read) = mpsc::channel();
    let (resume, resume_read) = mpsc::channel();
    let (done, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        BEFORE_SOCKET_READ.with(|hook| {
            hook.replace(Some(Box::new(move || {
                reading.send(()).unwrap();
                resume_read.recv().unwrap();
            })));
        });
        let mut transport = RustlsHttpTransport::new();
        transport.proxy = None;
        let request = Request::get(&format!("https://{address}/"));
        let opened = transport.open(&request, &signal).map(|_| ());
        done.send(opened).unwrap();
    });

    // The first read is rustls waiting for the server handshake. Keep the
    // peer open, send no reply and abort immediately before prepare().
    at_read.recv_timeout(wait).unwrap();
    let mut peer = accept_within(&listener, wait).expect("TLS client connected");
    peer.set_read_timeout(Some(wait)).unwrap();
    let mut record = [0; 1];
    peer.read_exact(&mut record).unwrap();
    assert_eq!(record[0], 22, "expected a TLS handshake record");
    controller.abort();
    resume.send(()).unwrap();

    let error = result
        .recv_timeout(wait)
        .expect("TLS worker must stop retrying when the signal fires")
        .unwrap_err();
    worker.join().unwrap();
    assert_eq!(
        error,
        HostError::Failed("AbortError: The operation was aborted".into())
    );
}

#[test]
fn aborting_a_partly_written_request_on_a_pooled_connection_does_not_resend_it() {
    use crate::stdlib::fetch::Sent;

    let wait = Duration::from_secs(5);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let (body_byte, received) = mpsc::channel();
    let (done, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut transport = RustlsHttpTransport::new();
        transport.proxy = None;
        let mut request = Request::get(&url);
        request.timeout = Some(wait);
        assert_eq!(transport.send(&request).unwrap().body, b"ok");

        let control = AbortController::new();
        let signal = control.signal();
        let received = Mutex::new(received);
        request.method = "POST".into();
        request.body = Some(vec![7; 1 << 20]);
        request.sent = Some(Sent(
            Arc::new(move |loaded, total| {
                assert!(loaded > 0 && loaded < total);
                assert_eq!(received.lock().unwrap().recv_timeout(wait).unwrap(), 7);
                control.abort();
            }),
            None,
        ));
        done.send(transport.open(&request, &signal).unwrap_err())
            .unwrap();
    });

    let mut peer = accept_within(&listener, wait).expect("initial GET connected");
    assert!(request_head(&mut peer).starts_with("GET / "));
    peer.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
        .unwrap();
    assert!(request_head(&mut peer).starts_with("POST / "));
    let mut byte = [0];
    peer.read_exact(&mut byte).unwrap();
    body_byte.send(byte[0]).unwrap();
    assert_eq!(
        result.recv_timeout(wait).expect("upload returned"),
        HostError::Failed("AbortError: The operation was aborted".into())
    );
    worker.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
