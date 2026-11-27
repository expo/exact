//! A drag of a node heard at its source (LLP 1094.000 D16) on the hosts
//! the presenter lifts for: the node's `dragstart` when the host says the
//! platform began the lift, before the app's own drop side hears the drag,
//! and its `dragend` once that drop lands, delivered or not; a lift the
//! platform never began, or one asked again, ends with no event.
use super::*;
use exact_runner::{KeyModifiers, Value};

const APP: &str = r##"component App
  state log = ""
  state zoned = true
  action started(e: DragEvent)
    log = `${log}start ${e.offsetX},${e.offsetY} ${e.dropEffect};`
  action ended(e: DragEvent)
    log = `${log}end ${e.dropEffect};`
  action entered(e: DragEvent)
    log = `${log}enter;`
  action left(e: DragEvent)
    log = `${log}leave;`
  action dropped(e: DragEvent)
    log = `${log}drop ${e.text} ${e.dropEffect};`
  action hide
    zoned = false
  view
    column
      box testId="card" draggable=true dataTransfer=DataTransfer(text="Mount Fuji") dragstart=started dragend=ended width=200 height=80
        text "Card"
      button testId="hide" press=hide height=20
        text "Hide"
      box testId="around" dragenter=entered dragleave=left width=300 height=200
        if zoned
          box testId="zone" drop=dropped width=300 height=200
"##;

fn boot() -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(APP),
        (),
        (400., 800.),
        2.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_requests();
    p.serve_windows(Box::new(super::drag_out_tests::Blank));
    p.frame();
    p
}

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let kernel = p.host().kernel();
    kernel
        .node_by_key(kernel.find_by_test_id(name)[0])
        .unwrap()
        .id
}

fn log(p: &Presenter<()>) -> String {
    p.host
        .runner()
        .slot("log")
        .and_then(Value::as_str)
        .expect("log")
        .to_string()
}

/// A long press 10 points into the card, and the number of the lift it
/// asked the reader for, the request's last word.
fn lift(p: &mut Presenter<()>) -> u32 {
    let rect = p.box_of(id(p, "card")).unwrap().rect;
    assert!(p.long_press(rect.0 + 10., rect.1 + 10., 100.));
    p.run_commands(|| ());
    let words = p.take_requests().unwrap_or_default();
    let (mut at, mut lift) = (0, None);
    while at < words.len() {
        let n = words[at + 2] as usize;
        if words[at] == 23 {
            lift = Some(words[at + 2 + n] as u32);
        }
        at += 3 + n;
    }
    lift.expect("a drag request")
}

fn zone_point(p: &mut Presenter<()>) -> (f32, f32) {
    let rect = p.box_of(id(p, "around")).unwrap().rect;
    (rect.0 + 20., rect.1 + 20.)
}

fn text(text: &str) -> Dropped {
    Dropped {
        text: text.into(),
        ..Default::default()
    }
}

#[test]
fn the_source_hears_its_start_before_its_own_drop_and_its_end_once_the_drop_lands() {
    let mut p = boot();
    let held = KeyModifiers::default();
    let lift = lift(&mut p);
    assert_eq!(log(&p), "", "nothing before the platform begins it");
    p.lift_began(lift, held);
    p.lift_began(lift, held);
    assert_eq!(log(&p), "start 10,10 none;", "once, at the point it began");
    let point = zone_point(&mut p);
    p.drag_over(point, held);
    let drop = p.drop_released_lift(point, held, Some(lift)).unwrap();
    // Android's end comes before the drop's items arrive
    p.lift_ended(lift, true, held);
    assert_eq!(log(&p), "start 10,10 none;enter;");
    assert!(p.drop_landed(drop, &text("Fuji")).is_some());
    assert_eq!(log(&p), "start 10,10 none;enter;drop Fuji copy;end copy;");
}

#[test]
fn a_drop_whose_node_is_gone_when_its_items_arrive_ends_none_after_its_leave() {
    let mut p = boot();
    let held = KeyModifiers::default();
    let lift = lift(&mut p);
    p.lift_began(lift, held);
    let point = zone_point(&mut p);
    p.drag_over(point, held);
    let drop = p.drop_released_lift(point, held, Some(lift)).unwrap();
    p.lift_ended(lift, true, held);
    let hide = id(&p, "hide");
    p.tap(hide).unwrap();
    assert_eq!(p.drop_landed(drop, &text("Fuji")), None);
    assert_eq!(log(&p), "start 10,10 none;enter;leave;end none;");
}

/// A lift the host refuses, the platform never beginning it, ends with no
/// event; one asked while another waits for its host's answer leaves that
/// one waiting, as the platform may still begin it.
#[test]
fn a_refused_lift_ends_with_no_event_and_each_lift_waits_for_its_answer() {
    let mut p = boot();
    let held = KeyModifiers::default();
    let first = lift(&mut p);
    p.lift_ended(first, false, held);
    p.lift_began(first, held);
    assert_eq!(log(&p), "", "a refused lift: neither event");
    let second = lift(&mut p);
    let third = lift(&mut p);
    assert_ne!(second, third);
    p.lift_ended(third, false, held);
    p.lift_began(second, held);
    p.lift_ended(second, false, held);
    assert_eq!(
        log(&p),
        "start 10,10 none;end none;",
        "the first asked began after the next was asked, then was cancelled"
    );
}

/// A list row a commit renews for another item during its drag (LLP 1078)
/// is a new mount: it hears no end of the drag its old item began.
#[test]
fn a_source_renewed_for_another_item_hears_no_end() {
    let mut p = boot();
    let held = KeyModifiers::default();
    let lift = lift(&mut p);
    p.lift_began(lift, held);
    let card = id(&p, "card");
    p.drag_renewed(&[card]);
    p.lift_ended(lift, true, held);
    assert_eq!(log(&p), "start 10,10 none;");
}

/// A lift begun while an earlier one's drop still loads keeps that earlier
/// one's end waiting for it; a drop the host knows as no lift's, another
/// app's drag, decides no lift's end.
#[test]
fn each_lift_ends_by_its_own_drop() {
    let mut p = boot();
    let held = KeyModifiers::default();
    let first = lift(&mut p);
    p.lift_began(first, held);
    let point = zone_point(&mut p);
    p.drag_over(point, held);
    let drop = p.drop_released_lift(point, held, Some(first)).unwrap();
    p.lift_ended(first, true, held);
    let second = lift(&mut p);
    p.lift_began(second, held);
    assert_eq!(log(&p), "start 10,10 none;enter;start 10,10 none;");
    // Another app's drag drops while the second lift runs, its release sending its enter: the second's end does
    // not wait for it
    p.drag_over(point, held);
    let foreign = p.drop_released(point, held).unwrap();
    p.lift_ended(second, false, held);
    assert!(p.drop_landed(drop, &text("Fuji")).is_some());
    assert!(p.drop_landed(foreign, &text("Other")).is_some());
    assert_eq!(
        log(&p),
        "start 10,10 none;enter;start 10,10 none;enter;end none;drop Fuji copy;end copy;drop Other copy;"
    );
}

/// The host's drags gone with what reported them (Linux's listener), a drop
/// still loading fails with its leaves, and each lift ends with nothing
/// taking it.
#[test]
fn drags_gone_end_each_lift_after_its_drop_fails() {
    let mut p = boot();
    let held = KeyModifiers::default();
    let lift = lift(&mut p);
    p.lift_began(lift, held);
    let point = zone_point(&mut p);
    p.drag_over(point, held);
    p.drop_released_lift(point, held, Some(lift)).unwrap();
    p.drags_gone();
    assert_eq!(log(&p), "start 10,10 none;enter;leave;end none;");
}
