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
