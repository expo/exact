//! Redirect admission and physical release (LLP 1162 D1, D8; LLP 1120 D3–D5).
use super::super::{Core, RequestClass, RequestOut, WORKERS};
use super::*;
use ibex2::stdlib::abort::AbortSignal;
use ibex2::stdlib::fetch::{Body, Headers, StreamingResponse, Transport};
use std::sync::Condvar;

const HANG: Duration = Duration::from_secs(30);
const GRANTS: &str = "net.fetch https://*.example.test\nfs.write app:/cache";
const A: &str = "https://a.example.test/start";
const B: &str = "https://b.example.test/next";

#[derive(Default)]
struct Held {
    seen: Vec<(usize, String)>,
    replies: HashMap<String, (u16, Headers)>,
    stopped: bool,
    active: [HashMap<String, usize>; 2],
    peak: usize,
}
#[derive(Default)]
struct Server(Arc<(Mutex<Held>, Condvar)>);
struct Lane(usize, Arc<(Mutex<Held>, Condvar)>);

impl Transport for Lane {
    fn open(
        &self,
        request: &ibex2::stdlib::fetch::Request,
        signal: &AbortSignal,
    ) -> Result<StreamingResponse, HostError> {
        let (lock, changed) = &*self.1;
        let mut state = lock.lock().unwrap();
        let count = state.active[self.0]
            .entry(origin(&request.url))
            .or_default();
        *count += 1;
        let count = *count;
        state.peak = state.peak.max(count);
        state.seen.push((self.0, request.url.clone()));
        changed.notify_all();
        while !state.stopped && !state.replies.contains_key(&request.url) {
            state = changed.wait(state).unwrap();
        }
        if state.stopped {
            return Err(ibex2::stdlib::fetch_async::aborted());
        }
        let (status, headers) = state.replies.remove(&request.url).expect("held reply");
        *state.active[self.0].get_mut(&origin(&request.url)).unwrap() -= 1;
        Ok(StreamingResponse {
            status,
            status_text: String::new(),
            headers,
            body: Body::new(
                Box::new(std::io::Cursor::new(b"answer".to_vec())),
                request.body_limit(),
                signal.clone(),
            ),
            url: request.url.clone(),
            redirected: false,
        })
    }
}

impl Server {
    fn core(&self, asynchronous: bool) -> Core {
        let owners = (0..WORKERS)
            .map(|worker| {
                Some(
                    ibex2::host::Host::with_transport(Box::new(Lane(
                        usize::from(worker != 0),
                        self.0.clone(),
                    )))
                    .endow(ibex2::grant::GrantSet::parse(GRANTS).unwrap()),
                )
            })
            .collect();
        Core::with_owners_lanes(
            owners,
            GRANTS,
            Box::new(|| {}),
            Arc::new(ibex2::host::Host::new),
            asynchronous.then(asynch_lanes),
        )
    }
    fn seen(&self, url: &str) -> usize {
        self.seen_count(url, 1)
    }
    fn seen_count(&self, url: &str, count: usize) -> usize {
        let state = self.0 .0.lock().unwrap();
        let (state, _) = self
            .0
             .1
            .wait_timeout_while(state, HANG, |s| {
                s.seen.iter().filter(|(_, u)| u == url).count() < count
            })
            .unwrap();
        let lane = state.seen.iter().find(|(_, u)| u == url).map(|(l, _)| *l);
        drop(state);
        lane.unwrap_or_else(|| panic!("{url} did not start"))
    }
    fn reply(&self, url: &str, location: Option<&str>) {
        let mut headers = Headers::new();
        if let Some(location) = location {
            headers.set_response("Location", location);
        }
        self.0 .0.lock().unwrap().replies.insert(
            url.into(),
            (if location.is_some() { 302 } else { 200 }, headers),
        );
        self.0 .1.notify_all();
    }
    fn release(&self) {
        self.0 .0.lock().unwrap().stopped = true;
        self.0 .1.notify_all();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.release();
    }
}

fn job(ticket: u64, url: &str, independent: bool) -> RequestOut {
    RequestOut {
        ticket,
        target: "redirect".into(),
        request: if independent {
            Request::get(url).independent_http(1024)
        } else {
            Request::get(url)
        },
        class: RequestClass::Read,
        forced: false,
    }
}

fn wait(core: &Core, until: impl Fn(&State) -> bool) {
    let state = core.shared.state.lock().unwrap();
    let deadline = Instant::now() + HANG;
    let mut state = state;
    while !until(&state) {
        if Instant::now() >= deadline {
            drop(state);
            panic!("redirect condition did not hold");
        }
        // Workers do not notify an empty queue when its last job ends.
        state = core
            .shared
            .ready
            .wait_timeout(
                state,
                if core.machine.is_some() {
                    deadline.saturating_duration_since(Instant::now())
                } else {
                    Duration::from_millis(50)
                },
            )
            .unwrap()
            .0;
    }
}

fn drain(core: &Core, count: usize) -> Vec<u64> {
    let mut done = Vec::new();
    for _ in 0..count {
        wait(core, super::super::has_ready);
        core.begin_pump();
        let reply = core.drain();
        assert_eq!(reply.len(), 1);
        let (ticket, outcome, _) = reply.into_iter().next().unwrap();
        assert!(matches!(outcome, Outcome::Response(_)), "{outcome:?}");
        done.push(ticket);
    }
    done
}

fn finish(core: &mut Core, server: &Server) {
    if core.machine.is_some() {
        core.retire();
        server.release();
        zero(core);
    } else {
        core.forget(|_| false);
        server.release();
        zero(core);
        core.retire();
    }
}

fn zero(core: &Core) {
    wait(core, |s| s.running.is_empty() && s.bytes == [0, 0]);
    let state = core.shared.state.lock().unwrap();
    assert_eq!(state.counts, [0, 0]);
    assert_eq!(state.book.in_flight, 0);
    assert!(state.book.origins.iter().all(HashMap::is_empty));
    assert!(state.book.download_ends.is_empty());
    assert!(state.book.downloads.is_empty());
    assert!(state.book.held_counts.is_empty());
    assert!(state.book.uploads.is_empty());
    assert!(state.book.reserved.is_empty());
    assert!(state.book.hops.iter().all(VecDeque::is_empty));
    assert!(!state.book.blocking);
}

#[test]
fn a_redirected_read_and_its_send_pass_held_independent_exchanges() {
    for asynchronous in [false, true] {
        let server = Server::default();
        let mut core = server.core(asynchronous);
        for ticket in 1..super::super::INDEPENDENT as u64 {
            let url = format!("https://o{ticket}.example.test/held");
            core.run(job(ticket, &url, true), None).unwrap();
            assert_eq!(server.seen(&url), 1);
        }
        core.run(job(100, A, false), None).unwrap();
        let route = server.seen(A);
        if asynchronous {
            assert_eq!(route, 1);
        }
        for ticket in 10..12 {
            core.run(
                job(
                    ticket,
                    &format!("https://o{ticket}.example.test/held"),
                    true,
                ),
                None,
            )
            .unwrap();
        }
        if asynchronous {
            wait(&core, |s| s.jobs[1].is_empty());
        }
        let mut send = job(101, "https://send.example.test/", false);
        send.class = RequestClass::Ordered;
        send.request.method = "POST".into();
        core.run(send, None).unwrap();
        server.reply(A, Some(B));
        server.seen(B);
        server.reply(B, None);
        server.seen("https://send.example.test/");
        server.reply("https://send.example.test/", None);
        assert_eq!(drain(&core, 2), [100, 101]);
        assert!(core.shared.state.lock().unwrap().completed[1].is_empty());
        finish(&mut core, &server);
    }
}

fn redirect(
    script: &super::super::async_tests::Script,
    from: &str,
    to: &str,
) -> super::super::async_tests::Started {
    let mut exchange = script.take(from);
    let mut headers = Headers::new();
    headers.set_response("Location", to);
    assert_eq!(
        exchange.sink.head(Head {
            status: 302,
            headers,
            declared_length: None
        }),
        Flow::Skip
    );
    exchange.sink.end(Ok(()));
    exchange
}

#[test]
fn a_redirect_checks_the_destination_origin_for_a_new_or_existing_member() {
    use super::super::async_tests::{core, Script};
    for member in [false, true] {
        let script = Arc::new(Script::default());
        let (mut core, _) = core(&script, GRANTS);
        for ticket in 1..=PER_ORIGIN as u64 {
            let url = format!("{B}/{ticket}");
            core.run(job(ticket, &url, true), None).unwrap();
            script.wait_for(&url);
        }
        let from = if member { format!("{B}/1") } else { A.into() };
        if !member {
            core.run(job(100, A, false), None).unwrap();
            script.wait_for(A);
        }
        redirect(&script, &from, B);
        script.wait_for(B);
        let (shared, ordered) = {
            let state = core.shared.state.lock().unwrap();
            (
                state.book.origins[1][&origin(B)],
                state.book.origins[0].get(&origin(B)).copied().unwrap_or(0),
            )
        };
        assert!(
            shared <= PER_ORIGIN,
            "destination has {shared} shared exchanges"
        );
        assert!(ordered <= 1);
        script.answer(B, b"redirected");
        assert_eq!(drain(&core, 1), [if member { 1 } else { 100 }]);
        core.retire();
        zero(&core);
    }
}

#[test]
fn late_redirect_metrics_keep_the_lane_that_sent_the_hop() {
    use super::super::async_tests::{core, Script};
    let script = Arc::new(Script::default());
    let (mut core, _) = core(&script, GRANTS);
    for ticket in 1..=PER_ORIGIN as u64 {
        let url = format!("{B}/{ticket}");
        core.run(job(ticket, &url, true), None).unwrap();
        script.wait_for(&url);
    }
    core.run(job(100, A, false), None).unwrap();
    script.wait_for(A);
    let mut first = redirect(&script, A, B);
    script.wait_for(B);
    first.sink.metrics(HopMetrics {
        protocol: Some("h2".into()),
        ..HopMetrics::default()
    });
    wait(&core, |s| s.book.multiplexed[1].contains(&origin(A)));
    assert!(!core.shared.state.lock().unwrap().book.multiplexed[0].contains(&origin(A)));
    drop(first);
    script.answer(B, b"fallback");
    assert_eq!(drain(&core, 1), [100]);
    core.retire();
    zero(&core);
}

#[test]
fn every_file_redirect_end_is_released_even_after_the_answer() {
    use super::super::tests::body_to::Files;
    let files = Files::new();
    let server = Server::default();
    let mut core = server.core(true);
    core.set_app_roots(files.roots());
    for ticket in 1..=6 {
        let (entered, held) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let (answered, answering) = mpsc::channel();
        let (finish, finishing) = mpsc::channel();
        let resume = Mutex::new(resume);
        let finishing = Mutex::new(finishing);
        *core.machine.as_ref().unwrap().body_to_hook.lock().unwrap() =
            Some(Arc::new(move |phase, run| {
                if phase == body_to::Phase::RedirectEnded && run.0 == ticket {
                    entered.send(()).unwrap();
                    resume.lock().unwrap().recv_timeout(HANG).unwrap();
                } else if phase == body_to::Phase::ReplyEnded && run.0 == ticket {
                    answered.send(run).unwrap();
                    finishing.lock().unwrap().recv_timeout(HANG).unwrap();
                }
            }));
        let mut request = job(ticket, A, true);
        request.request = request.request.body_to("app:/cache/a");
        core.run(request, None).unwrap();
        server.seen_count(A, ticket as usize);
        server.reply(A, Some(B));
        held.recv_timeout(HANG).unwrap();
        server.seen_count(B, ticket as usize);
        server.reply(B, None);
        let run = answering.recv_timeout(HANG).unwrap();
        assert_eq!(drain(&core, 1), [ticket]);
        release.send(()).unwrap();
        wait(&core, |s| s.book.download_ends.contains_key(&run));
        finish.send(()).unwrap();
        wait(&core, |s| s.running.is_empty());
    }
    core.run(job(100, A, true), None).unwrap();
    server.seen_count(A, 7);
    server.reply(A, None);
    assert_eq!(drain(&core, 1), [100]);
    finish(&mut core, &server);
}

#[test]
fn portable_redirects_respect_the_origin_limit_from_outside_and_inside_it() {
    for asynchronous in [false, true] {
        for member in [false, true] {
            let server = Server::default();
            let mut core = server.core(asynchronous);
            for ticket in 1..=PER_ORIGIN as u64 {
                let url = format!("{B}/{ticket}");
                core.run(job(ticket, &url, true), None).unwrap();
                server.seen(&url);
            }
            let from = if member { format!("{B}/1") } else { A.into() };
            if !member {
                core.run(job(100, A, false), None).unwrap();
                assert_eq!(server.seen(A), 0);
            }
            server.reply(&from, Some(B));
            assert_eq!(server.seen(B), usize::from(member));
            server.reply(B, None);
            assert_eq!(drain(&core, 1), [if member { 1 } else { 100 }]);
            assert!(server.0 .0.lock().unwrap().peak <= PER_ORIGIN);
            finish(&mut core, &server);
        }
    }
}

#[test]
fn two_redirects_and_a_denied_hop_release_shared_and_ordered_routes() {
    for asynchronous in [false, true] {
        for fallback in [false, true] {
            for (denied, ordered) in [(false, false), (true, false), (false, true), (true, true)] {
                let server = Server::default();
                let mut core = server.core(asynchronous);
                if fallback {
                    for ticket in 1..=super::super::INDEPENDENT as u64 {
                        let url = format!("https://o{ticket}.example.test/held");
                        core.run(job(ticket, &url, true), None).unwrap();
                        server.seen(&url);
                    }
                }
                let mut request = job(100, A, false);
                if ordered {
                    request.class = RequestClass::Ordered;
                }
                core.run(request, None).unwrap();
                let route = server.seen(A);
                if asynchronous || fallback || ordered {
                    assert_eq!(route, usize::from(!fallback && !ordered));
                }
                server.reply(A, Some(B));
                server.seen(B);
                let last = if denied {
                    "https://denied.test/"
                } else {
                    "https://c.example.test/final"
                };
                server.reply(B, Some(last));
                if denied {
                    wait(&core, super::super::has_ready);
                    core.begin_pump();
                    let outcome = core.drain().pop().unwrap().1;
                    let Outcome::Failed { kind, message } = outcome else {
                        panic!("the denied redirect was followed: {outcome:?}");
                    };
                    assert_eq!(kind, FailureKind::Refused);
                    assert_eq!(
                        message,
                        "outside the app's grants (net.fetch): redirected to https://denied.test"
                    );
                    assert!(server
                        .0
                         .0
                        .lock()
                        .unwrap()
                        .seen
                        .iter()
                        .all(|(_, url)| url != last));
                } else {
                    assert_eq!(server.seen(last), route);
                    server.reply(last, None);
                    assert_eq!(drain(&core, 1), [100]);
                }
                finish(&mut core, &server);
            }
        }
    }
}

#[test]
fn a_redirect_whose_transport_refuses_preparation_returns_its_places() {
    use super::super::async_tests::{core, Script};
    for fallback in [false, true] {
        let script = Arc::new(Script::default());
        let (mut core, _) = core(&script, GRANTS);
        if fallback {
            for ticket in 1..=IN_FLIGHT as u64 {
                let url = format!("https://o{ticket}.example.test/held");
                core.run(job(ticket, &url, true), None).unwrap();
                script.wait_for(&url);
            }
        }
        core.run(job(100, A, false), None).unwrap();
        script.wait_for(A);
        script.refuse_next.store(true, Ordering::Release);
        redirect(&script, A, B);
        wait(&core, super::super::has_ready);
        core.begin_pump();
        assert!(matches!(
            core.drain().pop().unwrap().1,
            Outcome::Failed { .. }
        ));
        wait(&core, |s| !s.running.iter().any(|r| r.ticket == 100));
        {
            let state = core.shared.state.lock().unwrap();
            assert!(state.book.reserved.is_empty());
            assert!(state
                .book
                .origins
                .iter()
                .all(|o| !o.contains_key(&origin(A)) && !o.contains_key(&origin(B))));
        }
        core.retire();
        zero(&core);
    }
}

#[test]
fn a_forget_abort_or_retirement_between_hops_leaves_no_reservation() {
    use super::super::async_tests::{core, Script};
    for terminal in ["forget", "abort", "retire"] {
        let script = Arc::new(Script::default());
        let (mut core, _) = core(&script, GRANTS);
        for ticket in 1..=PER_ORIGIN as u64 {
            let url = format!("{B}/{ticket}");
            core.run(job(ticket, &url, true), None).unwrap();
            script.wait_for(&url);
        }
        core.run(job(100, A, true), None).unwrap();
        script.wait_for(A);
        redirect(&script, A, B);
        wait(&core, |s| s.book.hops[1].iter().any(|h| h.run.0 == 100));
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.book.in_flight, PER_ORIGIN);
            assert_eq!(state.book.origins[1][&origin(B)], PER_ORIGIN);
            assert!(state.bytes[1] > 0);
        }
        match terminal {
            "forget" => core.forget(|t| t != 100),
            "abort" => core.cancel(&[100]),
            _ => core.retire(),
        }
        if terminal != "retire" {
            wait(&core, |s| !s.running.iter().any(|r| r.ticket == 100));
            let state = core.shared.state.lock().unwrap();
            assert_eq!(
                state.bytes[1],
                state.running.iter().map(|r| r.charge).sum::<usize>()
            );
            assert_eq!(state.book.in_flight, PER_ORIGIN);
            assert_eq!(state.counts, [0, PER_ORIGIN]);
            assert!(state.book.reserved.is_empty());
            assert!(state.book.hops[1].is_empty());
            drop(state);
            core.retire();
        }
        zero(&core);
        assert_eq!(script.wait(0), 0);
    }
}
