//! A mouse's lift on a desktop host (LLP 1094.000 D13): which press arms it,
//! which gesture takes the contact before it, what the press must still
//! arm at the slop, and the picture it carries.
use super::drag_out::{Lift, Lifter, Offers};
use super::*;
use std::cell::RefCell;
use std::rc::Rc;

const APP: &str = r##"component App
  state label = "Fuji"
  state pans = 0
  state swipes = 0
  state presses = 0
  state hidden = false
  action moved(dx: number, dy: number)
    pans = pans + 1
  action swiped
    swipes = swipes + 1
  action pressed
    presses = presses + 1
  action rename
    label = "Kawaguchi"
  action hide
    hidden = true
  view
    column
      box testId="card" draggable=true dataTransfer=DataTransfer(text=label, url="https://example.com/fuji") width=300 height=60
      box testId="pan-parent" draggable=true dataTransfer=DataTransfer(text="parent") width=300 height=60
        box testId="pan-child" pan=moved touch-action="none" width=300 height=40
      box testId="pan-outer" pan=moved touch-action="none" width=300 height=60
        box testId="pan-inner" draggable=true dataTransfer=DataTransfer(text="inner") width=300 height=40
      box testId="swipe" draggable=true dataTransfer=DataTransfer(text="swipe") swiperight=swiped touch-action="pan-y" width=300 height=60
      box testId="nest" draggable=true dataTransfer=DataTransfer(text="nest") width=300 height=60
        button testId="nest-button" press=pressed width=100 height=40
          text "Open"
      button testId="pressable" draggable=true dataTransfer=DataTransfer(text="pressable") press=pressed width=300 height=60
        text "Press"
      box testId="changing" draggable=true dataTransfer=DataTransfer(text=label) pointerdown=rename width=300 height=60
      box testId="hiding" draggable=true dataTransfer=DataTransfer(text="hiding") visibility=(hidden ? "hidden" : "visible") pointerdown=hide width=300 height=40
      box testId="wide" draggable=true dataTransfer=DataTransfer(text="wide") width=500 height=20
      box testId="files" draggable=true dataTransfer=DataTransfer(files=["assets/a.png"]) width=300 height=40
      box testId="around-files" draggable=true dataTransfer=DataTransfer(text="around") width=300 height=60
        box testId="inner-files" draggable=true dataTransfer=DataTransfer(files=["assets/a.png"]) width=300 height=40
"##;

type Lifts = Rc<RefCell<Vec<Lift>>>;

const ALL: Offers = Offers {
    text: true,
    url: true,
    files: true,
};

fn boot() -> (Presenter<()>, Lifts) {
    boot_offering(ALL, true)
}

/// A presenter whose lifter offers `offers` and answers `takes`.
fn boot_offering(offers: Offers, takes: bool) -> (Presenter<()>, Lifts) {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(APP),
        (),
        (300., 600.),
        2.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let lifts: Lifts = Rc::default();
    let heard = lifts.clone();
    p.set_lifter(Some(Lifter {
        offers,
        lift: Box::new(move |lift| {
            heard.borrow_mut().push(lift);
            takes
        }),
    }));
    p.set_pointer_kind(PointerKind::Mouse);
    p.frame();
    (p, lifts)
}

/// A press 10 points into `name`'s box, a drag by (`dx`, `dy`) in steps, and
/// the release there.
fn drag(p: &mut Presenter<()>, name: &str, dx: f32, dy: f32) {
    let kernel = p.host().kernel();
    let id = kernel
        .node_by_key(kernel.find_by_test_id(name)[0])
        .unwrap()
        .id;
    let rect = p.box_of(id).unwrap().rect;
    let (x, y) = (rect.0 + 10., rect.1 + 10.);
    let t = p.host().now() + 10.;
    p.pointer_down(x, y, t).unwrap();
    for step in 1..=4 {
        let f = step as f32 / 4.;
        p.pointer_move(x + dx * f, y + dy * f, t + 16. * step as f64)
            .unwrap();
    }
    p.pointer_up(x + dx, y + dy, t + 80.).unwrap();
    p.frame();
}

fn view(p: &Presenter<()>, name: &str) -> ViewId {
    let kernel = p.host().kernel();
    kernel
        .node_by_key(kernel.find_by_test_id(name)[0])
        .unwrap()
        .id
}

fn slot(p: &Presenter<()>, name: &str) -> f64 {
    p.host()
        .runner()
        .slot(name)
        .and_then(exact_runner::Value::as_number)
        .unwrap()
}

fn texts(lifts: &Lifts) -> Vec<String> {
    lifts.borrow().iter().map(|l| l.text.clone()).collect()
}

#[test]
fn a_mouse_drag_past_the_slop_lifts_what_the_node_carries() {
    let (mut p, lifts) = boot();
    drag(&mut p, "card", 2., 0.);
    assert!(
        lifts.borrow().is_empty(),
        "a move within the slop lifts nothing"
    );
    drag(&mut p, "card", 40., 0.);
    let lift = lifts.borrow_mut().pop().expect("the card lifts");
    assert_eq!(
        (lift.text.as_str(), lift.url.as_str()),
        ("Fuji", "https://example.com/fuji")
    );
    assert_eq!(lift.hotspot, (10., 10.), "held where the press held it");
}

#[test]
fn a_pan_on_a_child_takes_the_drag_and_one_on_an_ancestor_yields_to_the_node() {
    let (mut p, lifts) = boot();
    drag(&mut p, "pan-child", 40., 0.);
    assert!(texts(&lifts).is_empty(), "the child's pan is innermost");
    assert!(slot(&p, "pans") > 0.);
    let pans = slot(&p, "pans");
    drag(&mut p, "pan-inner", 40., 0.);
    assert_eq!(texts(&lifts), ["inner"], "the node is innermost");
    assert_eq!(slot(&p, "pans"), pans);
}

#[test]
fn a_one_way_swipe_takes_its_direction_and_the_other_falls_through_to_the_lift() {
    let (mut p, lifts) = boot();
    drag(&mut p, "swipe", 60., 0.);
    assert!(texts(&lifts).is_empty(), "rightward is the swipe's");
    drag(&mut p, "swipe", -60., 0.);
    assert_eq!(
        texts(&lifts),
        ["swipe"],
        "leftward, which it does not hear, lifts the node"
    );
}

#[test]
fn a_press_inside_keeps_the_contact_and_a_press_on_the_node_is_cancelled_by_its_lift() {
    let (mut p, lifts) = boot();
    drag(&mut p, "nest-button", 40., 0.);
    assert!(
        texts(&lifts).is_empty(),
        "the button inside keeps the mouse"
    );
    assert_eq!(
        slot(&p, "presses"),
        1.,
        "and is pressed, released still inside it"
    );
    drag(&mut p, "pressable", 40., 0.);
    assert_eq!(texts(&lifts), ["pressable"]);
    assert_eq!(
        slot(&p, "presses"),
        1.,
        "the lift cancelled the node's press"
    );
}

#[test]
fn a_node_whose_data_changed_since_the_press_does_not_lift() {
    let (mut p, lifts) = boot();
    drag(&mut p, "changing", 40., 0.);
    assert!(
        texts(&lifts).is_empty(),
        "what it carries changed at the press"
    );
}

#[test]
fn the_lift_carries_the_nodes_box_as_the_frame_showed_it() {
    let (mut p, lifts) = boot();
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
    drop(frame);
    drag(&mut p, "card", 40., 0.);
    let lift = lifts.borrow_mut().pop().unwrap();
    let picture = lift.picture.expect("the shown frame's pixels");
    assert_eq!(
        (picture.width, picture.height, picture.scale),
        (600, 120, 2.)
    );
    assert_eq!(picture.rgba.len(), 600 * 120 * 4);
}

#[test]
fn a_node_with_only_values_the_host_does_not_offer_is_not_a_source() {
    let offers = Offers {
        files: false,
        ..ALL
    };
    let (mut p, lifts) = boot_offering(offers, true);
    drag(&mut p, "files", 40., 0.);
    assert!(texts(&lifts).is_empty(), "files alone offer nothing here");
    drag(&mut p, "inner-files", 40., 0.);
    assert_eq!(
        texts(&lifts),
        ["around"],
        "the nearest source around it lifts"
    );
    let (mut p, lifts) = boot();
    drag(&mut p, "inner-files", 40., 0.);
    let lift = lifts.borrow_mut().pop().unwrap();
    assert_eq!(
        lift.files,
        ["assets/a.png"],
        "a host that offers files lifts the inner node"
    );
}

#[test]
fn a_lift_the_host_declines_leaves_the_contact_as_if_nothing_armed() {
    let (mut p, lifts) = boot_offering(ALL, false);
    drag(&mut p, "pressable", 40., 0.);
    assert_eq!(texts(&lifts), ["pressable"], "the host was asked");
    assert_eq!(
        slot(&p, "presses"),
        1.,
        "and the press, released inside, is pressed"
    );
    drag(&mut p, "pan-inner", 40., 0.);
    assert_eq!(texts(&lifts), ["pressable", "inner"]);
    assert!(slot(&p, "pans") > 0., "the pan around it takes the drag");
    // One move past the slop then the release: the pan begins on that move
    let pans = slot(&p, "pans");
    let rect = p.box_of(view(&p, "pan-inner")).unwrap().rect;
    let (x, y, t) = (rect.0 + 10., rect.1 + 10., p.host().now() + 10.);
    p.pointer_down(x, y, t).unwrap();
    p.pointer_move(x + 40., y, t + 16.).unwrap();
    p.pointer_up(x + 40., y, t + 32.).unwrap();
    p.frame();
    assert!(slot(&p, "pans") > pans, "the pan heard the one move");
}

#[test]
fn a_source_hidden_at_the_press_lifts_nothing() {
    let (mut p, lifts) = boot();
    drag(&mut p, "hiding", 40., 0.);
    assert!(texts(&lifts).is_empty(), "its pointerdown hid it");
}

#[test]
fn a_box_partly_outside_the_frame_keeps_its_shown_part_and_its_hotspot() {
    let (mut p, lifts) = boot();
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
    drop(frame);
    drag(&mut p, "wide", 40., 0.);
    let lift = lifts.borrow_mut().pop().unwrap();
    let picture = lift.picture.expect("the part the frame shows");
    assert_eq!(
        (picture.width, picture.height),
        (600, 40),
        "cut at the frame's right edge"
    );
    assert_eq!(lift.hotspot, (10., 10.));
}

#[test]
fn without_a_lifter_or_under_the_agent_a_mouse_lifts_nothing() {
    let (mut p, lifts) = boot();
    p.set_lifter(None);
    drag(&mut p, "card", 40., 0.);
    assert!(texts(&lifts).is_empty());
}
