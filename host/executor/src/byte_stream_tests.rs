use super::*;

/// A TypeScript stream's request: a body that is not events is bytes.
fn bytes_stream(ticket: u64, path: &str, ceiling: u32) -> RequestOut {
    let mut out = stream(ticket, path, ceiling);
    out.request.bytes = true;
    out
}

/// Waits until the reader has queued `n` undelivered bytes for `ticket`.
fn queued(core: &Core, ticket: u64, n: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let waiting: usize = {
            let state = core.shared.state.lock().unwrap();
            (state.completed.iter().flatten())
                .filter(|done| done.ticket == ticket && done.message)
                .map(|done| match &done.outcome {
                    Outcome::Message(m) => m.binary.as_ref().map_or(0, Vec::len),
                    _ => 0,
                })
                .sum()
        };
        assert!(waiting <= n, "queue holds {waiting} bytes, bound {n}");
        if waiting == n {
            return;
        }
        assert!(Instant::now() < deadline, "{waiting} of {n} bytes queued");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn binary(outcome: &Outcome) -> &[u8] {
    match outcome {
        Outcome::Message(m) => m.binary.as_deref().expect("a binary message"),
        other => panic!("expected a message, got {other:?}"),
    }
}

#[test]
fn a_byte_stream_joins_reads_until_the_runner_takes_them() {
    let (core, script, woke) = setup();
    core.run_owned(bytes_stream(1, "/bytes", 4096), None)
        .unwrap();
    let tx = feed(&script, 0);
    for chunk in [&b"\x00\x01"[..], b"\x02", b"\x03\x04\x05"] {
        tx.send(chunk.to_vec()).unwrap();
    }
    queued(&core, 1, 6);
    let (ticket, first) = next(&core, &woke);
    assert_eq!(
        (ticket, binary(&first)),
        (1, &[0, 1, 2, 3, 4, 5][..]),
        "three reads, one message, in order"
    );
    tx.send(vec![6, 7]).unwrap();
    assert_eq!(binary(&next(&core, &woke).1), [6, 7]);
    drop(tx);
    let (_, end) = next(&core, &woke);
    assert!(
        matches!(&end, Outcome::Failed { kind: FailureKind::Network, message } if message == "the byte stream ended"),
        "{end:?}"
    );
    assert!(settled(&core), "the stream's reservation ends with it");
}

fn read_at_least(script: &Script, n: usize) {
    let read = script.read.0.lock().unwrap();
    let (read, _) = script
        .read
        .1
        .wait_timeout_while(read, Duration::from_secs(5), |read| *read < n)
        .unwrap();
    assert!(*read >= n, "read {} of {n} bytes", *read);
}

#[test]
fn a_byte_stream_waits_at_its_bound_and_delivers_four_mib_in_order() {
    for bound in [1 << 20, 32 << 10] {
        let (core, script, woke) = setup();
        core.run_owned(bytes_stream(1, "/bytes", bound as u32), None)
            .unwrap();
        let tx = feed(&script, 0);
        let expected: Vec<u8> = (0..4 << 20).map(|i| (i % 251) as u8).collect();
        let chunk = bound.min(64 << 10);
        for bytes in expected.chunks(chunk) {
            tx.send(bytes.to_vec()).unwrap();
        }
        drop(tx);
        let mut got = Vec::new();
        while got.len() < expected.len() {
            // Leave the reader at its bound before each drain. It may hold
            // one further transport read, but cannot append it yet.
            let waiting = bound.min(expected.len() - got.len());
            queued(&core, 1, waiting);
            read_at_least(&script, (got.len() + waiting + chunk).min(expected.len()));
            if got.len() + waiting < expected.len() {
                assert!(!core.shared.state.lock().unwrap().running.is_empty());
            }
            let (_, outcome) = next(&core, &woke);
            assert!(binary(&outcome).len() <= bound);
            got.extend_from_slice(binary(&outcome));
        }
        assert_eq!(got, expected);
        assert!(matches!(
            next(&core, &woke).1,
            Outcome::Failed {
                kind: FailureKind::Network,
                ..
            }
        ));
        assert!(settled(&core));
    }
}

#[test]
fn letting_go_aborting_or_retiring_a_byte_stream_ends_a_full_queue_wait() {
    for stop in ["forget", "abort", "retire"] {
        let (mut core, script, _) = setup();
        core.run_owned(bytes_stream(1, "/bytes", 8), None).unwrap();
        let tx = feed(&script, 0);
        tx.send(vec![1; 8]).unwrap();
        queued(&core, 1, 8);
        tx.send(vec![2; 8]).unwrap();
        read_at_least(&script, 16);
        match stop {
            "forget" => core.forget(|_| false),
            "abort" => core.cancel(&[1]),
            _ => core.retire(),
        }
        let ended = script.ended.0.lock().unwrap();
        let (ended, _) = script
            .ended
            .1
            .wait_timeout_while(ended, Duration::from_secs(5), |n| *n == 0)
            .unwrap();
        assert_eq!(*ended, 1, "{stop}: the body was not released");
        if stop != "retire" {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !settled(&core) {
                assert!(
                    Instant::now() < deadline,
                    "{stop}: the reader kept its reservation"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

#[test]
fn a_byte_stream_that_is_not_2xx_and_a_rust_sources_body_are_one_answer() {
    let (core, _, woke) = setup();
    core.run_owned(bytes_stream(1, "/plain", 4096), None)
        .unwrap();
    let (_, outcome) = next(&core, &woke);
    assert!(
        matches!(&outcome, Outcome::Response(r) if r.status == 404),
        "{outcome:?}"
    );
    core.run_owned(stream(2, "/other", 4096), None).unwrap();
    let (_, outcome) = next(&core, &woke);
    assert!(
        matches!(&outcome, Outcome::Response(r) if r.status == 200 && r.body == b"done"),
        "{outcome:?}"
    );
    assert!(settled(&core));
}

#[test]
fn a_byte_stream_with_no_body_is_one_answer_as_on_the_web() {
    let (core, _, woke) = setup();
    core.run_owned(bytes_stream(1, "/empty", 4096), None)
        .unwrap();
    let (_, outcome) = next(&core, &woke);
    assert!(
        matches!(&outcome, Outcome::Response(r) if r.status == 204),
        "{outcome:?}"
    );
    let mut head = bytes_stream(2, "/other", 4096);
    head.request.method = "HEAD".into();
    core.run_owned(head, None).unwrap();
    let (_, outcome) = next(&core, &woke);
    assert!(
        matches!(&outcome, Outcome::Response(r) if r.status == 200),
        "{outcome:?}"
    );
    assert!(settled(&core));
}
