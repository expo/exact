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
        p.reader_drag_ended(list, (0., 1500.), Some((0., 450.))),
        None
    );
    assert_eq!(log(&p), "s..e(0,1.5,0,450)");
}

#[test]
fn a_target_is_clamped_to_the_range_and_none_without_one() {
    let mut p = boot(LIST);
    let list = told(&mut p);
    assert_eq!(
        p.reader_drag_ended(list, (0., -800.), Some((0., -90.))),
        None
    );
    assert_eq!(
        p.reader_drag_ended(list, (0., 3000.), Some((0., 5000.))),
        None
    );
    assert_eq!(p.reader_drag_ended(list, (20., 0.), None), None);
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
    assert_eq!(quiet.reader_drag_ended(list, (0., 100.), None), None);
    assert_eq!(log(&quiet), "");
    assert!(p
        .reader_drag_ended(id(&p, "list"), (f64::NAN, 0.), None)
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

#[test]
fn agent_reader_scrolls_both_axes_and_retains_the_position_for_return() {
    let mut p = boot(LIST);
    let list = id(&p, "list");
    let reply: serde_json::Value = serde_json::from_str(
        &p.agent_scroll_drag(&serde_json::json!({
            "id": list, "scrollDrag": [0, -400], "ms": 250
        }))
        .unwrap(),
    )
    .unwrap();
    assert!((reply["scrolledBy"][1].as_f64().unwrap() - 400.).abs() < 0.01);
    assert!(p.contact_position().is_none());
    assert!(
        log(&p).starts_with('s') && log(&p).ends_with("e(0,0,-,-)"),
        "{}",
        log(&p)
    );
    p.frame();
    assert!((p.scroll[&list].1 - 400.).abs() < 0.01);
    p.agent_scroll_drag(&serde_json::json!({"id": list, "scrollDrag": [0, 400], "ms": 250}))
        .unwrap();
    assert!(p.scroll[&list].1.abs() < 0.01);

    let mut p = boot("component App\n  view\n    scroll width=300 height=200 overflow-x=\"scroll\" overflow-y=\"hidden\" testId=\"shelf\"\n      box width=1000 height=200\n");
    let shelf = id(&p, "shelf");
    p.agent_scroll_drag(&serde_json::json!({"id": shelf, "scrollDrag": [-260, 0], "ms": 250}))
        .unwrap();
    p.frame();
    assert!((p.scroll[&shelf].0 - 260.).abs() < 0.01);
    p.agent_scroll_drag(&serde_json::json!({"id": shelf, "scrollDrag": [-2000, 0], "ms": 250}))
        .unwrap();
    assert!(
        (p.scroll[&shelf].0 - 700.).abs() < 0.01,
        "{:?}",
        p.scroll[&shelf]
    );
}

#[test]
fn agent_reader_scroll_refuses_bad_input_and_covered_starts_without_moving() {
    let mut p = boot(LIST);
    let list = id(&p, "list");
    for request in [
        serde_json::json!({"id": list, "scrollDrag": [0], "ms": 250}),
        serde_json::json!({"id": list, "scrollDrag": [0, -40], "ms": -1}),
        serde_json::json!({"id": list, "scrollDrag": [0, -40], "ms": 250, "from": [200, 350]}),
        serde_json::json!({"id": id(&p, "log"), "scrollDrag": [0, -40], "ms": 250}),
    ] {
        assert!(p.agent_scroll_drag(&request).is_err());
    }
    assert_eq!(p.scroll.get(&list).copied().unwrap_or_default(), (0., 0.));
    assert_eq!(log(&p), "");
}

#[test]
fn agent_reader_scroll_preserves_pan_ownership_and_nested_containment() {
    let mut p = boot(
        r#"component App
  state n = 0
  action drag(dx: number, dy: number)
    n = n + 1
  view
    scroll width=300 height=200 testId="outer" overflow-x="scroll" overflow-y="scroll" pan=drag touch-action="pan-y"
      box width=1000 height=1000
"#,
    );
    let outer = id(&p, "outer");
    let probe = p
        .agent_scroll_drag(
            &serde_json::json!({"id": outer, "scrollDrag": [40, 0], "ms": 250, "probe": true}),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&probe).unwrap()["scrollDrag"],
        "presenter"
    );
    assert!(p
        .agent_scroll_drag(&serde_json::json!({"id": outer, "scrollDrag": [10, -40], "ms": 250}))
        .unwrap_err()
        .contains("gesture owns"));
    assert_eq!(p.scroll.get(&outer).copied().unwrap_or_default(), (0., 0.));

    let mut p = boot(
        r#"component App
  view
    scroll width=300 height=200 testId="outer" overflow-x="scroll" overflow-y="scroll"
      scroll width=1000 height=100 testId="inner" overscroll-behavior-y="contain"
        box width=1000 height=1000
      box width=1000 height=1000
"#,
    );
    let outer = id(&p, "outer");
    assert!(p
        .agent_scroll_drag(
            &serde_json::json!({"id": outer, "scrollDrag": [-40, -10], "ms": 250, "from": [50, 50]})
        )
        .unwrap_err()
        .contains("contains this axis"));
    assert_eq!(p.scroll.get(&outer).copied().unwrap_or_default(), (0., 0.));
    p.agent_scroll_drag(
        &serde_json::json!({"id": outer, "scrollDrag": [-40, 0], "ms": 250, "from": [50, 50]}),
    )
    .unwrap();
    assert!((p.scroll[&outer].0 - 40.).abs() < 0.01);
}
