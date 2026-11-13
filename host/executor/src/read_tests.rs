//! Read overlap and ordered delivery over held loopback HTTP (LLP 1162 D1).
use super::*;
use exact_runner::RequestClass;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

const HANG: Duration = Duration::from_secs(10);

#[derive(Default)]
struct ServerState {
    seen: Vec<(u64, String)>,
    held: HashMap<u64, TcpStream>,
    closed: HashSet<u64>,
    stopping: bool,
}

struct Server {
    address: std::net::SocketAddr,
    state: Arc<(Mutex<ServerState>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new((Mutex::new(ServerState::default()), Condvar::new()));
        let server = state.clone();
        let thread = std::thread::spawn(move || {
            let mut clients = Vec::new();
            for stream in listener.incoming() {
                let stream = stream.unwrap();
                if server.0.lock().unwrap().stopping {
                    break;
                }
                let server = server.clone();
                clients.push(std::thread::spawn(move || serve(stream, server)));
            }
            for client in clients {
                client.join().unwrap();
            }
        });
        Self {
            address,
            state,
            thread: Some(thread),
        }
    }

    fn request(&self, ticket: u64) -> Request {
        Request::get(&format!("http://{}/{ticket}", self.address))
    }

    fn post(&self, ticket: u64) -> Request {
        Request::post_json(&format!("http://{}/{ticket}", self.address), "{}")
    }

    fn core(&self, asynchronous: bool) -> (Core, Receiver<()>) {
        self.core_with_grants(asynchronous, &format!("net.fetch http://{}", self.address))
    }

    fn core_with_grants(&self, asynchronous: bool, grants: &str) -> (Core, Receiver<()>) {
        let owners = (0..WORKERS)
            .map(|_| {
                Some(
                    crate::tests::lane_host().endow(ibex2::grant::GrantSet::parse(grants).unwrap()),
                )
            })
            .collect();
        let (wake, woke) = channel();
        let core = Core::with_owners_lanes(
            owners,
            grants,
            Box::new(move || {
                let _ = wake.send(());
            }),
            Arc::new(crate::tests::lane_host),
            asynchronous.then(asynch_lanes),
        );
        (core, woke)
    }

    fn wait(&self, until: impl Fn(&ServerState) -> bool) {
        let state = self.state.0.lock().unwrap();
        let (state, _) = self
            .state
            .1
            .wait_timeout_while(state, HANG, |s| !until(s))
            .unwrap();
        // Released first: a failure here must not poison the lock `Drop` takes.
        let (met, seen) = (until(&state), format!("{:?}", state.seen));
        drop(state);
        assert!(met, "server saw {seen}");
    }

    fn seen(&self, tickets: &[u64]) {
        self.wait(|s| tickets.iter().all(|t| s.seen.iter().any(|(id, _)| id == t)));
    }

    fn reply(&self, ticket: u64, body: &[u8]) {
        let mut socket = self.state.0.lock().unwrap().held.remove(&ticket).unwrap();
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        socket.write_all(body).unwrap();
        socket.shutdown(Shutdown::Write).unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        {
            let mut state = self.state.0.lock().unwrap();
            state.stopping = true;
            for socket in state.held.values() {
                let _ = socket.shutdown(Shutdown::Both);
            }
        }
        let _ = TcpStream::connect(self.address);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn serve(mut socket: TcpStream, server: Arc<(Mutex<ServerState>, Condvar)>) {
    socket.set_read_timeout(Some(HANG)).unwrap();
    let mut head = Vec::new();
    let mut byte = [0];
    while !head.ends_with(b"\r\n\r\n") {
        if socket.read(&mut byte).unwrap_or(0) == 0 {
            return;
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).unwrap();
    let mut first = head.lines().next().unwrap().split_whitespace();
    let method = first.next().unwrap().to_owned();
    let ticket = first
        .next()
        .unwrap()
        .trim_start_matches('/')
        .parse()
        .unwrap();
    let length = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .map_or(0, |(_, value)| value.trim().parse::<usize>().unwrap());
    socket.read_exact(&mut vec![0; length]).unwrap();
    {
        let mut state = server.0.lock().unwrap();
        if state.stopping {
            return;
        }
        state.seen.push((ticket, method));
        state.held.insert(ticket, socket.try_clone().unwrap());
        server.1.notify_all();
    }
    let mut bytes = [0; 1024];
    while let Ok(n) = socket.read(&mut bytes) {
        if n == 0 {
            break;
        }
    }
    let mut state = server.0.lock().unwrap();
    state.held.remove(&ticket);
    state.closed.insert(ticket);
    server.1.notify_all();
}

fn job(ticket: u64, request: Request, class: RequestClass) -> RequestOut {
    RequestOut {
        ticket,
        target: "test".into(),
        request,
        forced: false,
        class,
    }
}

fn read_job(server: &Server, ticket: u64) -> RequestOut {
    job(ticket, server.request(ticket), RequestClass::Read)
}

fn collect(core: &Core, woke: &Receiver<()>, count: usize) -> Vec<(u64, Outcome)> {
    let end = Instant::now() + HANG;
    let mut done = Vec::new();
    while done.len() < count {
        core.begin_pump();
        let pump = core.drain();
        assert!(pump.len() <= 1, "one completion per pump");
        done.extend(
            pump.into_iter()
                .map(|(ticket, outcome, _)| (ticket, outcome)),
        );
        if done.len() < count {
            woke.recv_timeout(end.saturating_duration_since(Instant::now()))
                .expect("completion wake");
        }
    }
    done
}

fn tickets(done: &[(u64, Outcome)]) -> Vec<u64> {
    done.iter().map(|(ticket, _)| *ticket).collect()
}

fn wait_state(core: &Core, until: impl Fn(&State) -> bool) {
    let end = Instant::now() + HANG;
    let mut state = core.shared.state.lock().unwrap();
    while !until(&state) {
        assert!(Instant::now() < end, "executor condition did not hold");
        // An out-of-order completion has no presenter wake (LLP 1162 D1).
        state = core
            .shared
            .ready
            .wait_timeout(state, Duration::from_millis(50))
            .unwrap()
            .0;
    }
}

fn completed(core: &Core, ticket: u64) {
    wait_state(core, |s| s.completed[0].iter().any(|d| d.ticket == ticket));
}

fn assert_queued(core: &Core, ticket: u64) {
    assert!(core.shared.state.lock().unwrap().jobs[0]
        .iter()
        .any(|j| j.ticket == ticket));
}

#[test]
fn six_reads_at_the_unsaid_ceiling_overlap_and_the_seventh_waits() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        for ticket in 1..=7 {
            core.run(read_job(&server, ticket), None).unwrap();
        }
        server.seen(&[1, 2, 3, 4, 5, 6]);
        {
            let mut state = core.shared.state.lock().unwrap();
            assert_eq!(server.state.0.lock().unwrap().held.len(), 6);
            assert_eq!(state.running.len(), 6);
            assert_eq!(state.jobs[0].front().unwrap().ticket, 7);
            assert_eq!(state.ordered, (1..=7).collect::<VecDeque<_>>());
            assert_eq!(state.counts, [7, 0]);
            assert!(state.bytes[0] < 7 * MAX_BODY);
            assert_eq!(state.bytes[1], 0);
            for run in &state.running {
                assert!(run.charge >= MAX_BODY && run.charge < MAX_BODY + MAX_REQUEST);
            }
            assert!(reads::next_job(&core.shared, &mut state, 0).is_none());
            assert!(reads::next_job(&core.shared, &mut state, 1).is_none());
        }
        server.reply(1, b"one");
        server.seen(&[7]);
        assert_eq!(server.state.0.lock().unwrap().held.len(), 6);
        for ticket in 2..=7 {
            server.reply(ticket, b"done");
        }
        let done = collect(&core, &woke, 7);
        assert_eq!(tickets(&done), (1..=7).collect::<Vec<_>>());
        assert!(done.iter().all(|(_, o)| matches!(o, Outcome::Response(_))));
        assert!(core.ordered_idle());
        wait_state(&core, |s| s.bytes == [0, 0]);
        assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
    }
}

#[test]
fn replies_released_in_reverse_order_drain_in_admission_order() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        for ticket in [30, 10, 20] {
            let mut r = read_job(&server, ticket);
            if ticket == 10 {
                r.request.method = "HEAD".into();
            }
            core.run(r, None).unwrap();
        }
        server.seen(&[30, 10, 20]);
        for ticket in [20, 10] {
            server.reply(ticket, b"");
            completed(&core, ticket);
            assert!(core.drain().is_empty());
            assert!(!core.ordered_idle());
        }
        server.reply(30, b"first");
        assert_eq!(tickets(&collect(&core, &woke, 3)), [30, 10, 20]);
        assert!(server
            .state
            .0
            .lock()
            .unwrap()
            .seen
            .contains(&(10, "HEAD".into())));
    }
}

#[test]
fn a_read_held_behind_an_earlier_reply_reports_its_delivery_wait() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, _) = server.core(asynchronous);
        for ticket in [1, 2] {
            core.run(read_job(&server, ticket), None).unwrap();
        }
        server.seen(&[1, 2]);
        server.reply(2, b"second");
        completed(&core, 2);
        let (finished_at, elapsed_ms) = {
            let state = core.shared.state.lock().unwrap();
            let done = state.completed[0].iter().find(|d| d.ticket == 2).unwrap();
            (done.finished_at, done.elapsed_ms)
        };
        let hold = Duration::from_millis(375);
        let now = finished_at + hold;
        assert!(core.drain_with_clock(|| now).is_empty());
        server.reply(1, b"first");
        completed(&core, 1);
        let now = now.max(Instant::now());
        assert_eq!(core.drain_with_clock(|| now)[0].0, 1);
        let (ticket, outcome, timing) = core.drain_with_clock(|| now).pop().unwrap();
        assert_eq!(ticket, 2);
        assert!(matches!(outcome, Outcome::Response(_)));
        let timing = timing.unwrap();
        assert_eq!(timing.elapsed_ms, elapsed_ms);
        assert!(timing.delivery_wait_ms >= hold.as_millis() as u64);
    }
}

#[test]
fn an_unheld_reply_reports_zero_delivery_wait() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, _) = server.core(asynchronous);
        core.run(read_job(&server, 1), None).unwrap();
        server.seen(&[1]);
        server.reply(1, b"first");
        completed(&core, 1);
        let now = core.shared.state.lock().unwrap().completed[0][0].finished_at;
        let (ticket, outcome, timing) = core.drain_with_clock(|| now).pop().unwrap();
        assert_eq!(ticket, 1);
        assert!(matches!(outcome, Outcome::Response(_)));
        assert_eq!(timing.unwrap().delivery_wait_ms, 0);
    }
}

#[test]
fn reads_keep_posts_storage_work_and_handoffs_in_their_places() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        for ticket in 1..=2 {
            core.run(read_job(&server, ticket), None).unwrap();
        }
        core.run(
            job(3, Request::storage(vec![]), RequestClass::Ordered),
            None,
        )
        .unwrap();
        core.run(job(4, server.post(4), RequestClass::Ordered), None)
            .unwrap();
        core.run(read_job(&server, 5), None).unwrap();
        let (ran, running) = channel();
        core.run(
            job(6, Request::continuation(6), RequestClass::Ordered),
            Some(Box::new(move || {
                ran.send(()).unwrap();
                Outcome::Storage(vec![6])
            })),
        )
        .unwrap();
        let (handed, replies) = channel();
        core.run_owned(
            job(7, Request::continuation(7), RequestClass::Ordered),
            Some(OwnedWork::Later(Box::new(move |reply| {
                handed.send(reply).unwrap();
            }))),
        )
        .unwrap();
        core.run(read_job(&server, 8), None).unwrap();
        server.seen(&[1, 2]);
        server.reply(2, b"two");
        completed(&core, 2);
        assert_queued(&core, 3);
        assert_queued(&core, 4);
        server.reply(1, b"one");
        server.seen(&[4]);
        completed(&core, 3);
        assert_queued(&core, 5);
        assert!(running.try_recv().is_err());
        server.reply(4, b"post");
        server.seen(&[5]);
        assert_queued(&core, 6);
        server.reply(5, b"five");
        running.recv_timeout(HANG).unwrap();
        let reply = replies.recv_timeout(HANG).unwrap();
        server.seen(&[8]);
        reply.send(Outcome::Storage(vec![7]));
        server.reply(8, b"eight");
        let done = collect(&core, &woke, 8);
        assert_eq!(tickets(&done), (1..=8).collect::<Vec<_>>());
        assert!(matches!(
            done[2].1,
            Outcome::Failed {
                kind: FailureKind::Unsupported,
                ..
            }
        ));
        assert_eq!(done[5].1, Outcome::Storage(vec![6]));
        assert_eq!(done[6].1, Outcome::Storage(vec![7]));
    }
}

#[test]
fn the_ordered_worker_runs_a_read_and_post_while_all_shared_workers_are_held() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        for ticket in 1..=6 {
            core.run(
                job(
                    ticket,
                    server.request(ticket).independent_http(4096),
                    RequestClass::Ordered,
                ),
                None,
            )
            .unwrap();
        }
        server.seen(&[1, 2, 3, 4, 5, 6]);
        core.run(read_job(&server, 7), None).unwrap();
        core.run(job(8, server.post(8), RequestClass::Ordered), None)
            .unwrap();
        server.seen(&[7]);
        server.reply(7, b"read");
        server.seen(&[8]);
        server.reply(8, b"post");
        assert_eq!(tickets(&collect(&core, &woke, 2)), [7, 8]);
        assert_eq!(server.state.0.lock().unwrap().held.len(), 6);
        assert_eq!(core.shared.state.lock().unwrap().counts, [0, 6]);
        core.forget(|_| false);
        wait_state(&core, |s| s.counts == [0, 0]);
        wait_state(&core, |s| s.bytes == [0, 0]);
        assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
    }
}

#[test]
fn short_ordered_bytes_wait_without_stalling_an_earlier_read_or_its_delivery() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        let (_, _, ceiling) = reads::reservation(&server.request(1), true).unwrap();
        let (_, _, ordered) = reservation(&server.request(1)).unwrap();
        assert_eq!(ordered - ceiling, MAX_BODY);
        let held = BYTES[0] - (2 * ceiling + (1 << 20));
        core.shared.state.lock().unwrap().bytes[0] = held;
        for ticket in 1..=3 {
            core.run(read_job(&server, ticket), None).unwrap();
        }
        server.seen(&[1, 2]);
        assert_queued(&core, 3);
        let size = (2 << 20) + 3;
        server.reply(2, &vec![42; size]);
        completed(&core, 2);
        assert_queued(&core, 3);
        assert!(core.drain().is_empty());
        server.reply(1, b"slow");
        server.seen(&[3]);
        server.reply(3, b"last");
        let done = collect(&core, &woke, 3);
        assert_eq!(tickets(&done), [1, 2, 3]);
        let Outcome::Response(reply) = &done[1].1 else {
            panic!("{:?}", done[1]);
        };
        assert_eq!(reply.body.len(), size);
        assert!(reply.body.capacity() <= MAX_BODY);
        if !asynchronous {
            // The workers collect into an exact-sized buffer; the machine grows
            // a buffer within its ceiling (LLP 1120 D5).
            assert_eq!(reply.body.capacity(), size);
        }
        assert!(reply.body.iter().all(|b| *b == 42));
        wait_state(&core, |s| s.bytes == [held, 0]);
        let mut state = core.shared.state.lock().unwrap();
        assert_eq!(state.bytes, [held, 0]);
        state.bytes[0] -= held;
    }
}

fn pass_deadline() {
    let abort = AbortController::new();
    let (passed, wait) = channel();
    let _registration = abort.signal().register(move || {
        let _ = passed.send(());
    });
    let _deadline = Deadline::arm(600, &abort).unwrap();
    wait.recv_timeout(HANG).unwrap();
}

#[test]
fn a_reads_deadline_covers_neither_its_queue_wait_nor_its_completed_wait() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        core.run(job(1, server.post(1), RequestClass::Ordered), None)
            .unwrap();
        server.seen(&[1]);
        let mut r = read_job(&server, 2);
        r.request = r.request.timeout(500);
        core.run(r, None).unwrap();
        pass_deadline();
        assert_queued(&core, 2);
        assert!(core.drain().is_empty());
        server.reply(1, b"post");
        server.seen(&[2]);
        server.reply(2, b"read");
        assert!(collect(&core, &woke, 2)
            .iter()
            .all(|(_, o)| matches!(o, Outcome::Response(_))));

        core.run(read_job(&server, 3), None).unwrap();
        let mut r = read_job(&server, 4);
        r.request = r.request.timeout(500);
        core.run(r, None).unwrap();
        server.seen(&[3, 4]);
        server.reply(4, b"in");
        completed(&core, 4);
        pass_deadline();
        assert!(core.drain().is_empty());
        server.reply(3, b"slow");
        let done = collect(&core, &woke, 2);
        assert_eq!(tickets(&done), [3, 4]);
        assert!(done.iter().all(|(_, o)| matches!(o, Outcome::Response(_))));
    }
}

#[test]
fn a_read_that_times_out_delivers_failure_in_its_ordered_place() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        core.run(read_job(&server, 1), None).unwrap();
        let mut r = read_job(&server, 2);
        r.request = r.request.timeout(500);
        core.run(r, None).unwrap();
        // Its deadline may pass before the transport has sent it.
        server.seen(&[1]);
        completed(&core, 2);
        assert!(core.drain().is_empty());
        server.reply(1, b"first");
        let done = collect(&core, &woke, 2);
        assert_eq!(tickets(&done), [1, 2]);
        assert!(matches!(
            done[1].1,
            Outcome::Failed {
                kind: FailureKind::Timeout,
                ..
            }
        ));
        assert!(core.ordered_idle());
    }
}

#[test]
fn forgetting_skips_a_queued_read_and_aborts_a_running_read_freeing_its_place() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        core.run(job(1, server.post(1), RequestClass::Ordered), None)
            .unwrap();
        server.seen(&[1]);
        core.run(read_job(&server, 2), None).unwrap();
        core.run(read_job(&server, 3), None).unwrap();
        core.forget(|ticket| ticket != 2);
        server.reply(1, b"post");
        server.seen(&[3]);
        core.run(job(4, server.post(4), RequestClass::Ordered), None)
            .unwrap();
        core.forget(|ticket| ticket != 3);
        server.wait(|s| s.closed.contains(&3));
        server.seen(&[4]);
        server.reply(4, b"last");
        assert_eq!(tickets(&collect(&core, &woke, 2)), [1, 4]);
        assert!(!server
            .state
            .0
            .lock()
            .unwrap()
            .seen
            .iter()
            .any(|(t, _)| *t == 2));
        assert!(core.ordered_idle());
        wait_state(&core, |s| s.bytes == [0, 0]);
        assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);

        core.run(read_job(&server, 5), None).unwrap();
        server.seen(&[5]);
        core.run(job(6, server.post(6), RequestClass::Ordered), None)
            .unwrap();
        core.cancel(&[5]);
        server.seen(&[6]);
        server.reply(6, b"after abort");
        assert_eq!(tickets(&collect(&core, &woke, 1)), [6]);
        wait_state(&core, |s| s.bytes == [0, 0]);
        assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
    }
}

#[test]
fn retiring_reads_releases_every_worker_and_its_reservations() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, _) = server.core(asynchronous);
        for ticket in 1..=7 {
            core.run(read_job(&server, ticket), None).unwrap();
        }
        server.seen(&[1, 2, 3, 4, 5, 6]);
        let owner = Arc::downgrade(&core.shared);
        drop(core);
        server.wait(|s| (1..=6).all(|t| s.closed.contains(&t)));
        let end = Instant::now() + HANG;
        while owner.upgrade().is_some() {
            assert!(
                Instant::now() < end,
                "a worker still holds its owner and charges"
            );
            let state = server.state.0.lock().unwrap();
            drop(
                server
                    .state
                    .1
                    .wait_timeout(state, Duration::from_millis(50))
                    .unwrap(),
            );
        }
        assert_eq!(server.state.0.lock().unwrap().seen.len(), 6);
    }
}

#[test]
fn post_reads_overlap_but_a_parallel_send_waits_for_them() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        core.run(job(1, server.post(1), RequestClass::Read), None)
            .unwrap();
        core.run(read_job(&server, 2), None).unwrap();
        core.run(job(3, server.request(3), RequestClass::Parallel(3)), None)
            .unwrap();
        core.run(read_job(&server, 4), None).unwrap();
        server.seen(&[1, 2]);
        assert_queued(&core, 3);
        {
            let mut state = core.shared.state.lock().unwrap();
            assert!(reads::next_job(&core.shared, &mut state, 0).is_none());
            assert!(reads::next_job(&core.shared, &mut state, 1).is_none());
        }
        server.reply(1, b"post");
        completed(&core, 1);
        assert_queued(&core, 3);
        server.reply(2, b"read");
        server.seen(&[3]);
        assert_queued(&core, 4);
        server.reply(3, b"parallel");
        server.seen(&[4]);
        server.reply(4, b"last");
        assert_eq!(tickets(&collect(&core, &woke, 4)), [1, 2, 3, 4]);
    }
}

#[test]
fn a_waiting_post_read_is_admitted_on_an_out_of_order_completion_wake() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        for ticket in 1..=16 {
            core.run(read_job(&server, ticket), None).unwrap();
        }
        core.run(job(17, server.post(17), RequestClass::Read), None)
            .unwrap();
        core.run(read_job(&server, 18), None).unwrap();
        server.seen(&[1, 2, 3, 4, 5, 6]);
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.counts[0], 16);
            assert_eq!(
                state.fenced.iter().map(Fenced::ticket).collect::<Vec<_>>(),
                [17, 18]
            );
        }
        core.begin_pump();
        while woke.try_recv().is_ok() {}
        server.reply(2, b"second");
        woke.recv_timeout(HANG)
            .expect("a freed effect slot wakes the host");
        core.begin_pump();
        assert!(core.drain().is_empty(), "ticket 1 still holds delivery");
        {
            let state = core.shared.state.lock().unwrap();
            assert!(state.ordered.contains(&17));
            assert_eq!(state.fenced.front().unwrap().ticket(), 18);
        }
        for ticket in 3..=16 {
            server.seen(&[ticket]);
            server.reply(ticket, b"read");
        }
        server.seen(&[17]);
        assert!(server.state.0.lock().unwrap().held.contains_key(&1));
        server.reply(17, b"post");
        server.reply(1, b"first");
        let mut done = Vec::new();
        for count in [17, 18] {
            if count == 18 {
                server.seen(&[18]);
                server.reply(18, b"last");
            }
            while done.len() < count {
                woke.recv_timeout(HANG).expect("a host pump needs a wake");
                core.begin_pump();
                done.extend(
                    core.drain()
                        .into_iter()
                        .map(|(ticket, outcome, _)| (ticket, outcome)),
                );
            }
        }
        assert_eq!(tickets(&done), (1..=18).collect::<Vec<_>>());
        assert!(done
            .iter()
            .all(|(_, outcome)| matches!(outcome, Outcome::Response(_))));
        assert!(core.ordered_idle());
    }
}

#[test]
fn a_refusal_from_waiting_admission_wakes_even_when_no_reply_can_drain() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        for ticket in 1..=128 {
            core.run(read_job(&server, ticket), None).unwrap();
        }
        core.run(job(129, server.post(129), RequestClass::Read), None)
            .unwrap();
        for ticket in 2..=114 {
            server.seen(&[ticket]);
            server.reply(ticket, b"read");
        }
        wait_state(&core, |s| s.light == 113);
        // Isolate the admission turn from transport wakes (LLP 1162 D1).
        core.notify();
        woke.recv_timeout(HANG).expect("the host pumps");
        core.begin_pump();
        assert!(core.drain().is_empty(), "ticket 1 still holds delivery");
        woke.recv_timeout(HANG).expect("the deferred refusal wakes");
        assert_eq!(
            core.resume_ordered(),
            [(129, "native executor admission limit reached")]
        );
        assert!(core.resume_ordered().is_empty());
        core.forget(|_| false);
        wait_state(&core, |s| s.counts[0] == 0);
        assert!(core.ordered_idle());
    }
}

#[test]
fn read_admission_forgetting_and_the_refusal_fence_stay_on_the_ordered_lane() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        for ticket in 1..=128 {
            core.run(read_job(&server, ticket), None).unwrap();
        }
        server.seen(&[1, 2, 3, 4, 5, 6]);
        assert_eq!(
            core.run(read_job(&server, 129), None),
            Err("native executor admission limit reached")
        );
        core.run(read_job(&server, 130), None).unwrap();
        assert_eq!(
            core.shared
                .state
                .lock()
                .unwrap()
                .fenced
                .front()
                .unwrap()
                .ticket(),
            130
        );
        core.forget(|ticket| ticket == 130);
        wait_state(&core, |s| s.counts[0] == 0);
        assert!(core.ordered_idle());
        assert_eq!(core.resume_ordered(), []);
        server.seen(&[130]);
        server.reply(130, b"after refusal");
        assert_eq!(tickets(&collect(&core, &woke, 1)), [130]);
        wait_state(&core, |s| s.bytes == [0, 0]);
        assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
    }
}

#[test]
fn reads_and_parallel_sends_wait_for_a_handoff_to_be_made_but_not_completed() {
    for asynchronous in [false, true] {
        for class in [RequestClass::Read, RequestClass::Parallel(0)] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous);
            let (entered, entering) = channel();
            let (release, held) = channel();
            core.run(
                job(1, Request::continuation(1), RequestClass::Ordered),
                Some(Box::new(move || {
                    entered.send(()).unwrap();
                    held.recv_timeout(HANG).unwrap();
                    Outcome::Storage(vec![1])
                })),
            )
            .unwrap();
            entering.recv_timeout(HANG).unwrap();
            let (handed, replies) = channel();
            core.run_owned(
                job(2, Request::continuation(2), RequestClass::Ordered),
                Some(OwnedWork::Later(Box::new(move |reply| {
                    handed.send(reply).unwrap();
                }))),
            )
            .unwrap();
            core.run(job(3, server.post(3), class), None).unwrap();
            {
                let mut state = core.shared.state.lock().unwrap();
                assert!(state.jobs[0].iter().any(|j| j.ticket == 2));
                assert!(state.jobs[0].iter().any(|j| j.ticket == 3));
                assert!(reads::next_job(&core.shared, &mut state, 1).is_none());
            }
            release.send(()).unwrap();
            let reply = replies.recv_timeout(HANG).unwrap();
            server.seen(&[3]);
            // Admission after the handoff must obey the same rule.
            core.run(job(4, server.post(4), class), None).unwrap();
            server.seen(&[4]);
            server.reply(4, b"later");
            completed(&core, 4);
            server.reply(3, b"earlier");
            completed(&core, 3);
            assert_eq!(tickets(&collect(&core, &woke, 1)), [1]);
            assert!(core.drain().is_empty());
            reply.send(Outcome::Storage(vec![2]));
            assert_eq!(tickets(&collect(&core, &woke, 3)), [2, 3, 4]);
        }
    }
}

#[test]
fn twenty_four_post_reads_wait_at_sixteen_and_drain_in_order_without_a_refusal() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        for ticket in 1..=24 {
            let mut request = job(ticket, server.post(ticket), RequestClass::Read);
            request.target = format!("resource-{}", (ticket - 1) / 8);
            core.run(request, None).unwrap();
            let state = core.shared.state.lock().unwrap();
            assert!(state.counts[0] - state.light <= COUNTS[0]);
        }
        server.seen(&[1, 2, 3, 4, 5, 6]);
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.counts, [16, 0]);
            assert_eq!(state.light, 0);
            assert_eq!(state.fenced.len(), 8);
            assert!(!state.ordered_barrier);
        }
        core.run(job(25, server.post(25), RequestClass::Ordered), None)
            .unwrap();
        core.run(read_job(&server, 26), None).unwrap();
        let mut done = Vec::new();
        for first in [1, 7, 13, 19] {
            let wave: Vec<_> = (first..first + 6).collect();
            server.seen(&wave);
            for &ticket in wave.iter().rev() {
                server.reply(ticket, b"query");
                completed(&core, ticket);
            }
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.light, 0, "a POST read holds its effect until drained");
            assert!(state.counts[0] <= COUNTS[0]);
            drop(state);
            for _ in 0..6 {
                done.extend(collect(&core, &woke, 1));
                let state = core.shared.state.lock().unwrap();
                assert!(state.counts[0] - state.light <= COUNTS[0]);
                assert!(!state.ordered_barrier);
            }
        }
        assert_eq!(tickets(&done), (1..=24).collect::<Vec<_>>());
        assert!(done.iter().all(|(_, o)| matches!(o, Outcome::Response(_))));
        server.seen(&[25]);
        assert_queued(&core, 26);
        server.reply(25, b"send");
        server.seen(&[26]);
        server.reply(26, b"read");
        assert_eq!(tickets(&collect(&core, &woke, 2)), [25, 26]);
        assert!(core.ordered_idle());
        wait_state(&core, |s| s.bytes == [0, 0]);
        assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
    }
}

#[test]
fn forgotten_running_queued_and_fenced_post_reads_are_sent_and_drop_their_outcomes() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        for ticket in 1..=17 {
            core.run(job(ticket, server.post(ticket), RequestClass::Read), None)
                .unwrap();
        }
        server.seen(&[1, 2, 3, 4, 5, 6]);
        assert_queued(&core, 7);
        core.forget(|ticket| ![1, 7, 17].contains(&ticket));
        {
            let state = core.shared.state.lock().unwrap();
            let running = state.running.iter().find(|r| r.ticket == 1).unwrap();
            assert!(running.forgotten && running.abort.is_none());
            assert!(
                state.jobs[0]
                    .iter()
                    .find(|j| j.ticket == 7)
                    .unwrap()
                    .forgotten
            );
            assert!(
                matches!(state.fenced.front(), Some(Fenced::Held(r, _, _, true)) if r.ticket == 17)
            );
        }
        server.reply(1, b"forgotten running");
        let mut done = Vec::new();
        for ticket in 2..=17 {
            server.seen(&[ticket]);
            server.reply(ticket, b"query");
            if ticket == 7 || ticket == 17 {
                wait_state(&core, |s| !s.running.iter().any(|r| r.ticket == ticket));
            } else {
                done.extend(collect(&core, &woke, 1));
            }
        }
        assert_eq!(
            tickets(&done),
            (2..=16).filter(|t| *t != 7).collect::<Vec<_>>()
        );
        assert!(core.drain().is_empty());
        assert!(core.ordered_idle());
        wait_state(&core, |s| s.bytes == [0, 0]);
        assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
    }
}

#[test]
fn three_parallel_sends_leave_shared_workers_for_three_independent_reads() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        for ticket in 1..=4 {
            core.run(
                job(ticket, server.post(ticket), RequestClass::Parallel(0)),
                None,
            )
            .unwrap();
        }
        server.seen(&[1, 2, 3]);
        assert_queued(&core, 4);
        for ticket in 5..=7 {
            core.run(
                job(
                    ticket,
                    server.request(ticket).independent_http(4096),
                    RequestClass::Read,
                ),
                None,
            )
            .unwrap();
        }
        server.seen(&[5, 6, 7]);
        {
            let mut state = core.shared.state.lock().unwrap();
            assert_eq!(state.running.len(), 6);
            assert!(reads::next_job(&core.shared, &mut state, 0).is_none());
            assert!(reads::next_job(&core.shared, &mut state, 1).is_none());
        }
        server.reply(3, b"third");
        server.seen(&[4]);
        server.reply(4, b"fourth");
        completed(&core, 4);
        assert!(core.drain().is_empty());
        for ticket in [2, 1, 5, 6, 7] {
            server.reply(ticket, b"done");
        }
        let done = collect(&core, &woke, 7);
        let ordered: Vec<_> = tickets(&done).into_iter().filter(|t| *t <= 4).collect();
        assert_eq!(ordered, [1, 2, 3, 4]);
        assert!(done.iter().all(|(_, o)| matches!(o, Outcome::Response(_))));
    }
}

#[test]
fn non_siblings_wait_for_every_parallel_send_to_end() {
    for asynchronous in [false, true] {
        for class in [
            RequestClass::Ordered,
            RequestClass::Read,
            RequestClass::Parallel(1),
        ] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous);
            for ticket in 1..=2 {
                core.run(
                    job(ticket, server.post(ticket), RequestClass::Parallel(0)),
                    None,
                )
                .unwrap();
            }
            core.run(job(3, server.post(3), class), None).unwrap();
            server.seen(&[1, 2]);
            assert_queued(&core, 3);
            server.reply(2, b"second");
            completed(&core, 2);
            assert_queued(&core, 3);
            server.reply(1, b"first");
            server.seen(&[3]);
            server.reply(3, b"last");
            assert_eq!(tickets(&collect(&core, &woke, 3)), [1, 2, 3]);
        }
    }
}

#[test]
fn the_ordered_worker_runs_a_parallel_send_and_post_under_held_independent_http() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous);
        for ticket in 1..=6 {
            core.run(
                job(
                    ticket,
                    server.request(ticket).independent_http(4096),
                    RequestClass::Ordered,
                ),
                None,
            )
            .unwrap();
        }
        server.seen(&[1, 2, 3, 4, 5, 6]);
        core.run(job(7, server.post(7), RequestClass::Parallel(0)), None)
            .unwrap();
        core.run(job(8, server.post(8), RequestClass::Ordered), None)
            .unwrap();
        server.seen(&[7]);
        server.reply(7, b"parallel");
        server.seen(&[8]);
        server.reply(8, b"ordered");
        assert_eq!(tickets(&collect(&core, &woke, 2)), [7, 8]);
        assert_eq!(server.state.0.lock().unwrap().held.len(), 6);
        core.forget(|_| false);
        wait_state(&core, |s| s.counts == [0, 0]);
    }
}

#[test]
fn a_seventeenth_parallel_send_is_refused_and_forgotten_sends_still_run() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, _) = server.core(asynchronous);
        for ticket in 1..=16 {
            core.run(
                job(ticket, server.post(ticket), RequestClass::Parallel(0)),
                None,
            )
            .unwrap();
        }
        server.seen(&[1, 2, 3]);
        assert_eq!(
            core.run(job(17, server.post(17), RequestClass::Parallel(0)), None),
            Err("native executor admission limit reached")
        );
        core.forget(|_| false);
        for ticket in 1..=16 {
            server.seen(&[ticket]);
            server.reply(ticket, b"forgotten send");
        }
        wait_state(&core, |s| s.counts == [0, 0]);
        assert!(core.drain().is_empty());
        assert_eq!(server.state.0.lock().unwrap().seen.len(), 16);
        wait_state(&core, |s| s.bytes == [0, 0]);
        assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
    }
}

#[test]
fn post_reads_keep_the_fences_count_and_byte_caps_and_refusals_wait_their_turn() {
    for asynchronous in [false, true] {
        for large in [false, true] {
            let server = Server::new();
            let (core, _) = server.core(asynchronous);
            for ticket in 1..=16 {
                core.run(job(ticket, server.post(ticket), RequestClass::Read), None)
                    .unwrap();
            }
            server.seen(&[1, 2, 3, 4, 5, 6]);
            let held = if large { 21 } else { 128 };
            for ticket in 17..=17 + held {
                let mut request = server.post(ticket);
                if large {
                    request.body = Vec::with_capacity(3 << 20);
                }
                core.run(job(ticket, request, RequestClass::Read), None)
                    .unwrap();
            }
            core.resume_waiting();
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.counts[0], 16);
            assert!(!state.ordered_barrier, "a refusal cannot pass held reads");
            assert_eq!(state.fenced.len(), held as usize + 1);
            assert!(state.fenced_bytes <= ORDERED_WAITING_BYTES);
            assert!(matches!(state.fenced.back(), Some(Fenced::Refused(t, _)) if *t == 17 + held));
            assert!(state
                .fenced
                .iter()
                .take(held as usize)
                .all(|f| matches!(f, Fenced::Held(..))));
        }
    }
}

#[test]
fn held_portable_workers_at_other_origins_leave_the_ordered_read_and_send_free() {
    let server = Server::new();
    let held: Vec<_> = (0..INDEPENDENT).map(|_| Server::new()).collect();
    let grants = std::iter::once(&server)
        .chain(held.iter())
        .map(|s| format!("net.fetch http://{}", s.address))
        .collect::<Vec<_>>()
        .join("\n");
    let (core, woke) = server.core_with_grants(true, &grants);
    for (index, origin) in held.iter().enumerate() {
        let ticket = index as u64 + 1;
        core.run(
            job(
                ticket,
                origin.request(ticket).independent_http(4096),
                RequestClass::Ordered,
            ),
            None,
        )
        .unwrap();
        origin.seen(&[ticket]);
    }
    core.run(read_job(&server, 1000), None).unwrap();
    core.run(job(1001, server.post(1001), RequestClass::Ordered), None)
        .unwrap();
    server.seen(&[1000]);
    {
        let state = core.shared.state.lock().unwrap();
        let read = state.running.iter().find(|r| r.ticket == 1000).unwrap();
        assert_eq!((read.lane, read.transport_lane), (0, 0));
    }
    server.reply(1000, b"fallback");
    server.seen(&[1001]);
    server.reply(1001, b"send");
    assert_eq!(tickets(&collect(&core, &woke, 2)), [1000, 1001]);
    assert!(held
        .iter()
        .all(|s| s.state.0.lock().unwrap().held.len() == 1));
    core.forget(|_| false);
    wait_state(&core, |s| s.running.is_empty() && s.bytes == [0, 0]);
}

#[test]
fn forgetting_a_prefetch_cancels_a_safe_read_running_or_queued_as_any_fetch() {
    for asynchronous in [false, true] {
        for prefetch in [false, true] {
            for queued in [false, true] {
                let server = Server::new();
                let (core, woke) = server.core(asynchronous);
                core.enable_http_cache();
                if queued {
                    core.run(job(1, server.post(1), RequestClass::Ordered), None)
                        .unwrap();
                    server.seen(&[1]);
                }
                let mut request = server.request(2);
                request.prefetch = prefetch;
                core.run(job(2, request, RequestClass::Read), None).unwrap();
                if !queued {
                    server.seen(&[2]);
                }
                core.forget(|ticket| ticket != 2);
                if queued {
                    assert!(!core.shared.state.lock().unwrap().jobs[0]
                        .iter()
                        .any(|j| j.ticket == 2));
                    server.reply(1, b"write");
                    assert_eq!(tickets(&collect(&core, &woke, 1)), [1]);
                } else {
                    server.wait(|s| s.closed.contains(&2));
                }
                core.run(read_job(&server, 3), None).unwrap();
                server.seen(&[3]);
                server.reply(3, b"later");
                let done = collect(&core, &woke, 1);
                assert_eq!(tickets(&done), [3]);
                assert!(matches!(&done[0].1, Outcome::Response(r) if r.body == b"later"));
                wait_state(&core, |s| s.running.is_empty() && s.bytes == [0, 0]);
                assert!(core.ordered_idle());
                assert!(core.drain().is_empty());
                let state = server.state.0.lock().unwrap();
                assert_eq!(state.seen.len(), 2);
                assert!(!queued || !state.seen.iter().any(|(ticket, _)| *ticket == 2));
            }
        }
    }
}
