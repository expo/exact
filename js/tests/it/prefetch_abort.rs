use super::*;
use exact_apple::executor::Executor;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::channel;
use std::time::Duration;

#[test]
fn a_typescript_prefetch_can_be_aborted_after_its_answer_is_forgotten() {
    const TEST: &str = "castle::fetch_cache::prefetch_abort::a_typescript_prefetch_can_be_aborted_after_its_answer_is_forgotten";
    if std::env::var_os("EXACT_PREFETCH_ABORT_TEST").is_none() {
        for asynchronous in [false, true] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture"])
                .env("EXACT_PREFETCH_ABORT_TEST", "1")
                .env("EXACT_HTTP_ASYNC", if asynchronous { "1" } else { "0" })
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "asynchronous={asynchronous}\n{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    }
    let hang = Duration::from_secs(10);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}/next");
    let (seen, arrived) = channel();
    let (ended, closed) = channel();
    let server = std::thread::spawn(move || {
        for i in 0..2 {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(hang)).unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            seen.send(()).unwrap();
            if i == 0 {
                let mut byte = [0];
                ended
                    .send(socket.read(&mut byte).is_ok_and(|n| n == 0))
                    .unwrap();
            } else {
                socket.write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 4\r\nConnection: close\r\n\r\nsent").unwrap();
            }
        }
    });
    let src = format!(
        r#"
component App
  state visible = true
  resource result = prefetchHolding(visible, "{url}", true) as shape string
  mutation stopped as shape string
  action hide
    visible = false
  action halt
    send stopped = abortPrefetch()
  view
    column
      text result
      button press=hide testId="hide"
        text "Hide"
      button press=halt testId="halt"
        text "Abort"
"#
    );
    let grants = "net.fetch http:";
    let mut data = Module::loaded(
        include_bytes!(concat!(env!("OUT_DIR"), "/network.hbc")).to_vec(),
        "test.network",
        grants,
    )
    .unwrap();
    data.set_budget_ms(f64::INFINITY);
    let mut runner = Runner::boot(
        contract::compile(&src).unwrap(),
        data,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let bindings = ibex2::host::Host::new().endow(ibex2::grant::GrantSet::parse(grants).unwrap());
    let (woke, wakes) = channel::<()>();
    extern "C" fn wake(ctx: *mut std::ffi::c_void) {
        // SAFETY: the sender outlives the executor.
        #[allow(unsafe_code)]
        let sender = unsafe { &*ctx.cast::<std::sync::mpsc::Sender<()>>() };
        let _ = sender.send(());
    }
    let executor = Executor::start(
        Some(bindings),
        grants,
        Some((
            wake,
            (&woke as *const _ as *mut std::sync::mpsc::Sender<()>).cast(),
        )),
    );
    let requests = runner.take_requests();
    assert_eq!(requests.len(), 1);
    let ticket = requests[0].ticket;
    for request in requests {
        executor.run(request, None).unwrap();
    }
    arrived.recv_timeout(hang).unwrap();
    let view = |runner: &Runner<Module>, id| {
        runner
            .kernel()
            .node_by_key(runner.kernel().find_by_test_id(id)[0])
            .unwrap()
            .id
    };
    runner
        .dispatch(view(&runner, "hide"), Event::Press)
        .unwrap();
    executor.forget(|ticket| runner.holds(ticket));
    runner
        .dispatch(view(&runner, "halt"), Event::Press)
        .unwrap();
    while let Some((ticket, outcome)) = runner.take_aborted_answer() {
        runner.fulfill(ticket, outcome).unwrap();
    }
    let aborts = runner.take_explicit_aborts();
    assert_eq!(
        aborts,
        [ticket],
        "a later module turn still names the forgotten exchange"
    );
    executor.cancel(&aborts);
    executor.forget(|ticket| runner.holds(ticket));
    assert!(
        closed.recv_timeout(hang).unwrap(),
        "the server must see the aborted connection end"
    );
    executor
        .run(
            exact_runner::RequestOut {
                ticket: ticket + 100,
                target: "later read".into(),
                request: exact_runner::Request::get(&url),
                class: exact_runner::RequestClass::Read,
                forced: false,
            },
            None,
        )
        .unwrap();
    arrived.recv_timeout(hang).unwrap();
    let outcome = loop {
        executor.begin_pump();
        if let Some(outcome) = executor.drain().pop() {
            break outcome;
        }
        wakes.recv_timeout(hang).unwrap();
    };
    assert!(matches!(outcome.1, Outcome::Response(r) if r.body == b"sent"));
    assert!(!outcome.2.unwrap().cached);
    server.join().unwrap();
}
