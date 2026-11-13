//! A reader's scroll drag: one `scrolldragstart`, then one `scrolldragend`
//! with UIKit's units and sign, after the drag's own `scroll`s.
use super::*;

fn boot(source: &str) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    for _ in 0..8 {
        assert!(p.pump(p.host.now()).is_none());
        if p.dirty() {
            let _ = p.frame();
        }
    }
    p
}

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host().kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

fn log(p: &Presenter<()>) -> String {
    let k = p.host().kernel();
    let node = k.node_by_key(k.find_by_test_id("log")[0]).unwrap();
    node.props.str(PropId::Text).unwrap_or_default().to_string()
}

/// A scroller 300 tall over 1,000 of content that logs its drag: `s` a
/// start, `e(vx,vy,left,top)` an end, `-` for a target it has none of, and
/// `.` for each `scroll`.
const LIST: &str = r#"fn at(n: option<number>): string = match n { case some(v) => `${v}`, case none => "-" }

component App
  state log = ""
  action start
    log = log + "s"
  action moved(x: number, y: number)
    log = log + "."
  action ended(e: ScrollDragEvent)
    log = log + `e(${e.velocityX},${e.velocityY},${at(e.targetLeft)},${at(e.targetTop)})`
  view
    column width=400 height=400
      scroll testId="list" width=400 height=300 scrolldragstart=start scrolldragend=ended scroll=moved
        box width=400 height=1000
      text log testId="log"
"#;

/// The reader is told of the list, as at a touch's down.
fn told(p: &mut Presenter<()>) -> ViewId {
    let list = id(p, "list");
    p.reader_under(200., 100., 1.);
    assert!(p.reader_knows(list));
    list
}

#[test]
fn a_drag_starts_once_and_ends_with_uikits_velocity_and_target() {
    let mut p = boot(LIST);
    let list = told(&mut p);
    assert_eq!(p.reader_drag_began(list), None);
    assert_eq!(log(&p), "s");
    assert_eq!(p.reader_scrolled(list, (0., 40.), 0), Some((0., 40.)));
    assert_eq!(p.reader_scrolled(list, (0., 60.), 0), Some((0., 60.)));
    // The finger moved up at 1,500 points a second: the offset grows, so
    // the velocity is positive, in points a millisecond.
    assert_eq!(
        p.reader_drag_ended(list, (0., 1500.), Some((0., 450.)), true),
        None
    );
    assert_eq!(log(&p), "s..e(0,1.5,0,450)");
}

#[test]
fn a_target_is_clamped_to_the_range_and_none_without_one() {
    let mut p = boot(LIST);
    let list = told(&mut p);
    assert_eq!(
        p.reader_drag_ended(list, (0., -800.), Some((0., -90.)), true),
        None
    );
    assert_eq!(
        p.reader_drag_ended(list, (0., 3000.), Some((0., 5000.)), true),
        None
    );
    assert_eq!(p.reader_drag_ended(list, (20., 0.), None, false), None);
    assert_eq!(
        log(&p),
        "e(0,-0.8,0,0)e(0,3,0,700)e(0.02,0,-,-)",
        "the range is 1,000 less the 300 port"
    );
}

#[test]
fn only_a_known_shown_scroller_with_the_handler_hears_it() {
    let mut p = boot(LIST);
    let list = id(&p, "list");
    // Never told of to the reader: nothing.
    assert_eq!(p.reader_drag_began(list), None);
    assert_eq!(p.reader_drag_began(PAGE), None);
    assert_eq!(log(&p), "");
    let mut quiet = boot(&LIST.replace(" scrolldragstart=start scrolldragend=ended", ""));
    let list = told(&mut quiet);
    assert_eq!(quiet.reader_drag_began(list), None);
    assert_eq!(quiet.reader_drag_ended(list, (0., 100.), None, false), None);
    assert_eq!(log(&quiet), "");
    assert!(p
        .reader_drag_ended(id(&p, "list"), (f64::NAN, 0.), None, false)
        .is_some());
}

#[test]
fn a_wheel_sends_no_drag() {
    let mut p = boot(LIST);
    let list = told(&mut p);
    p.wheel(list, 0., 120.).unwrap();
    assert!(
        !log(&p).contains('s') && !log(&p).contains('e'),
        "{}",
        log(&p)
    );
}

/// The list again, its `scrollend`s logged as `E` and its `scroll`s left
/// out: what the finger holds and lets go of.
const ENDS: &str = r#"component App
  state log = ""
  action start
    log = log + "s"
  action ended(e: ScrollDragEvent)
    log = log + "e"
  action settled
    log = log + "E"
  view
    column width=400 height=400
      scroll testId="list" width=400 height=300 scrolldragstart=start scrolldragend=ended scrollend=settled
        box width=400 height=1000
      text log testId="log"
"#;

/// The reader moves `list` by `dy` at `at`, as `CanvasHost::scrolled`
/// tells it.
fn sample(p: &mut Presenter<()>, list: ViewId, at: f64, dy: f32) {
    assert_eq!(p.clock(at), (at, None));
    assert_eq!(p.reader_scrolled(list, (0., dy), 0), Some((0., dy)));
    p.scroll_end_reader_sampled(list, at);
}

#[test]
fn a_finger_that_rests_on_the_list_owes_no_scrollend_until_it_lifts() {
    let mut p = boot(ENDS);
    let list = told(&mut p);
    assert_eq!(p.reader_drag_began(list), None);
    sample(&mut p, list, 0., 40.);
    // Held still well past the quiet: the drag is not over.
    sample(&mut p, list, 50., 20.);
    assert_eq!(p.advance(400.), None);
    assert_eq!(p.timer_due_ms(), None);
    assert_eq!(log(&p), "s");
    // Let go with nothing thrown: the scroll is over now.
    assert_eq!(
        p.reader_drag_ended(list, (0., 0.), Some((0., 60.)), false),
        None
    );
    assert_eq!(p.timer_due_ms(), Some(400.));
    assert_eq!(p.advance(400.), None);
    assert_eq!(log(&p), "seE");
}

#[test]
fn a_fling_ends_the_scroll_once_when_the_reader_says_it_stopped() {
    let mut p = boot(ENDS);
    let list = told(&mut p);
    assert_eq!(p.reader_drag_began(list), None);
    sample(&mut p, list, 0., 40.);
    assert_eq!(p.clock(300.), (300., None));
    assert_eq!(
        p.reader_drag_ended(list, (0., 2000.), Some((0., 200.)), true),
        None
    );
    // Frames of the fling, however far apart they come.
    for at in [316., 500., 520.] {
        sample(&mut p, list, at, 30.);
    }
    assert_eq!(p.advance(900.), None);
    assert_eq!(log(&p), "se", "the fling is the same scroll until it stops");
    p.reader_fling_ended(list);
    assert_eq!(p.advance(900.), None);
    assert_eq!(log(&p), "seE");
    assert_eq!(p.advance(2000.), None);
    assert_eq!(log(&p), "seE");
}

#[test]
fn a_new_touch_ends_a_scroll_whose_end_never_came() {
    let mut p = boot(ENDS);
    let list = told(&mut p);
    assert_eq!(p.reader_drag_began(list), None);
    sample(&mut p, list, 0., 40.);
    assert_eq!(p.advance(400.), None);
    assert_eq!(log(&p), "s");
    // The reader that had the drag went with it; this is the next one's down.
    told(&mut p);
    assert_eq!(p.advance(400.), None);
    assert_eq!(log(&p), "sE");
}

/// A list that refreshes: its pull sends `refresh`, and the app's
/// `refreshing` holds while the reload runs.
const REFRESH: &str = r#"component App
  state log = ""
  state loading = false
  action reload
    log = log + "r"
    loading = true
  action done
    loading = false
  view
    column width=400 height=400
      scroll testId="list" width=400 height=300 refresh=reload refreshing=loading
        box width=400 height=1000
      button "Done" press=done testId="done"
      text log testId="log"
"#;

/// Scroller `list`'s flags in what the reader is told at a down there.
fn flags(p: &mut Presenter<()>, list: ViewId) -> i32 {
    let words = p.reader_under(200., 100., 1.);
    words[3..]
        .chunks_exact(11)
        .find(|e| e[0] == list as i32)
        .unwrap()[1]
}

/// Scroller `list`'s flags in the statement after a frame, when there is one.
fn stated(p: &mut Presenter<()>, list: ViewId) -> Option<i32> {
    let words = p.reader_scrollers(&[], 1.)?;
    Some(words.chunks(9).find(|s| s[0] == list as i32).unwrap()[1])
}

#[test]
fn a_pull_sends_refresh_and_the_reader_is_told_refreshing() {
    let mut p = boot(REFRESH);
    let list = id(&p, "list");
    assert_eq!(
        flags(&mut p, list) & 48,
        16,
        "it hears refresh, not refreshing"
    );
    assert_eq!(p.reader_refresh(list), None);
    assert_eq!(log(&p), "r");
    assert_eq!(stated(&mut p, list).map(|f| f & 48), Some(48));
    let done = id(&p, "done");
    assert_eq!(p.host.dispatch_at(done, Event::Press, p.host.now()), None);
    assert_eq!(p.after_commit(), None);
    assert_eq!(
        stated(&mut p, list).map(|f| f & 48),
        Some(16),
        "refreshing ended: told again"
    );
}

#[test]
fn a_short_or_empty_list_that_refreshes_still_takes_a_pull() {
    for content in ["box width=400 height=100", "column"] {
        let source = REFRESH.replace("box width=400 height=1000", content);
        for tag in ["scroll", "list"] {
            let mut p =
                boot(&source.replace("      scroll testId", &format!("      {tag} testId")));
            let list = id(&p, "list");
            assert_eq!(
                flags(&mut p, list) & 18,
                18,
                "{tag} of {content}: up and down, and refreshes"
            );
            // One that does not refresh has nothing to scroll, and is no scroller under the finger.
            let mut p = boot(
                &source
                    .replace("      scroll testId", &format!("      {tag} testId"))
                    .replace(" refresh=reload", ""),
            );
            let words = p.reader_under(200., 100., 1.);
            assert_eq!(words[1], 0, "{tag} of {content}");
        }
    }
}

#[test]
fn a_scroller_that_hears_no_refresh_is_never_pulled() {
    let mut p = boot(&REFRESH.replace(" refresh=reload", ""));
    let list = id(&p, "list");
    assert_eq!(flags(&mut p, list) & 48, 0);
    assert_eq!(p.reader_refresh(list), None);
    assert_eq!(log(&p), "");
    // Nor one the reader was never told of.
    let mut p = boot(REFRESH);
    assert_eq!(p.reader_refresh(id(&p, "list")), None);
    assert_eq!(log(&p), "");
}
