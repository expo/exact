//! Ordered reads share HTTP capacity and retain an ordered fallback (LLP 1162 D1).
use super::super::{
    async_tests::{core, Script},
    Core, RequestClass, RequestOut,
};
use super::*;

const HANG: Duration = Duration::from_secs(30);
const GRANTS: &str = "net.fetch https://*.example.test";

fn url(ticket: u64) -> String {
    format!("https://o{ticket}.example.test/")
}

fn job(ticket: u64, class: RequestClass) -> RequestOut {
    RequestOut {
        ticket,
        target: "read".into(),
        request: Request::get(&url(ticket)),
        forced: false,
        class,
    }
}

fn wait(core: &Core, until: impl Fn(&State) -> bool) {
    let state = core.shared.state.lock().unwrap();
    let (state, _) = core
        .shared
        .ready
        .wait_timeout_while(state, HANG, |s| !until(s))
        .unwrap();
    assert!(until(&state), "machine condition did not hold");
}

fn drain(core: &Core, count: usize) -> Vec<u64> {
    let mut tickets = Vec::new();
    for _ in 0..count {
        wait(core, super::super::has_ready);
        core.begin_pump();
        let done = core.drain();
        assert_eq!(done.len(), 1);
        let (ticket, outcome, _) = done.into_iter().next().unwrap();
        assert!(matches!(outcome, Outcome::Response(_)), "{outcome:?}");
        tickets.push(ticket);
    }
    tickets
}

fn independent(core: &Core, script: &Script, count: usize) {
    for ticket in 1..=count as u64 {
        let mut request = job(ticket, RequestClass::Ordered);
        request.request = request.request.independent_http(1024);
        core.run(request, None).unwrap();
    }
    assert_eq!(script.wait(count), count);
    wait(core, |s| s.book.in_flight == count);
}

#[test]
fn a_full_http_cap_leaves_the_ordered_send_and_read_fallback_free() {
    let script = Arc::new(Script::default());
    let (core, _) = core(&script, GRANTS);
    independent(&core, &script, IN_FLIGHT);
    for (ticket, class) in [
        (1000, RequestClass::Ordered),
        (1001, RequestClass::Read),
        (1002, RequestClass::Ordered),
    ] {
        let mut request = job(ticket, class);
        if class == RequestClass::Ordered {
            request.request.method = "POST".into();
        }
        core.run(request, None).unwrap();
    }
    for ticket in 1000..=1002 {
        script.wait_for(&url(ticket));
        assert_eq!(script.wait(IN_FLIGHT + 1), IN_FLIGHT + 1);
        {
            let state = core.shared.state.lock().unwrap();
            let running = state.running.iter().find(|r| r.ticket == ticket).unwrap();
            assert_eq!((running.lane, running.transport_lane), (0, 0));
            assert!(state.book.blocking);
            assert_eq!(state.book.in_flight, IN_FLIGHT);
        }
        script.answer(&url(ticket), b"ordered progress");
    }
    assert_eq!(drain(&core, 3), [1000, 1001, 1002]);
    assert_eq!(script.wait(IN_FLIGHT), IN_FLIGHT);
    core.forget(|_| false);
    for ticket in 1..=IN_FLIGHT as u64 {
        script.answer(&url(ticket), b"released");
    }
    wait(&core, |s| {
        s.running.is_empty() && s.bytes == [0, 0] && s.book.in_flight == 0
    });
    assert_eq!(core.shared.state.lock().unwrap().counts, [0, 0]);
}

#[test]
fn reads_wait_for_shared_places_start_in_ask_order_and_deliver_in_ask_order() {
    let script = Arc::new(Script::default());
    let (core, _) = core(&script, GRANTS);
    let shared = super::super::INDEPENDENT - 1;
    let held = IN_FLIGHT - shared;
    independent(&core, &script, held);
    let first = 1000;
    let last = first + IN_FLIGHT as u64;
    for ticket in first..=last {
        core.run(job(ticket, RequestClass::Read), None).unwrap();
    }
    let fallback = first + shared as u64;
    assert_eq!(script.wait(IN_FLIGHT + 1), IN_FLIGHT + 1);
    {
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.book.in_flight, IN_FLIGHT);
        assert_eq!(state.jobs[0].front().unwrap().ticket, fallback + 1);
        let routes: Vec<_> = state
            .running
            .iter()
            .filter(|r| r.lane == 0)
            .map(|r| (r.ticket, r.transport_lane))
            .collect();
        assert_eq!(
            routes,
            (first..=fallback)
                .map(|t| (t, usize::from(t != fallback)))
                .collect::<Vec<_>>()
        );
    }
    let mut next = fallback + 1;
    for ticket in first + 1..=last {
        if ticket == fallback {
            continue;
        }
        script.wait_for(&url(ticket));
        script.answer(&url(ticket), b"ready behind the first");
        if next <= last {
            script.wait_for(&url(next));
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.book.in_flight, IN_FLIGHT);
            assert_eq!(
                state.jobs[0].front().map(|j| j.ticket),
                (next < last).then_some(next + 1)
            );
            next += 1;
        }
        assert!(core.drain().is_empty());
    }
    script.answer(&url(fallback), b"fallback");
    script.answer(&url(first), b"first");
    assert_eq!(
        drain(&core, (last - first + 1) as usize),
        (first..=last).collect::<Vec<_>>()
    );
    core.forget(|_| false);
    for ticket in 1..=held as u64 {
        script.answer(&url(ticket), b"released");
    }
    wait(&core, |s| {
        s.running.is_empty() && s.bytes == [0, 0] && s.book.in_flight == 0
    });
}

#[test]
fn a_forgotten_shared_read_keeps_its_place_and_bytes_until_its_physical_end() {
    let script = Arc::new(Script::default());
    let (core, _) = core(&script, GRANTS);
    core.run(job(1, RequestClass::Read), None).unwrap();
    assert_eq!(script.wait(1), 1);
    core.forget(|_| false);
    wait(&core, |s| {
        s.running.iter().any(|r| r.ticket == 1 && r.terminal)
    });
    {
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.book.in_flight, 1);
        assert!(state.bytes[0] >= MAX_BODY);
        assert_eq!(state.bytes[1], 0);
        assert_eq!(state.counts, [0, 0]);
    }
    script.answer(&url(1), b"cancelled");
    wait(&core, |s| {
        s.running.is_empty() && s.bytes == [0, 0] && s.book.in_flight == 0
    });
    assert!(core.drain().is_empty());
}

#[test]
fn reads_share_the_independent_origins_limit_and_fall_back_one_at_a_time() {
    let script = Arc::new(Script::default());
    let (core, _) = core(&script, GRANTS);
    let same_origin = |ticket| format!("https://same.example.test/{ticket}");
    for ticket in 1..PER_ORIGIN as u64 {
        let mut request = job(ticket, RequestClass::Ordered);
        request.request = Request::get(&same_origin(ticket)).independent_http(1024);
        core.run(request, None).unwrap();
    }
    assert_eq!(script.wait(PER_ORIGIN - 1), PER_ORIGIN - 1);
    for ticket in 1000..=1002 {
        let mut request = job(ticket, RequestClass::Read);
        request.request.url = same_origin(ticket);
        core.run(request, None).unwrap();
    }
    script.wait_for(&same_origin(1001));
    {
        let state = core.shared.state.lock().unwrap();
        let origin = origin(&same_origin(1));
        assert_eq!(state.book.origins[1][&origin], PER_ORIGIN);
        assert_eq!(state.book.origins[0][&origin], 1);
        assert_eq!(state.jobs[0].front().unwrap().ticket, 1002);
        assert!(state.book.blocking);
    }
    core.forget(|t| t >= 1000);
    script.answer(&same_origin(1), b"released");
    script.wait_for(&same_origin(1002));
    {
        let state = core.shared.state.lock().unwrap();
        assert_eq!(
            state
                .running
                .iter()
                .find(|r| r.ticket == 1002)
                .unwrap()
                .transport_lane,
            1
        );
        assert_eq!(state.book.origins[1][&origin(&same_origin(1))], PER_ORIGIN);
    }
    for ticket in [1002, 1001, 1000] {
        script.answer(&same_origin(ticket), b"read");
    }
    assert_eq!(drain(&core, 3), [1000, 1001, 1002]);
    for ticket in 2..PER_ORIGIN as u64 {
        script.answer(&same_origin(ticket), b"released");
    }
    wait(&core, |s| {
        s.running.is_empty() && s.bytes == [0, 0] && s.book.in_flight == 0
    });
    assert!(core
        .shared
        .state
        .lock()
        .unwrap()
        .book
        .origins
        .iter()
        .all(HashMap::is_empty));
}

#[test]
fn a_shared_read_refused_before_transport_returns_its_reserved_place() {
    let script = Arc::new(Script::default());
    let (core, _) = core(&script, GRANTS);
    let mut request = job(1, RequestClass::Read);
    request.request.grants = Some("net.fetch https://other.example.test".into());
    core.run(request, None).unwrap();
    wait(&core, |s| {
        !s.completed[0].is_empty() && s.running.is_empty()
    });
    {
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.book.in_flight, 0);
        assert!(state.book.reserved.is_empty());
        assert!(state.book.origins.iter().all(HashMap::is_empty));
    }
    let outcome = core.drain().pop().unwrap().1;
    assert!(matches!(
        outcome,
        Outcome::Failed {
            kind: FailureKind::Refused,
            ..
        }
    ));
    assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
}
