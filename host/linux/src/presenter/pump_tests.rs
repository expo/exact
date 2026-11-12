use super::*;
#[path = "../../../apple/src/pump_fixture.rs"]
mod fixture;
use fixture::{plan, Replies};

#[test]
fn ready_replies_share_one_presenter_layout_with_count_and_time_bounds() {
    for (count, advance_after, first) in [(20, None, 20), (40, None, 32), (10, Some(3), 3)] {
        let mut source = Replies::new();
        source.advance_after = advance_after;
        let clock = source.clock.clone();
        let (mut p, error) = Presenter::boot_with(
            &plan(count),
            source,
            (390., 844.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let (seq, layouts) = (p.host.runner().seq(), p.host.layout_calls);
        assert_eq!(p.pump_with_clock(0., || clock.get()), None);
        assert_eq!(
            p.host.runner().data_ref().parsed,
            (0..first).collect::<Vec<_>>()
        );
        assert_eq!(p.host.runner().seq() - seq, first as u64);
        assert_eq!(p.host.layout_calls - layouts, 1);
        if first < count {
            assert!(
                p.executor.take_wake(),
                "remaining replies rearm the presenter"
            );
            assert_eq!(p.pump_with_clock(0., || clock.get()), None);
            assert_eq!(
                p.host.runner().data_ref().parsed,
                (0..count).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn a_presenter_error_preserves_prior_transactions_and_leaves_later_replies() {
    let source = Replies::new();
    let clock = source.clock.clone();
    let (mut p, error) = Presenter::boot_with(
        &fixture::fault_plan(),
        source,
        (390., 844.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let seq = p.host.runner().seq();
    assert!(p.pump_with_clock(0., || clock.get()).is_some());
    assert_eq!(p.host.runner().data_ref().parsed, [0, 1, 2]);
    assert_eq!(p.host.runner().seq() - seq, 2);
    assert_eq!(p.pump_with_clock(0., || clock.get()), None);
    assert_eq!(p.host.runner().data_ref().parsed, [0, 1, 2, 3]);
}

#[test]
fn a_presenter_refusal_keeps_its_ordered_turn() {
    let mut source = Replies::new();
    source.refuse = Some(1);
    let clock = source.clock.clone();
    let (mut p, error) = Presenter::boot_with(
        &plan(4),
        source,
        (390., 844.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    assert_eq!(p.pump_with_clock(0., || clock.get()), None);
    assert_eq!(p.host.runner().data_ref().parsed, [0, 1]);
    assert_eq!(p.pump_with_clock(0., || clock.get()), None);
    assert_eq!(p.host.runner().data_ref().parsed, [0, 1, 2, 3]);
}
