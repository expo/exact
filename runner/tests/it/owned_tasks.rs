//! A child's tasks share its state lifetime and run in its current frame.
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{agent, Runner, WaitKinds};

const SOURCE: &str = include_str!("../../../semantics/corpus/timers/owned.contract");

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

fn total<D: exact_runner::DataSource>(r: &Runner<D>) -> f64 {
    match r.slot("total").unwrap() {
        Value::Number(n) => *n,
        v => panic!("{v:?}"),
    }
}

fn act(r: &mut Runner<()>, name: &str) {
    r.act(name, Vec::new()).unwrap();
}

fn open(r: &mut Runner<()>, id: &str) {
    r.act("open", vec![Value::str(id)]).unwrap();
}

#[test]
fn owned_tasks_gate_rekey_and_end_with_each_instance() {
    let mut r = boot(SOURCE);
    assert_eq!(r.timer_due_ms(), None);
    open(&mut r, "a");
    r.advance(10.).unwrap();
    open(&mut r, "b");
    r.advance(20.).unwrap();
    assert_eq!(total(&r), 3.);
    let tree = agent::tree(&r);
    assert!(tree.contains("count-a") && tree.contains("count-b"));
    act(&mut r, "toggle");
    assert_eq!(r.timer_due_ms(), None);
    r.advance(50.).unwrap();
    assert_eq!(total(&r), 3.);
    act(&mut r, "toggle");
    r.advance(55.).unwrap();
    act(&mut r, "rekey");
    r.advance(60.).unwrap();
    assert_eq!(total(&r), 3.);
    r.advance(65.).unwrap();
    assert_eq!(total(&r), 5.);
    act(&mut r, "back");
    r.advance(75.).unwrap();
    assert_eq!(total(&r), 6.);
    act(&mut r, "back");
    assert_eq!(r.timer_due_ms(), None);
    r.advance(95.).unwrap();
    open(&mut r, "a");
    r.advance(105.).unwrap();
    assert_eq!(total(&r), 7.);
    assert!(agent::tree(&r).contains("\"text\":\"1\""));
}

#[test]
fn owned_tasks_keep_native_callback_identity_and_wait_admission() {
    let mut r = boot(SOURCE);
    r.set_live_clock(Some(Box::new(|| 0.)));
    r.use_native_timers(true);
    open(&mut r, "a");
    let old = r.native_timers().pop().unwrap();
    act(&mut r, "back");
    open(&mut r, "b");
    let new = r.native_timers().pop().unwrap();
    assert!(new.index != old.index || new.generation != old.generation);
    assert!(r
        .fire_native_timer(old.index, old.generation, 10.)
        .receipts
        .is_empty());
    assert_eq!(total(&r), 0.);
    assert!(r
        .fire_native_timer(new.index, new.generation, 10.)
        .error
        .is_none());
    assert_eq!(total(&r), 1.);

    let mut r = boot(&SOURCE.replace("every(10, bump)", "after(10, bump, wait=scrollend)"));
    r.set_live_clock(Some(Box::new(|| 0.)));
    r.hold_waits(WaitKinds::ALL);
    open(&mut r, "a");
    assert!(r.one_shot_armed());
    assert_eq!(r.outstanding().one_shots.len(), 1);
    r.advance(20.).unwrap();
    assert_eq!(total(&r), 0.);
    assert!(r.waits().scrollend.held);
    assert!(r
        .release_waits(WaitKinds::SCROLLEND, 20.)
        .advanced
        .error
        .is_none());
    assert_eq!(total(&r), 1.);
    act(&mut r, "back");
    assert!(!r.waits().scrollend.held);
}

#[test]
fn owned_frame_and_change_tasks_run_in_their_instances() {
    let mut r = boot(&SOURCE.replace("every(10, bump)", "every(frame, bump)"));
    open(&mut r, "a");
    open(&mut r, "b");
    assert!(r.wants_frames());
    r.present_frames(true);
    assert!(r.frame(100.).error.is_none());
    assert_eq!(total(&r), 2.);
    act(&mut r, "toggle");
    assert!(!r.wants_frames());
    assert!(r.frame(200.).error.is_none());
    assert_eq!(total(&r), 2.);

    let mut r = boot(&SOURCE.replace("every(10, bump)", "change(bump)"));
    open(&mut r, "a");
    assert_eq!(total(&r), 0.);
    assert!(r.land_then().error.is_none());
    assert_eq!(total(&r), 1.);
    act(&mut r, "rekey");
    act(&mut r, "back");
    assert!(r.land_then().receipts.is_empty());
    assert_eq!(total(&r), 1.);
}

#[test]
fn a_change_that_replaces_its_owner_yields_before_the_replacements_timer() {
    let mut r = boot(
        r#"component App
  state starting = true
  state total = 0
  action replace
    starting = false
  action add
    total = total + 1
  view
    column
      if starting
        Start(replace=replace)
      else
        Clock(add=add)
component Start
  props
    replace: action
  task ready key=0
    change(done)
  action done
    replace()
  view
    text "Starting"
component Clock
  props
    add: action
  task tick mount
    every(10, bump)
  action bump
    add()
  view
    text "Running"
"#,
    );
    let first = r.advance_timed(100.);
    assert!(first.error.is_none());
    assert!(first.needs_apply);
    assert_eq!(first.receipts.len(), 1);
    assert_eq!(first.now_ms, 0.);
    assert_eq!(r.slot("starting"), Some(&Value::Bool(false)));
    assert_eq!(total(&r), 0.);
    assert_eq!(r.timer_due_ms(), Some(10.));

    let remaining = r.advance_timed(100.);
    assert!(remaining.error.is_none());
    assert!(!remaining.needs_apply);
    assert_eq!(remaining.receipts.len(), 10);
    assert_eq!(remaining.now_ms, 100.);
    assert_eq!(total(&r), 10.);
}

#[test]
fn reload_and_checkpoint_start_tasks_over_carried_state() {
    let mut r = boot(SOURCE);
    open(&mut r, "a");
    r.advance(15.).unwrap();
    let plan = contract::compile(SOURCE).unwrap();
    let mut carried = Runner::boot_carrying(
        plan.clone(),
        (),
        Kernel::with_monospace(),
        &r.carry(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(total(&carried), 1.);
    assert_eq!(carried.timer_due_ms(), Some(25.));
    carried.advance(25.).unwrap();
    assert_eq!(total(&carried), 2.);
    let checkpoint = r.document_checkpoint("/counter/a");
    let mut restored = Runner::boot_checkpoint(
        plan,
        (),
        Kernel::with_monospace(),
        &checkpoint,
        Vec::new(),
        Default::default(),
        Default::default(),
        "/counter/a",
    )
    .unwrap();
    assert_eq!(total(&restored), 0.);
    assert_eq!(restored.timer_due_ms(), Some(25.));
    restored.advance(25.).unwrap();
    assert_eq!(total(&restored), 1.);
}

#[test]
fn a_child_outside_regions_runs_with_its_current_props() {
    let mut r = boot(
        r#"component App
  state total = 0
  state step = 1
  action add(n: number)
    total = total + n
  action faster
    step = 3
  view
    Clock(step=step, add=add)
component Clock
  props
    step: number
    add: action
  task tick mount
    every(10, bump)
  action bump
    add(step)
  view
    text "Clock"
"#,
    );
    r.advance(10.).unwrap();
    act(&mut r, "faster");
    r.advance(20.).unwrap();
    assert_eq!(total(&r), 4.);
}

#[test]
fn a_hidden_tabs_task_stops_until_the_tab_is_active_again() {
    let source = r#"routes nav
  tab a "/"
  tab b "/b"
component App
  state total = 0
  action pick(name: string)
    nav = select(nav, name)
  action add
    total = total + 1
  view
    main navigationKey=`${top(nav).id}` navigationBack="back"
      for t in nav.tabs key=t.name
        column role="tabpanel" id=`panel-${t.name}`
          Clock(active=nav.tab == t.name, add=add)
      row role="tablist"
        button press=pick("a") role="tab" aria-controls="panel-a" aria-selected=(nav.tab == "a")
          text "A"
        button press=pick("b") role="tab" aria-controls="panel-b" aria-selected=(nav.tab == "b")
          text "B"
component Clock
  props
    active: bool
    add: action
  state count = 0
  task tick when active
    every(10, bump)
  action bump
    count = count + 1
    add()
  view
    text `${count}`
"#;
    let mut r = boot(source);
    r.advance(10.).unwrap();
    assert_eq!(total(&r), 1.);
    r.act("pick", vec![Value::str("b")]).unwrap();
    r.advance(20.).unwrap();
    assert_eq!(total(&r), 2.);
    r.act("pick", vec![Value::str("a")]).unwrap();
    r.advance(30.).unwrap();
    assert_eq!(total(&r), 3.);
}

#[test]
fn a_new_child_task_refuses_before_publishing_and_can_mount_after_repair() {
    let source = r#"component App
  state shown = false
  state divisor = 0
  action show
    shown = true
  action repair
    divisor = 1
  view
    column
      text `${shown}`
      if shown
        Clock(divisor=divisor)
component Clock
  props
    divisor: number
  state count = 0
  task tick key=(1 / divisor)
    every(10, bump)
  action bump
    count = count + 1
  view
    text `${count}` testId="clock"
"#;
    for fallback in [false, true] {
        let source = if fallback {
            source.replace(
                "Clock(divisor=divisor)",
                "Clock(divisor=divisor)\n      else\n        Clock(divisor=1)",
            )
        } else {
            source.to_owned()
        };
        let mut r = boot(&source);
        r.advance(10.).unwrap();
        let before = agent::tree(&r);
        assert!(matches!(
            r.act("show", vec![]),
            Err(exact_runner::RunnerError::TaskKey { .. })
        ));
        assert_eq!(r.slot("shown"), Some(&Value::Bool(false)));
        assert_eq!(agent::tree(&r), before);
        assert_eq!(r.timer_due_ms(), fallback.then_some(20.));
        r.advance(20.).unwrap();
        if fallback {
            assert!(agent::tree(&r).contains("\"text\":\"2\""));
        }
        act(&mut r, "repair");
        act(&mut r, "show");
        r.advance(30.).unwrap();
        assert!(agent::tree(&r).contains("\"text\":\"1\""));
    }
}

#[test]
fn owned_task_ties_follow_declaration_order_after_reverse_mounts() {
    let source = r#"component App
  state first = false
  state second = false
  state total = 0
  action showFirst
    first = true
  action showSecond
    second = true
  action add(digit: number)
    total = total * 10 + digit
  view
    column
      if first
        Clock(digit=1, add=add)
      if second
        Clock(digit=2, add=add)
component Clock
  props
    digit: number
    add: action
  task tick mount
    every(frame, bump)
  action bump
    add(digit)
  view
    text `${digit}`
"#;
    for frame in [true, false] {
        let source = if frame {
            source.to_owned()
        } else {
            source.replace("every(frame, bump)", "after(10, bump, wait=scrollend)")
        };
        let mut r = boot(&source);
        if frame {
            r.present_frames(true);
        } else {
            r.set_live_clock(Some(Box::new(|| 0.)));
            r.hold_waits(WaitKinds::ALL);
        }
        act(&mut r, "showSecond");
        act(&mut r, "showFirst");
        if frame {
            assert!(r.frame(10.).error.is_none());
        } else {
            for _ in 0..4 {
                let result = r.release_waits(WaitKinds::SCROLLEND, 10.);
                assert!(result.advanced.error.is_none());
                if !result.continuing {
                    break;
                }
            }
        }
        assert_eq!(total(&r), 12., "frame={frame}");
    }
}

#[test]
fn an_owned_answer_cannot_publish_before_its_task_key_is_valid() {
    struct Data;
    impl exact_runner::DataSource for Data {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
            Ok(Value::Number(1.))
        }
    }
    let source = r#"component App
  state shown = false
  action show
    shown = true
  view
    column
      text `${shown}`
      if shown
        Clock()
component Clock
  resource answer = read() as shape number
  task tick key=(1 / (answer - 1))
    every(10, bump)
  action bump
    refresh answer
  view
    text `${answer}`
"#;
    let mut r = Runner::boot(
        contract::compile(source).unwrap(),
        Data,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let before = agent::tree(&r);
    assert!(matches!(
        r.act("show", vec![]),
        Err(exact_runner::RunnerError::TaskKey { .. })
    ));
    assert_eq!(agent::tree(&r), before);
    assert_eq!(r.slot("shown"), Some(&Value::Bool(false)));
}

#[test]
fn a_refused_tab_can_be_realized_after_repair() {
    let mut r = boot(
        r#"routes nav
  tab a "/"
  tab b "/b"
component App
  state divisor = 1
  action pick(name: string)
    nav = select(nav, name)
  action breakKey
    divisor = 0
  action repair
    divisor = 1
  view
    main navigationKey=`${top(nav).id}` navigationBack="back"
      for t in nav.tabs key=t.name
        column role="tabpanel" id=`panel-${t.name}`
          for e in t.stack key=e.id
            column navigationKey=`${e.id}`
              Clock(name=t.name, divisor=t.name == "b" ? divisor : 1)
      row role="tablist"
        button press=pick("a") testId="tab-a" role="tab" aria-controls="panel-a" aria-selected=(nav.tab == "a")
          text "A"
        button press=pick("b") testId="tab-b" role="tab" aria-controls="panel-b" aria-selected=(nav.tab == "b")
          text "B"
      button press=breakKey testId="break"
        text "Break"
      button press=repair testId="repair"
        text "Repair"
      text nav.tab testId="selected"
component Clock
  props
    name: string
    divisor: number
  state count = 0
  task tick key=(1 / divisor)
    every(10, bump)
  action bump
    count = count + 1
  view
    text `${count}` testId=`clock-${name}`
"#,
    );
    act(&mut r, "breakKey");
    let before = agent::tree(&r);
    assert!(matches!(
        r.act("pick", vec![Value::str("b")]),
        Err(exact_runner::RunnerError::TaskKey { .. })
    ));
    assert_eq!(agent::tree(&r), before);
    act(&mut r, "repair");
    r.act("pick", vec![Value::str("b")]).unwrap();
    r.advance(10.).unwrap();
    assert!(agent::tree(&r).contains("clock-b"));
    assert_eq!(r.tasks().iter().filter(|(_, due)| due.is_some()).count(), 2);
}

#[test]
fn destroying_a_task_owner_does_not_evaluate_its_key() {
    let mut r = boot(
        r#"component App
  state shown = true
  state divisor = 1
  action hide
    shown = false
    divisor = 0
  view
    column
      if shown
        Clock(divisor=divisor)
component Clock
  props
    divisor: number
  state count = 0
  task tick key=(1 / divisor)
    every(10, bump)
  action bump
    count = count + 1
  view
    text `${count}` testId="clock"
"#,
    );
    r.advance(10.).unwrap();
    act(&mut r, "hide");
    assert_eq!(r.slot("shown"), Some(&Value::Bool(false)));
    assert!(!agent::tree(&r).contains("\"testId\":\"clock\""));
    assert_eq!(r.timer_due_ms(), None);
}

#[test]
fn owned_task_keys_read_settled_resources_on_mount_and_boot() {
    struct Data;
    impl exact_runner::DataSource for Data {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
            Ok(Value::Number(2.))
        }
    }
    let source = r#"component App
  state shown = false
  action show
    shown = true
  view
    column
      if shown
        Clock()
component Clock
  resource n = read() as shape number
  state count = 0
  task tick key=(1 / n)
    every(10, bump)
  action bump
    count = count + 1
  view
    text `${n}:${count}` testId="clock"
"#;
    for shown in [false, true] {
        let source = source.replace("shown = false", &format!("shown = {shown}"));
        let mut r = Runner::boot(
            contract::compile(&source).unwrap(),
            Data,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        r.act("show", vec![]).unwrap();
        r.advance(10.).unwrap();
        assert!(agent::tree(&r).contains("2:1"));
    }
}

#[test]
fn refused_tab_preparation_rolls_back_requests_and_tasks() {
    struct Data;
    impl exact_runner::DataSource for Data {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
            unreachable!()
        }
        fn answer(
            &mut self,
            _: &mut exact_runner::Store,
            _: &str,
            _: &[Value],
        ) -> Result<exact_runner::Answer, exact_runner::DataError> {
            Ok(exact_runner::Answer::Later(exact_runner::Request::get(
                "https://tasks.test/read",
            )))
        }
    }
    let source = r#"routes nav
  tab a "/"
  tab b "/b"
component App
  state divisor = 0
  resource root = read() as shape number
  task rootTick mount
    every(20, noop)
  action pick(name: string)
    nav = select(nav, name)
  action noop
    divisor = divisor
  action repair
    divisor = 1
  view
    main navigationKey=`${top(nav).id}` navigationBack="back"
      for t in nav.tabs key=t.name
        column role="tabpanel" id=`panel-${t.name}`
          for e in t.stack key=e.id
            column navigationKey=`${e.id}`
              if t.name == "b"
                Clock(divisor=divisor)
      row role="tablist"
        button press=pick("a") role="tab" aria-controls="panel-a" aria-selected=(nav.tab == "a")
          text "A"
        button press=pick("b") role="tab" aria-controls="panel-b" aria-selected=(nav.tab == "b")
          text "B"
component Clock
  props
    divisor: number
  resource answer = read() as shape number
  task tick key=(1 / divisor)
    every(10, bump)
  action bump
    refresh answer
  view
    text `${answer}`
"#;
    for every in [false, true] {
        let mut r = Runner::boot(
            contract::compile(source).unwrap(),
            Data,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let before = agent::tree(&r);
        let panels = r.unopened_tabs().to_vec();
        assert_eq!(panels.len(), 1);
        let result = if every {
            r.build_every_tab()
        } else {
            r.build_tabs(&panels)
        };
        assert!(matches!(
            result,
            Err(exact_runner::RunnerError::TaskKey { .. })
        ));
        assert_eq!(agent::tree(&r), before);
        assert_eq!(r.unopened_tabs(), panels);
        let requests = r.take_requests();
        assert_eq!(requests.len(), 1, "a refused tab leaked its request");
        assert_eq!(requests[0].target, "root");
        assert_eq!(r.timer_due_ms(), Some(20.));
        r.act("repair", vec![]).unwrap();
        assert_eq!(r.unopened_tabs(), panels);
        assert!(r.take_requests().is_empty());
        if every {
            r.build_every_tab().unwrap();
        } else {
            r.build_tabs(&panels).unwrap();
        }
        assert!(r.unopened_tabs().is_empty());
        assert_eq!(r.take_requests().len(), 1);
        assert_eq!(r.timer_due_ms(), Some(10.));
    }
}

#[test]
fn owned_task_ties_follow_instance_creation_order() {
    let source = include_str!("../../../semantics/corpus/timers/owned-order.contract");
    for task in [
        "every(10, bump)",
        "every(frame, bump)",
        "after(10, bump, wait=scrollend)",
    ] {
        for change in ["toggle", "gate", "rekey"] {
            let mut r = boot(&source.replace("every(10, bump)", task));
            r.act("pick", vec![Value::str("b")]).unwrap();
            act(&mut r, change);
            if change != "rekey" {
                act(&mut r, change);
            }
            if task.contains("frame") {
                r.present_frames(true);
                assert!(r.frame(10.).error.is_none());
            } else if task.contains("wait") {
                r.set_live_clock(Some(Box::new(|| 0.)));
                r.hold_waits(WaitKinds::ALL);
                for _ in 0..4 {
                    let result = r.release_waits(WaitKinds::SCROLLEND, 10.);
                    assert!(result.advanced.error.is_none());
                    if !result.continuing {
                        break;
                    }
                }
            } else {
                r.advance(10.).unwrap();
            }
            assert_eq!(
                total(&r),
                if change == "toggle" { 21. } else { 12. },
                "{task}: {change}"
            );
        }
    }
}

#[test]
fn a_refused_replacement_keeps_its_owners_held_reply_and_queue() {
    struct Data;
    impl exact_runner::DataSource for Data {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
            unreachable!()
        }
        fn answer(
            &mut self,
            _: &mut exact_runner::Store,
            _: &str,
            _: &[Value],
        ) -> Result<exact_runner::Answer, exact_runner::DataError> {
            let mut request = exact_runner::Request::get("https://tasks.test/save");
            request.method = "POST".into();
            Ok(exact_runner::Answer::Later(request))
        }
        fn parse(
            &mut self,
            _: &mut exact_runner::Store,
            _: &str,
            _: &[Value],
            _: exact_runner::Outcome,
        ) -> Result<exact_runner::Answer, exact_runner::DataError> {
            Ok(exact_runner::Answer::Now(Value::Number(7.)))
        }
    }
    let source = r#"component App
  state replacing = false
  state divisor = 0
  state total = 0
  action replace
    replacing = true
  action add
    total = total + 1
  view
    column
      if replacing
        Clock(divisor=divisor)
      else
        Editor(add=add)
component Editor
  props
    add: action
  mutation saved as shape number queue then done
  action save
    send saved = save()
  action done
    add()
  view
    button press=save testId="save"
      text (pending(saved) ? "pending" : "ready")
component Clock
  props
    divisor: number
  state count = 0
  task tick key=(1 / divisor)
    every(10, noop)
  action noop
    count = count + 1
  view
    text "Clock"
"#;
    let mut r = Runner::boot(
        contract::compile(source).unwrap(),
        Data,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let found: serde_json::Value = serde_json::from_str(&agent::handle(
        &r,
        r#"{"op":"tree","target":"save","shallow":true}"#,
    ))
    .unwrap();
    let id = found["nodes"][0]["id"].as_u64().unwrap() as u32;
    r.dispatch(id, exact_runner::Event::Press).unwrap();
    let held = r.take_requests().pop().unwrap();
    r.dispatch(id, exact_runner::Event::Press).unwrap();
    let before = agent::tree(&r);
    assert!(matches!(
        r.act("replace", vec![]),
        Err(exact_runner::RunnerError::TaskKey { .. })
    ));
    assert_eq!(agent::tree(&r), before);
    r.fulfill(held.ticket, exact_runner::Outcome::Storage(vec![]))
        .unwrap();
    assert!(r.land_then().error.is_none());
    assert_eq!(total(&r), 1., "the surviving owner's then must run");
    let queued = r.take_requests();
    assert_eq!(queued.len(), 1, "the surviving owner's queue must drain");
    r.fulfill(queued[0].ticket, exact_runner::Outcome::Storage(vec![]))
        .unwrap();
    assert!(r.land_then().error.is_none());
    assert_eq!(total(&r), 2.);
    assert!(!agent::tree(&r).contains("pending"));
}
