//! `startViewTransition()` on the Windows host's painter (LLP 1013.003): the
//! action's commit cross-fades the whole window from how it last showed,
//! over 0.25 s with CSS `ease`, as the web's default root transition does.
use exact_painted::{presenter::PainterChoice, Presenter};
use std::{cell::Cell, rc::Rc};

/// A red ground that `swap` turns blue in a view transition, and `plain`
/// turns back with none.
const APP: &str = r##"component App
  state blue = false
  action swap
    blue = !blue
    startViewTransition()
  action plain
    blue = !blue
  view
    column width=200 height=200
      view testId="ground" width=200 height=150 background-color=(blue ? "#0000ff" : "#ff0000")
      row height=50
        button press=swap testId="swap"
          text "Swap"
        button press=plain testId="plain"
          text "Plain"
"##;

/// A presenter on a display clock the test moves, painted once at 0.
fn boot(source: &str) -> (Presenter<()>, Rc<Cell<f64>>) {
    let plan = contract::compile(source).unwrap().encode();
    let (mut p, error) = Presenter::boot_with(
        &plan,
        (),
        (200., 200.),
        1.,
        Default::default(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let wall = Rc::new(Cell::new(0.));
    let read = wall.clone();
    p.set_live_clock(Some(Rc::new(move || read.get())));
    frame(&mut p, &wall, 0.);
    (p, wall)
}

/// A display frame at `at` ms, and the ground's colour in it (red, blue).
fn frame(p: &mut Presenter<()>, wall: &Cell<f64>, at: f64) -> (u8, u8) {
    wall.set(at);
    p.tick(at);
    assert_eq!(p.advance(at), None);
    let pixels = p.frame();
    let pixel = pixels.pixel(100, 75).unwrap();
    (pixel.red(), pixel.blue())
}

fn press(p: &mut Presenter<()>, name: &str) {
    let k = p.host().kernel();
    let id = k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id;
    p.tap(id).unwrap();
    p.run_commands(|| ());
}

#[test]
fn a_view_transition_fades_the_window_from_how_it_showed() {
    let (mut p, wall) = boot(APP);
    assert_eq!(frame(&mut p, &wall, 10.), (255, 0));
    press(&mut p, "swap");
    assert!(p.needs_animation_frame(), "a fade owes frames");
    // The commit's first frame, at the fade's start, shows the window as it was.
    assert_eq!(frame(&mut p, &wall, 10.), (255, 0));
    // Halfway, CSS `ease` is 80 % through: the old look's share is 20 %.
    let (red, blue) = frame(&mut p, &wall, 10. + 125.);
    assert!((45..=60).contains(&red), "red {red}");
    let sum = u16::from(red) + u16::from(blue);
    assert!(sum.abs_diff(255) <= 2, "the shares add up: {red} + {blue}");
    let (red, blue) = frame(&mut p, &wall, 10. + 200.);
    assert!(red < 10 && blue > 245, "{red} {blue}");
    // After 0.25 s the commit shows alone.
    assert_eq!(frame(&mut p, &wall, 10. + 250.), (0, 255));
    // A commit with no transition cuts.
    press(&mut p, "plain");
    assert_eq!(frame(&mut p, &wall, 300.), (255, 0));
    assert_eq!(frame(&mut p, &wall, 310.), (255, 0));
}

#[test]
fn a_view_transition_started_mid_fade_starts_from_what_showed() {
    let (mut p, wall) = boot(APP);
    press(&mut p, "swap");
    frame(&mut p, &wall, 0.);
    let (red, blue) = frame(&mut p, &wall, 125.);
    assert!(red > 0 && blue > 0);
    // Swapped back halfway: the blend that showed fades into red.
    press(&mut p, "swap");
    let (again, _) = frame(&mut p, &wall, 125.);
    assert!(again.abs_diff(red) <= 2, "{again} vs {red}");
    assert_eq!(frame(&mut p, &wall, 125. + 250.), (255, 0));
}

#[test]
fn less_motion_cuts_a_view_transition() {
    let (mut p, wall) = boot(APP);
    let mut preferences = p.host().runner().viewport().preferences;
    preferences.reduced_motion = true;
    p.set_preferences(preferences);
    frame(&mut p, &wall, 10.);
    press(&mut p, "swap");
    assert_eq!(frame(&mut p, &wall, 10.), (0, 255));
}

/// Under a driver's clock, an agent's, the fade runs on that clock: a drive
/// that moves it halfway sees the blend, and one 0.25 s on sees the commit.
#[test]
fn a_view_transition_runs_on_a_drivers_clock() {
    let plan = contract::compile(APP).unwrap().encode();
    let (mut p, error) = Presenter::boot_with(
        &plan,
        (),
        (200., 200.),
        1.,
        Default::default(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let ground = |p: &mut Presenter<()>| {
        let pixels = p.frame();
        let pixel = pixels.pixel(100, 75).unwrap();
        (pixel.red(), pixel.blue())
    };
    assert_eq!(ground(&mut p), (255, 0));
    let start = p.host().now();
    press(&mut p, "swap");
    assert_eq!(ground(&mut p), (255, 0));
    p.clock(start + 125.);
    let (red, _) = ground(&mut p);
    assert!((45..=60).contains(&red), "red {red}");
    p.clock(start + 250.);
    assert_eq!(ground(&mut p), (0, 255));
}
