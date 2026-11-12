use super::*;
#[path = "../pump_fixture.rs"]
mod fixture;
use fixture::{plan, Replies};
use std::sync::atomic::{AtomicUsize, Ordering};

static WAKES: AtomicUsize = AtomicUsize::new(0);
extern "C" fn wake(_: *mut c_void) {
    WAKES.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn ready_replies_share_one_layout_and_batch_with_count_and_time_bounds() {
    for (count, advance_after, first) in [(20, None, 20), (40, None, 32), (10, Some(3), 3)] {
        let mut source = Replies::new();
        source.advance_after = advance_after;
        let clock = source.clock.clone();
        let mut bridge = Bridge::new();
        let mut hooks = Hooks::none();
        hooks.wake = Some(wake);
        bridge.boot(&plan(count), source, hooks, 390., 844.);
        let host = bridge.host.as_ref().unwrap();
        let (seq, layouts) = (host.runner().seq(), host.layout_calls);
        WAKES.store(0, Ordering::SeqCst);
        let len = bridge.pump_with_clock(0., || clock.get());
        let batch: serde_json::Value =
            serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap();
        assert!(batch["error"].is_null(), "{batch}");
        let host = bridge.host.as_ref().unwrap();
        assert_eq!(
            host.runner().data_ref().parsed,
            (0..first).collect::<Vec<_>>()
        );
        assert_eq!(
            host.runner().seq() - seq,
            first as u64,
            "one transaction a reply"
        );
        assert_eq!(
            host.layout_calls - layouts,
            1,
            "one layout in the returned batch"
        );
        if first < count {
            assert!(
                WAKES.load(Ordering::SeqCst) > 0,
                "remaining replies wake again"
            );
            bridge.pump_with_clock(0., || clock.get());
            assert_eq!(
                bridge.host.as_ref().unwrap().runner().data_ref().parsed,
                (0..count).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn a_returned_error_keeps_prior_transactions_and_leaves_later_replies_queued() {
    let source = Replies::new();
    let clock = source.clock.clone();
    let mut bridge = Bridge::new();
    bridge.boot(&fixture::fault_plan(), source, Hooks::none(), 390., 844.);
    let seq = bridge.host.as_ref().unwrap().runner().seq();
    let len = bridge.pump_with_clock(0., || clock.get());
    let batch: serde_json::Value =
        serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap();
    assert!(batch["error"].is_string(), "{batch}");
    let host = bridge.host.as_ref().unwrap();
    assert_eq!(host.runner().data_ref().parsed, [0, 1, 2]);
    assert_eq!(host.runner().seq() - seq, 2);
    let len = bridge.pump_with_clock(0., || clock.get());
    let batch: serde_json::Value =
        serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap();
    assert!(batch["error"].is_null(), "{batch}");
    assert_eq!(
        bridge.host.as_ref().unwrap().runner().data_ref().parsed,
        [0, 1, 2, 3]
    );
}

#[test]
fn a_retained_ordered_refusal_takes_its_turn_between_replies() {
    let mut source = Replies::new();
    source.refuse = Some(1);
    let clock = source.clock.clone();
    let mut bridge = Bridge::new();
    bridge.boot(&plan(4), source, Hooks::none(), 390., 844.);
    assert!(bridge
        .host
        .as_ref()
        .unwrap()
        .runner()
        .data_ref()
        .parsed
        .is_empty());
    bridge.pump_with_clock(0., || clock.get());
    assert_eq!(
        bridge.host.as_ref().unwrap().runner().data_ref().parsed,
        [0, 1]
    );
    bridge.pump_with_clock(0., || clock.get());
    assert_eq!(
        bridge.host.as_ref().unwrap().runner().data_ref().parsed,
        [0, 1, 2, 3]
    );
}
