//! `swipeleft`, the mirror of `swiperight` (Charlie, 2026-11-10;
//! `issues/20261022-no-swipeleft.md`): the same recognizer, the other way.
use super::*;
use exact_motion::Property;

fn source(handlers: &str) -> String {
    format!(
        r#"component App
  state rights = 0
  state lefts = 0
  action right
    rights = rights + 1
  action left
    lefts = lefts + 1
  view
    column width=400 height=500
      box testId="row" width=400 height=100 {handlers} touch-action="pan-y" transition="translate -exact-spring(300, 30, 1)"
        box testId="indicator" swipeIndicator=true opacity=0 scale=0 width=10 height=10
        text "swipe me"
      text `${{rights}} ${{lefts}}` testId="count"
"#
    )
}
fn boot(src: &str) -> Presenter<()> {
    let (p, error) = Presenter::boot_with(
        &crate::test_plan(src),
        (),
        (400., 500.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}
fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host.kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}
/// `rights lefts`: how many times each action ran.
fn count(p: &Presenter<()>) -> &str {
    p.host
        .kernel()
        .node(id(p, "count"))
        .unwrap()
        .props
        .str(PropId::Text)
        .unwrap()
}
/// A drag from x = 300 by `dx`, recognized at its first 10 points.
fn swipe(p: &mut Presenter<()>, dx: f32, at: f64) {
    assert!(p.pointer_down(300., 40., at).unwrap());
    p.pointer_move(300. + dx.signum() * 10., 40., at + 10.)
        .unwrap();
    p.pointer_up(300. + dx, 40., at + 50.).unwrap();
}

#[test]
fn a_leftward_swipe_past_the_knee_runs_swipeleft_once() {
    let mut p = boot(&source("swipeleft=left"));
    let row = id(&p, "row");
    let indicator = id(&p, "indicator");
    assert!(p.pointer_down(300., 40., 0.).unwrap());
    assert!(p.pointer_move(290., 40., 10.).unwrap());
    assert!(p.pointer_move(200., 40., 30.).unwrap());
    assert!(p.host.presented(row).translate.0 < -64.);
    assert_eq!(
        p.host.presented(indicator).opacity,
        1.,
        "the indicator follows the way the node goes"
    );
    assert!(p.pointer_up(200., 40., 50.).unwrap());
    assert_eq!(count(&p), "0 1");
    assert!(!p.pointer_up(200., 40., 60.).unwrap());
    assert_eq!(count(&p), "0 1");
    p.tick(10_000.);
    assert_eq!(p.host.presented(row).translate.0, 0.);
}

#[test]
fn short_of_the_knee_neither_swipe_runs() {
    for handlers in ["swipeleft=left", "swiperight=right swipeleft=left"] {
        let mut p = boot(&source(handlers));
        swipe(&mut p, -50., 0.);
        assert_eq!(count(&p), "0 0", "{handlers}");
    }
}

#[test]
fn each_way_refuses_a_start_the_other_way() {
    // A swiperight-only node refuses a leftward start, and a swipeleft-only
    // node a rightward one: nothing is held and nothing runs.
    for (handlers, dx) in [("swiperight=right", -120.), ("swipeleft=left", 120.)] {
        let mut p = boot(&source(handlers));
        let row = id(&p, "row");
        assert!(p.pointer_down(300., 40., 0.).unwrap());
        assert!(
            !p.pointer_move(300. + dx / 12., 40., 10.).unwrap(),
            "{handlers}"
        );
        assert!(!p.host.engine().is_held(
            exact_kernel::motion::motion_node(p.host.kernel().node(row).unwrap().key),
            Property::Translate
        ));
        p.pointer_up(300. + dx, 40., 50.).unwrap();
        assert_eq!(p.host.presented(row).translate.0, 0., "{handlers}");
        assert_eq!(count(&p), "0 0", "{handlers}");
    }
}

#[test]
fn a_node_with_both_runs_the_way_it_went() {
    let mut p = boot(&source("swiperight=right swipeleft=left"));
    swipe(&mut p, -120., 0.);
    assert_eq!(count(&p), "0 1");
    p.tick(10_000.);
    swipe(&mut p, 120., 10_000.);
    assert_eq!(count(&p), "1 1");
}

#[test]
fn the_direction_is_the_screens_in_a_right_to_left_layout() {
    let src = source("swipeleft=left").replace(
        "column width=400 height=500",
        "column width=400 height=500 direction=\"rtl\"",
    );
    let mut p = boot(&src);
    swipe(&mut p, -120., 0.);
    assert_eq!(count(&p), "0 1");
    p.tick(10_000.);
    swipe(&mut p, 120., 10_000.);
    assert_eq!(count(&p), "0 1");
}

/// The retained picture's route (`retained_action.rs`) carries `swipeleft`
/// as it carries `swiperight`: a left swipe on a row the CPU picture shows
/// while the next one is parked completes, as the ordinary path does.
#[test]
fn retained_actions_swipe_left_completes_as_the_ordinary_path_does() {
    use super::swipe_tests::retained_actions::{ack, block_body, boot, id, point, selected, Rows};
    let _service = crate::content_region::test_service();
    let src = super::swipe_tests::retained_actions::source()
        .replace("swiperight=replyTo(m.id)", "swipeleft=replyTo(m.id)");
    let mut p = boot(&src, true, Rows::default());
    let mut ordinary = boot(&src, false, Rows::default());
    ack(&mut p);
    ack(&mut ordinary);
    let row = id(&p, "reply-hit-message-0");
    let start = point(&mut p, row);
    let _gate = block_body(&mut p, 1.);
    for (arm, q) in [("retained", &mut p), ("ordinary", &mut ordinary)] {
        assert!(q.pointer_down(start.0, start.1, 20.).unwrap(), "{arm}");
        assert!(
            q.pointer_move(start.0 - 10., start.1, 30.).unwrap(),
            "{arm}"
        );
        assert!(
            q.pointer_move(start.0 - 80., start.1, 40.).unwrap(),
            "{arm}"
        );
        assert!(q.pointer_up(start.0 - 80., start.1, 50.).unwrap(), "{arm}");
        assert_eq!(selected(q), exact_runner::Value::str("message-0"), "{arm}");
        assert_eq!(q.collection_interaction(), None, "{arm}");
    }
}
