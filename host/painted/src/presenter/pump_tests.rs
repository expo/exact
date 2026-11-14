use super::*;
#[path = "../../../apple/src/pump_fixture.rs"]
mod fixture;
use fixture::{plan, Replies};

#[test]
fn completion_during_an_activation_probe_keeps_its_next_turn() {
    use exact_runner::{DataError, Value};
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Loading {
        ready: Cell<bool>,
        finish_on_probe: Cell<bool>,
        wake: RefCell<Option<Box<dyn FnOnce() + Send>>>,
    }
    impl DataSource for Loading {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::Number(99.))
        }
        fn preload(&self) -> Result<bool, DataError> {
            let ready = self.ready.get();
            if self.finish_on_probe.replace(false) {
                // Completion races with a probe that already observed loading.
                self.ready.set(true);
                self.wake.borrow_mut().take().unwrap()();
            }
            Ok(ready)
        }
        fn when_preloaded(&self, wake: Box<dyn FnOnce() + Send>) {
            *self.wake.borrow_mut() = Some(wake);
        }
    }
    let plan = crate::test_plan(include_str!(
        "../../../../logic/tests/fixtures/activation.contract"
    ));
    let (mut p, error) = Presenter::boot_with(
        &plan,
        Loading::default(),
        (40., 40.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
    assert!(p.module_pending());
    p.host.runner().data_ref().finish_on_probe.set(true);
    p.executor.notify();
    assert!(p.pump(0.).is_none());
    assert!(p.module_pending());
    assert!(
        p.executor.take_wake(),
        "completion during the readiness probe must survive wake acknowledgement"
    );
    assert!(p.pump(0.).is_none());
    assert!(!p.module_pending());
    let frame = p.display_frame().unwrap();
    assert_eq!(frame.pixels.pixel(39, 39).unwrap().blue(), 255);
}

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
