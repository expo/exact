use super::*;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver};

pub(in super::super) const HANG: Duration = Duration::from_secs(10);
pub(in super::super) const FRESH: &[(&str, &str)] = &[("Cache-Control", "private, max-age=60")];

pub(in super::super) struct Plan {
    status: u16,
    headers: Headers,
    body: Vec<u8>,
    pub(in super::super) held: Option<Receiver<()>>,
    pub(in super::super) disconnect: bool,
    truncated: bool,
    /// The `Content-Length` the head states, when not the body's.
    pub(in super::super) declares: Option<u64>,
    /// Held open after the head and body until this is told.
    pub(in super::super) after: Option<Receiver<()>>,
}
impl Plan {
    pub(in super::super) fn new(status: u16, headers: &[(&str, &str)], body: &[u8]) -> Self {
        Self {
            status,
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
            body: body.to_vec(),
            held: None,
            disconnect: false,
            truncated: false,
            declares: None,
            after: None,
        }
    }
}
#[derive(Default)]
struct ServerState {
    plans: HashMap<String, VecDeque<Plan>>,
    seen: Vec<(String, String, Headers)>,
    stop: bool,
}
pub(in super::super) struct Server {
    pub(in super::super) address: std::net::SocketAddr,
    state: Arc<(Mutex<ServerState>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub(in super::super) fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new((Mutex::new(ServerState::default()), Condvar::new()));
        let server = state.clone();
        let thread = std::thread::spawn(move || {
            let mut clients = Vec::new();
            for socket in listener.incoming() {
                let socket = socket.unwrap();
                if server.0.lock().unwrap().stop {
                    break;
                }
                let server = server.clone();
                clients.push(std::thread::spawn(move || serve(socket, server)));
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
    pub(in super::super) fn plan(&self, path: &str, plan: Plan) {
        self.state
            .0
            .lock()
            .unwrap()
            .plans
            .entry(path.into())
            .or_default()
            .push_back(plan);
    }
    pub(in super::super) fn request(&self, path: &str) -> Request {
        Request::get(&format!("http://{}{path}", self.address))
    }
    pub(in super::super) fn core(&self, asynchronous: bool, cache: bool) -> (Core, Receiver<()>) {
        self.core_grants(
            asynchronous,
            cache,
            &format!("net.fetch http://{}", self.address),
        )
    }
    pub(in super::super) fn core_grants(
        &self,
        asynchronous: bool,
        cache: bool,
        grants: &str,
    ) -> (Core, Receiver<()>) {
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
        if cache {
            core.enable_http_cache();
        }
        (core, woke)
    }
    pub(in super::super) fn count(&self) -> usize {
        self.state.0.lock().unwrap().seen.len()
    }
    pub(in super::super) fn seen(&self, count: usize) {
        let state = self.state.0.lock().unwrap();
        let (state, _) = self
            .state
            .1
            .wait_timeout_while(state, HANG, |s| s.seen.len() < count)
            .unwrap();
        let count_seen = state.seen.len();
        drop(state);
        assert!(
            count_seen >= count,
            "only {} requests reached the server",
            count_seen
        );
    }
    fn headers(&self, index: usize) -> Headers {
        self.state.0.lock().unwrap().seen[index].2.clone()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.state.0.lock().unwrap().stop = true;
        let _ = TcpStream::connect(self.address);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn serve(mut socket: TcpStream, server: Arc<(Mutex<ServerState>, Condvar)>) {
    socket.set_read_timeout(Some(HANG)).unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        if socket.read(&mut byte).unwrap_or(0) == 0 {
            return;
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).unwrap();
    let mut line = head.lines().next().unwrap().split_whitespace();
    let method = line.next().unwrap().to_owned();
    let path = line.next().unwrap().to_owned();
    let headers: Headers = head
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.into(), v.trim().into()))
        .collect();
    let length = first(&headers, "content-length").map_or(0, |s| s.parse().unwrap());
    socket.read_exact(&mut vec![0; length]).unwrap();
    let plan = {
        let mut state = server.0.lock().unwrap();
        state.seen.push((method.clone(), path.clone(), headers));
        server.1.notify_all();
        let plans = state.plans.get_mut(&path).expect("a planned response");
        // URLSession sends a GET again when its connection closes unanswered,
        // so a GET's disconnect answers every attempt.
        if method == "GET" && plans.front().is_some_and(|p| p.disconnect) {
            return;
        }
        plans.pop_front().expect("a planned response")
    };
    if let Some(held) = plan.held {
        let _ = held.recv_timeout(HANG);
    }
    if plan.disconnect {
        return;
    }
    let reason = match plan.status {
        200 => "OK",
        304 => "Not Modified",
        302 => "Found",
        _ => "Error",
    };
    let mut head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
        plan.status,
        plan.declares
            .unwrap_or((plan.body.len() + usize::from(plan.truncated)) as u64)
    );
    for (k, v) in plan.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    if socket.write_all(head.as_bytes()).is_ok() {
        let _ = socket.write_all(&plan.body);
    }
    if let Some(after) = plan.after {
        let _ = after.recv_timeout(HANG);
    }
    let _ = socket.shutdown(Shutdown::Write);
}
pub(in super::super) fn job(ticket: u64, request: Request) -> RequestOut {
    RequestOut {
        ticket,
        target: "cache-test".into(),
        forced: false,
        request,
        class: RequestClass::Read,
    }
}
pub(in super::super) fn collect(
    core: &Core,
    woke: &Receiver<()>,
) -> (u64, Outcome, CompletionTiming) {
    let end = Instant::now() + HANG;
    loop {
        core.begin_pump();
        if let Some((ticket, outcome, timing)) = core.drain().pop() {
            return (ticket, outcome, timing.unwrap());
        }
        woke.recv_timeout(end.saturating_duration_since(Instant::now()))
            .expect("completion");
    }
}
pub(in super::super) fn ask(
    core: &Core,
    woke: &Receiver<()>,
    request: RequestOut,
) -> (Response, bool) {
    let ticket = request.ticket;
    core.run(request, None).unwrap();
    let (got, outcome, timing) = collect(core, woke);
    assert_eq!(got, ticket);
    let Outcome::Response(response) = outcome else {
        panic!("{outcome:?}");
    };
    (response, timing.cached)
}
pub(in super::super) fn read(
    core: &Core,
    woke: &Receiver<()>,
    server: &Server,
    ticket: u64,
    path: &str,
) -> (Response, bool) {
    ask(core, woke, job(ticket, server.request(path)))
}

#[test]
fn fresh_reads_keep_each_headers_value_order_and_ignore_name_order() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan(
            "/a",
            Plan::new(
                200,
                &[("Cache-Control", "max-age=60"), ("Vary", "X-Part")],
                b"account A",
            ),
        );
        server.plan("/a", Plan::new(200, FRESH, b"reversed"));
        server.plan("/a", Plan::new(200, FRESH, b"account B"));
        let mut request = server.request("/a");
        request.headers = vec![
            ("Authorization".into(), "A".into()),
            ("X-Part".into(), "1".into()),
            ("x-part".into(), "2".into()),
        ];
        assert!(!ask(&core, &woke, job(1, request.clone())).1);
        request.headers = vec![
            ("x-part".into(), "1".into()),
            ("authorization".into(), "A".into()),
            ("X-PART".into(), "2".into()),
        ];
        let (reply, cached) = ask(&core, &woke, job(2, request.clone()));
        assert!(cached);
        assert_eq!(reply.body, b"account A");
        assert!(first(&reply.headers, "age").is_some());
        request.headers.swap(0, 2);
        let (reply, cached) = ask(&core, &woke, job(3, request.clone()));
        assert!(!cached);
        assert_eq!(reply.body, b"reversed");
        request.headers[1].1 = "B".into();
        assert_eq!(ask(&core, &woke, job(3, request)).0.body, b"account B");
        assert_eq!(server.count(), 3);
    }
}

#[test]
fn quoted_policy_lists_store_only_when_no_store_and_vary_star_are_absent() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        for (case, (headers, stores)) in [
            (vec![], false),
            (vec![("Cache-Control", "no-store, max-age=60")], false),
            (
                vec![("Cache-Control", "max-age=60"), ("Vary", "Accept, *")],
                false,
            ),
            (vec![("Cache-Control", "s-maxage=60")], false),
            (
                vec![("Cache-Control", r#"max-age=60, ext="a\"b", no-store"#)],
                false,
            ),
            (
                vec![("Cache-Control", "max-age=60"), ("Vary", r#""a\"b", *"#)],
                false,
            ),
            (
                vec![("Cache-Control", r#"max-age=60, private="x, no-store, y""#)],
                true,
            ),
            (
                vec![("Cache-Control", r#"max-age=60, ext="a\"b, no-store, c""#)],
                true,
            ),
            (
                vec![("Cache-Control", "max-age=60"), ("Vary", r#""x, *, y""#)],
                true,
            ),
            (
                vec![("Cache-Control", "max-age=60"), ("Vary", r#""a\"b, *, c""#)],
                true,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let path = format!("/{case}");
            for body in [b"first".as_slice(), b"second"] {
                server.plan(&path, Plan::new(200, &headers, body));
            }
            assert!(!read(&core, &woke, &server, 1, &path).1);
            let (reply, cached) = read(&core, &woke, &server, 2, &path);
            assert_eq!(cached, stores, "{headers:?}");
            assert_eq!(
                reply.body,
                if stores {
                    b"first".as_slice()
                } else {
                    b"second"
                }
            );
        }
        for _ in 0..2 {
            server.plan("/request-no-store", Plan::new(200, FRESH, b"body"));
        }
        let mut request = server.request("/request-no-store");
        request
            .headers
            .push(("Cache-Control".into(), "no-store".into()));
        for ticket in 1..=2 {
            assert!(!ask(&core, &woke, job(ticket, request.clone())).1);
        }
        assert_eq!(server.count(), 18);
    }
}

#[test]
fn forced_reads_replace_fresh_entries_and_unstorable_replies_drop_them() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        for (headers, body) in [
            (FRESH, b"old".as_slice()),
            (FRESH, b"forced"),
            (&[][..], b"uncached"),
            (FRESH, b"last"),
        ] {
            server.plan("/a", Plan::new(200, headers, body));
        }
        read(&core, &woke, &server, 1, "/a");
        let mut forced = job(2, server.request("/a"));
        forced.forced = true;
        assert!(!ask(&core, &woke, forced.clone()).1);
        assert!(first(&server.headers(1), "if-none-match").is_none());
        let (reply, cached) = read(&core, &woke, &server, 3, "/a");
        assert!(cached);
        assert_eq!(reply.body, b"forced");
        forced.ticket = 4;
        ask(&core, &woke, forced);
        assert_eq!(read(&core, &woke, &server, 5, "/a").0.body, b"last");
        assert_eq!(server.count(), 4);
    }
}

#[test]
fn only_successful_writes_invalidate_every_header_variant() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        for body in [b"A".as_slice(), b"B"] {
            server.plan("/a", Plan::new(200, FRESH, body));
        }
        let mut a = server.request("/a");
        a.headers.push(("Authorization".into(), "A".into()));
        let mut b = a.clone();
        b.headers[0].1 = "B".into();
        ask(&core, &woke, job(1, a.clone()));
        ask(&core, &woke, job(2, b.clone()));
        let mut post = server.request("/a");
        post.method = "POST".into();
        let mut write = job(3, post);
        write.class = RequestClass::Ordered;
        server.plan("/a", Plan::new(500, &[], b"failed"));
        ask(&core, &woke, write.clone());
        assert!(ask(&core, &woke, job(4, a.clone())).1);
        assert!(ask(&core, &woke, job(5, b.clone())).1);
        server.plan("/a", Plan::new(200, &[], b"saved"));
        write.ticket = 6;
        ask(&core, &woke, write);
        for body in [b"new A".as_slice(), b"new B"] {
            server.plan("/a", Plan::new(200, FRESH, body));
        }
        assert_eq!(ask(&core, &woke, job(7, a)).0.body, b"new A");
        assert_eq!(ask(&core, &woke, job(8, b)).0.body, b"new B");
        assert_eq!(server.count(), 6);
    }
}

#[test]
fn redirected_200_replies_are_never_stored() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        for _ in 0..2 {
            server.plan("/a", Plan::new(302, &[("Location", "/b")], b""));
            server.plan("/b", Plan::new(200, FRESH, b"redirected"));
        }
        for ticket in 1..=2 {
            assert!(!read(&core, &woke, &server, ticket, "/a").1);
        }
        assert_eq!(server.count(), 4);
    }
}

#[test]
fn a_fresh_hit_still_requires_the_sources_current_grants() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan("/a", Plan::new(200, FRESH, b"secret"));
        read(&core, &woke, &server, 1, "/a");
        let mut request = server.request("/a");
        request.grants = Some(String::new());
        core.run(job(2, request), None).unwrap();
        let (_, outcome, timing) = collect(&core, &woke);
        assert!(matches!(
            outcome,
            Outcome::Failed {
                kind: FailureKind::Refused,
                ..
            }
        ));
        assert!(!timing.cached);
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn driver_faults_and_answers_precede_a_hit_and_never_replace_it() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan("/a", Plan::new(200, FRESH, b"real"));
        read(&core, &woke, &server, 1, "/a");
        let mut builder = exact_plan::builder::PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        builder.node(
            exact_kernel::NodeType::View as u8,
            None,
            None,
            0,
            &[],
            &[],
            None,
        );
        let mut runner = exact_runner::Runner::boot(
            builder.finish().unwrap(),
            (),
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        runner
            .faults()
            .arm(&server.request("/a").url, Some(1))
            .unwrap();
        let request = job(2, server.request("/a"));
        let Some(exact_runner::Dispatch::Run(work)) = runner.fault_dispatch(&request) else {
            panic!("armed fault");
        };
        core.run_owned(request, Some(work)).unwrap();
        assert!(matches!(
            collect(&core, &woke).1,
            Outcome::Failed {
                kind: FailureKind::Network,
                ..
            }
        ));
        core.run(
            job(3, server.request("/a")),
            Some(Box::new(|| {
                Outcome::Response(Response {
                    status: 200,
                    headers: vec![("cache-control".into(), "max-age=60".into())],
                    body: b"driver answer".to_vec(),
                })
            })),
        )
        .unwrap();
        assert!(!collect(&core, &woke).2.cached);
        let (reply, cached) = read(&core, &woke, &server, 4, "/a");
        assert!(cached);
        assert_eq!(reply.body, b"real");
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn a_hit_takes_no_worker_and_drains_after_an_earlier_slow_read() {
    for (asynchronous, method) in [
        (false, "GET"),
        (false, "POST"),
        (true, "GET"),
        (true, "POST"),
    ] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan("/cached", Plan::new(200, FRESH, b"cached"));
        read(&core, &woke, &server, 1, "/cached");
        let (release, held) = channel();
        let mut slow = Plan::new(200, &[], b"slow");
        slow.held = Some(held);
        server.plan("/slow", slow);
        let mut request = server.request("/slow");
        request.method = method.into();
        core.run(job(2, request), None).unwrap();
        server.seen(2);
        core.run(job(3, server.request("/cached")), None).unwrap();
        {
            let state = core.shared.state.lock().unwrap();
            assert!(state.completed[0].iter().any(|d| d.ticket == 3 && d.cached));
            assert!(!state.running.iter().any(|r| r.ticket == 3));
        }
        assert!(core.drain().is_empty());
        release.send(()).unwrap();
        assert_eq!(collect(&core, &woke).0, 2);
        assert_eq!(collect(&core, &woke).0, 3);
        assert_eq!(server.count(), 2);
    }
}

#[test]
fn lru_bytes_are_bounded_and_replies_over_four_mib_are_not_stored() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        let body = vec![7; 3 << 20];
        for i in 0..5 {
            let path = format!("/{i}");
            server.plan(&path, Plan::new(200, FRESH, &body));
            read(&core, &woke, &server, i + 1, &path);
        }
        assert!(read(&core, &woke, &server, 6, "/0").1);
        server.plan("/5", Plan::new(200, FRESH, &body));
        read(&core, &woke, &server, 7, "/5");
        assert!(read(&core, &woke, &server, 8, "/0").1);
        server.plan("/1", Plan::new(200, FRESH, b"evicted"));
        assert_eq!(read(&core, &woke, &server, 9, "/1").0.body, b"evicted");
        let huge = vec![1; (4 << 20) + 1];
        for _ in 0..2 {
            server.plan("/huge", Plan::new(200, FRESH, &huge));
        }
        for ticket in 10..=11 {
            assert!(!read(&core, &woke, &server, ticket, "/huge").1);
        }
        let state = core.shared.state.lock().unwrap();
        let cache = state.cache.as_ref().unwrap();
        assert!(cache.bytes <= CAPACITY);
        assert_eq!(
            cache.bytes,
            cache
                .entries
                .iter()
                .chain(&cache.retired)
                .map(|e| e.bytes)
                .sum::<usize>()
        );
        assert_eq!(server.count(), 9);
    }
}

#[test]
fn read_posts_ordered_gets_and_independent_gets_never_use_or_fill_the_cache() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        for (i, class, method, independent) in [
            (0, RequestClass::Read, "POST", false),
            (1, RequestClass::Ordered, "GET", false),
            (2, RequestClass::Read, "GET", true),
        ] {
            let path = format!("/{i}");
            server.plan(&path, Plan::new(200, FRESH, b"read"));
            read(&core, &woke, &server, 1, &path);
            let mut request = server.request(&path);
            request.method = method.into();
            if independent {
                request.http = HttpScheduling::Independent {
                    max_response_bytes: 1024,
                };
            }
            for ticket in 2..=3 {
                server.plan(&path, Plan::new(200, FRESH, b"wire"));
                let mut request = job(ticket, request.clone());
                request.class = class;
                assert!(!ask(&core, &woke, request).1);
            }
            if method == "POST" {
                server.plan(&path, Plan::new(200, FRESH, b"after write"));
                assert!(!read(&core, &woke, &server, 4, &path).1);
            } else {
                assert!(read(&core, &woke, &server, 4, &path).1);
            }
        }
        assert_eq!(server.count(), 10);
    }
}

#[test]
fn the_render_style_core_does_not_enable_the_private_cache() {
    let server = Server::new();
    let (core, woke) = server.core(false, false);
    for _ in 0..2 {
        server.plan("/a", Plan::new(200, FRESH, b"wire"));
    }
    for ticket in 1..=2 {
        assert!(!read(&core, &woke, &server, ticket, "/a").1);
    }
    assert_eq!(server.count(), 2);
}

#[test]
fn age_and_freshness_use_rfc_9111_arithmetic_and_monotonic_residence() {
    let headers = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    };
    let date = "Sun, 06 Nov 1994 08:49:37 GMT";
    let mut policy = headers(&[
        ("Date", date),
        ("Age", "20"),
        ("Cache-Control", "max-age=60"),
    ]);
    let (age, lifetime, _) = freshness(
        &policy,
        Duration::from_secs(5),
        Duration::from_secs(784111787),
    )
    .unwrap();
    assert_eq!(
        age,
        Duration::from_secs(25),
        "Age plus response delay wins over apparent age"
    );
    assert_eq!(lifetime, Duration::from_secs(60));
    assert_eq!(
        freshness(
            &policy,
            Duration::from_secs(5),
            Duration::from_secs(784111807)
        )
        .unwrap()
        .0,
        Duration::from_secs(30)
    );
    policy = headers(&[("Date", date), ("Expires", "Sun, 06 Nov 1994 08:50:37 GMT")]);
    assert_eq!(
        freshness(&policy, Duration::ZERO, Duration::from_secs(784111777))
            .unwrap()
            .1,
        Duration::from_secs(60)
    );
    policy = headers(&[("ETag", "v1"), ("Cache-Control", "max-age=60, max-age=120")]);
    assert!(
        freshness(&policy, Duration::ZERO, Duration::from_secs(784111777))
            .unwrap()
            .2
    );
    policy = headers(&[("Date", "0"), ("Cache-Control", "max-age=60")]);
    assert!(
        freshness(&policy, Duration::ZERO, Duration::from_secs(784111777))
            .unwrap()
            .2
    );
    assert_eq!(format_date(784111777), date);
    let at = Instant::now();
    let request = Request::get("https://example.test/");
    let response = Response {
        status: 200,
        headers: vec![],
        body: vec![],
    };
    let mut entry = Entry {
        preloads: 0,
        key: Key::of(&request),
        response,
        stored: at,
        age: Duration::from_secs(50),
        lifetime: Duration::from_secs(60),
        validate: false,
        bytes: 0,
        used: AtomicUsize::new(0),
    };
    assert!(entry.fresh(&request, at + Duration::from_secs(9)));
    assert!(!entry.fresh(&request, at + Duration::from_secs(10)));
    entry.validate = true;
    assert!(!entry.fresh(&request, at));
    for value in [
        "Sun, 06 Nov 1994 08:49:37 GMT",
        "Sunday, 06-Nov-94 08:49:37 GMT",
        "Sun Nov  6 08:49:37 1994",
    ] {
        assert_eq!(http_date(value), Some(784111777));
    }
    assert_eq!(http_date("Sun, 31 Feb 1994 08:49:37 GMT"), None);
    assert_eq!(
        http_date("Sun, 06 Nov 9223372036854775807 08:49:37 GMT"),
        None
    );
    assert_eq!(
        http_date("Sunday, 06-Nov-9223372036854775807 08:49:37 GMT"),
        None
    );
}

#[test]
fn last_modified_alone_revalidates_and_stale_unvalidated_replies_go_to_the_wire() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan(
            "/modified",
            Plan::new(
                200,
                &[("Last-Modified", "Sun, 06 Nov 1994 08:49:37 GMT")],
                b"original",
            ),
        );
        server.plan(
            "/modified",
            Plan::new(304, &[("Cache-Control", "max-age=60")], b""),
        );
        read(&core, &woke, &server, 1, "/modified");
        let (response, cached) = read(&core, &woke, &server, 2, "/modified");
        assert_eq!(response.body, b"original");
        assert!(cached);
        assert!(first(&server.headers(1), "if-none-match").is_none());
        assert_eq!(
            first(&server.headers(1), "if-modified-since"),
            Some("Sun, 06 Nov 1994 08:49:37 GMT")
        );
        assert!(read(&core, &woke, &server, 3, "/modified").1);
        for body in [b"old".as_slice(), b"new"] {
            server.plan(
                "/stale",
                Plan::new(200, &[("Cache-Control", "max-age=0")], body),
            );
        }
        read(&core, &woke, &server, 4, "/stale");
        assert_eq!(read(&core, &woke, &server, 5, "/stale").0.body, b"new");
        assert!(first(&server.headers(3), "if-none-match").is_none());
        assert!(first(&server.headers(3), "if-modified-since").is_none());
        assert_eq!(server.count(), 4);
    }
}

#[test]
fn expires_age_and_no_cache_control_whether_a_reply_needs_the_network() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan(
            "/expires",
            Plan::new(
                200,
                &[
                    ("Date", "Sun, 06 Nov 2094 08:49:37 GMT"),
                    ("Expires", "Sun, 06 Nov 2094 08:50:37 GMT"),
                ],
                b"fresh",
            ),
        );
        read(&core, &woke, &server, 1, "/expires");
        assert!(read(&core, &woke, &server, 2, "/expires").1);
        for path in ["/age", "/date", "/no-cache"] {
            let headers = match path {
                "/age" => vec![("Cache-Control", "max-age=60"), ("Age", "60")],
                "/date" => vec![
                    ("Cache-Control", "max-age=60"),
                    ("Date", "Sun, 06 Nov 1994 08:49:37 GMT"),
                ],
                _ => vec![("Cache-Control", "no-cache, max-age=60"), ("ETag", "v1")],
            };
            for _ in 0..2 {
                server.plan(path, Plan::new(200, &headers, b"wire"));
            }
            assert!(!read(&core, &woke, &server, 3, path).1);
            assert!(!read(&core, &woke, &server, 4, path).1);
        }
        assert_eq!(server.count(), 7);
    }
}

#[test]
fn unsafe_driver_answers_invalidate_and_url_schemes_are_case_insensitive() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        let mut request = server.request("/a");
        request.url = request.url.replacen("http:", "HTTP:", 1);
        server.plan("/a", Plan::new(200, FRESH, b"old"));
        ask(&core, &woke, job(1, request.clone()));
        let mut post = request.clone();
        post.method = "POST".into();
        post.url = post.url.replacen("HTTP:", "http:", 1);
        core.run(
            job(2, post),
            Some(Box::new(|| {
                Outcome::Response(Response {
                    status: 200,
                    headers: vec![],
                    body: vec![],
                })
            })),
        )
        .unwrap();
        assert_eq!(collect(&core, &woke).0, 2);
        server.plan("/a", Plan::new(200, FRESH, b"new"));
        let (reply, cached) = ask(&core, &woke, job(3, request));
        assert!(!cached);
        assert_eq!(reply.body, b"new");
        assert_eq!(server.count(), 2);
    }
}

#[path = "cache_mode_tests.rs"]
mod modes;

#[test]
fn request_no_store_preserves_an_existing_entry_for_its_key() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan("/a", Plan::new(200, FRESH, b"kept"));
        read(&core, &woke, &server, 1, "/a");
        let mut request = server.request("/a");
        request
            .headers
            .push(("Cache-Control".into(), "no-store".into()));
        {
            let mut state = core.shared.state.lock().unwrap();
            let cache = state.cache.as_mut().unwrap();
            Arc::get_mut(cache.entries.front_mut().unwrap())
                .unwrap()
                .key = Key::of(&request);
        }
        server.plan("/a", Plan::new(200, FRESH, b"exchange"));
        assert!(!ask(&core, &woke, job(2, request)).1);
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.cache.as_ref().unwrap().entries.len(), 1);
        assert_eq!(
            state.cache.as_ref().unwrap().entries[0].response.body,
            b"kept"
        );
    }
}

#[test]
fn writes_invalidate_every_answering_hop_before_its_body_or_later_hop_fails() {
    for asynchronous in [false, true] {
        for end in ["ok", "truncated", "disconnected"] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            for path in ["/a", "/b"] {
                server.plan(path, Plan::new(200, FRESH, b"old"));
                read(&core, &woke, &server, 1, path);
            }
            server.plan("/a", Plan::new(307, &[("Location", "/b")], b""));
            let mut final_reply = Plan::new(200, &[], b"saved");
            final_reply.truncated = end == "truncated";
            final_reply.disconnect = end == "disconnected";
            server.plan("/b", final_reply);
            let mut write = server.request("/a");
            write.method = "POST".into();
            core.run(job(2, write), None).unwrap();
            let outcome = collect(&core, &woke).1;
            assert_eq!(matches!(outcome, Outcome::Response(_)), end == "ok");
            for path in ["/a", "/b"] {
                server.plan(path, Plan::new(200, FRESH, b"new"));
                let (reply, cached) = read(&core, &woke, &server, 3, path);
                assert!(!cached, "{asynchronous}: {end}: {path}");
                assert_eq!(reply.body, b"new");
            }
        }
    }
}

#[test]
fn a_write_whose_reply_is_refused_for_its_declared_length_invalidates_its_location() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan("/item", Plan::new(200, FRESH, b"old"));
        read(&core, &woke, &server, 1, "/item");
        // URLSession tells a head once some of its body has come.
        let mut reply = Plan::new(201, &[("Location", "/item")], &[b'.'; 2048]);
        reply.declares = Some(1 << 40);
        let (close, after) = channel();
        reply.after = Some(after);
        server.plan("/write", reply);
        let mut write = server.request("/write");
        write.method = "POST".into();
        core.run(job(2, write), None).unwrap();
        assert!(matches!(collect(&core, &woke).1, Outcome::Failed { .. }));
        close.send(()).unwrap();
        server.plan("/item", Plan::new(200, FRESH, b"new"));
        let (reply, cached) = read(&core, &woke, &server, 3, "/item");
        assert!(!cached);
        assert_eq!(reply.body, b"new");
    }
}

#[test]
fn write_locations_invalidate_only_the_same_origin_in_canonical_form() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        for path in ["/items/1", "/other", "/foreign"] {
            server.plan(path, Plan::new(200, FRESH, b"old"));
            read(&core, &woke, &server, 1, path);
        }
        let foreign = "https://elsewhere.test/foreign";
        {
            let mut state = core.shared.state.lock().unwrap();
            let entry = state
                .cache
                .as_mut()
                .unwrap()
                .entries
                .iter_mut()
                .find(|e| e.key.url.ends_with("/foreign"))
                .unwrap();
            Arc::get_mut(entry).unwrap().key = Key::of(&Request::get(foreign));
        }
        let canonical = format!("HTTP://{}/items/./1#fragment", server.address);
        server.plan(
            "/items",
            Plan::new(
                201,
                &[("Location", &canonical), ("Content-Location", "other")],
                b"",
            ),
        );
        let mut write = server.request("/items");
        write.method = "POST".into();
        ask(&core, &woke, job(2, write.clone()));
        for path in ["/items/1", "/other"] {
            server.plan(path, Plan::new(200, FRESH, b"new"));
            assert!(!read(&core, &woke, &server, 3, path).1);
        }
        server.plan(
            "/items",
            Plan::new(
                201,
                &[
                    ("Location", foreign),
                    ("Content-Location", "//elsewhere.test/foreign"),
                ],
                b"",
            ),
        );
        ask(&core, &woke, job(4, write));
        assert!(core
            .shared
            .state
            .lock()
            .unwrap()
            .cache
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.key.url == foreign));
    }
}

#[test]
fn canonical_url_identity_matches_the_transports_url_parser() {
    let base = Key::of(&Request::get("https://api.test/"));
    for url in ["HTTPS://API.TEST:443", "https://api.test/#fragment"] {
        assert!(base == Key::of(&Request::get(url)), "{url}");
    }
    for url in [
        "http://api.test/",
        "https://api.test:444/",
        "https://api.test/?",
        "https://api.test/%2f",
        "https://api.test/a/..",
        "https://user@api.test/",
    ] {
        assert!(base != Key::of(&Request::get(url)), "{url}");
    }
}

#[test]
fn hit_completions_wait_for_lane_bytes_before_copying_the_body() {
    for (asynchronous, mode) in [
        (false, "drain"),
        (false, "forget"),
        (true, "drain"),
        (true, "forget"),
        (true, "retire"),
    ] {
        let forget_hit = mode == "forget";
        let server = Server::new();
        let (mut core, woke) = server.core(asynchronous, true);
        server.plan("/cached", Plan::new(200, FRESH, &vec![1; MAX_ENTRY - 2048]));
        read(&core, &woke, &server, 1, "/cached");
        let mut releases = Vec::new();
        for ticket in 2..8 {
            let (release, held) = channel();
            releases.push(release);
            let mut plan = Plan::new(200, &[], b"read");
            plan.held = Some(held);
            server.plan("/held", plan);
            core.run(job(ticket, server.request("/held")), None)
                .unwrap();
        }
        server.seen(7);
        for ticket in 8..40 {
            core.run(job(ticket, server.request("/cached")), None)
                .unwrap();
        }
        {
            let state = core.shared.state.lock().unwrap();
            let bytes = state.bytes[0];
            let waiting = !state.jobs[0].is_empty();
            drop(state);
            assert!(bytes <= BYTES[0], "{bytes} exceeds {}", BYTES[0]);
            assert!(waiting);
        }
        if mode == "retire" {
            core.retire();
            for release in releases {
                release.send(()).unwrap();
            }
            let state = core.shared.state.lock().unwrap();
            let (state, _) = core
                .shared
                .ready
                .wait_timeout_while(state, HANG, |s| !s.running.is_empty())
                .unwrap();
            assert!(state.running.is_empty());
            assert_eq!(state.bytes, [0, 0]);
            assert_eq!(state.counts, [0, 0]);
            assert!(state.jobs.iter().all(VecDeque::is_empty));
            assert!(state.completed.iter().all(VecDeque::is_empty));
            assert!(state.cache.is_none());
            continue;
        }
        if forget_hit {
            core.forget(|ticket| ticket != 8);
            let waiting = !core.shared.state.lock().unwrap().jobs[0].is_empty();
            assert!(
                !waiting,
                "forgetting a completed hit makes room for the waiting hit"
            );
        }
        let count = if forget_hit { 37 } else { 38 };
        for release in releases {
            release.send(()).unwrap();
        }
        let mut completed = Vec::new();
        let end = Instant::now() + HANG;
        while completed.len() < count {
            core.begin_pump();
            completed.extend(core.drain());
            if completed.len() < count {
                woke.recv_timeout(end.saturating_duration_since(Instant::now()))
                    .unwrap();
            }
        }
        assert_eq!(
            completed.iter().map(|c| c.0).collect::<Vec<_>>(),
            (2..40)
                .filter(|ticket| !forget_hit || *ticket != 8)
                .collect::<Vec<_>>()
        );
        assert!(completed
            .iter()
            .filter(|c| c.0 >= 8)
            .all(|c| c.2.unwrap().cached));
        assert_eq!(server.count(), 7);
    }
}

#[test]
fn a_non_app_read_checks_its_reach_outside_the_state_lock_and_can_hit() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan("/a", Plan::new(200, FRESH, b"cached"));
        read(&core, &woke, &server, 1, "/a");
        let mut request = server.request("/a");
        request.reach.origin = exact_runner::CallOrigin::Tool;
        {
            let mut state = core.shared.state.lock().unwrap();
            let (lane, charge, limit) = reads::reservation(&request, true).unwrap();
            state.bytes[lane] += charge;
            state.counts[lane] += 1;
            state.ordered.push_back(2);
            state.jobs[lane].push_back(Job {
                ticket: 2,
                request,
                forced: false,
                work: None,
                waiting_charge: charge,
                charge,
                limit,
                forgotten: false,
                queued_deadline: None,
                overlap: true,
                class: RequestClass::Read,
            });
            serve_ready(&mut state);
            let left_to_worker = state.completed[0].is_empty();
            drop(state);
            assert!(
                left_to_worker,
                "a reach that may resolve DNS belongs off the state lock"
            );
        }
        core.shared.ready.notify_all();
        let (_, outcome, timing) = collect(&core, &woke);
        assert!(matches!(outcome, Outcome::Response(_)));
        assert!(timing.cached);
        assert_eq!(server.count(), 1);
    }
}

#[test]
fn canonical_invalidation_drops_a_default_port_alias() {
    let server = Server::new();
    let (core, woke) = server.core(false, true);
    server.plan("/item", Plan::new(200, FRESH, b"old"));
    read(&core, &woke, &server, 1, "/item");
    {
        let mut state = core.shared.state.lock().unwrap();
        let entry = state.cache.as_mut().unwrap().entries.front_mut().unwrap();
        Arc::get_mut(entry).unwrap().key = Key::of(&Request::get("https://API.test:443/item"));
    }
    let request = Request::post_json("https://api.test/item", "{}");
    core.run(
        job(2, request),
        Some(Box::new(|| {
            Outcome::Response(Response {
                status: 200,
                headers: vec![],
                body: vec![],
            })
        })),
    )
    .unwrap();
    collect(&core, &woke);
    assert!(core
        .shared
        .state
        .lock()
        .unwrap()
        .cache
        .as_ref()
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn verbatim_paths_that_the_transport_sends_do_not_share_entries() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        for path in ["/a/../item", "/%69tem", "/item"] {
            server.plan(path, Plan::new(200, FRESH, path.as_bytes()));
            let (reply, cached) = read(&core, &woke, &server, 1, path);
            assert!(!cached);
            assert_eq!(reply.body, path.as_bytes());
        }
        assert_eq!(server.count(), 3);
    }
}

#[test]
fn a_direct_write_invalidates_its_location_while_its_successful_heads_body_is_held() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan("/b", Plan::new(200, FRESH, b"old"));
        read(&core, &woke, &server, 1, "/b");
        // URLSession tells a head only once some body bytes have arrived (LLP 1162 D4).
        let mut plan = Plan::new(200, &[("Location", "/b")], &[b'.'; 2048]);
        plan.declares = Some(4096);
        let (close, after) = channel();
        plan.after = Some(after);
        server.plan("/a", plan);
        let mut write = server.request("/a");
        write.method = "POST".into();
        core.run(job(2, write), None).unwrap();
        let state = core.shared.state.lock().unwrap();
        let (state, waited) = core
            .shared
            .ready
            .wait_timeout_while(state, HANG, |state| {
                !state.cache.as_ref().unwrap().entries.is_empty()
            })
            .unwrap();
        let invalidated = state.cache.as_ref().unwrap().entries.is_empty();
        let unfinished = state.running.iter().any(|r| r.ticket == 2 && !r.terminal);
        drop(state);
        close.send(()).unwrap();
        assert!(
            invalidated && !waited.timed_out(),
            "the head drops /b before the body ends"
        );
        assert!(unfinished);
        assert!(matches!(collect(&core, &woke).1, Outcome::Failed { .. }));
        server.plan("/b", Plan::new(200, FRESH, b"new"));
        let (reply, cached) = read(&core, &woke, &server, 3, "/b");
        assert!(!cached);
        assert_eq!(reply.body, b"new");
    }
}

#[test]
fn a_held_fresh_hit_is_admitted_on_a_freed_slot_without_taking_a_worker_or_effect_slot() {
    let server = Server::new();
    let (core, woke) = server.core(false, true);
    server.plan("/cached", Plan::new(200, FRESH, b"cached"));
    read(&core, &woke, &server, 0, "/cached");
    let mut releases = Vec::new();
    for ticket in 1..=17 {
        let (release, held) = channel();
        releases.push(Some(release));
        let path = format!("/held/{ticket}");
        let mut plan = Plan::new(200, &[], b"read");
        plan.held = Some(held);
        server.plan(&path, plan);
        let mut request = server.request(&path);
        if ticket == 17 {
            request.method = "POST".into();
        }
        core.run(job(ticket, request), None).unwrap();
    }
    core.run(job(18, server.request("/cached")), None).unwrap();
    server.seen(7);
    {
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.counts[0] - state.light, 16);
        assert_eq!(
            state.fenced.iter().map(Fenced::ticket).collect::<Vec<_>>(),
            [17, 18]
        );
    }
    for ticket in [2, 3] {
        releases[ticket - 1].take().unwrap().send(()).unwrap();
        let end = Instant::now() + HANG;
        loop {
            woke.recv_timeout(end.saturating_duration_since(Instant::now()))
                .expect("a freed slot wakes held admission before ticket 1 ends");
            core.begin_pump();
            let state = core.shared.state.lock().unwrap();
            if state.completed[0]
                .iter()
                .any(|done| done.ticket == ticket as u64)
            {
                break;
            }
        }
    }
    {
        let state = core.shared.state.lock().unwrap();
        assert!(state.completed[0]
            .iter()
            .any(|done| done.ticket == 18 && done.cached));
        assert!(!state.running.iter().any(|run| run.ticket == 18));
        assert!(!state.jobs[0].iter().any(|job| job.ticket == 18));
        assert!(state.fenced.is_empty());
        assert_eq!(state.counts[0] - state.light, 15);
    }
    assert!(core.drain().is_empty(), "ticket 1 still holds delivery");
    for release in releases.into_iter().flatten() {
        release.send(()).unwrap();
    }
    for ticket in 1..=18 {
        let (got, outcome, timing) = collect(&core, &woke);
        assert_eq!(got, ticket);
        assert!(matches!(outcome, Outcome::Response(_)));
        assert_eq!(timing.cached, ticket == 18);
    }
    let state = core.shared.state.lock().unwrap();
    assert_eq!(state.counts, [0, 0]);
    assert_eq!(state.light, 0);
    assert_eq!(state.bytes, [0, 0]);
    assert!(state.running.is_empty());
    assert_eq!(server.count(), 18);
}

#[test]
fn a_write_that_fails_unanswered_invalidates_its_url() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let (core, woke) = server.core(asynchronous, true);
        server.plan("/b", Plan::new(200, FRESH, b"old"));
        read(&core, &woke, &server, 1, "/b");
        let mut lost = Plan::new(200, &[], b"");
        lost.disconnect = true;
        server.plan("/b", lost);
        let mut write = server.request("/b");
        write.method = "POST".into();
        core.run(job(2, write), None).unwrap();
        assert!(matches!(collect(&core, &woke).1, Outcome::Failed { .. }));
        server.plan("/b", Plan::new(200, FRESH, b"new"));
        let (reply, cached) = read(&core, &woke, &server, 3, "/b");
        assert!(!cached);
        assert_eq!(reply.body, b"new");
    }
}

#[path = "cache_revalidation_tests.rs"]
mod revalidation;

#[test]
fn a_redirected_writes_outcome_does_not_resolve_the_last_heads_locations_again() {
    for asynchronous in [false, true] {
        let first_hop = Server::new();
        let last_hop = Server::new();
        let grants = format!(
            "net.fetch http://{}\nnet.fetch http://{}",
            first_hop.address, last_hop.address
        );
        let (core, woke) = first_hop.core_grants(asynchronous, true, &grants);
        for server in [&first_hop, &last_hop] {
            server.plan("/item", Plan::new(200, FRESH, b"old"));
            read(&core, &woke, server, 1, "/item");
        }
        let destination = last_hop.request("/write").url;
        first_hop.plan("/write", Plan::new(307, &[("Location", &destination)], b""));
        last_hop.plan(
            "/write",
            Plan::new(200, &[("Content-Location", "/item")], b"saved"),
        );
        let mut write = first_hop.request("/write");
        write.method = "POST".into();
        ask(&core, &woke, job(2, write));
        first_hop.plan("/item", Plan::new(200, FRESH, b"unexpected"));
        let (reply, cached) = read(&core, &woke, &first_hop, 3, "/item");
        assert!(cached);
        assert_eq!(reply.body, b"old");
        last_hop.plan("/item", Plan::new(200, FRESH, b"new"));
        let (reply, cached) = read(&core, &woke, &last_hop, 4, "/item");
        assert!(!cached);
        assert_eq!(reply.body, b"new");
    }
}

#[path = "cache_order_tests.rs"]
mod order;
