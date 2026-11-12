//! Cache and machine lifetimes meet without changing lane ownership (LLP 1162 D4, D8).
use super::super::{
    async_tests::{core, core_with_upload_copy, Script, Started},
    tests::body_to::Files,
    Core, RequestClass, RequestOut,
};
use super::*;
use exact_runner::CacheMode;
use ibex2::stdlib::fetch::Headers;

const HANG: Duration = Duration::from_secs(30);
const GRANTS: &str = "net.fetch https://*.example.test\nfs.write app:/cache";
const A: &str = "https://a.example.test/a";
const B: &str = "https://b.example.test/b";

fn job(ticket: u64, url: &str) -> RequestOut {
    RequestOut {
        ticket,
        target: "cache-machine".into(),
        request: Request::get(url),
        forced: false,
        class: RequestClass::Read,
    }
}

fn wait(core: &Core, until: impl Fn(&State) -> bool) {
    let state = core.shared.state.lock().unwrap();
    let (state, _) = core
        .shared
        .ready
        .wait_timeout_while(state, HANG, |s| !until(s))
        .unwrap();
    let met = until(&state);
    drop(state);
    assert!(met, "cache-machine condition did not hold");
}

fn drain(core: &Core, woke: &mpsc::Receiver<()>, count: usize) -> Vec<(u64, Outcome, bool)> {
    let mut done = Vec::new();
    while done.len() < count {
        core.begin_pump();
        done.extend(
            core.drain()
                .into_iter()
                .map(|(t, o, timing)| (t, o, timing.expect("HTTP timing").cached)),
        );
        if done.len() < count {
            woke.recv_timeout(HANG).expect("no completion wake");
        }
    }
    done
}

fn head(exchange: &mut Started, status: u16, headers: &[(&str, &str)]) {
    let mut fields = Headers::new();
    for (k, v) in headers {
        fields.set_response(k, v);
    }
    exchange.sink.head(Head {
        status,
        headers: fields,
        declared_length: None,
    });
}

fn seed(core: &Core, woke: &mpsc::Receiver<()>, script: &Script, url: &str, stale: bool) {
    core.run(job(1, url), None).unwrap();
    script.wait_for(url);
    let mut exchange = script.take(url);
    head(
        &mut exchange,
        200,
        &[
            (
                "Cache-Control",
                if stale { "max-age=0" } else { "max-age=3600" },
            ),
            ("ETag", "\"old\""),
        ],
    );
    exchange.sink.data(b"old");
    exchange.sink.end(Ok(()));
    drop(exchange);
    assert!(matches!(&drain(core, woke, 1)[0].1, Outcome::Response(r) if r.body == b"old"));
    wait(core, |s| s.running.is_empty() && s.bytes == [0, 0]);
}

fn zero(core: &Core) {
    wait(core, |s| s.running.is_empty() && s.bytes == [0, 0]);
    let state = core.shared.state.lock().unwrap();
    assert_eq!(state.counts, [0, 0]);
    assert_eq!(state.book.in_flight, 0);
    assert!(state.book.origins.iter().all(HashMap::is_empty));
    assert!(state.book.hops.iter().all(VecDeque::is_empty));
    assert!(state.book.held_counts.is_empty());
    assert!(state.completed.iter().all(VecDeque::is_empty));
}

#[test]
fn an_overlapping_hit_uses_no_transport_or_place_when_both_lanes_are_full() {
    let script = Arc::new(Script::default());
    let (mut core, woke) = core(&script, GRANTS);
    core.enable_http_cache();
    seed(&core, &woke, &script, A, false);
    for ticket in 10..10 + IN_FLIGHT as u64 {
        let mut request = job(ticket, &format!("https://o{ticket}.example.test/"));
        request.request = request.request.independent_http(1024);
        core.run(request, None).unwrap();
    }
    assert_eq!(script.wait(IN_FLIGHT), IN_FLIGHT);
    core.run(job(100, B), None).unwrap();
    script.wait_for(B);
    core.run(job(101, A), None).unwrap();
    {
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.book.in_flight, IN_FLIGHT);
        assert!(state.book.blocking);
        assert!(state.completed[0]
            .iter()
            .any(|d| d.ticket == 101 && d.cached));
        assert!(!state.running.iter().any(|r| r.ticket == 101));
        assert!(state.jobs[0].is_empty());
    }
    assert_eq!(script.wait(0), IN_FLIGHT + 1);
    assert!(core.drain().is_empty());
    core.retire();
    zero(&core);
}

#[test]
fn an_overlapping_redirected_revalidation_retries_in_its_original_place() {
    for retire in [false, true] {
        let script = Arc::new(Script::default());
        let (mut core, woke) = core(&script, GRANTS);
        core.enable_http_cache();
        seed(&core, &woke, &script, A, true);
        core.run(job(2, B), None).unwrap();
        script.wait_for(B);
        let mut request = job(3, A);
        request.request = request.request.timeout(60_000);
        core.run(request, None).unwrap();
        script.wait_for(A);
        let (run, charge) = {
            let state = core.shared.state.lock().unwrap();
            let r = state.running.iter().find(|r| r.ticket == 3).unwrap();
            assert_eq!((r.lane, r.transport_lane), (0, 1));
            ((r.ticket, r.generation), r.charge)
        };
        let mut redirect = script.take(A);
        assert_eq!(
            redirect._request.headers.get("if-none-match"),
            Some("\"old\"")
        );
        head(
            &mut redirect,
            302,
            &[("Location", "https://c.example.test/conditional")],
        );
        redirect.sink.end(Ok(()));
        drop(redirect);
        let url = "https://c.example.test/conditional";
        script.wait_for(url);
        let mut conditional = script.take(url);
        head(&mut conditional, 304, &[]);
        conditional.sink.end(Ok(()));
        drop(conditional);
        script.wait_for(A);
        let mut retry = script.take(A);
        assert_eq!(retry._request.headers.get("if-none-match"), None);
        assert_eq!(retry._request.headers.get("if-modified-since"), None);
        {
            let state = core.shared.state.lock().unwrap();
            let r = state.running.iter().find(|r| r.ticket == 3).unwrap();
            assert_eq!((r.ticket, r.generation), run);
            assert_eq!(r.charge, charge);
            assert_eq!(r.transport_lane, 1);
            assert_eq!(state.book.in_flight, 2);
            assert_eq!(state.counts, [2, 0]);
        }
        if retire {
            core.retire();
            retry.sink.end(Err(ibex2::stdlib::fetch_async::aborted()));
            drop(retry);
        } else {
            head(&mut retry, 200, &[("Cache-Control", "max-age=3600")]);
            retry.sink.data(b"new");
            retry.sink.end(Ok(()));
            drop(retry);
            wait(&core, |s| s.completed[0].iter().any(|d| d.ticket == 3));
            assert!(core.drain().is_empty());
            script.answer(B, b"first");
            let done = drain(&core, &woke, 2);
            assert_eq!(done.iter().map(|d| d.0).collect::<Vec<_>>(), [2, 3]);
            assert!(
                matches!(&done[1].1, Outcome::Response(r) if r.status == 200 && r.body == b"new")
            );
            assert!(!done[1].2);
        }
        zero(&core);
    }
}

#[test]
fn a_redirected_revalidation_retry_checks_its_original_origin_again() {
    let script = Arc::new(Script::default());
    let (mut core, woke) = core(&script, GRANTS);
    core.enable_http_cache();
    seed(&core, &woke, &script, A, true);
    core.run(job(2, A), None).unwrap();
    script.wait_for(A);
    let mut conditional = script.take(A);
    head(&mut conditional, 302, &[("Location", B)]);
    conditional.sink.end(Ok(()));
    drop(conditional);
    script.wait_for(B);
    for ticket in 10..10 + PER_ORIGIN as u64 {
        let mut held = job(ticket, &format!("{A}/{ticket}"));
        held.request = held.request.independent_http(1024);
        core.run(held, None).unwrap();
    }
    assert_eq!(script.wait(PER_ORIGIN + 1), PER_ORIGIN + 1);
    let mut redirected = script.take(B);
    head(&mut redirected, 304, &[]);
    redirected.sink.end(Ok(()));
    drop(redirected);
    script.wait_for(A);
    let mut retry = script.take(A);
    assert_eq!(retry._request.headers.get("if-none-match"), None);
    {
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.book.origins[1][&origin(A)], PER_ORIGIN);
        assert_eq!(state.book.origins[0][&origin(A)], 1);
        assert_eq!(
            state
                .running
                .iter()
                .find(|r| r.ticket == 2)
                .unwrap()
                .transport_lane,
            0
        );
    }
    head(&mut retry, 200, &[]);
    retry.sink.data(b"unconditional");
    retry.sink.end(Ok(()));
    drop(retry);
    assert!(
        matches!(&drain(&core, &woke, 1)[0].1, Outcome::Response(r) if r.body == b"unconditional")
    );
    wait(&core, |s| !s.running.iter().any(|r| r.ticket == 2));
    core.retire();
    zero(&core);
}

#[test]
fn observed_failures_and_file_reply_heads_invalidate_the_cache() {
    for file in [false, true] {
        for failed in [false, true] {
            let files = Files::new();
            let script = Arc::new(Script::default());
            let (core, woke) = core_with_upload_copy(&script, GRANTS, file);
            core.set_app_roots(files.roots());
            core.enable_http_cache();
            let item = "https://a.example.test/item";
            seed(&core, &woke, &script, A, false);
            seed(&core, &woke, &script, item, false);
            let mut write = job(2, A);
            write.request.method = "POST".into();
            if file {
                write.request = write.request.body_to("app:/cache/a");
            }
            core.run(write, None).unwrap();
            script.wait_for(A);
            let exchange = script.take(A);
            {
                let state = core.shared.state.lock().unwrap();
                let r = state.running.iter().find(|r| r.ticket == 2).unwrap();
                assert_eq!(r.transport_lane, usize::from(!file));
            }
            // A transport may tell a head and fail before opening a body (LLP 1162 D4).
            std::thread::spawn(move || {
                let mut exchange = exchange;
                if failed {
                    let mut headers = Headers::new();
                    headers.set_response("Location", "/item");
                    exchange.sink.observed(201, &headers);
                    exchange
                        .sink
                        .end(Err(HostError::Failed("lost body".into())));
                } else {
                    head(&mut exchange, 201, &[("Location", "/item")]);
                    exchange.sink.data(b"saved");
                    exchange.sink.end(Ok(()));
                }
            })
            .join()
            .unwrap();
            let outcome = drain(&core, &woke, 1).remove(0).1;
            assert_eq!(matches!(outcome, Outcome::Failed { .. }), failed);
            if file && !failed {
                assert!(matches!(outcome, Outcome::Response(r) if r.body_written() == Some(5)));
                assert_eq!(std::fs::read(files.0.join("cache/a")).unwrap(), b"saved");
            }
            for (ticket, url) in [(3, A), (4, item)] {
                let mut probe = job(ticket, url);
                probe.request = probe.request.cache(CacheMode::OnlyIfCached);
                core.run(probe, None).unwrap();
                assert!(matches!(
                    drain(&core, &woke, 1)[0].1,
                    Outcome::Failed {
                        kind: FailureKind::Network,
                        ..
                    }
                ));
            }
            zero(&core);
        }
    }
}

#[test]
fn a_retired_refresh_ignores_late_transport_heads_and_releases_its_cache_state() {
    let script = Arc::new(Script::default());
    let (mut core, woke) = core(&script, GRANTS);
    core.enable_http_cache();
    seed(&core, &woke, &script, A, false);
    let mut refresh = job(2, A);
    refresh.forced = true;
    core.run(refresh, None).unwrap();
    script.wait_for(A);
    let mut late = script.take(A);
    let mut write = job(3, B);
    write.request.method = "POST".into();
    core.run(write, None).unwrap();
    script.wait_for(B);
    let mut late_head = script.take(B);
    wait(&core, |s| s.running.iter().all(|r| r.live == 1));
    assert!(core
        .shared
        .state
        .lock()
        .unwrap()
        .running
        .iter()
        .any(|r| r.cache_replacing.is_some()));
    core.retire();
    while woke.try_recv().is_ok() {}
    std::thread::spawn(move || {
        let mut headers = Headers::new();
        headers.set_response("Location", "/a");
        late_head.sink.observed(201, &headers);
        head(&mut late_head, 201, &[("Location", "/a")]);
        late_head
            .sink
            .end(Err(ibex2::stdlib::fetch_async::aborted()));
        head(&mut late, 200, &[("Cache-Control", "max-age=3600")]);
        late.sink.end(Ok(()));
    })
    .join()
    .unwrap();
    zero(&core);
    assert!(core.shared.state.lock().unwrap().cache.is_none());
    assert!(woke.try_recv().is_err());
}

struct HeldStream {
    entered: mpsc::Sender<()>,
    release: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl ibex2::stdlib::fetch::Transport for HeldStream {
    fn open(
        &self,
        _: &ibex2::stdlib::fetch::Request,
        _: &ibex2::stdlib::abort::AbortSignal,
    ) -> Result<ibex2::stdlib::fetch::StreamingResponse, HostError> {
        self.entered.send(()).unwrap();
        self.release.lock().unwrap().recv_timeout(HANG).unwrap();
        Err(ibex2::stdlib::fetch_async::aborted())
    }
}

#[test]
fn queued_cap_and_file_requests_settle_once_by_refusal_timeout_or_retirement() {
    for file in [false, true] {
        for terminal in ["crowded", "timeout", "retire"] {
            let script = Arc::new(Script::default());
            let (core, woke) = core_with_upload_copy(&script, GRANTS, file);
            let (entered, started) = mpsc::channel();
            let (release, held) = mpsc::channel();
            let held = Arc::new(Mutex::new(held));
            let mut core = core.streams_on(move || {
                ibex2::host::Host::with_transport(Box::new(HeldStream {
                    entered: entered.clone(),
                    release: held.clone(),
                }))
            });
            let count = if file { PER_ORIGIN } else { IN_FLIGHT };
            for ticket in 1..=count as u64 {
                let url = if file {
                    format!("{A}/{ticket}")
                } else {
                    format!("https://o{ticket}.example.test/")
                };
                let mut request = job(ticket, &url);
                request.request = request.request.independent_http(1024);
                core.run(request, None).unwrap();
            }
            assert_eq!(script.wait(count), count);
            let mut queued = job(100, A);
            queued.request = queued.request.independent_http(64 << 20).timeout(60_000);
            if file {
                queued.request = queued.request.body_to("app:/cache/a");
            }
            let (_, _, limit) = super::super::reservation(&queued.request).unwrap();
            core.run(queued, None).unwrap();
            wait(&core, |s| s.jobs[1].iter().any(|j| j.ticket == 100));
            assert_eq!(core.shared.queued_deadlines.load(Ordering::Acquire), 0);
            if terminal == "crowded" {
                let mut stream = job(101, B);
                stream.request = stream.request.independent_http(1);
                stream.request.stream = true;
                let (_, buffers, _) = super::super::reservation(&stream.request).unwrap();
                let size = (BYTES[1] - limit + 1 - buffers - MAX_HEADERS * 2).div_ceil(2) as u32;
                stream.request = stream.request.independent_http(size);
                core.run(stream, None).unwrap();
                started.recv_timeout(HANG).unwrap();
            } else if terminal == "timeout" {
                core.machine.as_ref().unwrap().timer.expire((100, 0));
            }
            if terminal == "retire" {
                core.retire();
            } else {
                let done = drain(&core, &woke, 1);
                assert_eq!(done[0].0, 100);
                assert!(matches!(&done[0].1, Outcome::Failed { kind, .. }
                    if *kind == if terminal == "crowded" { FailureKind::Refused } else { FailureKind::Timeout }));
                let state = core.shared.state.lock().unwrap();
                assert_eq!(state.counts[1], count + usize::from(terminal == "crowded"));
                assert!(state.jobs[1].is_empty());
                assert_eq!(
                    state.bytes[1],
                    state.running.iter().map(|r| r.charge).sum::<usize>()
                );
                drop(state);
                core.retire();
            }
            if terminal == "crowded" {
                release.send(()).unwrap();
            }
            zero(&core);
            assert_eq!(script.wait(0), 0);
        }
    }
}
