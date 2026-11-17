//! Material's press (LLP 1115.001 D9; `paint/ripple.rs`) through the
//! presenter: a finger's press on what presses starts a ripple at the
//! touch that spreads to rest and leaves after its lift, on the host's
//! clock; an authored press look replaces it; with animations removed and
//! on the agent's clock it is at rest at once and gone at the lift. The
//! look is the process's (`app::show_material_look`), so each test runs in
//! a child process of its own.
use super::*;

const APP: &str = r##"component App
  state presses = 0
  action go
    presses = presses + 1
  view
    column width=400 height=600 background-color="#ffffff"
      column press=go background-color="#ffffff" color="#000000" width=300 height=120 testId="card"
      column press=go background-color="#ffffff" width=300 height=120 testId="own"
        :active
          background-color="#00ff00"
      link href="/next" background-color="#ffffff" color="#000000" width=300 height=120 testId="link"
        text "Next"
      text testId="said" `${presses}`
"##;

fn material_process() -> bool {
    const CHILD: &str = "EXACT_RIPPLE_LOOK_TEST";
    if std::env::var_os(CHILD).is_some() {
        crate::app::show_material_look();
        return true;
    }
    let name = std::thread::current().name().unwrap().to_owned();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--nocapture"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

fn boot(reduced_motion: bool) -> Presenter<()> {
    boot_app(APP, reduced_motion)
}

/// `source` on a phone: a finger, and motion removed or not.
fn boot_app(source: &str, reduced_motion: bool) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 600.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let phone = Device {
        night: false,
        font_scale: 1.0,
        locale: "en-US".into(),
        format_locale: String::new(),
        rtl: false,
        hour24: false,
        reduced_motion,
        bold_text: false,
        high_contrast: false,
        touch: true,
        tv: false,
        touch_slop: 8.0,
        dates: String::new(),
    };
    assert!(p.set_device(&phone).is_none());
    p.set_pointer_kind(crate::presenter::PointerKind::Touch);
    p.frame();
    p
}

fn rect(p: &mut Presenter<()>, name: &str) -> Rect4 {
    let k = p.host().kernel();
    let id = k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id;
    p.rect_of(id).unwrap()
}

/// The red channel at a point: the card is white, and the ripple its black
/// ink at a share of 10 %.
fn red(p: &mut Presenter<()>, x: f32, y: f32) -> u8 {
    let c = p.frame().pixel(x as u32, y as u32).unwrap().demultiply();
    c.red()
}

#[test]
fn a_press_is_a_ripple_from_the_touch_that_leaves_after_the_lift() {
    if !material_process() {
        return;
    }
    let mut p = boot(false);
    let card = rect(&mut p, "card");
    let (near, far) = ((card.0 + 20., card.1 + 60.), (card.0 + 290., card.1 + 60.));
    let middle = (card.0 + 150., card.1 + 60.);
    // An idle page's clock is its last frame's: the press is timed by its
    // event, 1000 ms on.
    p.pointer_down(near.0, near.1, 1000.).unwrap();
    assert_eq!(
        red(&mut p, near.0, near.1),
        255,
        "nothing at the touch's instant"
    );
    assert!(p.needs_animation_frame(), "a ripple owes frames");
    // 100 ms on, the wave is round the touch and has not reached the far end.
    p.clock(1100.);
    let (at_touch, at_far) = (red(&mut p, near.0, near.1), red(&mut p, far.0, far.1));
    assert!(at_touch < 250, "the wave at the touch: {at_touch}");
    assert_eq!(at_far, 255, "and not yet at the far end");
    // Held past 450 ms it is at rest over the whole card, centred on its
    // middle: the pressed state's 10 %, five sixths of it shown, there.
    p.clock(1500.);
    let rest = red(&mut p, middle.0, middle.1);
    let want = 255. * (1. - 0.1 * (1. - 0.1 / 0.6));
    assert!(
        (f32::from(rest) - want).abs() <= 2.,
        "{rest} against {want}"
    );
    assert!(red(&mut p, far.0, far.1) < 252, "the far end too");
    assert!(!p.needs_animation_frame(), "at rest, nothing moves");
    // Lifted at 600 ms, it leaves over 375 ms.
    p.pointer_up(near.0, near.1, 1600.).unwrap();
    p.clock(1600.);
    p.clock(1800.);
    let leaving = red(&mut p, middle.0, middle.1);
    assert!(leaving > rest && leaving < 255, "fading: {leaving}");
    p.clock(2000.);
    assert_eq!(red(&mut p, middle.0, middle.1), 255, "gone");
    assert!(!p.needs_animation_frame());
    assert_eq!(said(&p), "1", "the press still pressed");
}

/// A tap shorter than the wave's entrance: it spreads to rest and leaves
/// 375 ms after, wherever the finger lifted.
#[test]
fn a_tap_spreads_to_rest_before_it_leaves() {
    if !material_process() {
        return;
    }
    let mut p = boot(false);
    let link = rect(&mut p, "link");
    let at = (link.0 + 250., link.1 + 100.);
    p.clock(1000.);
    p.pointer_down(at.0, at.1, 1000.).unwrap();
    p.pointer_up(at.0, at.1, 1040.).unwrap();
    p.clock(1440.);
    let corner = (link.0 + 20., link.1 + 110.);
    assert!(
        red(&mut p, corner.0, corner.1) < 252,
        "a link ripples, across it"
    );
    p.clock(1450.);
    assert!(p.needs_animation_frame());
    p.clock(1825.);
    assert_eq!(red(&mut p, corner.0, corner.1), 255);
}

/// The author's own press look replaces the ripple (LLP 1121 D8).
#[test]
fn an_authored_press_look_replaces_the_ripple() {
    if !material_process() {
        return;
    }
    let mut p = boot(false);
    let own = rect(&mut p, "own");
    let at = (own.0 + 150., own.1 + 60.);
    p.clock(1000.);
    p.pointer_down(at.0, at.1, 1000.).unwrap();
    p.clock(1300.);
    let c = p
        .frame()
        .pixel(at.0 as u32, at.1 as u32)
        .unwrap()
        .demultiply();
    assert_eq!(
        [c.red(), c.green(), c.blue()],
        [0, 255, 0],
        "its `:active` alone"
    );
    assert!(!p.needs_animation_frame());
    p.pointer_up(at.0, at.1, 1300.).unwrap();
}

/// With animations removed, and on the agent's clock, a press is the wave
/// at rest at once and the lift takes it away.
#[test]
fn without_motion_a_press_is_the_wave_at_rest() {
    if !material_process() {
        return;
    }
    let want = 255. * (1. - 0.1 * (1. - 0.1 / 0.6));
    for agent in [false, true] {
        let mut p = boot(!agent);
        let card = rect(&mut p, "card");
        let middle = (card.0 + 150., card.1 + 60.);
        p.clock(1000.);
        p.pointer_down(card.0 + 10., card.1 + 10., 1000.).unwrap();
        if agent {
            p.land_presses();
        }
        let rest = red(&mut p, middle.0, middle.1);
        assert!(
            (f32::from(rest) - want).abs() <= 2.,
            "agent {agent}: {rest}"
        );
        assert!(!p.needs_animation_frame(), "agent {agent}");
        p.pointer_up(card.0 + 10., card.1 + 10., 1000.).unwrap();
        if agent {
            p.land_presses();
        }
        assert_eq!(red(&mut p, middle.0, middle.1), 255, "agent {agent}");
    }
}

/// In what scrolls (here the page) a press waits the tap timeout: a touch
/// a scroller takes sooner (a cancel) shows no ripple at all; one held
/// shows it from 100 ms.
#[test]
fn a_press_in_what_scrolls_waits_the_tap_timeout() {
    if !material_process() {
        return;
    }
    const LIST: &str = r##"component App
  state n = 0
  action go
    n = n + 1
  view
    column width=400 background-color="#ffffff"
      column press=go background-color="#ffffff" color="#000000" width=300 height=120 testId="card"
      column height=2000
"##;
    let mut p = boot_app(LIST, false);
    let card = rect(&mut p, "card");
    let at = (card.0 + 150., card.1 + 60.);
    p.clock(1000.);
    p.pointer_down(at.0, at.1, 1000.).unwrap();
    assert_eq!(p.brush.ripples.keys().count(), 1, "a ripple waits");
    p.clock(1060.);
    p.pointer_cancel(1060.).unwrap();
    assert_eq!(
        p.brush.ripples.keys().count(),
        0,
        "cancelled, it never shows"
    );
    for t in [1100., 1300., 1600.] {
        p.clock(t);
        assert_eq!(red(&mut p, at.0, at.1), 255, "never shown, at {t}");
    }
    assert!(!p.needs_animation_frame());
    p.pointer_down(at.0, at.1, 2000.).unwrap();
    assert_eq!(p.brush.ripples.keys().count(), 1, "a ripple waits");
    p.clock(2090.);
    assert_eq!(red(&mut p, at.0, at.1), 255, "not yet");
    assert_eq!(p.brush.ripples.keys().count(), 1, "it still waits");
    p.clock(2250.);
    assert!(
        red(&mut p, at.0, at.1) < 252,
        "shown once the tap timeout is over"
    );
    p.pointer_cancel(2250.).unwrap();
}

fn said(p: &Presenter<()>) -> String {
    let k = p.host().kernel();
    let node = k.node_by_key(k.find_by_test_id("said")[0]).unwrap();
    node.props.str(PropId::Text).unwrap_or_default().to_owned()
}

/// A new tree (a reload, an update taking effect) takes the old one's
/// presses with it: a ripple held at the replacement shows on no node of
/// the new tree, which may reuse its view ids.
#[test]
fn a_replaced_tree_keeps_no_ripple() {
    if !material_process() {
        return;
    }
    let mut p = boot(false);
    let card = rect(&mut p, "card");
    p.clock(1000.);
    p.pointer_down(card.0 + 150., card.1 + 60., 1000.).unwrap();
    assert_eq!(p.brush.ripples.keys().count(), 1);
    p.replaced();
    assert_eq!(
        p.brush.ripples.keys().count(),
        0,
        "the old tree's ripple is gone"
    );
}
