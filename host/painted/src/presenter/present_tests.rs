//! A display host's presents (LLP 1003.001 D9) and the page's `presented`
//! (LLP 1069.000 D2, D6).
use super::super::{PainterChoice, Presenter};
#[cfg(target_os = "linux")]
use super::super::{PanelImage, SubmittedFrame};
use exact_kernel::ViewId;
use exact_runner::Event;
use std::path::PathBuf;
use std::rc::Rc;

const FADER: &str = r#"component App
  state faded = false
  action fade
    faded = true
  view
    column
      button "Fade" press=fade testId="fade"
      box testId="fader" opacity=(faded ? 0 : 1) transition="opacity 200ms linear" width=20 height=20
"#;

fn named(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host.kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

fn opacity(p: &Presenter<()>) -> f32 {
    p.host.presented(named(p, "fader")).opacity
}

/// A display host's presenter, booted awaiting its present, its clock live.
fn display() -> Presenter<()> {
    display_of(FADER)
}

fn display_of(source: &str) -> Presenter<()> {
    let previous = exact_runner::page::set_awaits_present(true);
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (100., 100.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    exact_runner::page::set_awaits_present(previous);
    assert!(error.is_none(), "{error:?}");
    p.presents_pictures(true);
    p.set_live_clock(Some(Rc::new(|| 0.)));
    p.frame();
    p
}

fn fade(p: &mut Presenter<()>, at: f64) {
    let id = named(p, "fade");
    assert_eq!(p.host.dispatch_at(id, Event::Press, at), None);
    assert_eq!(p.after_commit(), None);
}

#[test]
fn a_commits_motion_starts_at_the_present_of_the_picture_that_carried_it() {
    let mut p = display();
    fade(&mut p, 100.);
    assert!(p.dirty(), "a picture is owed to carry the hold");
    p.tick(150.);
    p.frame();
    let carried = p.picture_carried();
    assert!(!p.host.holds_uncarried());
    p.tick(170.);
    assert_eq!(opacity(&p), 1., "held until its present");
    assert_eq!(p.picture_presented(carried, 166.), None);
    p.tick(266.);
    assert!((opacity(&p) - 0.5).abs() < 1e-3, "{}", opacity(&p));
}

#[test]
fn a_present_for_another_host_starts_nothing() {
    let mut p = display();
    fade(&mut p, 100.);
    p.frame();
    let carried = p.picture_carried();
    p.hosts += 1;
    assert_eq!(p.picture_presented(carried, 116.), None);
    p.tick(400.);
    assert_eq!(opacity(&p), 1.);
}

#[test]
fn a_hidden_page_starts_what_it_holds_at_its_commit() {
    let mut p = display();
    fade(&mut p, 100.);
    p.tick(150.);
    assert_eq!(opacity(&p), 1.);
    let mut page = p.host.runner().page();
    page.hidden = true;
    assert_eq!(p.set_page(page), None);
    p.tick(200.);
    assert!((opacity(&p) - 0.5).abs() < 1e-3, "{}", opacity(&p));
}

#[test]
fn a_present_that_never_comes_releases_at_the_deadline() {
    let mut p = display();
    fade(&mut p, 100.);
    p.frame();
    p.picture_carried();
    assert_eq!(
        p.host.motion_next_ms(),
        Some(2100.),
        "the deadline is the wake"
    );
    p.tick(2099.);
    assert_eq!(opacity(&p), 1.);
    p.tick(2100.);
    p.tick(2101.);
    assert_eq!(opacity(&p), 0., "started at its commit, so long over");
}

#[test]
fn presented_latches_at_the_first_present_and_prefer_holds_it() {
    let mut p = display();
    assert!(!p.host.runner().presented().value());
    let carried = p.picture_carried();
    assert_eq!(p.picture_presented(carried, 16.), None);
    assert!(p.host.runner().presented().seen);
    assert_eq!(p.prefer_presented(Some(false)), None);
    assert!(!p.host.runner().presented().value());
    assert_eq!(p.prefer_presented(None), None);
    assert!(p.host.runner().presented().value());
}

#[test]
fn a_drivers_takeover_presents_the_page() {
    let mut p = display();
    assert!(!p.host.runner().presented().seen);
    p.set_live_clock(None);
    assert!(p.host.runner().presented().seen);
    assert!(
        !exact_runner::page::set_awaits_present(false),
        "takeover leaves the switch off"
    );
}

#[test]
fn the_boots_animation_plays_in_full_from_the_first_present() {
    let source = FADER
        .replace(
            "component App",
            "keyframes enter\n  from opacity=0\n  to opacity=1\ncomponent App",
        )
        .replace(
            "transition=\"opacity 200ms linear\"",
            "animation=\"enter 400ms linear both\"",
        );
    let mut p = display_of(&source);
    p.tick(500.);
    assert_eq!(opacity(&p), 0., "held through a slow first frame");
    let carried = p.picture_carried();
    assert_eq!(p.picture_presented(carried, 600.), None);
    p.tick(800.);
    assert!((opacity(&p) - 0.5).abs() < 1e-3, "{}", opacity(&p));
}

#[cfg(target_os = "linux")]
fn awaiting_scroll() -> (Presenter<()>, SubmittedFrame<PanelImage>) {
    let source = r#"component App
  state hot = false
  state edge = false
  state mounted = false
  action hover(over)
    hot = over
  action reached
    edge = true
  task schedule mount
    after(110, insert)
  action insert
    mounted = true
  view
    column width=100 height=100
      scroll testId="port" width=100 height=50
        box width=100 height=50
        box hover=hover width=100 height=50
        box width=100 height=50
      box testId="mount" width=100 height=15 press=insert
      box testId="fader" width=20 height=20 opacity=(hot ? 0 : 1) transition="opacity 200ms linear"
      box testId="edger" width=20 height=20 opacity=(edge ? 0 : 1) transition="opacity 200ms linear"
      if mounted
        list virtualized=true height=15 width=100
          for x in range(12) key=x
            box height=8 width=100
        list virtualized=true height=15 width=100
          for x in range(12) key=x
            box height=8 width=100
        list virtualized=true height=15 width=100 reachstart=reached
          for x in range(12) key=x
            box height=8 width=100
"#;
    let mut p = display_of(source);
    let a = p.panel_frame_ahead(true).unwrap();
    assert!(!p.host.runner().presented().seen);
    assert!(p.panel_complete(&a));
    assert_eq!(p.display_presented(&a, 16.), None);
    assert!(p.host.runner().presented().seen);
    p.pointer_move(10., 10., 90.).unwrap();

    let port = named(&p, "port");
    p.tick(100.);
    p.wheel(port, 0., 50.).unwrap();
    let b = p.panel_frame_ahead(true).unwrap();
    p.tick(110.);
    (p, b)
}

#[cfg(target_os = "linux")]
#[test]
fn ahead_picture_cannot_start_the_hover_that_its_preceding_ack_created() {
    let (mut p, b) = awaiting_scroll();
    let port = named(&p, "port");
    p.wheel(port, 0., 5.).unwrap();
    let c = p.panel_frame_ahead(true).unwrap();
    assert_eq!(p.host.engine().held_since(), None);

    assert!(p.panel_complete(&b));
    assert_eq!(p.display_presented(&b, 105.), None);
    p.follow_pointer();
    let began = Some(0.110);
    assert_eq!(p.host.engine().held_since(), began);
    assert!(p.panel_complete(&c));
    assert_eq!(p.display_presented(&c, 125.), None);
    p.follow_pointer();
    assert_eq!(p.host.engine().held_since(), began, "C predates the hover");
    assert_eq!(p.host.engine().sample_time(), 0.125, "C was presented");
    assert!(p.host.holds_uncarried());
    assert!(p.dirty(), "the first picture with the hover is still owed");

    let d = p.panel_frame_ahead(true).unwrap();
    assert!(!p.host.holds_uncarried());
    p.tick(150.);
    assert_eq!(opacity(&p), 1.);
    assert!(p.panel_complete(&d));
    assert_eq!(p.display_presented(&d, 160.), None);
    p.tick(260.);
    assert!((opacity(&p) - 0.5).abs() < 1e-3);
}

#[cfg(target_os = "linux")]
#[test]
fn a_hold_created_while_painting_ahead_waits_for_the_serial_picture() {
    let (mut p, b) = awaiting_scroll();
    assert_eq!(p.advance(110.), None);
    // Two list refinements run during the timer commit. The third reaches its edge
    // while the ahead picture paints, at the same time B's hover will use.
    assert!(p.panel_can_prepare_ahead());
    assert_eq!(p.host.engine().held_since(), None);
    assert!(p.panel_frame_ahead(true).is_none());
    assert_eq!(p.host.engine().held_since(), Some(0.110));
    assert!(
        p.host.holds_uncarried(),
        "the discarded paint has no receipt"
    );
    assert!(!p.panel_can_prepare_ahead(), "wait for B before retrying");
    assert!(p.wake_due_ms().is_none_or(|due| due > p.host.now()));

    assert!(p.panel_complete(&b));
    assert_eq!(p.display_presented(&b, 105.), None);
    p.follow_pointer();
    let d = p.panel_frame_ahead(true).unwrap();
    let edger = named(&p, "edger");
    p.tick(150.);
    assert_eq!((opacity(&p), p.host.presented(edger).opacity), (1., 1.));
    assert!(p.panel_complete(&d));
    assert_eq!(p.display_presented(&d, 160.), None);
    p.tick(260.);
    for value in [opacity(&p), p.host.presented(edger).opacity] {
        assert!((value - 0.5).abs() < 1e-3, "{value}");
    }
    let e = p.panel_frame_ahead(true).unwrap();
    p.tick(280.);
    let f = p.panel_frame_ahead(true).unwrap();
    assert!(p.panel_complete(&e));
    assert!(p.panel_complete(&f), "settled lists admit an ahead frame");
}
