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
        let grants = format!("net.fetch http://{}", self.address);
        let owners = (0..WORKERS)
            .map(|_| {
                Some(
                    ibex2::host::Host::new().endow(ibex2::grant::GrantSet::parse(&grants).unwrap()),
                )
            })
            .collect();
        let (wake, woke) = channel();
        let core = Core::with_owners_lanes(
            owners,
            &grants,
            Box::new(move || {
                let _ = wake.send(());
            }),
            Arc::new(ibex2::host::Host::new),
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
        assert!(until(&state), "server saw {:?}", state.seen);
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
    let server = Server::new();
    let (core, woke) = server.core(false);
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
        assert!(reads::next_job(&mut state, 0).is_none());
        assert!(reads::next_job(&mut state, 1).is_none());
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
    assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
}

#[test]
fn replies_released_in_reverse_order_drain_in_admission_order() {
    let server = Server::new();
    let (core, woke) = server.core(false);
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

#[test]
fn reads_keep_posts_storage_work_and_handoffs_in_their_places() {
    let server = Server::new();
    let (core, woke) = server.core(false);
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
    assert_queued(&core, 8);
    reply.send(Outcome::Storage(vec![7]));
    server.seen(&[8]);
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

#[test]
fn the_ordered_worker_runs_a_read_and_post_while_all_shared_workers_are_held() {
    let server = Server::new();
    let (core, woke) = server.core(false);
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
    assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
}

#[test]
fn short_ordered_bytes_wait_without_stalling_an_earlier_read_or_its_delivery() {
    let server = Server::new();
    let (core, woke) = server.core(false);
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
    assert_eq!((reply.body.len(), reply.body.capacity()), (size, size));
    assert!(reply.body.iter().all(|b| *b == 42));
    let mut state = core.shared.state.lock().unwrap();
    assert_eq!(state.bytes, [held, 0]);
    state.bytes[0] -= held;
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
    let server = Server::new();
    let (core, woke) = server.core(false);
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

#[test]
fn a_read_that_times_out_delivers_failure_in_its_ordered_place() {
    let server = Server::new();
    let (core, woke) = server.core(false);
    core.run(read_job(&server, 1), None).unwrap();
    let mut r = read_job(&server, 2);
    r.request = r.request.timeout(500);
    core.run(r, None).unwrap();
    server.seen(&[1, 2]);
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

#[test]
fn forgetting_skips_a_queued_read_and_aborts_a_running_read_freeing_its_place() {
    let server = Server::new();
    let (core, woke) = server.core(false);
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
    assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);

    core.run(read_job(&server, 5), None).unwrap();
    server.seen(&[5]);
    core.run(job(6, server.post(6), RequestClass::Ordered), None)
        .unwrap();
    core.cancel(&[5]);
    server.seen(&[6]);
    server.reply(6, b"after abort");
    assert_eq!(tickets(&collect(&core, &woke, 1)), [6]);
    assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
}

#[test]
fn retiring_reads_releases_every_worker_and_its_reservations() {
    let server = Server::new();
    let (core, _) = server.core(false);
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

#[test]
fn read_posts_and_parallel_requests_keep_ordered_worker_behavior() {
    let server = Server::new();
    let (core, woke) = server.core(false);
    core.run(job(1, server.post(1), RequestClass::Read), None)
        .unwrap();
    core.run(read_job(&server, 2), None).unwrap();
    core.run(job(3, server.request(3), RequestClass::Parallel(3)), None)
        .unwrap();
    core.run(read_job(&server, 4), None).unwrap();
    server.seen(&[1]);
    assert_queued(&core, 2);
    assert_queued(&core, 3);
    {
        let mut state = core.shared.state.lock().unwrap();
        assert!(reads::next_job(&mut state, 0).is_none());
        assert!(reads::next_job(&mut state, 1).is_none());
    }
    server.reply(1, b"post");
    server.seen(&[2]);
    assert_queued(&core, 3);
    server.reply(2, b"read");
    server.seen(&[3]);
    assert_queued(&core, 4);
    server.reply(3, b"parallel");
    server.seen(&[4]);
    server.reply(4, b"last");
    assert_eq!(tickets(&collect(&core, &woke, 4)), [1, 2, 3, 4]);
}

#[test]
fn read_admission_forgetting_and_the_refusal_fence_stay_on_the_ordered_lane() {
    let server = Server::new();
    let (core, woke) = server.core(false);
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
    assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
}

#[test]
fn the_asynchronous_core_keeps_read_and_parallel_classes_ordered() {
    let server = Server::new();
    let (core, woke) = server.core(true);
    core.run(read_job(&server, 1), None).unwrap();
    core.run(read_job(&server, 2), None).unwrap();
    core.run(job(3, server.request(3), RequestClass::Parallel(1)), None)
        .unwrap();
    server.seen(&[1]);
    assert_queued(&core, 2);
    assert_queued(&core, 3);
    server.reply(1, b"one");
    server.seen(&[2]);
    assert_queued(&core, 3);
    server.reply(2, b"two");
    server.seen(&[3]);
    server.reply(3, b"three");
    assert_eq!(tickets(&collect(&core, &woke, 3)), [1, 2, 3]);
}
