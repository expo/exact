use super::*;
use ibex2::{
    grant::GrantSet,
    host::Host,
    secrets::MemoryStore,
    stdlib::{
        abort::AbortController,
        fetch::{Headers, Response, StreamingResponse, Transport},
    },
};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Mutex,
    },
};

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/687/tmp")
            .join(format!(
                "transfer-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> File {
        std::fs::write(self.0.join(name), bytes).unwrap();
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.0.join(name))
            .unwrap()
    }
    fn transfer(&self, upload: Option<File>, remaining: Option<u64>) -> Transfer {
        Transfer {
            method: if upload.is_some() { "POST" } else { "GET" }.into(),
            credential_key: "transfer.1.0".into(),
            upload,
            response: self.file("response", &[]),
            remaining,
        }
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[derive(Debug)]
struct Seen {
    url: String,
    method: String,
    headers: Headers,
    body: Vec<u8>,
}
struct Script {
    responses: Mutex<VecDeque<Response>>,
    seen: Arc<Mutex<Vec<Seen>>>,
}
impl Transport for Script {
    fn open(
        &self,
        request: &Request,
        signal: &AbortSignal,
    ) -> Result<StreamingResponse, HostError> {
        let mut body = Vec::new();
        if let Some(file) = &request.body_file {
            file.0.reader().unwrap().read_to_end(&mut body).unwrap();
            if let Some(sent) = &request.sent {
                for count in (1..=body.len()).step_by(1024) {
                    (sent.0)(count as u64, file.0.len());
                }
                (sent.0)(body.len() as u64, file.0.len());
            }
        }
        self.seen.lock().unwrap().push(Seen {
            url: request.url.clone(),
            method: request.method.clone(),
            headers: request.headers.clone(),
            body,
        });
        Ok(self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected hop")
            .into_stream(request.body_limit(), signal.clone()))
    }
}
fn reply(status: u16, location: Option<&str>) -> Response {
    let mut headers = Headers::new();
    headers.set("content-type", "text/plain");
    if let Some(location) = location {
        headers.set("location", location);
    }
    Response {
        status,
        headers,
        status_text: String::new(),
        body: b"answer".to_vec(),
        url: String::new(),
        redirected: false,
    }
}
fn script(
    responses: Vec<Response>,
    grants: &str,
) -> (Carrier, Arc<Mutex<Vec<Seen>>>, Arc<MemoryStore>) {
    let seen = Arc::new(Mutex::new(vec![]));
    let store = Arc::new(MemoryStore::new());
    let fetch = Host::with_transport(Box::new(Script {
        responses: Mutex::new(responses.into()),
        seen: seen.clone(),
    }))
    .endow(GrantSet::parse(grants).unwrap())
    .fetch;
    (Carrier::new(fetch, store.clone()), seen, store)
}
fn quiet() -> Arc<dyn Fn(Progress) + Send + Sync> {
    Arc::new(|_| {})
}
fn keep(carrier: &Carrier, url: &str) {
    carrier
        .keep(
            "transfer.1.0",
            url,
            &[("Authorization".into(), "Bearer private".into())],
        )
        .unwrap();
}

#[test]
fn credentials_are_kept_only_in_the_supplied_store_and_removed_on_retirement() {
    let (carrier, seen, store) = script(vec![reply(201, None)], "net.fetch https://a.test");
    let public = carrier
        .keep(
            "transfer.1.0",
            "https://a.test/upload?token=hidden#fragment",
            &[("Authorization".into(), "Bearer private".into())],
        )
        .unwrap();
    assert_eq!(public, "https://a.test/upload");
    let private = store.get("transfer.1.0").unwrap().unwrap();
    assert!(private.contains("token=hidden") && private.contains("Bearer private"));
    assert!(seen.lock().unwrap().is_empty());
    let files = Files::new();
    let done = carrier
        .run(
            files.transfer(Some(files.file("upload", b"payload")), None),
            &AbortSignal::default(),
            quiet(),
        )
        .unwrap();
    assert_eq!(
        done,
        Completed {
            url: public,
            status: 201,
            content_type: "text/plain".into(),
            downloaded: 6
        }
    );
    assert!(!format!("{done:?}").contains("hidden"));
    assert_eq!(std::fs::read(files.0.join("response")).unwrap(), b"answer");
    assert_eq!(seen.lock().unwrap()[0].body, b"payload");
    carrier.forget("transfer.1.0").unwrap();
    assert!(store.get("transfer.1.0").unwrap().is_none());
    assert_eq!(
        carrier.run(files.transfer(None, None), &AbortSignal::default(), quiet()),
        Err(Failure::Fetch)
    );
}

#[test]
fn admission_and_request_validation_precede_storage_and_network() {
    let (carrier, seen, store) = script(vec![], "net.fetch https://a.test");
    for url in [
        "https://user:password@a.test/",
        "file:///etc/passwd",
        "not a URL",
    ] {
        assert_eq!(
            carrier.keep("transfer.1.0", url, &[]),
            Err(Refusal::InvalidRequest)
        );
    }
    for (name, value) in [
        ("bad name", "v"),
        ("X-A", "v\r\nX-B: leak"),
        ("X-A", "\r\nv"),
        ("X-A", "v\0"),
        ("X-A", "a\u{1}b"),
        ("X-A", "a\u{7f}b"),
    ] {
        assert_eq!(
            carrier.keep(
                "transfer.1.0",
                "https://a.test/",
                &[(name.into(), value.into())]
            ),
            Err(Refusal::InvalidRequest)
        );
    }
    assert_eq!(
        carrier.keep("transfer.1.0", "https://b.test/?secret", &[]),
        Err(Refusal::NotAllowed)
    );
    assert_eq!(
        carrier.keep(
            "transfer.1.0",
            "https://a.test/",
            &[("x".into(), "x".repeat(METADATA_BYTES))]
        ),
        Err(Refusal::InvalidRequest)
    );
    assert!(store.get("transfer.1.0").unwrap().is_none());
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn every_grant_form_uses_fetch_admission() {
    for (grant, allowed, denied) in [
        (
            "net.fetch https://a.test:444",
            "https://a.test:444/a",
            "https://a.test/a",
        ),
        (
            "net.fetch https://*.example.test",
            "https://a.example.test/",
            "https://other.test/",
        ),
        (
            "net.fetch http:",
            "http://any.test:8090/",
            "https://any.test/",
        ),
        (
            "net.fetch local-network",
            "http://192.168.3.47:8910/",
            "http://169.254.169.254/",
        ),
        (
            "net.fetch local-network",
            "http://[fd00::1234]/",
            "http://public.test/",
        ),
        (
            "net.fetch local-network",
            "http://lamp.local/",
            "http://lamp.example/",
        ),
    ] {
        let (carrier, seen, _) = script(vec![], grant);
        assert!(
            carrier.keep("transfer.1.0", allowed, &[]).is_ok(),
            "{grant} {allowed}"
        );
        assert_eq!(
            carrier.keep("transfer.1.0", denied, &[]),
            Err(Refusal::NotAllowed)
        );
        assert!(seen.lock().unwrap().is_empty());
    }
}

#[test]
fn credentials_do_not_return_after_a_cross_origin_redirect() {
    let (carrier, seen, _) = script(
        vec![
            reply(307, Some("https://b.test/middle")),
            reply(308, Some("https://a.test/end")),
            reply(200, None),
        ],
        "net.fetch https://a.test\nnet.fetch https://b.test",
    );
    keep(&carrier, "https://a.test/start?secret=1");
    let files = Files::new();
    carrier
        .run(
            files.transfer(Some(files.file("upload", b"payload")), None),
            &AbortSignal::default(),
            quiet(),
        )
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[2].url, "https://a.test/end");
    assert_eq!(seen[0].headers.get("authorization"), Some("Bearer private"));
    assert_eq!(seen[1].headers.get("authorization"), None);
    assert_eq!(seen[2].headers.get("authorization"), None);
    assert!(seen
        .iter()
        .all(|hop| hop.body == b"payload" && hop.method == "POST"));
}

#[test]
fn a_denied_or_credentialed_redirect_is_a_fetch_failure_before_the_next_hop() {
    for location in [
        "https://b.test/?secret",
        "https://u:p@a.test/",
        "file:///secret",
        "mailto:secret@example.test",
        "ftp://a.test/secret",
        "http://[invalid",
    ] {
        let (carrier, seen, _) =
            script(vec![reply(302, Some(location))], "net.fetch https://a.test");
        keep(&carrier, "https://a.test/start");
        let files = Files::new();
        assert_eq!(
            carrier.run(files.transfer(None, None), &AbortSignal::default(), quiet()),
            Err(Failure::Fetch),
            "redirect to {location}"
        );
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert_eq!(
            std::fs::metadata(files.0.join("response")).unwrap().len(),
            0
        );
    }
}

#[test]
fn redirect_method_and_body_rules_are_fetch_rules() {
    for (status, method, next, body) in [
        (301, "POST", "GET", false),
        (302, "PUT", "PUT", true),
        (303, "HEAD", "HEAD", false),
        (303, "PUT", "GET", false),
        (307, "POST", "POST", true),
        (308, "PUT", "PUT", true),
    ] {
        let (carrier, seen, _) = script(
            vec![reply(status, Some("/end")), reply(200, None)],
            "net.fetch https://a.test",
        );
        carrier
            .keep(
                "transfer.1.0",
                "https://a.test/start",
                &[("content-type".into(), "application/octet-stream".into())],
            )
            .unwrap();
        let files = Files::new();
        let upload = (method != "HEAD").then(|| files.file("upload", b"payload"));
        let mut transfer = files.transfer(upload, None);
        transfer.method = method.into();
        carrier
            .run(transfer, &AbortSignal::default(), quiet())
            .unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen[1].method, next, "{status} {method}");
        assert_eq!(!seen[1].body.is_empty(), body, "{status} {method}");
        if next == "GET" {
            assert!(!seen[1].headers.has("content-type"));
        }
    }
}

#[test]
fn redirects_keep_the_original_gate_and_restarts_recheck_authority() {
    struct Gate(Arc<Mutex<Vec<String>>>);
    impl ibex2::boundary::HopGate for Gate {
        fn admit(&self, url: &url::Url) -> Result<(), HostError> {
            self.0.lock().unwrap().push(url.to_string());
            if url.path() == "/denied" {
                Err(HostError::NotAllowed {
                    reason: "wake cannot prompt".into(),
                })
            } else {
                Ok(())
            }
        }
    }
    let (mut carrier, seen, store) = script(
        vec![reply(302, Some("http://lamp.local/denied"))],
        "net.fetch local-network",
    );
    let hops = Arc::new(Mutex::new(vec![]));
    carrier.fetch = carrier.fetch.with_gate(Arc::new(Gate(hops.clone())));
    keep(&carrier, "http://lamp.local/first");
    let files = Files::new();
    assert_eq!(
        carrier.run(files.transfer(None, None), &AbortSignal::default(), quiet()),
        Err(Failure::Fetch)
    );
    assert_eq!(
        hops.lock().unwrap().last().unwrap(),
        "http://lamp.local/denied"
    );
    assert_eq!(seen.lock().unwrap().len(), 1);
    let (mut restarted, unseen, _) = script(vec![], "net.fetch https://different.test");
    restarted.secrets = store;
    assert_eq!(
        restarted.run(files.transfer(None, None), &AbortSignal::default(), quiet()),
        Err(Failure::Fetch)
    );
    assert!(unseen.lock().unwrap().is_empty());
}

#[test]
fn download_ceiling_counts_this_attempt_and_non_success_bodies_are_kept() {
    for (limit, expected) in [
        (Some(0), Err(Failure::DownloadTotalExceeded)),
        (Some(5), Err(Failure::DownloadTotalExceeded)),
        (Some(6), Ok(6)),
        (None, Ok(6)),
    ] {
        let (carrier, _, _) = script(vec![reply(404, None)], "net.fetch https://a.test");
        keep(&carrier, "https://a.test/file");
        let files = Files::new();
        let done = carrier.run(
            files.transfer(None, limit),
            &AbortSignal::default(),
            quiet(),
        );
        assert_eq!(
            done.as_ref().map(|d| d.downloaded).map_err(|e| *e),
            expected
        );
        if let Ok(done) = done {
            assert_eq!(done.status, 404);
            assert_eq!(std::fs::read(files.0.join("response")).unwrap(), b"answer");
        } else {
            assert!(std::fs::metadata(files.0.join("response")).unwrap().len() <= limit.unwrap());
        }
    }
}

#[test]
fn cancellation_during_copy_keeps_an_incomplete_file_unpublished() {
    let (carrier, _, _) = script(vec![reply(200, None)], "net.fetch https://a.test");
    keep(&carrier, "https://a.test/file");
    let files = Files::new();
    let abort = AbortController::new();
    let signal = abort.signal();
    assert_eq!(
        carrier.run(
            files.transfer(None, None),
            &signal,
            Arc::new(move |_| abort.abort())
        ),
        Err(Failure::Cancelled)
    );
    assert_eq!(std::fs::read(files.0.join("response")).unwrap(), b"answer");
    let abort = AbortController::new();
    abort.abort();
    assert_eq!(
        carrier.run(files.transfer(None, None), &abort.signal(), quiet()),
        Err(Failure::Cancelled)
    );
}

#[test]
fn upload_readers_restart_from_zero_without_changing_each_others_offsets() {
    let files = Files::new();
    let body = Upload {
        file: Arc::new(files.file("upload", b"abcdef")),
        len: 6,
        signal: AbortSignal::default(),
    };
    let mut first = body.reader().unwrap();
    let mut second = body.reader().unwrap();
    let mut pair = [0; 2];
    first.read_exact(&mut pair).unwrap();
    assert_eq!(&pair, b"ab");
    second.read_exact(&mut pair).unwrap();
    assert_eq!(&pair, b"ab");
    first.read_exact(&mut pair).unwrap();
    assert_eq!(&pair, b"cd");
    body.file.set_len(2).unwrap();
    assert_eq!(
        first.read(&mut pair).unwrap_err().kind(),
        std::io::ErrorKind::UnexpectedEof
    );
}

fn portable(grants: &str) -> Carrier {
    let fetch = Host::with_transport(Box::new(ibex2::transport::RustlsHttpTransport::new()))
        .endow(GrantSet::parse(grants).unwrap())
        .fetch;
    Carrier::new(fetch, Arc::new(MemoryStore::new()))
}
fn head(socket: &mut std::net::TcpStream) -> String {
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut bytes = vec![];
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        socket.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
        assert!(bytes.len() <= METADATA_BYTES);
    }
    String::from_utf8(bytes).unwrap()
}

#[test]
fn rust_transport_streams_an_upload_and_response_larger_than_64_mib() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let length = 65 * 1024 * 1024 + 17;
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let headers = head(&mut socket);
        assert!(headers.starts_with("POST /caf%C3%A9?token=%C3%A9 HTTP/1.1\r\n"));
        assert!(headers
            .to_ascii_lowercase()
            .contains(&format!("content-length: {length}")));
        let mut chunk = [0; CHUNK];
        let mut left = length;
        while left > 0 {
            let len = left.min(CHUNK);
            socket.read_exact(&mut chunk[..len]).unwrap();
            assert!(chunk[..len].iter().all(|b| *b == 0));
            left -= len;
        }
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n").unwrap();
        let mut left = length;
        while left > 0 {
            let len = left.min(CHUNK);
            socket.write_all(&chunk[..len]).unwrap();
            left -= len;
        }
    });
    let carrier = portable(&format!("net.fetch {origin}"));
    keep(&carrier, &format!("{origin}/café?token=é#fragment"));
    let files = Files::new();
    let upload = files.file("upload", &[]);
    upload.set_len(length as u64).unwrap();
    let progress = Arc::new(Mutex::new((0, 0)));
    let told = progress.clone();
    let done = carrier
        .run(
            files.transfer(Some(upload), None),
            &AbortSignal::default(),
            Arc::new(move |p| {
                let mut told = told.lock().unwrap();
                match p {
                    Progress::Upload { sent, total } => {
                        assert_eq!(total, length as u64);
                        told.0 = sent;
                    }
                    Progress::Download { received } => {
                        assert!(received >= told.1);
                        told.1 = received;
                    }
                }
            }),
        )
        .unwrap();
    server.join().unwrap();
    assert_eq!(done.downloaded, length as u64);
    assert_eq!(*progress.lock().unwrap(), (length as u64, length as u64));
    assert_eq!(
        std::fs::metadata(files.0.join("response")).unwrap().len(),
        length as u64
    );
}

#[test]
fn abort_interrupts_a_real_socket_wait() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (ready, received) = mpsc::sync_channel(0);
    let (release, released) = mpsc::sync_channel(0);
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        head(&mut socket);
        ready.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    let carrier = portable(&format!("net.fetch {origin}"));
    keep(&carrier, &origin);
    let files = Files::new();
    let transfer = files.transfer(None, None);
    let abort = AbortController::new();
    let signal = abort.signal();
    let (result, done) = mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        result
            .send(carrier.run(transfer, &signal, quiet()))
            .unwrap();
    });
    received.recv_timeout(Duration::from_secs(10)).unwrap();
    abort.abort();
    assert_eq!(
        done.recv_timeout(Duration::from_secs(2)).unwrap(),
        Err(Failure::Cancelled)
    );
    release.send(()).unwrap();
    server.join().unwrap();
    worker.join().unwrap();
}

#[test]
fn a_truncated_response_file_is_written_from_zero() {
    let (carrier, _, _) = script(vec![reply(200, None)], "net.fetch https://a.test");
    keep(&carrier, "https://a.test/");
    let files = Files::new();
    let mut transfer = files.transfer(None, None);
    transfer.response.write_all(&[0; 100]).unwrap();
    transfer.response.set_len(0).unwrap();
    let done = carrier
        .run(transfer, &AbortSignal::default(), quiet())
        .unwrap();
    assert_eq!(done.downloaded, 6);
    assert_eq!(std::fs::read(files.0.join("response")).unwrap(), b"answer");
}

#[test]
fn urls_are_serialized_for_the_transport_and_all_methods_are_uppercase() {
    let (carrier, seen, _) = script(
        vec![reply(200, None)],
        "net.fetch https://xn--bcher-kva.test",
    );
    keep(&carrier, "https://bücher.test/café?token=é#fragment");
    let files = Files::new();
    let mut transfer = files.transfer(None, None);
    transfer.method = "patch".into();
    let done = carrier
        .run(transfer, &AbortSignal::default(), quiet())
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen[0].url,
        "https://xn--bcher-kva.test/caf%C3%A9?token=%C3%A9#fragment"
    );
    assert_eq!(seen[0].method, "PATCH");
    assert_eq!(done.url, "https://xn--bcher-kva.test/caf%C3%A9");
}

#[test]
fn invalid_methods_and_get_head_bodies_are_invalid_requests() {
    let (carrier, seen, _) = script(vec![], "net.fetch https://a.test");
    keep(&carrier, "https://a.test/");
    let files = Files::new();
    for (method, body) in [
        ("bad method", false),
        ("TRACE", false),
        ("get", true),
        ("head", true),
    ] {
        let mut transfer = files.transfer(body.then(|| files.file("upload", b"body")), None);
        transfer.method = method.into();
        let result = carrier.run(transfer, &AbortSignal::default(), quiet());
        assert_eq!(
            format!("{:?}", result.unwrap_err()),
            "InvalidRequest",
            "{method}"
        );
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn progress_is_coalesced_and_final_counts_are_always_reported() {
    let (carrier, _, _) = script(vec![reply(200, None)], "net.fetch https://a.test");
    keep(&carrier, "https://a.test/");
    let files = Files::new();
    let events = Arc::new(Mutex::new(vec![]));
    let told = events.clone();
    carrier
        .run(
            files.transfer(Some(files.file("upload", &[0; CHUNK])), None),
            &AbortSignal::default(),
            Arc::new(move |p| told.lock().unwrap().push((std::time::Instant::now(), p))),
        )
        .unwrap();
    let events = events.lock().unwrap();
    let uploads: Vec<_> = events
        .iter()
        .filter(|(_, p)| matches!(p, Progress::Upload { .. }))
        .collect();
    assert_eq!(
        uploads.last().unwrap().1,
        Progress::Upload {
            sent: CHUNK as u64,
            total: CHUNK as u64
        }
    );
    assert_eq!(events.last().unwrap().1, Progress::Download { received: 6 });
    // The final count is unconditional; every intermediate notice shares one cadence.
    for pair in uploads[..uploads.len() - 1].windows(2) {
        assert!(pair[1].0.duration_since(pair[0].0) >= Duration::from_secs(1));
    }
}

#[test]
fn progress_cadence_uses_one_clock_for_uploads_and_downloads() {
    let events = Arc::new(Mutex::new(vec![]));
    let told = events.clone();
    let reporter = ProgressReporter::new(Arc::new(move |p| told.lock().unwrap().push(p)), Some(10));
    let start = Instant::now();
    reporter.update_at(Progress::Upload { sent: 1, total: 10 }, start);
    reporter.update_at(
        Progress::Upload {
            sent: 10,
            total: 10,
        },
        start + Duration::from_millis(100),
    );
    reporter.update_at(
        Progress::Download { received: 1 },
        start + Duration::from_millis(999),
    );
    reporter.update_at(
        Progress::Download { received: 2 },
        start + Duration::from_secs(1),
    );
    reporter.update_at(
        Progress::Download { received: 3 },
        start + Duration::from_millis(1100),
    );
    reporter.finish();
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            Progress::Upload { sent: 1, total: 10 },
            Progress::Download { received: 2 },
            Progress::Upload {
                sent: 10,
                total: 10
            },
            Progress::Download { received: 3 }
        ]
    );
}

#[test]
fn an_empty_response_and_a_failed_attempt_emit_their_final_counts() {
    for location in [None, Some("https://denied.test")] {
        let mut response = reply(if location.is_some() { 307 } else { 200 }, location);
        response.body.clear();
        let (carrier, _, _) = script(vec![response], "net.fetch https://a.test");
        keep(&carrier, "https://a.test/");
        let files = Files::new();
        let events = Arc::new(Mutex::new(vec![]));
        let told = events.clone();
        let result = carrier.run(
            files.transfer(None, None),
            &AbortSignal::default(),
            Arc::new(move |p| told.lock().unwrap().push(p)),
        );
        assert_eq!(result.is_ok(), location.is_none());
        assert_eq!(
            *events.lock().unwrap(),
            vec![Progress::Download { received: 0 }]
        );
    }
}
