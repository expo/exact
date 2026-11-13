use exact_plan::Value;
use exact_render::{Serve, Server};
use exact_runner::{DataError, DataSource};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

#[derive(Default)]
struct Data {
    calls: usize,
}
impl DataSource for Data {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        if source == "local" {
            self.calls += 1;
            Ok(Value::Number(self.calls as f64))
        } else {
            Err(DataError::UnknownSource(source.into()))
        }
    }
}

#[test]
fn request_pages_share_only_eligible_http_answers_within_their_server() {
    super::warm_transport();
    let api = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = api.local_addr().unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let counts = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
    let thread = std::thread::spawn({
        let stopped = stopped.clone();
        let counts = counts.clone();
        move || {
            for stream in api.incoming() {
                let mut stream = stream.unwrap();
                if stopped.load(Ordering::SeqCst) {
                    break;
                }
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    if stream.read(&mut byte).unwrap() == 0 {
                        break;
                    }
                    request.push(byte[0]);
                }
                let private = String::from_utf8_lossy(&request).contains("/private");
                let n = counts[usize::from(private)].fetch_add(1, Ordering::SeqCst) + 1;
                let body = format!("\"answer {n}\"");
                let cc = if private { "private" } else { "public" };
                write!(stream, "HTTP/1.1 200 OK\r\nConnection: close\r\nCache-Control: {cc}\r\ncache-control: max-age=60\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            }
        }
    });
    let api = Api {
        addr,
        stopped,
        counts,
        thread: Some(thread),
    };
    let plan = contract::compile(&format!(
        r#"
routes nav
  home "/" render=request
source public() as shape string
  url "http://{addr}/public"
source private() as shape string
  url "http://{addr}/private"
component App
  state local = 0
  resource module = local() as shape number
  resource public = public() as shape string
  resource private = private() as shape string
  view
    text `${{local}}/${{module}}: ${{public}} / ${{private}}`
"#
    ))
    .unwrap();
    let dir = super::serve::dist("http-cache");
    for server_index in 0..2 {
        let server = Server::bind(
            Serve {
                dist: dir.clone(),
                port: 0,
                name: "Cache".into(),
                origin: None,
                deadline: Duration::from_secs(10),
                renders: 2,
                queue: 4,
                viewport: Default::default(),
                lifetime: Duration::from_secs(60),
                generations: None,
            },
            plan.clone(),
            "",
        )
        .unwrap();
        let server = super::serve::serving(server, Data::default);
        for render in 0..3 {
            let (status, _, body) = super::serve::get(server.addr, "/");
            assert_eq!(status, 200, "{body}");
            assert!(
                body.contains(&format!(
                    "0/1: answer {} / answer {}",
                    server_index + 1,
                    server_index * 3 + render + 1
                )),
                "{body}"
            );
        }
        assert_eq!(api.counts[0].load(Ordering::SeqCst), server_index + 1);
        assert_eq!(api.counts[1].load(Ordering::SeqCst), (server_index + 1) * 3);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

struct Api {
    addr: std::net::SocketAddr,
    stopped: Arc<AtomicBool>,
    counts: Arc<[AtomicUsize; 2]>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Api {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        let result = self.thread.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}
