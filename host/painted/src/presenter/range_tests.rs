//! A range under the pointer and the keys (LLP 1069.001 D7), as HTML's
//! `input type="range"` behaves in a browser.
use super::*;
use std::path::PathBuf;

const LEVEL: &str = r#"component App
  state level = 50
  state log = ""
  action slide(v: number)
    level = v
    log = `${log}i${v} `
  action settle(v: number)
    log = `${log}c${v} `
  view
    column width=400
      box DIRECTION width=400
        input type="range" min=0 max=100 step="any" value=level input=slide change=settle testId="r" width=200
"#;

fn boot(source: &str) -> Presenter<()> {
    let (p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 600.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

fn named(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host.kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

fn level(p: &Presenter<()>) -> f64 {
    p.host.runner().slot("level").unwrap().as_number().unwrap()
}

/// The range's value after a mouse press and release `fraction` of the way
/// across its box, from the left.
fn pressed_at(direction: &str, fraction: f32) -> f64 {
    let mut p = boot(&LEVEL.replace("DIRECTION", direction));
    let (x, y, w, h) = p.box_of(named(&p, "r")).unwrap().rect;
    let at = (x + w * fraction, y + h / 2.0);
    let now = p.host.now();
    p.pointer_down(at.0, at.1, now).unwrap();
    p.pointer_up(at.0, at.1, now).unwrap();
    level(&p)
}

/// Runs the calling test again in a child process that shows the neutral
/// look, which is the whole process's: true in that child.
fn in_neutral_child() -> bool {
    const CHILD: &str = "EXACT_RANGE_NEUTRAL_TEST";
    if std::env::var_os(CHILD).is_some() {
        crate::app::show_neutral_look();
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

/// A right-to-left range has its minimum at the right, where it is painted,
/// so a press four fifths of the way across sets about a fifth of it (a
/// browser's `<div dir="rtl"><input type="range">`).
fn mirrored_press() {
    let ltr = pressed_at(r#"direction="ltr""#, 0.8);
    let rtl = pressed_at(r#"direction="rtl""#, 0.8);
    assert!(ltr > 75.0 && ltr < 85.0, "{ltr}");
    assert!((ltr + rtl - 100.0).abs() < 1e-6, "ltr {ltr}, rtl {rtl}");
}

#[test]
fn a_press_on_a_right_to_left_range_sets_the_value_under_it() {
    mirrored_press();
}

#[test]
fn a_press_on_a_right_to_left_range_sets_the_value_under_it_in_the_neutral_look() {
    if in_neutral_child() {
        mirrored_press();
    }
}

const SLIDER: &str = r#"component App
  state level = 5
  state log = ""
  action slide(v: number)
    level = v
    log = `${log}i${v} `
  action settle(v: number)
    log = `${log}c${v} `
  view
    column width=300
      input type="range" min=0 max=10 step=1 value=level input=slide change=settle testId="r" width=300
      box height=300
"#;

fn log(p: &Presenter<()>) -> String {
    let log = p.host.runner().slot("log").unwrap();
    log.as_str().unwrap().to_owned()
}

/// The slider, and the middle of its thumb at 5.
fn slider(pointer: PointerKind) -> (Presenter<()>, (f32, f32)) {
    let mut p = boot(SLIDER);
    p.set_pointer_kind(pointer);
    let (x, y, w, h) = p.box_of(named(&p, "r")).unwrap().rect;
    (p, (x + w / 2.0, y + h / 2.0))
}

/// Down at `from`, a move to each point, and up at the last.
fn drag(p: &mut Presenter<()>, from: (f32, f32), to: &[(f32, f32)]) -> Vec<String> {
    let now = p.host.now();
    p.pointer_down(from.0, from.1, now).unwrap();
    let mut seen = Vec::new();
    for &(x, y) in to {
        p.pointer_move(x, y, now).unwrap();
        seen.push(log(p));
    }
    let (x, y) = to.last().copied().unwrap_or(from);
    p.pointer_up(x, y, now).unwrap();
    seen
}

#[test]
fn a_dragged_range_follows_the_pointer_and_commits_on_the_lift() {
    for pointer in [PointerKind::Mouse, PointerKind::Touch] {
        let (mut p, (x, y)) = slider(pointer);
        let moves: Vec<_> = (1..=4).map(|i| (x + 20.0 * i as f32, y)).collect();
        let seen = drag(&mut p, (x, y), &moves);
        assert_eq!(
            seen,
            ["i6 ", "i6 ", "i6 i7 ", "i6 i7 i8 "],
            "{pointer:?}: an input as each step is crossed"
        );
        assert_eq!(log(&p), "i6 i7 i8 c8 ", "{pointer:?}: change on the lift");
        assert_eq!(level(&p), 8.0);
    }
}

#[test]
fn a_range_follows_a_mouse_that_leaves_it_until_the_up() {
    let (mut p, (x, y)) = slider(PointerKind::Mouse);
    drag(&mut p, (x, y), &[(x + 60.0, y + 120.0)]);
    assert_eq!(log(&p), "i7 c7 ");
}

#[test]
fn a_finger_that_scrolls_across_a_range_sets_nothing_and_a_tap_sets_it() {
    let (mut p, (x, y)) = slider(PointerKind::Touch);
    drag(&mut p, (x, y), &[(x + 3.0, y + 40.0), (x + 60.0, y + 80.0)]);
    assert_eq!(
        log(&p),
        "",
        "a finger moving across the track is the page's"
    );
    drag(&mut p, (x + 60.0, y), &[]);
    assert_eq!(log(&p), "i7 c7 ", "a tap sets the value under it, once");
}

const KEYED: &str = r#"component App
  state level = 50
  state log = ""
  action slide(v: number)
    level = v
    log = `${log}i${v} `
  action settle(v: number)
    log = `${log}c${v} `
  view
    column DIRECTION
      input type="range" min=0 max=MAX step=STEP value=level input=slide change=settle testId="r"
"#;

/// The value after each key at the focused range.
fn keyed(direction: &str, step: &str, keys: &[&str]) -> Vec<f64> {
    keyed_to(direction, "100", step, keys)
}

fn keyed_to(direction: &str, max: &str, step: &str, keys: &[&str]) -> Vec<f64> {
    let source = (KEYED.replace("DIRECTION", direction))
        .replace("MAX", max)
        .replace("STEP", step);
    let mut p = boot(&source);
    let id = named(&p, "r");
    assert!(p.set_focus(Some(id), p.host.now()).is_none());
    keys.iter()
        .map(|key| {
            p.key_down(key, p.host.now());
            level(&p)
        })
        .collect()
}

#[test]
fn a_focused_range_steps_on_its_keys() {
    let keys = [
        "ArrowRight",
        "ArrowUp",
        "ArrowLeft",
        "ArrowDown",
        "PageUp",
        "PageDown",
        "Home",
        "End",
    ];
    let ltr = r#"direction="ltr""#;
    assert_eq!(
        keyed(ltr, "1", &keys),
        [51., 52., 51., 50., 60., 50., 0., 100.]
    );
    assert_eq!(
        keyed(ltr, "20", &keys),
        [80., 100., 80., 60., 80., 60., 0., 100.],
        "from 60, where 50 snaps; a step larger than a tenth of the span is PageUp's too"
    );
    assert_eq!(
        keyed(r#"direction="rtl""#, "1", &keys[..4]),
        [49., 50., 51., 50.],
        "the minimum at the right: ArrowRight goes down"
    );
    let any = keyed_to(ltr, "200", r#""any""#, &keys[..2]);
    assert_eq!(any, [52., 54.], "a hundredth of the span under step=any");
}

#[test]
fn a_key_at_a_range_fires_input_then_change_and_nothing_at_a_bound() {
    let source = (KEYED.replace("DIRECTION", ""))
        .replace("MAX", "100")
        .replace("STEP", "50");
    let mut p = boot(&source);
    let id = named(&p, "r");
    assert!(p.set_focus(Some(id), p.host.now()).is_none());
    p.key_down("ArrowRight", p.host.now());
    p.key_down("ArrowRight", p.host.now());
    assert_eq!(log(&p), "i100 c100 ");
}
