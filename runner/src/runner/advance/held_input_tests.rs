//! The held input queue has one owner: the loop that services an input runs
//! the work due by its arrival, a wake's expiry included, and then the input,
//! once; an input at NaN, which no cutoff passes, is refused, not held.
use crate::{BackgroundState, DataError, DataSource, Event, InputCommit, Runner, Value, WakeHow};
use exact_kernel::{Kernel, NodeType};
use exact_plan::{asm::Asm, builder::PlanBuilder, EventKind, Opcode};
use std::sync::{Arc, Mutex};

/// Storage that never drains, so a wake whose invocations have ended stays
/// current until its deadline
struct Undrained;

impl DataSource for Undrained {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn background_state(&self) -> Option<BackgroundState> {
        Some(BackgroundState {
            queued: 1,
            ..Default::default()
        })
    }
}

/// A root view whose press runs an empty action, and a wake event with a
/// 100 ms budget whose action is empty too
fn boot() -> Runner<Undrained> {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let mut nothing = Asm::new();
    nothing.simple(Opcode::Unit).simple(Opcode::Return);
    let nothing = b.code(nothing);
    let pressed = b.action("pressed", &[], &[], nothing);
    let woke = b.action("woke", &[], &[], nothing);
    b.module_event(
        ("geofencing", "@fixture/geofencing"),
        "geofenceenter",
        woke,
        "wake",
        16,
        100,
    );
    b.node(
        NodeType::View as u8,
        None,
        None,
        0,
        &[],
        &[(EventKind::Press, pressed, &[])],
        None,
    );
    Runner::boot(
        b.finish().unwrap(),
        Undrained,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

type Told = Arc<Mutex<Vec<InputCommit>>>;

fn press(r: &mut Runner<Undrained>, at: f64, told: &Told) -> crate::Advanced {
    let root = r.roots()[0];
    let told = told.clone();
    r.dispatch_at_with_receipt(root, Event::Press, at, move |committed| {
        told.lock().unwrap().push(committed)
    })
}

#[test]
fn a_press_past_a_wakes_deadline_runs_once_after_the_expiry() {
    let mut r = boot();
    r.raise_wake(
        "geofencing",
        "geofenceenter",
        "[]",
        WakeHow {
            id: 1,
            cold: false,
            hidden: false,
        },
    )
    .unwrap();
    assert!(r.step_invocations().1.is_none());
    let deadline = r.wake_deadline().expect("the wake runs until its deadline");
    let told = Told::default();
    let at = deadline + 1.;
    let a = press(&mut r, at, &told);
    assert!(a.error.is_none(), "{:?}", a.error);
    let committed = InputCommit::Committed { opened: None };
    assert_eq!(*told.lock().unwrap(), [committed], "told once");
    assert_eq!(r.wake_deadline(), None, "the wake expired");
    let press_commit = a.receipts.last().expect("the press's commit");
    assert_eq!(press_commit.at_ms, at);
}

#[test]
fn a_press_at_nan_is_refused_not_held() {
    let mut r = boot();
    let told = Told::default();
    let a = press(&mut r, f64::NAN, &told);
    assert!(
        matches!(a.error, Some(crate::RunnerError::NonFiniteClock)),
        "{:?}",
        a.error
    );
    assert!(a.receipts.is_empty());
    assert_eq!(
        std::mem::take(&mut *told.lock().unwrap()),
        [InputCommit::Refused]
    );
    // Nothing waits behind it
    let at = r.now_ms() + 1.;
    let a = press(&mut r, at, &told);
    assert!(a.error.is_none(), "{:?}", a.error);
    assert_eq!(
        *told.lock().unwrap(),
        [InputCommit::Committed { opened: None }]
    );
    assert_eq!(a.receipts.len(), 1);
}
