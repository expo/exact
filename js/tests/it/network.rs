//! Resource request sets over the native executor and held HTTP replies (LLP 1162 D1, D2).
#![cfg(all(exact_js_engine, unix))]

use exact_js::Module;
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_render::Executor;
use exact_runner::{Dispatch, Runner};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/network.hbc"));
const LIMIT: Duration = Duration::from_secs(30);
const SHAPE: &str = "shape Result\n  text: string\n  order: string\n";

#[derive(Default)]
struct Counts {
    arrived: usize,
    active: usize,
    peak: usize,
    released: Vec<String>,
    first_release: Option<usize>,
}

struct Arrival {
    stream: TcpStream,
    method: String,
    path: String,
    body: Vec<u8>,
    released_before: Vec<String>,
}

struct Server {
    addr: SocketAddr,
    arrivals: Receiver<Arrival>,
    counts: Arc<Mutex<Counts>>,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, arrivals) = channel();
        let counts = Arc::new(Mutex::new(Counts::default()));
        let stopping = Arc::new(AtomicBool::new(false));
        let (seen, stop) = (counts.clone(), stopping.clone());
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let stream = stream.unwrap();
                if stop.load(Ordering::Acquire) {
                    break;
                }
                stream.set_read_timeout(Some(LIMIT)).unwrap();
                stream.set_write_timeout(Some(LIMIT)).unwrap();
                let released_before = seen.lock().unwrap().released.clone();
                let mut reader = BufReader::new(stream);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut words = first.split_whitespace();
                let method = words.next().unwrap().to_owned();
                let path = words.next().unwrap().trim_start_matches('/').to_owned();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = n.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                {
                    let mut c = seen.lock().unwrap();
                    c.arrived += 1;
                    c.active += 1;
                    c.peak = c.peak.max(c.active);
                }
                if tx
                    .send(Arrival {
                        stream: reader.into_inner(),
                        method,
                        path,
                        body,
                        released_before,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            addr,
            arrivals,
            counts,
            stopping,
            thread: Some(thread),
        }
    }

    fn origin(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn take(&self, count: usize) -> Vec<Arrival> {
        (0..count)
            .map(|_| {
                self.arrivals
                    .recv_timeout(LIMIT)
                    .expect("a request reached the server")
            })
            .collect()
    }

    fn release(&self, mut arrival: Arrival) {
        {
            let mut c = self.counts.lock().unwrap();
            let arrived = c.arrived;
            c.first_release.get_or_insert(arrived);
            c.active -= 1;
            c.released.push(arrival.path.clone());
        }
        write!(
            arrival.stream,
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            arrival.path.len(),
            arrival.path
        )
        .unwrap();
        arrival.stream.shutdown(Shutdown::Write).unwrap();
        // Wait for consumption so reverse releases cannot coalesce (LLP 1162 D1).
        assert_eq!(arrival.stream.read(&mut [0]).unwrap(), 0);
    }

    fn assert_counts(&self, total: usize, peak: usize) {
        let c = self.counts.lock().unwrap();
        assert_eq!(c.arrived, total);
        assert_eq!(c.released.len(), total);
        assert_eq!(c.active, 0);
        assert_eq!(c.peak, peak);
        assert_eq!(c.first_release, Some(peak));
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.addr);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn boot(server: &Server, component: &str) -> (Runner<Module>, Executor) {
    let src = format!("{SHAPE}{}", component.replace("ORIGIN", &server.origin()));
    let mut module = Module::loaded(HBC.to_vec(), "test.network", "net.fetch http:").unwrap();
    module.set_budget_ms(f64::INFINITY);
    let runner = Runner::boot(
        contract::compile(&src).unwrap(),
        module,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let executor = Executor::start(&format!("net.fetch {}", server.origin()));
    (runner, executor)
}

fn submit(r: &mut Runner<Module>, executor: &Executor) -> usize {
    let requests = r.take_requests();
    let count = requests.len();
    for request in requests {
        let dispatch = request
            .request
            .continuation
            .map(|token| r.dispatch_work(token));
        match dispatch {
            Some(Dispatch::Run(work)) => executor.run(request, Some(work)).unwrap(),
            Some(Dispatch::Again) => executor.again(&request).unwrap(),
            None | Some(Dispatch::Missing) => executor.run(request, None).unwrap(),
            _ => panic!("unexpected held or host dispatch"),
        }
    }
    count
}

fn settle(r: &mut Runner<Module>, executor: &Executor) {
    let until = Instant::now() + LIMIT;
    loop {
        submit(r, executor);
        if !r.has_pending() {
            assert!(
                r.failed_resources().is_empty(),
                "{:?}",
                r.failed_resources()
            );
            assert_eq!(r.data().in_flight(), 0);
            return;
        }
        let outcomes = executor.drain();
        if outcomes.is_empty() {
            assert!(Instant::now() < until, "pending: {:?}", r.pending());
            executor.wait(until);
        } else {
            for (ticket, outcome, elapsed) in outcomes {
                r.fulfill_measured(ticket, outcome, elapsed).unwrap();
            }
        }
    }
}

fn result(text: &str, order: &str) -> Value {
    Value::record(vec![Value::str(text), Value::str(order)])
}

fn numbers(count: usize) -> String {
    (0..count)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn batch(method: &str) {
    let server = Server::start();
    let (mut r, executor) = boot(
        &server,
        &format!(
            r#"
component App
  resource result = batch("ORIGIN", 10, "{method}") as shape Result
  view
    text result.text
"#
        ),
    );
    assert_eq!(submit(&mut r, &executor), 10);
    for count in [6, 4] {
        let arrivals = server.take(count);
        for a in &arrivals {
            assert_eq!(a.method, method);
            if method == "POST" {
                let body: serde_json::Value = serde_json::from_slice(&a.body).unwrap();
                assert_eq!(body["variables"]["id"], a.path);
                assert!(body["query"].as_str().unwrap().starts_with("query "));
            }
        }
        for a in arrivals {
            server.release(a);
        }
    }
    settle(&mut r, &executor);
    server.assert_counts(10, 6);
    assert_eq!(
        r.resource("result"),
        Some(&result(&numbers(10), &numbers(10)))
    );
}

#[test]
fn ten_gets_from_one_answer_overlap_six_at_a_time() {
    batch("GET");
}

#[test]
fn ten_graphql_posts_from_one_answer_overlap_six_at_a_time() {
    batch("POST");
}

#[test]
fn twenty_resources_overlap_six_at_a_time_and_all_settle() {
    let server = Server::start();
    let mut component = "component App\n".to_owned();
    for i in 0..20 {
        component.push_str(&format!(
            "  resource row{i} = one(\"ORIGIN\", \"{i}\", \"GET\", true) as shape Result\n"
        ));
    }
    component.push_str("  view\n    text row19.text\n");
    let (mut r, executor) = boot(&server, &component);
    assert_eq!(submit(&mut r, &executor), 20);
    for count in [6, 6, 6, 2] {
        for a in server.take(count) {
            assert_eq!(a.method, "GET");
            server.release(a);
        }
    }
    settle(&mut r, &executor);
    server.assert_counts(20, 6);
    for i in 0..20 {
        assert_eq!(
            r.resource(&format!("row{i}")),
            Some(&result(&i.to_string(), &numbers(i + 1)))
        );
    }
}

#[test]
fn a_send_waits_for_the_read_before_it_and_holds_the_read_after_it() {
    let server = Server::start();
    let (mut r, executor) = boot(
        &server,
        r#"
component App
  state next = false
  resource first = one("ORIGIN", "first", "GET", true) as shape Result
  mutation written as shape Result
  resource second = one("ORIGIN", "second", "GET", next) as shape Result
  action write
    send written = one("ORIGIN", "write", "POST", true)
  action read
    next = true
  view
    column
      text first.text
      text second.text
"#,
    );
    assert_eq!(submit(&mut r, &executor), 1);
    let first = server.take(1).remove(0);
    assert_eq!(first.path, "first");
    assert_eq!(first.method, "GET");
    assert!(first.released_before.is_empty());
    r.act("write", vec![]).unwrap();
    assert_eq!(submit(&mut r, &executor), 1);
    r.act("read", vec![]).unwrap();
    assert_eq!(submit(&mut r, &executor), 1);
    server.release(first);
    for (path, method, before) in [
        ("write", "POST", vec!["first"]),
        ("second", "GET", vec!["first", "write"]),
    ] {
        let a = server.take(1).remove(0);
        assert_eq!(a.path, path);
        assert_eq!(a.method, method);
        assert_eq!(a.released_before, before);
        server.release(a);
    }
    settle(&mut r, &executor);
    server.assert_counts(3, 1);
    assert_eq!(
        r.resource("second"),
        Some(&result("second", "first/write/second"))
    );
}

#[test]
fn three_sends_of_a_parallel_mutation_reach_the_server_together() {
    let server = Server::start();
    let (mut r, executor) = boot(
        &server,
        r#"
component App
  mutation sent as shape Result parallel
  action go
    send sent = one("ORIGIN", "0", "POST", true)
    send sent = one("ORIGIN", "1", "POST", true)
    send sent = one("ORIGIN", "2", "POST", true)
  view
    text "parallel"
"#,
    );
    r.act("go", vec![]).unwrap();
    assert_eq!(submit(&mut r, &executor), 3);
    let arrivals = server.take(3);
    for a in arrivals {
        assert_eq!(a.method, "POST");
        assert!(a.released_before.is_empty());
        server.release(a);
    }
    settle(&mut r, &executor);
    server.assert_counts(3, 3);
}

#[test]
fn reverse_replies_resume_the_modules_continuations_in_request_order() {
    let server = Server::start();
    let (mut r, executor) = boot(
        &server,
        r#"
component App
  resource result = batch("ORIGIN", 6, "GET") as shape Result
  view
    text result.text
"#,
    );
    assert_eq!(submit(&mut r, &executor), 6);
    let mut arrivals = server.take(6);
    arrivals.sort_by(|a, b| b.path.cmp(&a.path));
    for a in arrivals {
        server.release(a);
        assert!(r.has_pending());
    }
    settle(&mut r, &executor);
    server.assert_counts(6, 6);
    assert_eq!(
        server.counts.lock().unwrap().released,
        ["5", "4", "3", "2", "1", "0"]
    );
    assert_eq!(
        r.resource("result"),
        Some(&result(&numbers(6), &numbers(6)))
    );
}
