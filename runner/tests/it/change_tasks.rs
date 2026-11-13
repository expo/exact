use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::Runner;

fn boot(source: &str) -> Runner<()> {
    Runner::boot(
        contract::compile(source).unwrap(),
        (),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

const SOURCE: &str = r#"component App
  state input = 1
  state enabled = true
  state seen = 0
  state fired = 0
  state at = -1
  action apply
    seen = input
    fired = fired + 1
    at = performanceNow()
  action set(value: number)
    input = value
  action enable(value: bool)
    enabled = value
  task sync when enabled key=input
    change(apply)
  view
    text `${seen}`
"#;

#[test]
fn first_active_commit_and_latest_key_run_once_without_advancing_time() {
    let mut r = boot(SOURCE);
    assert_eq!(r.slot("fired"), Some(&Value::Number(0.0)));
    assert_eq!(r.timer_due_ms(), Some(0.0));
    assert_eq!(r.land_then().receipts.len(), 1);
    assert_eq!(r.slot("seen"), Some(&Value::Number(1.0)));
    assert_eq!(r.slot("at"), Some(&Value::Number(0.0)));
    assert_eq!(r.timer_due_ms(), None);
    r.act("set", vec![Value::Number(2.0)]).unwrap();
    r.act("set", vec![Value::Number(3.0)]).unwrap();
    assert_eq!(r.slot("fired"), Some(&Value::Number(1.0)));
    assert_eq!(r.land_then().receipts.len(), 1);
    assert_eq!(r.slot("seen"), Some(&Value::Number(3.0)));
    assert_eq!(r.slot("fired"), Some(&Value::Number(2.0)));
    r.act("set", vec![Value::Number(3.0)]).unwrap();
    assert!(r.land_then().receipts.is_empty());
    assert_eq!(r.now_ms(), 0.0);
}

#[test]
fn closed_gate_discards_pending_work_and_reactivation_runs_the_current_key() {
    let mut r = boot(SOURCE);
    r.act("enable", vec![Value::Bool(false)]).unwrap();
    assert!(r.land_then().receipts.is_empty());
    r.act("set", vec![Value::Number(9.0)]).unwrap();
    assert!(r.advance(10000.0).unwrap().is_empty());
    r.act("enable", vec![Value::Bool(true)]).unwrap();
    assert_eq!(r.slot("seen"), Some(&Value::Number(0.0)));
    r.land_then();
    assert_eq!(r.slot("seen"), Some(&Value::Number(9.0)));
    assert_eq!(r.slot("at"), Some(&Value::Number(10000.0)));
    assert_eq!(r.timer_due_ms(), None);
}

#[test]
fn refused_keys_and_actions_restore_state_without_requeuing_the_firing() {
    let mut r = boot(&SOURCE.replace("key=input", "key=1 / input"));
    r.land_then();
    assert!(r.act("set", vec![Value::Number(0.0)]).is_err());
    assert_eq!(r.slot("input"), Some(&Value::Number(1.0)));
    assert!(r.land_then().receipts.is_empty());
    let mut r = boot(&SOURCE.replace(
        "  view\n",
        "  task valid key=1 / (input - fired)\n    after(100, apply)\n  view\n",
    ));
    assert!(r.land_then().error.is_some());
    assert_eq!(r.slot("fired"), Some(&Value::Number(0.0)));
    assert_eq!(r.tasks()[0].1, None);
    r.act("set", vec![Value::Number(2.0)]).unwrap();
    assert!(r.land_then().error.is_none());
    assert_eq!(r.slot("seen"), Some(&Value::Number(2.0)));
}

#[test]
fn each_change_yields_its_receipt_before_the_next_action() {
    let source = SOURCE.replace("  view\n", "  state copied = -1\n  action copy\n    copied = seen\n  task following key=seen\n    change(copy)\n  task later mount\n    after(5, apply)\n  view\n");
    let mut r = boot(&source);
    let first = r.advance_timed(10.0);
    assert_eq!(first.receipts.len(), 1);
    assert!(first.needs_apply);
    assert_eq!(first.now_ms, 0.0);
    assert_eq!(r.slot("copied"), Some(&Value::Number(-1.0)));
    let second = r.advance_timed(10.0);
    assert_eq!(second.receipts.len(), 1);
    assert!(second.needs_apply);
    assert_eq!(r.slot("copied"), Some(&Value::Number(1.0)));
    let last = r.advance_timed(10.0);
    assert_eq!(last.receipts.len(), 1);
    assert_eq!(last.now_ms, 10.0);
    assert_eq!(r.slot("at"), Some(&Value::Number(5.0)));
}

#[test]
fn ordinary_timer_receipts_precede_the_change_they_queue() {
    let source = SOURCE.replace("enabled = true", "enabled = false").replace(
        "  view\n",
        "  action start\n    enabled = true\n  task later mount\n    after(5, start)\n  view\n",
    );
    let mut r = boot(&source);
    let first = r.advance_timed(20.0);
    assert_eq!(first.receipts.len(), 1);
    assert!(first.needs_apply);
    assert_eq!(first.now_ms, 5.0);
    assert_eq!(r.slot("fired"), Some(&Value::Number(0.0)));
    let second = r.advance_timed(20.0);
    assert_eq!(second.receipts.len(), 1);
    assert!(second.needs_apply);
    assert_eq!(second.now_ms, 5.0);
    assert_eq!(r.slot("at"), Some(&Value::Number(5.0)));
    assert_eq!(r.advance_timed(20.0).now_ms, 20.0);
}

#[test]
fn live_delivery_keeps_changes_out_of_native_timers() {
    let mut r = boot(SOURCE);
    r.set_live_clock(Some(Box::new(|| 100.0)));
    r.use_native_timers(true);
    assert!(r.native_timers().is_empty());
    assert_eq!(r.land_then_first().receipts.len(), 1);
    assert_eq!(r.slot("fired"), Some(&Value::Number(1.0)));
    assert_eq!(r.timer_due_ms(), None);
}

#[test]
fn change_tasks_require_keys_and_reject_direct_and_indirect_cycles() {
    for source in [
        SOURCE.replace(" when enabled key=input", " mount"),
        SOURCE.replace("change(apply)", "change(apply, wait=frame)"),
    ] {
        assert!(contract::compile(&source)
            .unwrap_err()
            .to_string()
            .contains("lower-change-task"));
    }
    for source in [
        SOURCE.replace("seen = input", "input = input + 1"),
        SOURCE
            .replace("key=input", "key=derived")
            .replace("  view\n", "  derive derived = input + 1\n  view\n")
            .replace("seen = input", "input = input + 1"),
        SOURCE.replace(
            "  view\n",
            "  action back\n    input = seen + 1\n  task loop key=seen\n    change(back)\n  view\n",
        ),
        SOURCE.replace("key=input", "key=value").replace("seen = input", "input = input + 1").replace("  view\n", "  resource value = read(input) as shape number\n  view\n"),
        SOURCE.replace("key=input", "key=pending(saved)").replace("seen = input", "send saved = save()").replace("  view\n", "  mutation saved as shape number\n  view\n"),
        SOURCE.replace("seen = input", "send saved = save()").replace("  view\n", "  mutation saved as shape number then back\n  action back\n    input = input + 1\n  view\n"),
        SOURCE.replace("seen = input", "mutate()").replace(
            "  view\n",
            "  action mutate\n    input = input + 1\n  view\n",
        ),
    ] {
        assert!(contract::compile(&source)
            .unwrap_err()
            .to_string()
            .contains("analyze-change-cycle"));
    }
}
