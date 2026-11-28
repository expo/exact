//! What each input's own commit did, told to the callback it carries: the
//! visit its handler opened or moved, apart from due work and other input,
//! whenever its handler runs.
use crate::{Event, InputCommit, Runner};
use std::sync::{Arc, Mutex};

const APP: &str = r#"routes nav
  tab home "/"
    item "/item/:id"
  tab other "/other"
component App
  state timer = false
  action open(id: string)
    nav = push(nav, path("item", id))
  action stay
    nav = nav
  action back
    nav = back(nav)
  action tab(name: string)
    nav = select(nav, name)
  action move
    nav = replace(nav, path("item", "b"))
  action mark
    nav = replace(nav, "/item/b#notes")
  action renew
    nav = push(back(nav), "/item/b")
  action away
    nav = select(push(nav, path("item", "c")), "other")
  action time
    timer = true
  action fire
    nav = push(nav, path("item", "timer"))
  task fires when timer
    after(100, fire)
  view
    column
      button press=open("a") testId="open-a"
        text "A"
      button press=open("b") testId="open-b"
        text "B"
      button press=stay testId="stay"
        text "Stay"
      button press=back testId="back"
        text "Back"
      button press=tab("other") testId="to-other"
        text "Other"
      button press=tab("home") testId="to-home"
        text "Home"
      button press=move testId="move"
        text "Move"
      button press=mark testId="mark"
        text "Mark"
      button press=renew testId="renew"
        text "Renew"
      button press=away testId="away"
        text "Away"
      text "Plain" testId="plain"
"#;

/// A change action keyed by `input`, which a timer moves, yields before an
/// input that arrives behind it: that input waits, held
const HELD: &str = r#"
  state armed = false
  state input = 0
  state seen = 0
  action arm
    armed = true
  action tick
    input = input + 1
  action apply
    seen = input
  task timed when armed
    after(50, tick)
  task sync key=input
    change(apply)
"#;

type Log = Arc<Mutex<Vec<(&'static str, InputCommit)>>>;

fn boot(held: bool) -> Runner<()> {
    let source = if held {
        APP.replace("component App\n", &format!("component App{HELD}"))
    } else {
        APP.to_string()
    };
    let plan = contract::compile(&source).unwrap();
    Runner::boot(
        plan,
        (),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn press(r: &mut Runner<()>, test_id: &'static str, at: f64, log: &Log) -> crate::Advanced {
    let key = r.kernel().find_by_test_id(test_id)[0];
    let view = r.kernel().node_by_key(key).unwrap().id;
    let log = log.clone();
    r.dispatch_at_with_receipt(view, Event::Press, at, move |c| {
        log.lock().unwrap().push((test_id, c))
    })
}

fn taken(log: &Log) -> Vec<(&'static str, InputCommit)> {
    std::mem::take(&mut *log.lock().unwrap())
}

/// The top's visit id and location as the router published them
fn top(r: &mut Runner<()>) -> (u64, String) {
    let change = r.take_router_change().unwrap();
    (change.top, change.url)
}

/// Advance as a host applying each receipt does, until the change action the
/// held input waits behind has run and yielded, its receipt applied
fn apply_change(r: &mut Runner<()>, at: f64) {
    for _ in 0..4 {
        if r.slot("seen") == Some(&crate::Value::Number(1.)) {
            return;
        }
        assert!(r.advance_timed(at).needs_apply, "each step yields");
    }
    panic!("the change action did not run");
}

/// Advance until every input held has run
fn drain(r: &mut Runner<()>, at: f64) {
    for _ in 0..8 {
        r.advance_timed(at);
    }
}

#[test]
fn a_press_is_told_the_visit_its_own_commit_opened_or_moved() {
    let log = Log::default();
    let mut r = boot(false);
    r.take_router_change();
    press(&mut r, "open-a", 1., &log);
    let a = top(&mut r);
    assert_eq!(
        taken(&log),
        [(
            "open-a",
            InputCommit::Committed {
                opened: Some(a.clone())
            }
        )]
    );
    // A replace keeps the visit and moves it
    press(&mut r, "move", 2., &log);
    let moved = top(&mut r);
    assert_eq!(moved, (a.0, "/item/b".into()));
    assert_eq!(
        taken(&log),
        [(
            "move",
            InputCommit::Committed {
                opened: Some(moved)
            }
        )]
    );
    // An equal router, a new fragment of the same visit, a tab left and returned to, and Back open none
    for id in ["stay", "mark", "to-other", "to-home", "back"] {
        press(&mut r, id, 3., &log);
        assert_eq!(
            taken(&log),
            [(id, InputCommit::Committed { opened: None })],
            "{id}"
        );
    }
    // A new visit at the same location is opened
    press(&mut r, "open-b", 4., &log);
    let b = top(&mut r);
    taken(&log);
    press(&mut r, "renew", 4., &log);
    let renewed = top(&mut r);
    assert_eq!(renewed.1, b.1);
    assert_ne!(renewed.0, b.0);
    assert_eq!(
        taken(&log),
        [(
            "renew",
            InputCommit::Committed {
                opened: Some(renewed)
            }
        )]
    );
    // A visit opened in a stack the commit leaves for another tab is not the selected top it opened
    press(&mut r, "away", 5., &log);
    assert_eq!(
        taken(&log),
        [("away", InputCommit::Committed { opened: None })]
    );
    // A press the view has no handler for is refused, and commits nothing
    press(&mut r, "plain", 4., &log);
    assert_eq!(taken(&log), [("plain", InputCommit::Refused)]);
}

#[test]
fn a_timer_that_falls_due_before_a_press_is_not_the_press_s() {
    let log = Log::default();
    let mut r = boot(false);
    r.act("time", vec![]).unwrap();
    r.take_router_change();
    // The timer opens its visit in the press's dispatch, before the press's handler, which opens none
    press(&mut r, "stay", 200., &log);
    let timer = top(&mut r);
    assert_eq!(timer.1, "/item/timer");
    assert_eq!(
        taken(&log),
        [("stay", InputCommit::Committed { opened: None })]
    );
    // A refused press behind it is told nothing opened
    let mut r = boot(false);
    r.act("time", vec![]).unwrap();
    r.take_router_change();
    press(&mut r, "plain", 200., &log);
    assert_eq!(top(&mut r).1, "/item/timer");
    assert_eq!(taken(&log), [("plain", InputCommit::Refused)]);
    // A press that moves the timer's visit is told that visit at its new location
    let mut r = boot(false);
    r.act("time", vec![]).unwrap();
    r.take_router_change();
    press(&mut r, "move", 200., &log);
    let moved = top(&mut r);
    assert_eq!(moved.1, "/item/b");
    assert_eq!(
        taken(&log),
        [(
            "move",
            InputCommit::Committed {
                opened: Some(moved)
            }
        )]
    );
}

#[test]
fn a_held_press_is_told_its_own_commit_when_its_handler_runs() {
    // Alone: told nothing at its dispatch's return, and its own commit once its handler runs in a later advance
    let log = Log::default();
    let mut r = boot(true);
    r.act("arm", vec![]).unwrap();
    let at = r.now_ms() + 100.;
    press(&mut r, "open-a", at, &log);
    assert!(
        taken(&log).is_empty(),
        "the press waits behind the change action"
    );
    drain(&mut r, at);
    let a = top(&mut r);
    assert_eq!(a.1, "/item/a");
    assert_eq!(
        taken(&log),
        [("open-a", InputCommit::Committed { opened: Some(a) })]
    );
    // A second press, once the change action's receipt is applied: its dispatch runs the first, then itself, and
    // each is told its own commit there
    for (first, second, opens) in [("stay", "open-b", "/item/b"), ("open-a", "stay", "/item/a")] {
        let mut r = boot(true);
        r.act("arm", vec![]).unwrap();
        let at = r.now_ms() + 100.;
        press(&mut r, first, at, &log);
        apply_change(&mut r, at);
        assert!(taken(&log).is_empty(), "the first still waits");
        press(&mut r, second, at + 1., &log);
        let opened = top(&mut r);
        assert_eq!(opened.1, opens);
        let (none, some) = (
            InputCommit::Committed { opened: None },
            InputCommit::Committed {
                opened: Some(opened),
            },
        );
        let want = if first == "stay" {
            [(first, none), (second, some)]
        } else {
            [(first, some), (second, none)]
        };
        assert_eq!(taken(&log), want);
    }
}
