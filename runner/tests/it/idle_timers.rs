//! A task firing whose action changes nothing commits nothing: no receipt
//! and no kernel transaction. The timer keeps its beat, and a firing that
//! changes something commits as any action does.
use exact_kernel::{Kernel, PropId};
use exact_plan::Value;
use exact_runner::{Event, Runner, RunnerError, TIMER_FIRE_LIMIT};
use std::{cell::Cell, rc::Rc};

/// `tick` every `ms`, and `bump` every 100 ms while `on`.
fn boot(tick: &str, ms: u32, text: &str) -> Runner<()> {
    let beat = match text.contains("beat") {
        true => "derive beat = floor(performanceNow() / 100)",
        false => "",
    };
    let source = format!(
        r#"component App
  state on = false
  state shown = 0
  state bumps = 0
  derive doubled = shown * 2
  {beat}
  action tick
    {tick}
  action start
    on = true
  action bump
    bumps = bumps + 1
  task ticker mount
    every({ms}, tick)
  task gated when on
    every(100, bump)
  view
    column
      text {text} testId="shown"
      button "start" press=start testId="start"
"#
    );
    Runner::boot(
        contract::compile(&source).unwrap(),
        (),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

const SHOWN: &str = "`${shown}/${doubled}`";
/// Bodies that change nothing: a branch not taken, the clock read and
/// dropped, and a state assigned the value it holds.
const IDLE: [&str; 3] = [
    "if on\n      shown = shown + 1",
    "if performanceNow() < 0\n      shown = 1",
    "shown = shown",
];

fn shown(r: &Runner<()>) -> String {
    let key = r.kernel().find_first_by_test_id("shown").unwrap();
    let node = r.kernel().node_by_key(key).unwrap();
    node.props.str(PropId::Text).unwrap().to_owned()
}

fn fired(lines: &[String]) -> usize {
    lines.iter().filter(|l| *l == "ran task tick ok").count()
}

#[test]
fn a_firing_that_changes_nothing_commits_nothing_and_keeps_its_beat() {
    for tick in IDLE {
        let mut r = boot(tick, 100, SHOWN);
        let (seq, epoch) = (r.seq(), r.kernel().epoch());
        for beat in 1..=5 {
            let at = f64::from(beat) * 100.0;
            let (a, ran) = r.trace_commits(|r| r.advance_timed(at));
            assert!(a.receipts.is_empty() && a.error.is_none(), "{tick}");
            assert_eq!(fired(&ran), 1, "{tick}: {ran:?}");
            assert_eq!(r.timer_due_ms(), Some(at + 100.0), "{tick}");
        }
        assert_eq!((r.seq(), r.kernel().epoch()), (seq, epoch), "{tick}");
        assert_eq!(shown(&r), "0/0");

        // The same through a platform timer's callback.
        let mut r = boot(tick, 100, SHOWN);
        let time = Rc::new(Cell::new(0.0));
        let sample = time.clone();
        r.use_native_timers(true);
        r.set_live_clock(Some(Box::new(move || sample.get())));
        let seq = r.seq();
        for beat in 1..=5 {
            let at = f64::from(beat) * 100.0;
            time.set(at);
            let generation = r.native_timers()[0].generation;
            let (a, ran) = r.trace_commits(|r| r.fire_native_timer(0, generation, at));
            assert!(a.receipts.is_empty() && a.error.is_none(), "{tick}");
            assert_eq!(fired(&ran), 1, "{tick}: {ran:?}");
            assert_eq!(r.native_timers()[0].due_ms, at + 100.0, "{tick}");
        }
        assert_eq!(r.seq(), seq, "{tick}");
    }
}

#[test]
fn the_firing_that_writes_commits_once_and_the_view_shows_it() {
    // The third firing is the first to write a new value.
    let tick = "shown = floor(performanceNow() / 300)";
    let mut r = boot(tick, 100, SHOWN);
    let seq = r.seq();
    let commits: Vec<usize> = (1..=5)
        .map(|beat| r.advance_timed(f64::from(beat) * 100.0).receipts.len())
        .collect();
    assert_eq!(commits, [0, 0, 1, 0, 0]);
    assert_eq!(r.seq(), seq + 1);
    assert_eq!(shown(&r), "1/2", "the derive followed the write");
    assert_eq!(r.timer_due_ms(), Some(600.0));

    // One advance over the five beats: the one commit, at its own time.
    let mut r = boot(tick, 100, SHOWN);
    let (a, ran) = r.trace_commits(|r| r.advance_timed(500.0));
    assert_eq!(fired(&ran), 5);
    assert_eq!(
        a.receipts.iter().map(|t| t.at_ms).collect::<Vec<_>>(),
        [300.0]
    );
    assert!(r.journal().any(|l| l.contains("advance → 5 timers fired")));
    assert_eq!(shown(&r), "1/2");
}

#[test]
fn a_gate_opened_between_idle_firings_arms_its_task() {
    let mut r = boot(IDLE[2], 100, SHOWN);
    assert!(r.advance(250.0).unwrap().is_empty());
    assert_eq!(r.timer_due_ms(), Some(300.0));
    let key = r.kernel().find_first_by_test_id("start").unwrap();
    let id = r.kernel().node_by_key(key).unwrap().id;
    r.dispatch(id, Event::Press).unwrap();
    // `gated` is armed at 250: it fires at 350, between idle beats.
    let a = r.advance_timed(400.0);
    assert_eq!(
        a.receipts.iter().map(|t| t.at_ms).collect::<Vec<_>>(),
        [350.0]
    );
    assert_eq!(r.slot("bumps"), Some(&Value::Number(1.0)));
    assert_eq!(r.timer_due_ms(), Some(450.0));
}

#[test]
fn a_view_that_reads_the_clock_still_commits_at_each_firing() {
    for text in ["`${floor(performanceNow() / 100)}`", "`${beat}`"] {
        let mut r = boot(IDLE[2], 100, text);
        assert_eq!(r.advance(300.0).unwrap().len(), 3, "{text}");
        assert_eq!(shown(&r), "3", "{text}");
    }
}

#[test]
fn idle_firings_count_toward_an_advances_fire_limit() {
    let mut r = boot(IDLE[0], 1, SHOWN);
    let a = r.advance_timed(10_000.0);
    assert!(a.receipts.is_empty());
    assert!(matches!(a.error, Some(RunnerError::TimerFireLimit { .. })));
    assert_eq!(a.now_ms, TIMER_FIRE_LIMIT as f64);
}
