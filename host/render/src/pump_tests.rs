use super::*;
#[path = "../../apple/src/pump_fixture.rs"]
mod fixture;
use exact_kernel::{Kernel, MonospaceMeasurer};
use fixture::{plan, Replies};

#[test]
fn ready_replies_share_a_render_pump_with_count_and_time_bounds() {
    for (count, advance_after, first) in [(20, None, 20), (40, None, 32), (10, Some(3), 3)] {
        let mut source = Replies::new();
        source.advance_after = advance_after;
        let clock = source.clock.clone();
        let plan = exact_plan::Plan::decode(&plan(count)).unwrap();
        let kernel = Kernel::new(Box::new(MonospaceMeasurer::default()));
        let mut runner = Runner::boot(plan, source, kernel, Default::default(), "/").unwrap();
        let seq = runner.seq();
        let executor = Executor::start("");
        assert_eq!(
            settle_with_clock(
                &mut runner,
                &executor,
                Instant::now() + std::time::Duration::from_secs(5),
                || clock.get()
            )
            .unwrap(),
            Settled::Complete
        );
        assert_eq!(runner.data_ref().parsed, (0..count).collect::<Vec<_>>());
        assert_eq!(runner.seq() - seq, count as u64);
        // Release is called once between pumps, never between their replies.
        let released = &runner.data_ref().released_at;
        assert!(released.contains(&first), "{released:?}");
        assert!(
            !released.iter().any(|n| *n > 0 && *n < first),
            "{released:?}"
        );
    }
}

#[test]
fn a_render_error_keeps_prior_transactions_and_leaves_later_replies_queued() {
    let source = Replies::new();
    let clock = source.clock.clone();
    let plan = exact_plan::Plan::decode(&fixture::fault_plan()).unwrap();
    let kernel = Kernel::new(Box::new(MonospaceMeasurer::default()));
    let mut runner = Runner::boot(plan, source, kernel, Default::default(), "/").unwrap();
    let seq = runner.seq();
    let executor = Executor::start("");
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    assert!(settle_with_clock(&mut runner, &executor, deadline, || clock.get()).is_err());
    assert_eq!(runner.data_ref().parsed, [0, 1, 2]);
    assert_eq!(runner.seq() - seq, 2);
    assert_eq!(
        settle_with_clock(&mut runner, &executor, deadline, || clock.get()).unwrap(),
        Settled::Complete
    );
    assert_eq!(runner.data_ref().parsed, [0, 1, 2, 3]);
}
