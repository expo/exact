//! A scroller's drag (Charlie, 2026-11-10;
//! `issues/20261021-scroll-drag-events-and-hit-slop.md`): `scrolldragstart`
//! with no payload, `scrolldragend` with a `ScrollDragEvent` to take or
//! leave; and `swipeleft`, the mirror of `swiperight`
//! (`issues/closed/20261022-no-swipeleft.md`).

use exact_kernel::{Kernel, PropId};
use exact_plan::{EventKind, Plan};
use exact_runner::{Event, Runner};

const APP: &str = r#"component App
  state seen = ""
  action began
    seen = `${seen}start;`
  action released(e: ScrollDragEvent)
    let left = match e.targetLeft { case some(n) => `${n}`, case none => "none" }
    let top = match e.targetTop { case some(n) => `${n}`, case none => "none" }
    seen = `${seen}end ${e.velocityX} ${e.velocityY} ${left} ${top};`
  action ended
    seen = `${seen}bare;`
  action swiped(way: string)
    seen = `${seen}${way};`
  view
    column
      text seen testId="seen"
      scroll scrolldragstart=began scrolldragend=released testId="pane" height=100
        text "a"
      scroll scrolldragend=ended testId="bare" height=100
        text "b"
      box swipeleft=swiped("left") swiperight=swiped("right") testId="row"
        text "c"
      column testId="plain"
"#;

fn boot() -> Runner<()> {
    let plan = contract::compile(APP).unwrap_or_else(|e| panic!("{e}"));
    let plan = Plan::decode(&plan.encode()).unwrap();
    Runner::boot(plan, (), Kernel::with_monospace(), Default::default(), "/").unwrap()
}

fn view(r: &Runner<()>, id: &str) -> exact_kernel::ViewId {
    let key = r.kernel().find_by_test_id(id)[0];
    r.kernel().node_by_key(key).unwrap().id
}

fn seen(r: &Runner<()>) -> String {
    let id = view(r, "seen");
    r.kernel()
        .node(id)
        .unwrap()
        .props
        .str(PropId::Text)
        .unwrap()
        .to_owned()
}

#[test]
fn a_drag_starts_then_releases_with_its_velocity_and_target() {
    let mut r = boot();
    let pane = view(&r, "pane");
    assert_eq!(
        r.handlers_of(pane),
        vec![EventKind::Scrolldragstart, EventKind::Scrolldragend]
    );
    r.dispatch(pane, Event::of_host_kind(62, "").unwrap())
        .unwrap();
    r.dispatch(pane, Event::of_host_kind(63, "0,-1.5").unwrap())
        .unwrap();
    r.dispatch(pane, Event::of_host_kind(63, "0,2.25,0,840").unwrap())
        .unwrap();
    assert_eq!(seen(&r), "start;end 0 -1.5 none none;end 0 2.25 0 840;");
}

#[test]
fn an_action_may_leave_the_record_and_a_node_without_a_handler_takes_nothing() {
    let mut r = boot();
    let bare = view(&r, "bare");
    r.dispatch(bare, Event::of_host_kind(63, "1,0").unwrap())
        .unwrap();
    let plain = view(&r, "plain");
    let receipt = r.dispatch(plain, Event::Scrolldragstart).unwrap();
    assert!(receipt.touched.is_empty());
    r.dispatch(plain, Event::of_host_kind(63, "1,0").unwrap())
        .unwrap();
    assert_eq!(seen(&r), "bare;");
}

#[test]
fn a_release_record_is_typed() {
    let wrong = APP.replace(
        "action ended\n    seen = `${seen}bare;`",
        "action ended(e: ScrollEvent)\n    seen = `${seen}bare;`",
    );
    assert!(contract::compile(&wrong).is_err());
    let start_with_record = APP.replace("action began\n", "action began(e: ScrollDragEvent)\n");
    assert!(contract::compile(&start_with_record).is_err());
}

#[test]
fn swipeleft_mirrors_swiperight_with_its_arguments() {
    let mut r = boot();
    let row = view(&r, "row");
    assert_eq!(
        r.handlers_of(row),
        vec![EventKind::Swipeleft, EventKind::Swiperight]
    );
    r.dispatch(row, Event::of_host_kind(61, "").unwrap())
        .unwrap();
    r.dispatch(row, Event::Swiperight).unwrap();
    r.dispatch(row, Event::Swipeleft).unwrap();
    assert_eq!(seen(&r), "left;right;left;");
}

#[test]
fn scroll_snap_compiles_each_axis_strictness_alignment_and_stop() {
    for axis in ["x", "y", "both"] {
        let source = format!("component App\n  view\n    scroll height=100 width=300 scroll-snap-type=\"{axis} mandatory\"\n      view scroll-snap-align=\"center\"\n");
        let plan = contract::compile(&source).unwrap();
        let r = Runner::boot(plan, (), Kernel::with_monospace(), Default::default(), "/").unwrap();
        let node = r.kernel().node(r.roots()[0]).unwrap();
        assert_eq!(
            node.style.scroll_snap_type.name(),
            format!("{axis} mandatory")
        );
        assert_eq!(
            r.kernel()
                .node(node.children()[0])
                .unwrap()
                .style
                .scroll_snap_align
                .name(),
            "center"
        );
    }
    let source = "component App\n  view\n    scroll height=100 width=300 scroll-snap-type=\"x proximity\"\n      view scroll-snap-align=\"start center\" scroll-snap-stop=\"always\"\n";
    let r = Runner::boot(
        contract::compile(source).unwrap(),
        (),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let node = r.kernel().node(r.roots()[0]).unwrap();
    assert_eq!(
        node.style.scroll_snap_type.name(),
        "x",
        "`proximity` is the axis alone"
    );
    let area = r.kernel().node(node.children()[0]).unwrap();
    assert_eq!(area.style.scroll_snap_align.name(), "start center");
    assert_eq!(
        area.style.scroll_snap_stop,
        exact_kernel::ScrollSnapStop::Always
    );
    for attr in [
        "scroll-snap-type=\"block mandatory\"",
        "scroll-snap-align=\"start center end\"",
        "scroll-snap-stop=\"never\"",
    ] {
        assert!(contract::compile(&format!(
            "component App\n  view\n    scroll height=100 {attr}\n"
        ))
        .is_err());
    }
}
