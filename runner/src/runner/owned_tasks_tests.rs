//! A refused reconcile preserves the owners, their work and their continuations.
use super::*;
use crate::{Answer, InFlight, Outcome, Request, Store};
use std::collections::BTreeMap;

#[derive(Default)]
struct Data {
    serial: u64,
    live: Vec<u64>,
    detector: Option<u64>,
    mirrors: Vec<Value>,
}

impl DataSource for Data {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!()
    }
    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        if source == "mirror" {
            self.mirrors.push(args[0].clone());
            return Ok(Answer::Now(args[0].clone()));
        }
        if source == "immediate" {
            return Ok(Answer::Now(Value::Number(1.)));
        }
        self.serial += 1;
        self.live.push(self.serial);
        if source == "detect" {
            self.detector = Some(self.serial);
        }
        let mut request = Request::continuation::<crate::host_work::TestWork>(self.serial);
        request.fetch = Some(self.serial);
        Ok(Answer::Later(request))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        source: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        assert_eq!(source, "detect");
        let token = self.detector.take().unwrap();
        self.live.retain(|t| *t != token);
        Ok(Answer::Now(Value::Number(7.)))
    }
    fn discard(&mut self, token: u64) {
        self.live.retain(|t| *t != token);
    }
    fn forgotten(&mut self, _: &Store, active: &[InFlight<'_>]) {
        self.live
            .retain(|t| active.iter().any(|p| p.continuation == Some(*t)));
    }
}

const SOURCE: &str = r#"routes nav
  tab a "/"
  tab b "/b"
component App
  state replacing = false
  state total = 0
  action pick(name: string)
    nav = select(nav, name)
  action replace
    replacing = true
  action argument(n: number)
    total = n
  action add
    total = total + 1
  view
    main navigationKey=`${top(nav).id}` navigationBack="back"
      for t in nav.tabs key=t.name
        column role="tabpanel" id=`panel-${t.name}`
          for e in t.stack key=e.id
            column navigationKey=`${e.id}`
              if t.name == "a" && !replacing
                Editor(add=add)
              else
                Candidate()
      row role="tablist"
        button press=pick("a") role="tab" aria-controls="panel-a" aria-selected=(nav.tab == "a")
          text "A"
        button press=pick("b") role="tab" aria-controls="panel-b" aria-selected=(nav.tab == "b")
          text "B"
      if !replacing
        Unsent(replace=replace)
      text (replacing ? "attempted" : "committed")
component Unsent
  props
    replace: action
  mutation firstSaved as shape number
  action save
    send firstSaved = held()
    replace()
  view
    button press=save testId="first-send"
      text "Save and replace"
component Editor
  props
    add: action
  resource answer = held() as shape number
  mutation saved as shape number queue refreshes answer then done
  mutation detected as shape number then done
  task tick mount
    every(20, done)
  action save
    send saved = held()
  action detect
    send detected = detect()
  action done
    add()
  view
    column
      button press=save testId="save"
        text (pending(saved) ? "pending" : "ready")
      button press=detect testId="detect"
        text `${answer}`
component Candidate
  resource answer = immediate() as shape number
  resource waiting = dependent(answer) when true as shape number
  state count = 0
  task tick when true key=(1 / (answer - 1))
    every(10, bump)
  action bump
    count = count + 1
  view
    text `${count}:${waiting}`
"#;

fn press(r: &mut Runner<Data>, target: &str) -> Result<CommitReceipt, RunnerError> {
    let tree: serde_json::Value = serde_json::from_str(&crate::agent::handle(
        r,
        &format!(r#"{{"op":"tree","target":"{target}","shallow":true}}"#),
    ))
    .unwrap();
    r.dispatch(
        tree["nodes"][0]["id"].as_u64().unwrap() as u32,
        crate::Event::Press,
    )
}

struct Ownership {
    fields: BTreeMap<&'static str, String>,
    timers: Vec<Timer>,
    requests: Vec<crate::RequestOut>,
}

fn ownership(r: &Runner<Data>) -> Ownership {
    let mut fields = BTreeMap::new();
    macro_rules! keep {
        ($($field:ident),* $(,)?) => {$(fields.insert(stringify!($field), format!("{:?}", r.$field));)*};
    }
    keep!(
        tree,
        pending,
        pending_mut,
        failed_mut,
        owned_sends,
        then_due,
        landed,
        unsent,
        refresh_next,
        reconciling,
        retired_fetches,
        row_writes,
        renewed,
        shown,
        deferred_edges,
        held_edges
    );
    fields.insert("ids", format!("{:?}", r.ids.checkpoint()));
    fields.insert("queues", format!("{:?}", r.queues.save()));
    fields.insert("writes", format!("{:?}", r.writes.records));
    fields.insert("letting", format!("{:?}", r.letting_len()));
    fields.insert("continuations", format!("{:?}", r.data.live));
    fields.insert(
        "timer-completions",
        format!(
            "{:?}",
            r.timers.iter().map(|t| &t.completion).collect::<Vec<_>>()
        ),
    );
    fields.insert(
        "resources",
        format!(
            "{:?}",
            r.resources
                .iter()
                .map(|row| (
                    &row.owned,
                    &row.state,
                    &row.value,
                    row.pending,
                    row.current,
                    &row.failed_args,
                    &row.failed_why,
                    row.stale,
                    row.awaiting,
                ))
                .collect::<Vec<_>>()
        ),
    );
    Ownership {
        fields,
        timers: r.timers.clone(),
        requests: r.requests.clone(),
    }
}

#[test]
fn every_reconcile_refusal_restores_ownership() {
    let plan = contract::compile(SOURCE).unwrap();
    let first_send_owner = plan
        .mutations
        .iter()
        .find(|m| crate::agent::declared_name(plan.str(m.name)) == "firstSaved")
        .unwrap()
        .owner
        .expect("first-send mutation has an owner");
    let first_send_region = plan.arm(first_send_owner).region.0;
    for refusal in [
        "key",
        "key-trap",
        "gate-trap",
        "resource-gate",
        "resource-arg",
    ] {
        for entry in ["replace", "first-send", "tab", "all-tabs"] {
            let mut r = Runner::boot(
                plan.clone(),
                Data::default(),
                Kernel::with_monospace(),
                Default::default(),
                "/",
            )
            .unwrap();
            r.set_live_clock(Some(Box::new(|| 0.)));
            r.use_native_timers(true);
            assert_eq!(r.native_timers().len(), 1);
            press(&mut r, "save").unwrap();
            press(&mut r, "save").unwrap();
            press(&mut r, "detect").unwrap();
            let detected = r
                .pending
                .iter()
                .find(|p| r.target_name(p.target) == "detected")
                .unwrap()
                .ticket;
            r.fulfill(detected, Outcome::Storage(vec![])).unwrap();
            assert!(r.then_due.iter().any(|due| due.is_finite()));
            // A parameter-reading body traps when evaluated without action arguments.
            let trap = r
                .plan
                .actions
                .iter()
                .find(|a| r.plan.str(a.name) == "argument")
                .unwrap()
                .body;
            let timer = r.plan.timers.iter().position(|t| t.keyed).unwrap();
            let resource = r
                .plan
                .resources
                .iter()
                .position(|q| crate::agent::declared_name(r.plan.str(q.name)) == "waiting")
                .unwrap();
            match refusal {
                "key-trap" => r.plan.timers[timer].key = trap,
                "gate-trap" => r.plan.timers[timer].gate = trap,
                "resource-gate" => {
                    let arg = r.plan.resources[resource].gate.start as usize;
                    r.plan.args[arg].expr = trap;
                }
                "resource-arg" => {
                    let arg = r.plan.resources[resource].args.start as usize;
                    r.plan.args[arg].expr = trap;
                }
                _ => {}
            }
            let before = ownership(&r);
            let panels = r.unopened_tabs().to_vec();
            let result = match entry {
                "replace" => r.act("replace", vec![]),
                "first-send" => {
                    let kernel = r.kernel();
                    let button = kernel.find_first_by_test_id("first-send").unwrap();
                    let view = kernel.node_by_key(button).unwrap().id;
                    let (_, frames) = r.find(view).unwrap();
                    let storage = Frame::row_of(&frames, first_send_region).unwrap();
                    assert!(storage.borrow().sends.is_empty());
                    let result = r.dispatch(view, crate::Event::Press);
                    assert!(
                        storage.borrow().sends.is_empty(),
                        "{refusal}: {entry}: instance sends"
                    );
                    result
                }
                "tab" => r.build_tabs(&panels),
                _ => r.build_every_tab(),
            };
            assert!(result.is_err(), "{refusal}: {entry}");
            assert!(!r.is_poisoned(), "{refusal}: {entry}: {result:?}");
            let after = ownership(&r);
            for (field, value) in &before.fields {
                assert_eq!(
                    after.fields.get(field),
                    Some(value),
                    "{refusal}: {entry}: {field}"
                );
            }
            assert!(
                before.timers == after.timers,
                "{refusal}: {entry}: scheduler"
            );
            assert!(
                before.requests == after.requests,
                "{refusal}: {entry}: outgoing requests"
            );
        }
    }
}

#[test]
fn a_sleeping_owned_resource_records_arguments_without_resettling() {
    let plan = contract::compile(
        r#"component App
  state show = true
  state number = 1
  view
    column
      if show
        Sleeping(number=number)
component Sleeping
  props
    number: number
  resource answer = mirror(number) when false as shape number
  view
    text `${answer}`
"#,
    )
    .unwrap();
    let number = plan
        .slots
        .iter()
        .position(|slot| plan.str(slot.name) == "number")
        .unwrap();
    let mut r = Runner::boot(
        plan,
        Data::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let owners = r.tree.as_ref().unwrap().owners();
    let row = owners[0].storage.borrow().requests[0].row.unwrap();
    assert!(r.resources[row].state.as_ref().unwrap().asleep);
    assert_eq!(r.owned_row(row).unwrap().want.args, [Value::Number(1.)]);

    r.slots[number] = Value::Number(2.);
    assert!(!r.reconcile_owned(&owners).unwrap());
    assert_eq!(r.owned_row(row).unwrap().want.args, [Value::Number(2.)]);
}

#[test]
fn a_sleeping_owned_resource_uses_changed_arguments_only_after_wake() {
    let plan = contract::compile(
        r#"component App
  state show = true
  state awake = false
  state number = 1
  action put(n: number)
    number = n
  action wake
    awake = true
  view
    column
      if show
        Sleeping(awake=awake, number=number)
component Sleeping
  props
    awake: bool
    number: number
  resource answer = mirror(range(number).length) when awake as shape number
  view
    text `${number}:${answer}:${current(answer)}` testId="answer"
"#,
    )
    .unwrap();
    let source = plan
        .resources
        .iter()
        .find(|resource| plan.str(resource.source) == "mirror")
        .unwrap();
    assert!(source.owner.is_some());
    // The instance's path, by which the boot's instance takes the page's
    // answer (LLP 1035.005.004 D8).
    let rendered = Runner::boot(
        plan.clone(),
        Data::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let path = (0..rendered.resources.len())
        .find_map(|i| rendered.owned_row(i))
        .unwrap()
        .path
        .clone();
    let seed = Checkpoint {
        answers: vec![(
            plan.str(source.name).into(),
            "mirror".into(),
            vec![Value::Number(1.)],
            Value::Number(91.),
        )],
        instances: vec![(0, path)],
        ..Default::default()
    };
    let mut r = Runner::boot_checkpoint(
        plan,
        Data::default(),
        Kernel::with_monospace(),
        &seed,
        vec![],
        Default::default(),
        Default::default(),
        "/",
    )
    .unwrap();
    let text = |r: &Runner<Data>| {
        let kernel = r.kernel();
        let key = kernel.find_by_test_id("answer")[0];
        kernel
            .node_by_key(key)
            .unwrap()
            .props
            .str(exact_kernel::PropId::Text)
            .unwrap()
            .to_owned()
    };
    assert_eq!(text(&r), "1:91:false");
    r.act("put", vec![Value::Number(2.)]).unwrap();
    assert_eq!(text(&r), "2:91:false");

    r.act("put", vec![Value::Number(3.)]).unwrap();
    assert_eq!(text(&r), "3:91:false");

    assert!(r.data.mirrors.is_empty());

    r.act("wake", vec![]).unwrap();
    assert_eq!(r.data.mirrors, [Value::Number(3.)]);
    assert_eq!(text(&r), "3:3:true");
}

#[test]
fn a_refused_render_preserves_a_pinned_row_in_a_nested_collection() {
    let plan = contract::compile(
        r#"component App
  state count = 0
  action change(nextCount: number)
    count = nextCount
  view
    column
      list testId="outer" virtualized=true estimated-item-height=120 height=120
        for groupKey in ["a"] key=groupKey
          column height=120
            list testId="inner" virtualized=true estimated-item-height=20 height=100
              for item in range(count == 0 ? 80 : 81) key=item
                row testId=`item-${item}` height=20
                  text groupKey
      if count != 0
        Candidate(count=count)
component Candidate
  props
    count: number
  state ticks = 0
  task tick when true key=(1 / (count - 1))
    every(10, bump)
  action bump
    ticks = ticks + 1
  view
    text `${ticks}`
"#,
    )
    .unwrap();
    let mut r = Runner::boot(
        plan,
        Data::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert!(r.transactional_render());
    let view = |r: &Runner<Data>, name| {
        let kernel = r.kernel();
        kernel
            .node_by_key(kernel.find_by_test_id(name)[0])
            .unwrap()
            .id
    };
    let report = |r: &mut Runner<Data>, list, offset, port, interaction_view| {
        let previous = r.collection(list).unwrap();
        r.collection_feedback(crate::CollectionFeedback {
            view: list,
            revision: previous.revision,
            scroll_sequence: previous.scroll_sequence + 1,
            offset,
            port_main: port,
            port_cross: 320.,
            cross: 320.,
            padding: [0.; 2],
            measurements: Vec::new(),
            focus_view: None,
            interaction_view,
        })
        .unwrap();
    };

    let outer = view(&r, "outer");
    report(&mut r, outer, 0., 120., None);
    let inner = view(&r, "inner");
    report(&mut r, inner, 0., 100., None);
    let pinned = view(&r, "item-0");
    report(&mut r, inner, 600., 100., Some(pinned));
    let before = r.collection(inner).unwrap();
    assert_eq!(before.count, 80);
    assert!(before.rows.iter().any(|row| row.root == pinned));
    assert!(before.rows.iter().any(|row| row.index > 20));

    assert!(r.act("change", vec![Value::Number(1.)]).is_err());
    assert!(!r.is_poisoned());
    assert_eq!(r.collection(inner).as_ref(), Some(&before));
    assert_eq!(view(&r, "item-0"), pinned);

    r.act("change", vec![Value::Number(2.)]).unwrap();
    assert_eq!(view(&r, "inner"), inner);
    let after = r.collection(inner).unwrap();
    assert_eq!(after.count, 81);
    assert!(after.revision > before.revision);
    assert_eq!(view(&r, "item-0"), pinned);
    report(&mut r, inner, 640., 100., None);
    assert!(r.kernel().node(pinned).is_none());
}
