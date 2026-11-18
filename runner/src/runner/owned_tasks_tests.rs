//! A refused reconcile preserves the owners, their work and their continuations.
use super::*;
use crate::{Answer, InFlight, Outcome, Request, Store};
use std::collections::BTreeMap;

#[derive(Default)]
struct Data {
    serial: u64,
    live: Vec<u64>,
    detector: Option<u64>,
}

impl DataSource for Data {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!()
    }
    fn answer(&mut self, _: &mut Store, source: &str, _: &[Value]) -> Result<Answer, DataError> {
        if source == "immediate" {
            return Ok(Answer::Now(Value::Number(1.)));
        }
        self.serial += 1;
        self.live.push(self.serial);
        if source == "detect" {
            self.detector = Some(self.serial);
        }
        let mut request = Request::continuation(self.serial);
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
      Unsent(replace=replace)
component Unsent
  props
    replace: action
  mutation saved as shape number
  action save
    send saved = held()
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
                "first-send" => press(&mut r, "first-send"),
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
