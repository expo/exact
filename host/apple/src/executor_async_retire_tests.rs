//! Retirement keeps physical releases and suppresses delivery (LLP 1162 D8).
use super::super::{
    async_tests::{core, upload, Files, Script},
    tests::{job, one_at_a_time},
    worker_budget, Core, RequestClass,
};
use super::*;
use std::sync::mpsc::{channel, Receiver};

const HANG: Duration = Duration::from_secs(10);
const GRANTS: &str = "net.fetch https://*.example.test";

fn wait(core: &Core, until: impl Fn(&State) -> bool) {
    let state = core.shared.state.lock().unwrap();
    let (state, _) = core
        .shared
        .ready
        .wait_timeout_while(state, HANG, |s| !until(s))
        .unwrap();
    let done = until(&state);
    drop(state);
    assert!(done, "retirement did not release its physical work");
}

fn zero(state: &State) {
    assert_eq!(state.bytes, [0, 0]);
    assert_eq!(state.counts, [0, 0]);
    assert_eq!(
        (state.light, state.agains, state.streams, state.fenced_bytes),
        (0, 0, 0, 0)
    );
    assert!(!state.book.blocking);
    assert_eq!(state.book.in_flight, 0);
    assert!(state.book.origins.iter().all(HashMap::is_empty));
    assert!(state.book.uploads.is_empty());
    assert!(state.book.downloads.is_empty());
    assert!(state.running.is_empty());
}

struct Gate(Arc<(Mutex<bool>, std::sync::Condvar)>);
impl Gate {
    fn new() -> Self {
        Self(Arc::default())
    }
    fn wait(&self) {
        let state = self.0 .0.lock().unwrap();
        let (state, _) = self
            .0
             .1
            .wait_timeout_while(state, HANG, |open| !*open)
            .unwrap();
        assert!(*state, "held operation was not released");
    }
    fn release(&self) {
        *self.0 .0.lock().unwrap() = true;
        self.0 .1.notify_all();
    }
}
impl Clone for Gate {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        self.release();
    }
}

#[test]
fn retirement_discards_undrained_outcomes_reasks_and_fenced_requests() {
    let script = Arc::new(Script::default());
    let (mut core, _) = core(&script, GRANTS);
    let mut read = job(1, Request::get("https://a.example.test/"));
    read.class = RequestClass::Read;
    core.run(read, None).unwrap();
    script.wait_for("https://a.example.test/");
    script.answer("https://a.example.test/", b"undrained");
    wait(&core, |s| s.running.is_empty());
    core.run(
        job(
            5,
            Request::get("https://e.example.test/").independent_http(1024),
        ),
        None,
    )
    .unwrap();
    script.wait_for("https://e.example.test/");
    script.answer("https://e.example.test/", b"undrained independent");
    wait(&core, |s| s.running.is_empty());
    core.run(
        job(
            6,
            Request::get("https://f.example.test/").independent_http(1024),
        ),
        None,
    )
    .unwrap();
    script.wait_for("https://f.example.test/");
    let mut physical = script.take("https://f.example.test/");
    physical.sink.head(Head {
        status: 200,
        headers: ibex2::stdlib::fetch::Headers::new(),
        declared_length: Some(2048),
    });
    wait(&core, |s| s.running.iter().all(|r| r.terminal));
    core.again(2).unwrap();
    assert_eq!(core.shared.state.lock().unwrap().light, 2);
    {
        let mut state = core.shared.state.lock().unwrap();
        state.ordered_barrier = true;
    }
    core.run(job(3, Request::get("https://b.example.test/")), None)
        .unwrap();
    assert!(!core.shared.state.lock().unwrap().fenced.is_empty());
    core.retire();
    let logical = core.shared.state.lock().unwrap().counts;
    physical
        .sink
        .end(Err(ibex2::stdlib::fetch_async::aborted()));
    drop(physical);
    wait(&core, |s| s.running.is_empty());
    zero(&core.shared.state.lock().unwrap());
    assert_eq!(
        logical,
        [0, 0],
        "undrained outcomes release their logical places"
    );
    assert!(core.drain().is_empty());
}

#[test]
fn retirement_releases_a_parked_uploads_waiting_charge() {
    let _turn = one_at_a_time();
    let files = Files::new();
    std::fs::write(files.0.join("tmp/file"), vec![7; 20 << 20]).unwrap();
    let script = Arc::new(Script::default());
    let (mut core, _) = core(&script, &format!("{GRANTS}\nfs.read app:/tmp"));
    core.set_app_roots(files.roots());
    core.run(
        job(
            1,
            Request::get("https://a.example.test/").independent_http(48 << 20),
        ),
        None,
    )
    .unwrap();
    script.wait_for("https://a.example.test/");
    let mut held = script.take("https://a.example.test/");
    core.run(
        job(
            2,
            upload("https://b.example.test/", "app:/tmp/file", 1 << 20),
        ),
        None,
    )
    .unwrap();
    wait(&core, |s| s.book.sized.contains_key(&2));
    let physical = core
        .shared
        .state
        .lock()
        .unwrap()
        .running
        .iter()
        .find(|r| r.ticket == 1)
        .unwrap()
        .charge;
    core.retire();
    assert_eq!(core.shared.state.lock().unwrap().bytes, [0, physical]);
    held.sink.end(Err(ibex2::stdlib::fetch_async::aborted()));
    drop(held);
    wait(&core, |s| s.running.is_empty() && s.bytes == [0, 0]);
    zero(&core.shared.state.lock().unwrap());
}

#[test]
fn retirement_waits_for_sizing_and_reading_threads_to_return() {
    use file::ReaderPhase;
    let _turn = one_at_a_time();
    for phase in [ReaderPhase::Size, ReaderPhase::Decision, ReaderPhase::Read] {
        let files = Files::new();
        std::fs::write(files.0.join("tmp/file"), vec![7; 4096]).unwrap();
        let script = Arc::new(Script::default());
        let (mut core, woke) = core(&script, &format!("{GRANTS}\nfs.read app:/tmp"));
        core.set_app_roots(files.roots());
        let held = Gate::new();
        let gate = held.clone();
        let (entered, started) = channel();
        *core.machine.as_ref().unwrap().reader_hook.lock().unwrap() =
            Some(Arc::new(move |at, _| {
                if at == phase {
                    entered.send(()).unwrap();
                    gate.wait();
                }
                Ok(())
            }));
        core.run(
            job(1, upload("https://a.example.test/", "app:/tmp/file", 1024)),
            None,
        )
        .unwrap();
        started.recv_timeout(HANG).unwrap();
        let charged = core.shared.state.lock().unwrap().bytes;
        core.retire();
        assert_eq!(core.shared.state.lock().unwrap().bytes, charged);
        held.release();
        wait(&core, |s| s.running.is_empty() && s.bytes == [0, 0]);
        zero(&core.shared.state.lock().unwrap());
        assert!(woke.try_recv().is_err());
    }
}

#[test]
fn a_retired_upload_waits_for_both_its_sending_hop_and_its_buffer() {
    let _turn = one_at_a_time();
    for buffer_first in [false, true] {
        let files = Files::new();
        std::fs::write(files.0.join("tmp/file"), vec![7; 4096]).unwrap();
        let script = Arc::new(Script::default());
        let (mut core, woke) = core(&script, &format!("{GRANTS}\nfs.read app:/tmp"));
        core.set_app_roots(files.roots());
        core.run(
            job(1, upload("https://a.example.test/", "app:/tmp/file", 1024)),
            None,
        )
        .unwrap();
        script.wait_for("https://a.example.test/");
        let mut held = script.take("https://a.example.test/");
        wait(&core, |s| s.running.iter().all(|r| r.live == 1));
        let buffer = held._request.body.take();
        core.forget(|_| false);
        core.retire();
        if buffer_first {
            drop(buffer);
            wait(&core, |s| s.book.uploads.values().all(|u| !u.buffer));
            assert_eq!(core.shared.state.lock().unwrap().book.uploads.len(), 1);
            held.sink.end(Err(ibex2::stdlib::fetch_async::aborted()));
        } else {
            held.sink.end(Err(ibex2::stdlib::fetch_async::aborted()));
            wait(&core, |s| s.running.is_empty());
            assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 4096]);
            drop(buffer);
        }
        drop(held);
        wait(&core, |s| s.running.is_empty() && s.bytes == [0, 0]);
        zero(&core.shared.state.lock().unwrap());
        while woke.try_recv().is_ok() {}
        core.waker()();
        assert!(woke.try_recv().is_err());
    }
}

#[test]
fn retirement_ends_queued_deadlines_even_while_disposal_is_blocked() {
    let script = Arc::new(Script::default());
    let (mut core, _) = core(&script, GRANTS);
    for ticket in 1..=7 {
        core.run(
            job(
                ticket,
                Request::get(&format!("https://a.example.test/{ticket}"))
                    .independent_http(1024)
                    .timeout(60_000),
            ),
            None,
        )
        .unwrap();
    }
    assert_eq!(script.wait(6), 6);
    wait(&core, |s| s.jobs[1].len() == 1);
    struct Block(Gate, mpsc::Sender<()>);
    impl Drop for Block {
        fn drop(&mut self) {
            self.1.send(()).unwrap();
            self.0.wait();
        }
    }
    let held = Gate::new();
    let (entered, started) = channel();
    let block = Block(held.clone(), entered);
    core.shared
        .state
        .lock()
        .unwrap()
        .discard
        .push(OwnedWork::Now(Box::new(move || {
            drop(block);
            failed(FailureKind::Aborted, "discarded work")
        })));
    core.shared.ready.notify_all();
    started.recv_timeout(HANG).unwrap();
    core.retire();
    let threads = core.shared.queued_deadlines.load(Ordering::Acquire);
    held.release();
    assert_eq!(threads, 0, "a queued deadline kept its own thread");
    wait(&core, |s| s.running.is_empty() && s.bytes == [0, 0]);
    zero(&core.shared.state.lock().unwrap());
}

#[test]
fn a_retired_stream_keeps_its_place_until_its_transport_returns() {
    let (entered, started) = channel();
    let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let streams = release.clone();
    let script = Arc::new(Script::default());
    let (core, woke) = core(&script, GRANTS);
    let mut core = core.streams_on(move || {
        ibex2::host::Host::with_transport(Box::new(Held {
            entered: entered.clone(),
            release: streams.clone(),
        }))
    });
    let mut request = Request::get("https://a.example.test/").independent_http(1024);
    request.stream = true;
    core.run(job(1, request), None).unwrap();
    started.recv_timeout(HANG).unwrap();
    core.retire();
    assert_eq!(core.shared.state.lock().unwrap().streams, 1);
    assert_ne!(core.shared.state.lock().unwrap().bytes, [0, 0]);
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    wait(&core, |s| s.running.is_empty());
    zero(&core.shared.state.lock().unwrap());
    assert!(woke.try_recv().is_err());
}

#[test]
fn the_scripted_transport_ends_unanswered_exchanges_at_retirement() {
    let script = Arc::new(Script::default());
    let (mut core, _) = core(&script, GRANTS);
    core.run(job(1, Request::get("https://a.example.test/")), None)
        .unwrap();
    script.wait_for("https://a.example.test/");
    core.retire();
    assert_eq!(script.wait(0), 0);
    wait(&core, |s| s.running.is_empty());
    zero(&core.shared.state.lock().unwrap());
}

#[test]
fn late_physical_ends_release_both_lanes_without_waking_the_host() {
    for (reverse, overlap) in [(false, false), (true, false), (false, true), (true, true)] {
        let script = Arc::new(Script::default());
        let (mut core, woke) = core(&script, GRANTS);
        let mut read = job(
            1,
            Request::get("https://a.example.test/").independent_http(1024),
        );
        core.run(read, None).unwrap();
        read = job(2, Request::get("https://b.example.test/"));
        read.class = if overlap {
            RequestClass::Read
        } else {
            RequestClass::Ordered
        };
        core.run(read, None).unwrap();
        core.run(job(3, Request::get("https://c.example.test/")), None)
            .unwrap();
        script.wait_for("https://a.example.test/");
        script.wait_for("https://b.example.test/");
        let mut ends = [
            script.take("https://a.example.test/"),
            script.take("https://b.example.test/"),
        ];
        wait(&core, |s| s.running.iter().all(|r| r.live == 1));
        core.retire();
        while woke.try_recv().is_ok() {}
        if reverse {
            ends.reverse();
        }
        for end in &mut ends {
            end.sink.end(Err(ibex2::stdlib::fetch_async::aborted()));
            end.sink.end(Err(ibex2::stdlib::fetch_async::aborted()));
        }
        drop(ends);
        wait(&core, |s| s.running.is_empty());
        zero(&core.shared.state.lock().unwrap());
        assert!(woke.try_recv().is_err());
    }
}

#[test]
fn a_retired_closure_keeps_its_thread_and_charge_until_it_returns() {
    let script = Arc::new(Script::default());
    let (mut core, woke) = core(&script, GRANTS);
    let (entered, started) = channel();
    let (release, held) = channel();
    core.run(
        job(1, Request::get("https://a.example.test/")),
        Some(Box::new(move || {
            entered.send(()).unwrap();
            held.recv_timeout(HANG).unwrap();
            failed(FailureKind::Aborted, "held closure")
        })),
    )
    .unwrap();
    started.recv_timeout(HANG).unwrap();
    let before = core.shared.state.lock().unwrap().bytes;
    core.retire();
    assert_eq!(core.shared.state.lock().unwrap().bytes, before);
    assert!(core.shared.state.lock().unwrap().book.blocking);
    release.send(()).unwrap();
    wait(&core, |s| s.running.is_empty());
    zero(&core.shared.state.lock().unwrap());
    assert!(woke.try_recv().is_err());
}

struct Held {
    entered: mpsc::Sender<()>,
    release: Arc<(Mutex<bool>, std::sync::Condvar)>,
}
impl ibex2::stdlib::fetch::Transport for Held {
    fn open(
        &self,
        _: &ibex2::stdlib::fetch::Request,
        _: &ibex2::stdlib::abort::AbortSignal,
    ) -> Result<ibex2::stdlib::fetch::StreamingResponse, HostError> {
        self.entered.send(()).unwrap();
        let state = self.release.0.lock().unwrap();
        let (state, _) = self
            .release
            .1
            .wait_timeout_while(state, HANG, |open| !*open)
            .unwrap();
        assert!(*state, "transport was not released");
        Err(ibex2::stdlib::fetch_async::aborted())
    }
}

#[test]
fn retained_handles_keep_no_machine_thread_or_permit_after_retirement() {
    let live_workers = worker_budget();
    let (entered, started) = channel();
    let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let owners = (0..2)
        .map(|_| {
            Some(
                ibex2::host::Host::with_transport(Box::new(Held {
                    entered: entered.clone(),
                    release: release.clone(),
                }))
                .endow(ibex2::grant::GrantSet::parse(GRANTS).unwrap()),
            )
        })
        .collect();
    let mut core = Core::with_owners_lanes(
        owners,
        GRANTS,
        Box::new(|| {}),
        Arc::new(crate::executor::core::tests::lane_host),
        Some(super::asynch_lanes()),
    );
    let waker = core.waker();
    let (reply, kept): (_, Receiver<Reply>) = channel();
    core.run_owned(
        job(1, Request::continuation(1)),
        Some(OwnedWork::Later(Box::new(move |r| {
            reply.send(r).unwrap();
        }))),
    )
    .unwrap();
    let reply = kept.recv_timeout(HANG).unwrap();
    core.run(job(2, Request::get("https://a.example.test/")), None)
        .unwrap();
    core.run(
        job(
            3,
            Request::get("https://b.example.test/")
                .independent_http(1024)
                .timeout(60_000),
        ),
        None,
    )
    .unwrap();
    for _ in 0..2 {
        started.recv_timeout(HANG).unwrap();
    }
    let shared = core.shared.clone();
    core.retire();
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    let bound = Instant::now() + HANG;
    while live_workers.load(Ordering::Acquire) != 0 {
        assert!(
            Instant::now() < bound,
            "retained handles kept worker permits"
        );
        std::thread::yield_now();
    }
    zero(&shared.state.lock().unwrap());
    waker();
    drop((reply, core));
    zero(&shared.state.lock().unwrap());
}
