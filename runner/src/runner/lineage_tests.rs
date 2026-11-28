//! LLP 1123 D6 and LLP 1133.023 D7a: a cause's lineage, which no admission
//! rule reads. The boot, a named action and a person's act the host marked
//! set it, a join keeps it only where both sides have it, and a commit with
//! no cause of its own and untraced data clear it.

use super::Cause;
use crate::request::{Request, Response};
use crate::{Answer, CallOrigin, DataError, DataSource, Event, Outcome, Runner, Store, Value};
use exact_kernel::{Kernel, ViewId};

/// Every send answers later; the source is ready when the test says.
struct Later(bool);

impl DataSource for Later {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("answers through answer")
    }

    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(Request::native(b"{}".to_vec())))
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::Number(0.0)))
    }

    fn ready(&self) -> bool {
        self.0
    }
}

fn boot(source: &str, ready: bool) -> Runner<Later> {
    let plan = contract::compile(source).unwrap();
    Runner::boot(
        plan,
        Later(ready),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn view(r: &Runner<Later>, id: &str) -> ViewId {
    let key = r.kernel().find_by_test_id(id)[0];
    r.kernel().node_by_key(key).unwrap().id
}

fn ok() -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: Vec::new(),
        body: b"{}".to_vec(),
    })
}

/// Answers every request out, and says of each the origin the host is
/// told and whether it had lineage.
fn requests(r: &mut Runner<Later>) -> Vec<(CallOrigin, bool)> {
    let sent = r.take_requests();
    let causes = (sent.iter())
        .map(|s| {
            (
                s.request.reach.origin,
                r.ticket_cause(s.ticket).independent(),
            )
        })
        .collect();
    for s in sent {
        r.fulfill(s.ticket, ok()).unwrap();
    }
    causes
}

const APP: CallOrigin = CallOrigin::App;

const PRESS: &str = "component App\n  state k = 0\n  resource v = load(k) as shape number\n  mutation m as shape number\n  action go\n    k = k + 1\n    send m = save(1)\n  view\n    button \"b\" press=go testId=\"b\"\n";

#[test]
fn lineage_is_not_authority() {
    assert_eq!(Cause::boot(), Cause::person());
    assert_eq!(Cause::person_act(), Cause::person());
    assert!(Cause::person_act().ordinary_app());
    assert!(Cause::boot().independent());
    assert!(!Cause::person().independent());
    assert!(!Cause::none().independent());
    assert!(!Cause::boot().lineage_with(Cause::person()).independent());
    assert!(Cause::boot()
        .lineage_with(Cause::person_act())
        .independent());
    let r = boot(PRESS, true);
    assert!(r
        .join_causes(Cause::boot(), Cause::person_act())
        .independent());
    assert!(!r.join_causes(Cause::boot(), Cause::person()).independent());
    assert!(!r
        .join_causes(Cause::person_act(), Cause::device())
        .independent());
}

/// What a marked act's commit asks and sends keeps its lineage; the same
/// press unmarked, as a script's or a correction's, does not.
#[test]
fn a_marked_acts_requests_keep_its_lineage() {
    let mut r = boot(PRESS, true);
    assert_eq!(requests(&mut r), [(APP, true)], "the boot's ask");
    let b = view(&r, "b");
    r.dispatch(b, Event::Act(Box::new(Event::Press))).unwrap();
    assert_eq!(requests(&mut r), [(APP, true); 2]);
    r.dispatch(b, Event::Press).unwrap();
    assert_eq!(requests(&mut r), [(APP, false); 2]);
    // A wrapper inside a wrapper is the same act.
    let twice = Event::Act(Box::new(Event::Act(Box::new(Event::Press))));
    r.dispatch(b, twice).unwrap();
    assert_eq!(requests(&mut r), [(APP, true); 2]);
    r.act("go", vec![]).unwrap();
    assert_eq!(requests(&mut r), [(APP, true); 2], "a named action");
}

/// A timer keeps the lineage of the commit that armed it.
#[test]
fn a_timer_a_marked_act_arms_keeps_its_lineage() {
    const ARMED: &str = "component App\n  state on = false\n  mutation m as shape number\n  action go\n    on = true\n  action fire\n    send m = save(1)\n  task armed when on\n    after(1, fire)\n  view\n    button \"b\" press=go testId=\"b\"\n";
    for marked in [true, false] {
        let mut r = boot(ARMED, true);
        let b = view(&r, "b");
        let press = match marked {
            true => Event::Act(Box::new(Event::Press)),
            false => Event::Press,
        };
        r.dispatch(b, press).unwrap();
        let at = r.now_ms() + 1.0;
        r.advance(at).unwrap();
        assert_eq!(requests(&mut r), [(APP, marked)]);
    }
}

/// The device's re-ask in a commit with no cause of its own keeps the last
/// ask's authority and clears its lineage.
#[test]
fn a_commit_with_no_cause_of_its_own_clears_lineage() {
    let mut r = boot(PRESS, true);
    requests(&mut r);
    let b = view(&r, "b");
    r.dispatch(b, Event::Act(Box::new(Event::Press))).unwrap();
    assert_eq!(requests(&mut r), [(APP, true); 2]);
    r.reasking(|r| r.commit_again(vec![0], "changed")).unwrap();
    assert_eq!(
        requests(&mut r),
        [(APP, false)],
        "the ask's authority, no lineage"
    );
}

/// A data-code call cannot be traced, so it has no lineage.
#[test]
fn untraced_data_has_no_lineage() {
    let mut r = boot(PRESS, true);
    let cause = r.as_cause(Cause::person_act(), |r| r.data_cause());
    assert!(!cause.independent());
}

/// A resource the boot asked before the data source was ready keeps the
/// boot's lineage at the source's first activation, through the cause its
/// waiting ask recorded at the boot, unless a peer's change to the store
/// joins it. An unready call asks nothing.
#[test]
fn the_boots_asks_keep_its_lineage_at_the_first_activation() {
    const LATE: &str =
        "component App\n  resource level = load(1) as shape number\n  view\n    text \"x\"\n";
    for peer in [false, true] {
        let mut r = boot(LATE, false);
        assert!(r.take_requests().is_empty(), "the ask waits");
        assert!(
            r.data_ready().unwrap().is_none(),
            "an unready call asks nothing"
        );
        r.data().0 = true;
        let change = crate::StoreChange {
            any: peer,
            read: false,
        };
        r.data_ready_after_store(change).unwrap();
        // A peer's change is no one's, as on main: the device's, without lineage.
        let origin = if peer { CallOrigin::Device } else { APP };
        assert_eq!(requests(&mut r), [(origin, !peer)], "peer {peer}");
    }
}

/// A kept module source (`"kept": true`) whose answer the boot shows from
/// the last session, its source not ready until the test says.
fn boot_kept(ready: bool) -> Runner<Later> {
    use exact_plan::{builder::PlanBuilder, TypeKind};
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let source = b.source("feeds", &[string], string);
    b.module(
        source,
        ("feeds", "feeds"),
        "feeds",
        false,
        true,
        "",
        "",
        "{\"kept\":true}",
    );
    let key = b.constant(&Value::str(""));
    b.resource("feeds", "feeds", &[key], string, None);
    b.node(
        exact_kernel::NodeType::View as u8,
        None,
        None,
        0,
        &[],
        &[],
        None,
    );
    let kept = (
        super::super::kept::kept_name("feeds"),
        super::super::kept::encode(&[Value::str("")], &Value::str("kept")),
    );
    let plan = b.finish().unwrap();
    Runner::boot_stored(
        plan,
        Later(ready),
        Kernel::with_monospace(),
        vec![kept],
        Default::default(),
        "/",
    )
    .unwrap()
}

/// A kept answer the boot shows records no ask, so the source's first
/// activation asks it with the boot's lineage through the fallback, and the
/// row leaves the boot's set once that ask stands.
#[test]
fn a_kept_answer_the_boot_showed_keeps_the_boots_lineage() {
    let mut r = boot_kept(false);
    assert!(
        r.origins.boot_asked.contains(&0),
        "{:?}",
        r.journal().collect::<Vec<_>>()
    );
    assert!(r.data_ready().unwrap().is_none());
    assert!(
        r.origins.boot_asked.contains(&0),
        "an unready call keeps it"
    );
    r.data().0 = true;
    r.data_ready().unwrap();
    let asked = r.asked_cause(0).expect("the activation asked it");
    assert!(asked.independent() && asked == Cause::person());
    assert!(!r.origins.boot_asked.contains(&0));
}

/// A source whose queue merges a waiting send into the one that replaces it,
/// and whose answers count up, so a reply changes what reads it.
struct Counting(f64);

impl DataSource for Counting {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("answers through answer")
    }

    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(Request::native(b"{}".to_vec())))
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        self.0 += 1.0;
        Ok(Answer::Now(Value::Number(self.0)))
    }

    fn queued_send_replacer(&mut self, _: Option<&str>) -> Option<crate::QueuedSendReplacer> {
        Some(|_, _, newer| Some(newer.to_vec()))
    }
}

fn boot_counting(source: &str) -> Runner<Counting> {
    let plan = contract::compile(source).unwrap();
    Runner::boot(
        plan,
        Counting(0.0),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

/// Each request out, by its target's name, with whether it had lineage.
fn named<D: DataSource>(r: &mut Runner<D>) -> Vec<(String, bool, u64)> {
    (r.take_requests().into_iter())
        .map(|s| {
            (
                s.target.clone(),
                r.ticket_cause(s.ticket).independent(),
                s.ticket,
            )
        })
        .collect()
}

/// A send that replaces the one waiting in a queue, and the write it shows
/// in, keep lineage only where both sends had it.
#[test]
fn a_replacing_send_keeps_lineage_only_where_both_had_it() {
    const QUEUED: &str = "component App\n  resource v = load(1) as shape number\n  mutation m as shape number queue refreshes v\n  action save\n    send m = save(1)\n  view\n    text \"x\"\n";
    for (waiting, replacing) in [(true, false), (false, true), (true, true)] {
        let mut r = boot_counting(QUEUED);
        r.take_requests();
        let send = |r: &mut Runner<Counting>, marked: bool| match marked {
            true => r.act("save", vec![]).map(|_| ()),
            false => r.as_person(|r| r.act_named("save", vec![])).map(|_| ()),
        };
        send(&mut r, true).unwrap();
        let first = named(&mut r);
        send(&mut r, waiting).unwrap();
        send(&mut r, replacing).unwrap();
        assert!(named(&mut r).is_empty(), "the later sends wait");
        let both = waiting && replacing;
        let write = r.writes.records.last().unwrap().cause;
        assert_eq!(write.independent(), both, "the write {waiting} {replacing}");
        // The queue's next send is due once the first one's refresh of `v` lands.
        r.fulfill(first[0].2, ok()).unwrap();
        for (_, _, ticket) in named(&mut r) {
            r.fulfill(ticket, ok()).unwrap();
        }
        let now = r.now_ms();
        r.advance(now).unwrap();
        let out: Vec<_> = named(&mut r).into_iter().filter(|o| o.0 == "m").collect();
        assert_eq!(out.len(), 1, "one merged send");
        assert_eq!(out[0].1, both, "the send {waiting} {replacing}");
    }
}

/// An announcement while a reader's request is in flight asks it again when
/// that reply lands: that ask has no lineage, though the reply's commit has
/// the act's and a reader the reply itself changes keeps it.
#[test]
fn an_announcements_deferred_ask_has_no_lineage() {
    const WATCHED: &str = "component App\n  state k = 0\n  resource v = load(k) as shape number\n  resource w = follow(v) as shape number\n  action go\n    k = k + 1\n  view\n    button \"b\" press=go testId=\"b\"\n";
    let mut r = boot_counting(WATCHED);
    for (_, _, ticket) in named(&mut r) {
        r.fulfill(ticket, ok()).unwrap();
    }
    for (_, _, ticket) in named(&mut r) {
        r.fulfill(ticket, ok()).unwrap();
    }
    let key = r.kernel().find_by_test_id("b")[0];
    let b = r.kernel().node_by_key(key).unwrap().id;
    r.dispatch(b, Event::Act(Box::new(Event::Press))).unwrap();
    let asked = named(&mut r);
    assert_eq!(asked.len(), 1);
    assert!(asked[0].1, "the act's own ask");
    r.changed(crate::ResourceChange::Source("load")).unwrap();
    assert!(named(&mut r).is_empty(), "it waits for the reply in flight");
    r.fulfill(asked[0].2, ok()).unwrap();
    let mut again = named(&mut r);
    again.sort();
    let lineage: Vec<_> = again.iter().map(|o| (o.0.as_str(), o.1)).collect();
    assert_eq!(lineage, [("v", false), ("w", true)]);
    // Once that ask has gone out, a marked act's next ask of `v` is its own.
    for (_, _, ticket) in again {
        r.fulfill(ticket, ok()).unwrap();
    }
    for (_, _, ticket) in named(&mut r) {
        r.fulfill(ticket, ok()).unwrap();
    }
    r.dispatch(b, Event::Act(Box::new(Event::Press))).unwrap();
    let next: Vec<_> = named(&mut r)
        .into_iter()
        .filter(|o| o.0 == "v")
        .map(|o| o.1)
        .collect();
    assert_eq!(next, [true]);
}

/// A resource the boot's settlement did not answer has no boot lineage at
/// the activation: the fallback is the app's, without lineage.
#[test]
fn a_row_the_boot_did_not_answer_falls_back_without_lineage() {
    const LATE: &str =
        "component App\n  resource level = load(1) as shape number\n  view\n    text \"x\"\n";
    let mut r = boot(LATE, false);
    r.forget_deferred(0);
    r.origins.boot_asked.remove(&0);
    r.data().0 = true;
    r.data_ready().unwrap();
    assert_eq!(requests(&mut r), [(APP, false)]);
}

/// The boot's set and the untraced asks are restored with a refused
/// commit's checkpoint and leave with a forgotten row.
#[test]
fn the_lineage_tables_follow_the_checkpoint_and_the_row() {
    let mut r = boot_kept(false);
    r.note_untraced_ask(0);
    let saved = r.origins_saved();
    r.forget_resource_row(0);
    assert!(!r.origins.boot_asked.contains(&0) && !r.origins.untraced.contains(&0));
    r.origins_restore(saved);
    assert!(r.origins.boot_asked.contains(&0) && r.origins.untraced.contains(&0));
}

/// An ask the boot left waiting records the boot's cause, lineage and all,
/// and a resource whose gate the boot left closed asked nothing: neither
/// needs the boot's fallback.
#[test]
fn a_waiting_ask_records_the_boots_lineage_and_a_closed_one_none() {
    const GATED: &str = "component App\n  state open = false\n  resource early = load(1) as shape number\n  resource late = load(2) when open as shape number\n  action go\n    open = true\n  view\n    text \"x\"\n";
    let r = boot(GATED, false);
    assert!(r.origins.boot_asked.is_empty());
    assert!(r.origins.deferred[&0].0.independent());
    assert!(!r.origins.deferred.contains_key(&1) && r.asked_cause(1).is_none());
}

/// A host's event kind marks a person's act in its high bit; the decoders
/// take it off and wrap the event they decode.
#[test]
fn a_hosts_kind_carries_the_act_mark_in_its_high_bit() {
    assert_eq!(Event::unmark_kind(7 | Event::ACT_KIND), (7, true));
    assert_eq!(Event::unmark_kind(7), (7, false));
    assert_eq!(Event::Press.acted(true), Event::Act(Box::new(Event::Press)));
    assert_eq!(Event::Press.acted(false), Event::Press);
}

/// A `then` runs under the reply it is armed in, so it keeps the lineage of
/// the act that sent the request.
#[test]
fn a_then_keeps_the_lineage_of_its_reply() {
    const THEN: &str = "component App\n  mutation m as shape number then done\n  mutation n as shape number\n  action save\n    send m = save(1)\n  action done\n    send n = save(2)\n  view\n    text \"x\"\n";
    for marked in [true, false] {
        let mut r = boot(THEN, true);
        r.take_requests();
        match marked {
            true => r.act("save", vec![]),
            false => r.as_person(|r| r.act_named("save", vec![])),
        }
        .unwrap();
        let sent = named(&mut r);
        r.fulfill(sent[0].2, ok()).unwrap();
        let now = r.now_ms();
        r.advance(now).unwrap();
        let after: Vec<_> = named(&mut r).into_iter().map(|o| (o.0, o.1)).collect();
        assert_eq!(after, [("n".to_string(), marked)]);
    }
}

/// A reply lands as a commit of its request's cause, which the runner puts back before that
/// commit concludes: a reply to a marked act's request whose commit moves layout renews the
/// budget; a reply to the same press unmarked does not.
#[test]
fn a_traced_reply_that_moves_layout_renews_the_budget() {
    const REPLY: &str = "shape Rect\n  x: number\n  y: number\n  width: number\n  height: number\nshape Hero\n  frame: option<Rect>\ncomponent App\n  state k = 0\n  resource v = load(k) as shape number\n  resource g = exactGeometry(\"hero\") as shape Hero\n  derive last = match g.frame { case some(f) => f.width, case none => 0 }\n  action go\n    k = k + 1\n  view\n    column\n      column id=\"hero\" width=(last + 1) height=10\n      column width=(v) height=1\n      button \"b\" press=go testId=\"b\"\n";
    let deliver = |r: &mut Runner<Counting>| {
        let root = r.roots()[0];
        let offer = exact_kernel::Offer::definite(400.0, 800.0);
        r.kernel_mut().compute_layout(root, offer).unwrap();
        r.refresh_geometry().unwrap().is_some()
    };
    for marked in [true, false] {
        let mut r = boot_counting(REPLY);
        r.mark_inputs(true);
        for (_, _, ticket) in named(&mut r) {
            r.fulfill(ticket, ok()).unwrap();
        }
        assert_eq!((0..20).take_while(|_| deliver(&mut r)).count(), 8);
        let key = r.kernel().find_by_test_id("b")[0];
        let b = r.kernel().node_by_key(key).unwrap().id;
        r.dispatch(b, Event::Press.acted(marked)).unwrap();
        assert!(!deliver(&mut r), "the press itself renews nothing");
        let asked = named(&mut r);
        r.fulfill(asked[0].2, ok()).unwrap();
        assert_eq!(deliver(&mut r), marked, "marked {marked}");
    }
}

/// A closed gate and a re-ask given up both drop the want of lineage, so a
/// later ask of the row is its own.
#[test]
fn a_closed_gate_or_a_given_up_reask_drops_the_untraced_ask() {
    let mut r = boot(PRESS, true);
    r.note_untraced_ask(0);
    r.forget_deferred(0);
    assert!(!r.origins.untraced.contains(&0));
    r.note_untraced_ask(0);
    r.give_up(&[0], &crate::RunnerError::Cycle);
    assert!(!r.origins.untraced.contains(&0));
}

/// A reader the launch left closed is stale, as `seed_kept` marks a kept
/// reader, yet the activation does not ask it: it takes nothing from the
/// lineage of the boot's ask beside it.
#[test]
fn a_dormant_reader_beside_a_boot_ask_leaves_its_lineage() {
    const GATED: &str = "component App\n  state open = false\n  resource early = load(1) as shape number\n  resource late = load(2) when open as shape number\n  view\n    text \"x\"\n";
    let mut r = boot(GATED, false);
    r.resources[1].stale = true;
    r.data().0 = true;
    r.data_ready().unwrap();
    assert_eq!(requests(&mut r), [(APP, true)]);
}

/// A marked event's record is the one its bare event offers.
#[test]
fn a_marked_events_record_is_its_bare_events() {
    let toggle = Event::Toggle {
        old_open: false,
        new_open: true,
    };
    let marked = toggle.clone().acted(true);
    assert_eq!(marked.record(), toggle.record());
    assert!(marked.record().is_some());
}

/// An activation whose only stale row is one it does not ask has no work
/// taking part, so no lineage, whatever that row recorded.
#[test]
fn an_activation_with_only_dormant_rows_has_no_lineage() {
    const DORMANT: &str = "component App\n  state open = false\n  resource late = load(2) when open as shape number\n  view\n    text \"x\"\n";
    let mut r = boot(DORMANT, false);
    r.resources[0].stale = true;
    // Even one whose stand-in the boot showed, which takes the boot's lineage.
    r.origins.boot_asked.insert(0);
    let cause = r.ready_cause(&[0], false);
    assert!(!cause.independent());
    assert_eq!(cause, Cause::person(), "its authority as before");
}

/// The activation re-asks a resource with the lineage of both its asks on
/// record: a traced waiting ask does not mask an untraced last one.
#[test]
fn an_untraced_last_ask_holds_the_activations_reask() {
    const LATE: &str =
        "component App\n  resource level = load(1) as shape number\n  view\n    text \"x\"\n";
    let mut r = boot(LATE, false);
    assert!(r.origins.deferred[&0].0.independent());
    r.origins.asked.insert(0, Cause::person());
    r.data().0 = true;
    r.data_ready().unwrap();
    assert_eq!(requests(&mut r), [(APP, false)]);
}
