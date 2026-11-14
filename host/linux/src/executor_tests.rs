#![cfg(target_os = "linux")]

use exact_painted::executor::Executor;
use exact_runner::{Outcome, Request, RequestClass, RequestOut, Work};
use std::sync::mpsc::{channel, TryRecvError};
use std::time::{Duration, Instant};

#[test]
fn retained_handoff_outcomes_drain_and_release_room_for_the_next_turn() {
    let executor = Executor::start("");
    let (handed, replies) = channel();
    for ticket in 1..=5 {
        let handed = handed.clone();
        executor
            .run(
                RequestOut {
                    class: RequestClass::Ordered,
                    ticket,
                    target: "test".into(),
                    request: Request::continuation(ticket),
                    forced: false,
                },
                Some(Work::Later(Box::new(move |reply| {
                    handed.send((ticket, reply)).unwrap();
                }))),
            )
            .unwrap();
    }
    // Four 120 MiB outcomes leave no room for the next turn's ceiling.
    // These capacities reserve address space without filling the buffers.
    for ticket in 1..=4 {
        let (seen, reply) = replies.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(seen, ticket);
        reply.send(Outcome::Storage(Vec::with_capacity(120 << 20)));
    }
    assert!(
        matches!(replies.try_recv(), Err(TryRecvError::Empty)),
        "the fifth handoff started before a drain released room"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    wait_ready(&executor, deadline);
    executor.begin_pump();
    assert!(
        matches!(replies.try_recv(), Err(TryRecvError::Empty)),
        "the fifth handoff started before a drain released room"
    );
    let mut done = executor.drain();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].0, 1);
    let (ticket, reply) = replies.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(ticket, 5);
    reply.send(Outcome::Storage(Vec::with_capacity(120 << 20)));
    while done.len() < 5 {
        wait_ready(&executor, deadline);
        executor.begin_pump();
        done.extend(executor.drain());
    }
    assert_eq!(
        done.iter().map(|d| d.0).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    for (_, outcome, _) in done {
        assert!(matches!(outcome, Outcome::Storage(ref bytes) if bytes.capacity() == 120 << 20));
    }
    assert!(executor.ordered_idle());
}

#[allow(unsafe_code)]
fn wait_ready(executor: &Executor, deadline: Instant) {
    let mut fd = libc::pollfd {
        fd: executor.fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "executor did not become ready");
        let timeout = remaining.as_millis().max(1).min(i32::MAX as u128) as i32;
        // SAFETY: fd is valid for this call and poll receives one record.
        let ready = unsafe { libc::poll(&mut fd, 1, timeout) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            assert_eq!(error.kind(), std::io::ErrorKind::Interrupted, "{error}");
            continue;
        }
        assert!(ready > 0, "executor did not become ready");
        assert_eq!(fd.revents, libc::POLLIN);
        return;
    }
}
