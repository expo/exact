use super::*;
use crate::pump::Pump;

#[test]
fn a_bounded_pump_keeps_the_ready_lanes_alternating() {
    let (core, _fixture, woke) = setup();
    for ticket in 0..3 {
        core.again(ticket).unwrap();
        core.run(
            job(
                10 + ticket,
                Request::get("https://example.test/ok").independent_http(4096),
            ),
            None,
        )
        .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut state = core.shared.state.lock().unwrap();
    while state.completed[1].len() < 3 {
        state = core
            .shared
            .ready
            .wait_timeout(state, Duration::from_millis(50))
            .unwrap()
            .0;
        if Instant::now() >= deadline {
            let count = state.completed[1].len();
            drop(state);
            panic!("only {count} independent replies completed");
        }
    }
    let independent: Vec<_> = state.completed[1].iter().map(|done| done.ticket).collect();
    drop(state);
    core.begin_pump();
    let now = Instant::now();
    let mut pump = Pump::new(|| now);
    let mut tickets = Vec::new();
    while let Some((ticket, _, _)) = pump.take(|| core.drain().pop()) {
        tickets.push(ticket);
    }
    assert_eq!(
        tickets,
        [0, independent[0], 1, independent[1], 2, independent[2]]
    );
    assert!(core.ordered_idle());
    drop(woke);
}

#[test]
fn completed_latency_is_measured_before_the_ui_drains_it() {
    let (core, fixture, woke) = setup();
    let before = Instant::now();
    core.run(job(991, Request::get("https://example.test/hold")), None)
        .unwrap();
    fixture.wait_held(1);
    fixture.release();
    woke.recv_timeout(Duration::from_secs(5)).unwrap();
    let completed_by = before.elapsed().as_millis() as u64;
    let now = Instant::now() + Duration::from_millis(20);
    let (ticket, outcome, elapsed) = core.drain_with_clock(|| now).pop().unwrap();
    assert_eq!(ticket, 991);
    assert!(matches!(outcome, Outcome::Response(_)));
    assert!(elapsed.unwrap().elapsed_ms <= completed_by);
}
