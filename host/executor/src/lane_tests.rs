//! The independent lane's workers share one transport (`lane.rs`), so its
//! reads reuse each other's kept-alive connections: counted at a loopback
//! HTTP/1.1 peer, on the portable transport and, on Apple, URLSession.
use super::tests::{collect, job};
use super::*;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::mpsc::channel;

/// A keep-alive peer that answers `/events` with an event stream's head and
/// a comment, held open, and anything else with `ok`. It counts the
/// connections whose first request was not `/events`.
struct Peer {
    url: String,
    reads: Arc<AtomicUsize>,
}

fn peer() -> Peer {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let reads = Arc::new(AtomicUsize::new(0));
    let counted = reads.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let counted = counted.clone();
            std::thread::spawn(move || {
                let (mut head, mut byte, mut first) = (Vec::new(), [0], true);
                while stream.read_exact(&mut byte).is_ok() {
                    head.push(byte[0]);
                    if !head.ends_with(b"\r\n\r\n") {
                        continue;
                    }
                    if head.starts_with(b"GET /events ") {
                        let _ = stream.write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n: open\n",
                        );
                        let _ = stream.read(&mut byte);
                        return;
                    }
                    if std::mem::take(&mut first) {
                        counted.fetch_add(1, SeqCst);
                    }
                    head.clear();
                    let reply = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
                    if stream.write_all(reply).is_err() {
                        return;
                    }
                }
            });
        }
    });
    Peer { url, reads }
}

fn core_on(peer: &Peer, host: fn() -> ibex2::host::Host) -> (Core, std::sync::mpsc::Receiver<()>) {
    let (wake, woke) = channel();
    let core = Core::start_on(
        None,
        &format!("net.fetch {}", peer.url),
        Box::new(move || {
            let _ = wake.send(());
        }),
        Arc::new(host),
        WorkerBudget::Thread,
    );
    (core, woke)
}

fn read(peer: &Peer, ticket: u64, scope: Option<&str>) -> RequestOut {
    let mut request = Request::get(&format!("{}/read", peer.url)).independent_http(4096);
    request.grants = scope.map(str::to_owned);
    job(ticket, request)
}

/// Six reads one after another, the last with a source's scope, each on
/// whichever worker is free: one connection carries them all.
fn reads_in_turn_share_one_connection(host: fn() -> ibex2::host::Host) {
    let peer = peer();
    let (core, woke) = core_on(&peer, host);
    let scope = format!("net.fetch {}", peer.url);
    for ticket in 1..=6 {
        let scoped = (ticket == 6).then_some(scope.as_str());
        core.run(read(&peer, ticket, scoped), None).unwrap();
        let outcome = collect(&core, &woke, 1).remove(0).1;
        assert!(
            matches!(&outcome, Outcome::Response(r) if r.body == b"ok"),
            "{outcome:?}"
        );
    }
    assert_eq!(
        peer.reads.load(SeqCst),
        1,
        "connections for six reads in turn"
    );
}

/// Six reads at once, then six more: the second six open no connection.
/// With four event streams open first, on transports of their own, the
/// reads wait for none of them.
fn reads_at_once_reuse_the_lanes_connections(host: fn() -> ibex2::host::Host) {
    let peer = peer();
    let (core, woke) = core_on(&peer, host);
    for ticket in 100..104 {
        let exact_runner::Answer::Later(request) =
            exact_runner::Answer::stream(Request::get(&format!("{}/events", peer.url)))
        else {
            unreachable!()
        };
        core.run(job(ticket, request), None).unwrap();
    }
    let mut opened = vec![];
    for wave in 0..2 {
        for ticket in 1..=6 {
            core.run(read(&peer, wave * 10 + ticket, None), None)
                .unwrap();
        }
        let outcomes = collect(&core, &woke, 6);
        assert!(
            (outcomes.iter()).all(|(_, o)| matches!(o, Outcome::Response(r) if r.body == b"ok")),
            "{outcomes:?}"
        );
        opened.push(peer.reads.load(SeqCst));
    }
    assert!(
        opened[0] <= 6 && opened[1] == opened[0],
        "connections after each six: {opened:?}"
    );
}

#[test]
fn portable_reads_in_turn_share_one_connection() {
    reads_in_turn_share_one_connection(crate::tests::abort::portable_host);
}

#[test]
fn portable_reads_at_once_reuse_the_lanes_connections() {
    reads_at_once_reuse_the_lanes_connections(crate::tests::abort::portable_host);
}

#[cfg(target_vendor = "apple")]
#[test]
fn platform_reads_in_turn_share_one_connection() {
    reads_in_turn_share_one_connection(ibex2::host::Host::new);
}

#[cfg(target_vendor = "apple")]
#[test]
fn platform_reads_at_once_reuse_the_lanes_connections() {
    reads_at_once_reuse_the_lanes_connections(ibex2::host::Host::new);
}
