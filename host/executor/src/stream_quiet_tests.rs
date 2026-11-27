//! A quiet HTTP stream (LLP 1016.000 D11). Its idle timeout ends it, on
//! the scripted transport. Against a real peer on each platform transport,
//! one with none stays open while its peer holds it past the transport's own
//! limits: rustls's 15 s for a body and 30 s for the whole exchange, and
//! URLSession's 60 s without data; those waits are real time, so they run on
//! the async lane.
use super::*;

fn timed_out(outcome: &Outcome) -> bool {
    matches!(outcome, Outcome::Failed { kind: FailureKind::Timeout, message } if message == "nothing arrived for 1000 ms")
}

/// An event stream or a byte stream that carries nothing for its idle
/// timeout ends as a Timeout, after what it had accepted, and any bytes
/// start the count again, an event stream's comment too.
#[test]
fn a_quiet_http_stream_ends_as_a_timeout_and_any_bytes_start_the_count_again() {
    let event: [&[u8]; 2] = [b"data: one\n\n", b": keep\n\n"];
    let bytes: [&[u8]; 2] = [b"\x01", b"\x02"];
    std::thread::scope(|scope| {
        for (path, chunks) in [("/events", event), ("/bytes", bytes)] {
            scope.spawn(move || {
                let (core, script, woke) = setup();
                let mut out = stream(1, path, 4096);
                out.request.bytes = path == "/bytes";
                out.request.idle_timeout_ms = Some(1000);
                let asked = Instant::now();
                core.run_owned(out, None).unwrap();
                let wire = feed(&script, 0);
                let mut seen = vec![];
                for chunk in chunks {
                    std::thread::sleep(Duration::from_millis(400));
                    wire.send(chunk.to_vec()).unwrap();
                }
                loop {
                    let (_, outcome) = next(&core, &woke);
                    let end = !matches!(outcome, Outcome::Message(_));
                    seen.push(outcome);
                    if end {
                        break;
                    }
                }
                let took = asked.elapsed();
                let messages = if path == "/bytes" { 1..=2 } else { 1..=1 };
                let (end, delivered) = seen.split_last().unwrap();
                assert!(
                    messages.contains(&delivered.len()) && timed_out(end),
                    "{path}: {seen:?}"
                );
                assert!(
                    took >= Duration::from_millis(1780),
                    "{path}: the second chunk started the count again: {took:?}"
                );
            });
        }
    });
}

/// A byte stream waiting at its bound for the runner is not quiet: the
/// host reads nothing then, so the wait counts for nothing.
#[test]
fn a_byte_stream_waiting_for_a_drain_is_not_quiet() {
    let (core, script, woke) = setup();
    let mut out = stream(1, "/bytes", 4096);
    out.request.bytes = true;
    out.request.idle_timeout_ms = Some(1000);
    core.run_owned(out, None).unwrap();
    let wire = feed(&script, 0);
    wire.send(vec![1; 4096]).unwrap();
    wire.send(vec![2]).unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let (_, first) = next(&core, &woke);
    assert!(
        matches!(&first, Outcome::Message(m) if m.binary.as_deref() == Some(&[1; 4096][..])),
        "{first:?}"
    );
    let (_, second) = next(&core, &woke);
    assert!(
        matches!(&second, Outcome::Message(m) if m.binary.as_deref() == Some(&[2][..])),
        "{second:?}"
    );
    drop(wire);
    let (_, end) = next(&core, &woke);
    assert!(
        matches!(&end, Outcome::Failed { kind: FailureKind::Network, message } if message == "the byte stream ended"),
        "{end:?}"
    );
}

/// A peer that answers with `content_type`, then sends `chunk` after each
/// of `gaps`, then closes.
fn paced(content_type: &'static str, gaps: Vec<Duration>, chunk: &'static [u8]) -> u16 {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.flush();
        for gap in gaps {
            std::thread::sleep(gap);
            let _ = stream.write_all(chunk);
            let _ = stream.flush();
        }
    });
    port
}

/// The stream's outcomes, to its end, against a `paced` peer: an event
/// stream unless `bytes`.
fn held(host: fn() -> ibex2::host::Host, bytes: bool, gaps: Vec<Duration>) -> Vec<Outcome> {
    let (content_type, chunk): (_, &'static [u8]) = if bytes {
        ("application/octet-stream", b"\x01\x02")
    } else {
        ("text/event-stream", b"data: tick\n\n")
    };
    let within = gaps.iter().sum::<Duration>() + Duration::from_secs(30);
    let port = paced(content_type, gaps, chunk);
    let (wake, woke) = channel();
    let core = Core::start_on(
        None,
        &format!("net.fetch http://127.0.0.1:{port}"),
        Box::new(move || {
            let _ = wake.send(());
        }),
        Arc::new(host),
        crate::WorkerBudget::Thread,
    );
    let mut out = stream(1, "/", 4096);
    out.request.url = format!("http://127.0.0.1:{port}/");
    out.request.bytes = bytes;
    core.run_owned(out, None).unwrap();
    let deadline = Instant::now() + within;
    let mut seen = vec![];
    loop {
        core.begin_pump();
        for (_, outcome, _) in core.drain() {
            let end = !matches!(outcome, Outcome::Message(_));
            seen.push(outcome);
            if end {
                return seen;
            }
        }
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "the stream never ended: {seen:?}");
        let _ = woke.recv_timeout(left);
    }
}

/// Each chunk the peer sent arrived, then its close ended the stream.
fn every_chunk_then_the_close(seen: &[Outcome], chunks: usize, bytes: bool) {
    let ended = if bytes {
        "the byte stream ended"
    } else {
        "the event stream ended"
    };
    let (end, messages) = seen.split_last().unwrap();
    assert!(
        messages.len() == chunks && messages.iter().all(|m| matches!(m, Outcome::Message(_))),
        "{seen:?}"
    );
    assert!(
        matches!(end, Outcome::Failed { kind: FailureKind::Network, message } if message == ended),
        "{seen:?}"
    );
}

#[test]
#[ignore = "async lane: a real socket held quiet for 32 s under rustls"]
fn a_stream_outlives_the_portable_transports_limits() {
    // Two 16 s gaps: each longer than the body's 15 s, together longer
    // than the exchange's 30 s.
    let gaps = || vec![Duration::from_secs(16); 2];
    let portable = crate::tests::abort::portable_host;
    std::thread::scope(|scope| {
        for bytes in [false, true] {
            scope.spawn(move || {
                every_chunk_then_the_close(&held(portable, bytes, gaps()), 2, bytes);
            });
        }
    });
}

#[cfg(target_vendor = "apple")]
#[test]
#[ignore = "async lane: a real socket held quiet for 61 s under URLSession"]
fn a_stream_outlives_urlsessions_quiet_limit() {
    // URLSession shows the head only with the first body bytes, so this
    // quiet is before the head as the executor sees it.
    let gaps = || vec![Duration::from_secs(61)];
    std::thread::scope(|scope| {
        for bytes in [false, true] {
            scope.spawn(move || {
                let seen = held(ibex2::host::Host::new, bytes, gaps());
                every_chunk_then_the_close(&seen, 1, bytes);
            });
        }
    });
}

/// LLP 1016.000 D12: `stream_open` delivers one standalone HTTP `open` head
/// before any SSE event or byte chunk, preserves response status and headers,
/// is omitted when false, and is refused at admission for non-HTTP/non-streams.
#[test]
fn stream_open_delivers_standalone_head_before_events_and_bytes() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for _ in 0..4 {
            let Ok((mut conn, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 4096];
            let n = conn.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            if req.starts_with("GET /no-content ") {
                let _ = write!(
                    conn,
                    "HTTP/1.1 204 No Content\r\nContent-Type: text/event-stream\r\nMcp-Session-Id: s-204\r\nConnection: close\r\n\r\n"
                );
                let _ = conn.flush();
                continue;
            }
            let is_bytes = req.starts_with("GET /bytes ");
            let ctype = if is_bytes {
                "application/octet-stream"
            } else {
                "text/event-stream"
            };
            let _ = write!(
                conn,
                "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nMcp-Session-Id: sess-42\r\nX-Protocol-Version: 2025-03-26\r\nX-Dup: a\r\nX-Dup: b\r\nConnection: close\r\n\r\n"
            );
            let _ = conn.flush();
            if is_bytes {
                let _ = conn.write_all(&[9, 8, 7]);
            } else {
                let _ =
                    conn.write_all(b"event: open\nid: srv-1\ndata: from-server\n\ndata: hello\n\n");
            }
            let _ = conn.flush();
        }
    });

    let (wake, woke) = channel();
    let core = Core::start_on(
        None,
        &format!("net.fetch http://127.0.0.1:{port}"),
        Box::new(move || {
            let _ = wake.send(());
        }),
        Arc::new(crate::tests::abort::portable_host),
        crate::WorkerBudget::Thread,
    );
    let drain_all = |ticket: u64, out: RequestOut| -> Vec<Outcome> {
        core.run_owned(out, None).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = vec![];
        loop {
            core.begin_pump();
            for (t, outcome, _) in core.drain() {
                if t != ticket {
                    continue;
                }
                let end = !matches!(outcome, Outcome::Message(_) | Outcome::Messages(_));
                seen.push(outcome);
                if end {
                    return seen;
                }
            }
            assert!(Instant::now() < deadline, "stream never ended: {seen:?}");
            let _ = woke.recv_timeout(Duration::from_millis(20));
        }
    };

    // 1. SSE with stream_open: true -> standalone open head, then server events, then end.
    let mut sse = stream(1, "/events", 4096);
    sse.request.url = format!("http://127.0.0.1:{port}/events");
    sse.request.stream_open = true;
    let seen = drain_all(1, sse);
    let Outcome::Message(first) = &seen[0] else {
        panic!("expected standalone open message first, got {seen:?}");
    };
    let head = first
        .head
        .as_ref()
        .expect("first message must carry StreamHead");
    assert_eq!(first.event, "open");
    assert_eq!(head.status, 200);
    assert!(head
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("mcp-session-id") && v == "sess-42"));
    assert!(head
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("x-protocol-version") && v == "2025-03-26"));
    assert!(head
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("x-dup") && v == "a, b"));
    // Server-sent `event: open` must carry head: None (no collision).
    let rest_messages: Vec<&Message> = seen[1..]
        .iter()
        .flat_map(|o| match o {
            Outcome::Message(m) => vec![m],
            Outcome::Messages(ms) => ms.iter().collect(),
            _ => vec![],
        })
        .collect();
    assert_eq!(rest_messages.len(), 2, "{rest_messages:?}");
    assert_eq!(rest_messages[0].event, "open");
    assert_eq!(rest_messages[0].id, "srv-1");
    assert_eq!(rest_messages[0].data, "from-server");
    assert!(rest_messages[0].head.is_none());
    assert_eq!(rest_messages[1].data, "hello");
    assert!(rest_messages[1].head.is_none());

    // 2. Byte stream with stream_open: true -> standalone open head before binary chunk.
    let mut bytes = stream(2, "/bytes", 4096);
    bytes.request.url = format!("http://127.0.0.1:{port}/bytes");
    bytes.request.bytes = true;
    bytes.request.stream_open = true;
    let seen = drain_all(2, bytes);
    assert!(matches!(&seen[0], Outcome::Message(m) if m.is_http_open()));
    assert!(
        seen[1..].iter().any(
            |o| matches!(o, Outcome::Message(m) if m.binary.as_deref() == Some(&[9, 8, 7][..]))
        ),
        "{seen:?}"
    );

    // 3. SSE with stream_open: false -> no open head delivered.
    let mut plain = stream(3, "/events", 4096);
    plain.request.url = format!("http://127.0.0.1:{port}/events");
    let seen = drain_all(3, plain);
    assert!(
        !seen
            .iter()
            .any(|o| matches!(o, Outcome::Message(m) if m.is_http_open())),
        "stream_open: false must not deliver open head: {seen:?}"
    );

    // 4. 204 No Content with Content-Type: text/event-stream and stream_open: true ->
    // bodyless response returns a terminal Response (matching browser), never open + error.
    let mut no_content = stream(4, "/no-content", 4096);
    no_content.request.url = format!("http://127.0.0.1:{port}/no-content");
    no_content.request.stream_open = true;
    let seen = drain_all(4, no_content);
    assert_eq!(
        seen.len(),
        1,
        "204 must return one terminal Response: {seen:?}"
    );
    let Outcome::Response(resp) = &seen[0] else {
        panic!("expected terminal Response for 204, got {seen:?}");
    };
    assert_eq!(resp.status, 204);
    assert!(resp
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("mcp-session-id") && v == "s-204"));

    // 5. Admission refuses stream_open on a WebSocket.
    let mut ws = stream(5, "/ws", 4096);
    ws.request.url = "wss://example.test/ws".into();
    ws.request.stream_open = true;
    assert!(core.run_owned(ws, None).is_err());
}

/// LLP 1016.000 D12: delivering the HTTP `open` head must not count as body
/// bytes received for `exactIdleTimeout`. Delay the head by 450 ms on a 1000 ms
/// idle timeout so a buggy reset at head delivery would fire at >= 1450 ms.
#[test]
fn stream_open_head_does_not_reset_idle_timeout() {
    let (core, script, woke) = setup();
    let mut out = stream(1, "/events", 4096);
    out.request.stream_open = true;
    out.request.idle_timeout_ms = Some(1000);
    let asked = Instant::now();
    core.run_owned(out, None).unwrap();
    std::thread::sleep(Duration::from_millis(450));
    let _wire = feed(&script, 0);
    let (_, first) = next(&core, &woke);
    assert!(
        matches!(&first, Outcome::Message(m) if m.is_http_open()),
        "expected open head first: {first:?}"
    );
    let (_, end) = next(&core, &woke);
    assert!(timed_out(&end), "expected idle timeout after open: {end:?}");
    let took = asked.elapsed();
    assert!(
        took >= Duration::from_millis(900) && took < Duration::from_millis(1300),
        "idle timeout elapsed unexpectedly (head must not reset the 1000 ms timer): {took:?}"
    );
}
