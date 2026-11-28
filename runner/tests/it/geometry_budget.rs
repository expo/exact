//! LLP 1123 D6's one budget of geometry corrections per session, kept where the host marks the
//! person's acts (`Runner::mark_inputs`): only an identified independent input renews it, a
//! correction's own work never does, and a comparison the spent budget held is delivered at the
//! next renewal, at the same layout count too.

use exact_kernel::{Kernel, Offer, ViewId};
use exact_plan::Value;
use exact_runner::{DataError, DataSource, Event, Runner};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("queried {name}")
    }
}

/// A plan whose `hero` is `width` wide, reading its own geometry, with `rest` in its root
/// component and `more` in its view.
fn boot(width: &str, rest: &str, more: &str) -> Runner<NoData> {
    let source = format!(
        "shape Rect
  x: number
  y: number
  width: number
  height: number
shape Hero
  available: bool
  generation: number
  frame: option<Rect>
component App
  state grows = 0
  action grow
    grows = grows + 1
{rest}  resource g = exactGeometry(\"hero\") as shape Hero
  derive last = match g.frame {{ case some(f) => f.width, case none => 0 }}
  view
    column
      column id=\"hero\" width={width} height=40
      button \"Grow\" press=grow testId=\"grow\"
{more}"
    );
    let mut r = Runner::boot(
        contract::compile(&source).unwrap(),
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.mark_inputs(true);
    r
}

fn lay_out(r: &mut Runner<NoData>, width: f32) {
    let root = r.roots()[0];
    r.kernel_mut()
        .compute_layout(root, Offer::definite(width, 800.0))
        .unwrap();
}

/// Lays out and delivers: whether the delivery committed.
fn deliver(r: &mut Runner<NoData>) -> bool {
    lay_out(r, 400.0);
    r.refresh_geometry().unwrap().is_some()
}

/// The deliveries that commit in a row, up to `most`.
fn run(r: &mut Runner<NoData>, most: usize) -> usize {
    (0..most).take_while(|_| deliver(r)).count()
}

fn view(r: &Runner<NoData>, test_id: &str) -> ViewId {
    let key = r.kernel().find_by_test_id(test_id)[0];
    r.kernel().node_by_key(key).unwrap().id
}

/// An act's end, through the timed dispatch that orders it behind the act's own input.
fn end_act(r: &mut Runner<NoData>) {
    let (root, now) = (r.roots()[0], r.now_ms());
    let advanced = r.dispatch_at(root, Event::ActEnd, now);
    assert!(advanced.error.is_none() && advanced.receipts.is_empty());
}

#[test]
fn a_loop_holds_at_eight_and_an_acts_end_renews_the_budget() {
    // `hero` is one wider than it last read: the boot's budget allows eight corrections, the
    // ninth is held with its comparison owed, and an act's end delivers it at the same layout
    // count, then allows seven more, through the direct dispatch and the held input alike
    let mut r = boot("(last + 1)", "", "");
    assert_eq!(run(&mut r, 20), 8);
    end_act(&mut r);
    assert!(
        r.refresh_geometry().unwrap().is_some(),
        "the owed comparison"
    );
    assert_eq!(run(&mut r, 20), 7);
    let wrapped = Event::Act(Box::new(Event::ActEnd));
    let (root, now) = (r.roots()[0], r.now_ms());
    r.dispatch_at(root, wrapped, now);
    assert!(
        r.refresh_geometry().unwrap().is_some(),
        "a marked end, the same"
    );
    assert_eq!(run(&mut r, 20), 7);
    assert!(
        r.dispatch(root, Event::ActEnd).is_err(),
        "only the timed dispatch takes an act's end"
    );
}

#[test]
fn layouts_no_input_identifies_spend_the_budget() {
    // `hero` is half the window: twenty widths no input identifies publish eight times, then
    // hold; with an act's end before each, every width publishes
    let mut r = boot("\"50%\"", "", "");
    let published = (0..20)
        .filter(|i| {
            lay_out(&mut r, 300.0 + 10.0 * *i as f32);
            r.refresh_geometry().unwrap().is_some()
        })
        .count();
    assert_eq!(published, 8);
    for i in 0..20 {
        end_act(&mut r);
        lay_out(&mut r, 600.0 + 10.0 * i as f32);
        assert!(r.refresh_geometry().unwrap().is_some(), "width {i}");
    }
}

#[test]
fn a_named_action_renews_the_budget_and_an_unknown_name_does_not() {
    let mut r = boot("(last + 1)", "", "");
    assert_eq!(run(&mut r, 20), 8);
    assert!(r.act("absent", vec![]).is_err());
    assert!(!deliver(&mut r), "no action ran");
    r.act("grow", vec![]).unwrap();
    assert_eq!(run(&mut r, 20), 8);
}

#[test]
fn an_acts_own_dispatch_does_not_renew_but_its_end_does() {
    // `hero` grows with each press: the marked press's own commit moves layout but renews
    // nothing, since the act renews once, at its end
    let mut r = boot("(last + 1 + grows)", "", "");
    assert_eq!(run(&mut r, 20), 8);
    let grow = view(&r, "grow");
    r.dispatch(grow, Event::Act(Box::new(Event::Press)))
        .unwrap();
    assert!(!deliver(&mut r), "the press alone renews nothing");
    end_act(&mut r);
    assert!(deliver(&mut r));
}

#[test]
fn a_timer_the_boot_armed_renews_and_one_a_correction_armed_does_not() {
    // A strip as wide as the ticks moves layout at each tick. A tick the boot's task arms
    // traces to the boot and renews the budget; one a correction's commit arms (`last` past
    // zero is a delivery's) is the correction's work and renews nothing
    for (task, renews) in [("mount", true), ("when last > 0", false)] {
        let rest = format!(
            "  state ticks = 0\n  action bump\n    ticks = ticks + 1\n  task tick {task}\n    after(10, bump)\n"
        );
        let mut r = boot("(last + 1)", &rest, "      column width=(ticks) height=1\n");
        assert_eq!(run(&mut r, 20), 8, "{task}");
        let at = r.now_ms() + 10.0;
        r.advance(at).unwrap();
        assert_eq!(deliver(&mut r), renews, "{task}");
    }
}

#[test]
fn a_chains_resize_events_hold_at_eight() {
    // A `resize` handler commits at each of `hero`'s layouts, as a host delivers resize events
    // before the facts: the handler's commits are no identified input, so the eight hold
    let mut r = boot(
        "(last + 1) resize=fit",
        "  state seen = 0\n  action fit(w: number, h: number, r: DOMRectReadOnly)\n    seen = w\n",
        "",
    );
    let mut commits = 0;
    for _ in 0..20 {
        lay_out(&mut r, 400.0);
        for (view, rect, _) in r.resize_due(0) {
            r.resize_delivered(view, rect);
            r.dispatch(view, Event::Resize(rect)).unwrap();
        }
        lay_out(&mut r, 400.0);
        match r.refresh_geometry().unwrap() {
            Some(_) => commits += 1,
            None => break,
        }
    }
    assert_eq!(commits, 8);
}

#[test]
fn a_standalone_resize_does_not_lend_its_cause_to_a_timer() {
    // A grip resize applies outside any commit, as the host's own act; the boot's timer that
    // follows moves layout under the boot's cause and renews the budget
    let rest = "  state ticks = 0\n  action bump\n    ticks = ticks + 1\n  task tick mount\n    after(10, bump)\n";
    let more = "      column width=(ticks) height=1\n      column resize=\"both\" overflow=\"auto\" width=100 height=10 testId=\"grip\"\n";
    let mut r = boot("(last + 1)", rest, more);
    assert_eq!(run(&mut r, 20), 8);
    let grip = view(&r, "grip");
    assert!(r.resize_box(grip, Some(150.0), None).unwrap().is_some());
    let at = r.now_ms() + 10.0;
    r.advance(at).unwrap();
    assert!(deliver(&mut r), "the boot's timer renews");
}

/// `hero` grows one per delivery; publishing a width of 4 divides by zero until `fix` runs.
const REFUSING: &str =
    "  state offset = 0\n  action fix\n    offset = 100\n  derive bad = 1 / (last - 4 + offset)\n";
const REFUSING_VIEW: &str = "      text `${bad}`\n      button \"Fix\" press=fix testId=\"fix\"\n";

#[test]
fn a_refused_delivery_is_not_charged() {
    let mut r = boot("(last + 1)", REFUSING, REFUSING_VIEW);
    assert_eq!(run(&mut r, 3), 3);
    lay_out(&mut r, 400.0);
    assert!(
        r.refresh_geometry().is_err(),
        "publishing 4 divides by zero"
    );
    let fix = view(&r, "fix");
    r.dispatch(fix, Event::Press).unwrap();
    assert_eq!(run(&mut r, 20), 5, "eight in all; the refusal charged none");
}

#[test]
fn a_refused_delivery_is_compared_again_at_the_next_renewal() {
    let mut r = boot("(last + 1)", REFUSING, REFUSING_VIEW);
    assert_eq!(run(&mut r, 3), 3);
    lay_out(&mut r, 400.0);
    assert!(r.refresh_geometry().is_err());
    assert!(
        r.refresh_geometry().unwrap().is_none(),
        "the same layout owes nothing"
    );
    end_act(&mut r);
    assert!(
        r.refresh_geometry().is_err(),
        "the renewal compares again, at the same layout count"
    );
}

#[test]
fn a_boot_timer_that_moves_no_layout_or_is_refused_renews_nothing() {
    // A tick the boot armed renews only when its own commit stands and invalidates layout:
    // one that writes state no box reads, and one whose commit divides by zero, renew nothing
    for (more, refuses) in [("", false), ("  derive bad = 1 / (ticks - 1)\n", true)] {
        let rest = format!(
            "  state ticks = 0\n  action bump\n    ticks = ticks + 1\n  task tick mount\n    after(10, bump)\n{more}"
        );
        let view = if refuses { "      text `${bad}`\n" } else { "" };
        let mut r = boot("(last + 1)", &rest, view);
        assert_eq!(run(&mut r, 20), 8);
        let at = r.now_ms() + 10.0;
        let advanced = r.advance(at);
        assert_eq!(advanced.is_err(), refuses);
        assert!(!deliver(&mut r), "refuses {refuses}");
    }
}

/// Whether the tick a delayed boot ask's answer arms renews a spent budget, beside a kept
/// reader gated by `gate` (the reader's last ask made untraced by an unmarked revision when
/// `untraced`, its gate closed before readiness when `closed`), the tick armed when `task`.
/// With `later` the boot ask's answer is a reply that lands after the activation, else it
/// answers within the activation's own commit.
fn kept_reader_renews(gate: &str, task: &str, untraced: bool, closed: bool, later: bool) -> bool {
    let source = format!(
        "shape Rect
  x: number
  y: number
  width: number
  height: number
shape Hero
  frame: option<Rect>
component App
  state opened = true
  state adjustment = 0
  action revise
    adjustment = adjustment + 1
  state ticks = 0
  action tick
    ticks = ticks + 1
  task awakened when {task}
    after(10, tick)
  action close
    opened = false
  resource waiting = load(\"waiting\") as shape number
  resource retained = loadKept(\"retained\") with adjustment when {gate} as shape number
  resource g = exactGeometry(\"hero\") as shape Hero
  derive last = match g.frame {{ case some(f) => f.width, case none => 0 }}
  view
    column
      column id=\"hero\" width=(last + 1) height=40
      column width=(waiting + ticks) height=1
      button \"Revise\" press=revise testId=\"revise\"
      button \"Close\" press=close testId=\"close\"
"
    );
    let mut plan = contract::compile(&source).unwrap();
    let retained = plan
        .resources
        .iter()
        .position(|row| plan.str(row.name) == "retained");
    plan.resources[retained.unwrap()].reader = true;
    let mut writer = exact_plan::bytes::Writer::default();
    Value::list(vec![Value::str("retained")]).encode(&mut writer);
    Value::Number(15.).encode(&mut writer);
    let kept = vec![(
        format!("{}retained", exact_runner::Store::KEPT),
        writer.into_vec(),
    )];
    let source = Seeded {
        ready: false,
        later,
    };
    let mut r = Runner::boot_stored(
        plan,
        source,
        Kernel::with_monospace(),
        kept,
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(r.shown_resource("retained"), Some(&Value::Number(15.)));
    for (on, button) in [(untraced, "revise"), (closed, "close")] {
        if on {
            let key = r.kernel().find_by_test_id(button)[0];
            let view = r.kernel().node_by_key(key).unwrap().id;
            r.dispatch(view, Event::Press).unwrap();
        }
    }
    r.data().ready = true;
    r.data_ready().unwrap();
    for out in r.take_requests() {
        let reply = exact_runner::Response {
            status: 200,
            headers: Vec::new(),
            body: b"40".to_vec(),
        };
        r.fulfill(out.ticket, exact_runner::Outcome::Response(reply))
            .unwrap();
    }
    r.mark_inputs(true);
    let deliver = |r: &mut Runner<Seeded>| {
        let root = r.roots()[0];
        r.kernel_mut()
            .compute_layout(root, Offer::definite(400., 800.))
            .unwrap();
        r.refresh_geometry().unwrap().is_some()
    };
    assert_eq!((0..20).take_while(|_| deliver(&mut r)).count(), 8);
    let at = r.now_ms() + 10.0;
    r.advance(at).unwrap();
    assert_eq!(r.slot("ticks"), Some(&Value::Number(1.0)));
    deliver(&mut r)
}

#[test]
fn the_first_activations_own_work_spends() {
    // The boot ask answers within the data source's activation, whose commit has no lineage:
    // the tick its answer arms renews nothing, whatever the kept reader beside it recorded,
    // and whether a gate is open, closed, or changed by that answer within the activation
    let cases = [
        ("opened", "waiting > 0", false, false),
        ("opened", "waiting > 0", true, true),
        ("opened && waiting == 0", "waiting > 0", true, false),
        (
            "opened || waiting > 0",
            "waiting > 0 && retained > 20",
            true,
            true,
        ),
    ];
    for (gate, task, untraced, closed) in cases {
        assert!(
            !kept_reader_renews(gate, task, untraced, closed, false),
            "{gate}"
        );
    }
}

#[test]
fn a_boot_asks_reply_keeps_its_own_lineage_beside_a_kept_reader() {
    // The boot ask's reply lands after the activation, under the lineage of its own ask that
    // waited: its tick renews, whatever the kept reader beside it recorded, open or closed
    for (untraced, closed) in [(false, false), (false, true), (true, false), (true, true)] {
        let renews = kept_reader_renews("opened", "waiting > 0", untraced, closed, true);
        assert!(renews, "untraced {untraced}, closed {closed}");
    }
}

/// A data source that answers once it is ready: the boot's `load` as a reply that lands
/// later when `later`, everything else at once.
struct Seeded {
    ready: bool,
    later: bool,
}

impl DataSource for Seeded {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("queried {source}")
    }

    fn ready(&self) -> bool {
        self.ready
    }

    fn answer(
        &mut self,
        _: &mut exact_runner::Store,
        source: &str,
        _: &[Value],
    ) -> Result<exact_runner::Answer, DataError> {
        Ok(match (source, self.later) {
            ("load", true) => {
                exact_runner::Answer::Later(exact_runner::Request::native(b"{}".to_vec()))
            }
            _ => exact_runner::Answer::Now(Value::Number(40.)),
        })
    }

    fn parse(
        &mut self,
        _: &mut exact_runner::Store,
        _: &str,
        _: &[Value],
        _: exact_runner::Outcome,
    ) -> Result<exact_runner::Answer, DataError> {
        Ok(exact_runner::Answer::Now(Value::Number(40.)))
    }
}
