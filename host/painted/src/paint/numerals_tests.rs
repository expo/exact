//! A number that rolls on the CPU painter: frames while it plays, none after,
//! none under Reduce Motion, and the clock's settle waits for it.
use crate::presenter::{PainterChoice, Presenter};
use std::path::PathBuf;

const APP: &str = r#"component App
  state n = 7
  task bump mount
    after(1000, step)
  action step
    n = 42
  view
    column width=300 height=200
      text `${n}` font-size=48 -exact-content-transition="numeric" testId="n"
"#;

fn boot(reduced_motion: bool) -> Presenter<()> {
    boot_source(APP, reduced_motion)
}

fn boot_source(source: &str, reduced_motion: bool) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (300., 200.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let preferences = exact_runner::Preferences {
        reduced_motion,
        ..Default::default()
    };
    assert!(p.set_preferences(preferences).is_none());
    let _ = p.frame();
    p
}

fn pixels(p: &mut Presenter<()>) -> Vec<u8> {
    p.frame().data().to_vec()
}

#[test]
fn a_changed_number_rolls_then_asks_for_no_more_frames() {
    let mut p = boot(false);
    assert!(!p.needs_animation_frame());
    p.clock(1000.);
    let start = pixels(&mut p);
    assert!(p.needs_animation_frame(), "the roll began with its commit");
    p.clock(1100.);
    let rolling = pixels(&mut p);
    assert!(p.needs_animation_frame());
    // Two changed glyphs: the second starts a beat after the first.
    p.clock(1000. + 550.);
    let settled = pixels(&mut p);
    assert!(!p.needs_animation_frame(), "the roll ended");
    assert_ne!(start, rolling);
    assert_ne!(rolling, settled);
    p.clock(2000.);
    assert_eq!(
        pixels(&mut p),
        settled,
        "the last frame of the roll is the number at rest"
    );
}

#[test]
fn under_reduce_motion_the_number_changes_in_place() {
    let mut p = boot(true);
    p.clock(1000.);
    let changed = pixels(&mut p);
    assert!(!p.needs_animation_frame());
    p.clock(2000.);
    assert_eq!(pixels(&mut p), changed);
}

#[test]
fn the_clock_settles_after_the_roll() {
    let mut p = boot(false);
    let reply = crate::agent::handle(&mut p, r#"{"op":"clock","to":1000}"#);
    assert!(reply.contains("\"clock\":1000"), "{reply}");
    let reply = crate::agent::handle(&mut p, r#"{"op":"clock","settle":true}"#);
    assert!(reply.contains("\"clock\":1550"), "{reply}");
    assert!(!p.needs_animation_frame());
}

fn decorated(from: u32, to: u32, line: &str, color: &str, shadow: &str) -> String {
    format!(
        r##"component App
  state n = {from}
  task bump mount
    after(1000, step)
  action step
    n = {to}
  view
    column width=300 height=200 padding=20 background-color="#ffffff"
      text font-size=48 font-variant-numeric="tabular-nums" color="#ff0000" text-decoration-line="{line}" text-shadow="{shadow}" -exact-content-transition="numeric"
        text `${{n}}` color="{color}"
"##
    )
}

#[test]
fn inherited_decorations_stay_at_rest_while_digits_roll() {
    for (from, to) in [(7, 8), (10, 11)] {
        for (line, color) in [("underline", "transparent"), ("line-through", "#000000")] {
            let source = decorated(from, to, line, color, "none");
            let mut p = boot_source(&source, false);
            let mut still = boot_source(&source, true);
            still.clock(1000.);
            let red = |pixels: &[u8]| {
                pixels
                    .chunks_exact(4)
                    .map(|p| p == [255, 0, 0, 255])
                    .collect::<Vec<_>>()
            };
            let expected = red(&pixels(&mut still));
            assert!(expected.iter().filter(|covered| **covered).count() > 40);
            for elapsed in [0., 100., 400., 500.] {
                p.clock(1000. + elapsed);
                let actual = red(&pixels(&mut p));
                let different = actual.iter().zip(&expected).filter(|(a, b)| a != b).count();
                assert_eq!(
                    different, 0,
                    "{line} moves or fades during {from} → {to} at {elapsed} ms"
                );
            }
        }
    }
}

#[test]
fn rolling_glyphs_and_stationary_decorations_cast_one_shadow() {
    let source = decorated(
        10,
        11,
        "underline line-through",
        "#000000",
        "0px 80px 0px #0000ff80",
    );
    let mut p = boot_source(&source, false);
    let mut lines = boot_source(
        &decorated(10, 11, "underline line-through", "transparent", "none"),
        true,
    );
    lines.clock(1000.);
    let reference = pixels(&mut lines);
    let points: Vec<_> = reference
        .chunks_exact(4)
        .enumerate()
        .filter_map(|(i, p)| (p == [255, 0, 0, 255]).then_some(i))
        .collect();
    assert!(points.len() > 80);
    for elapsed in [0., 100., 400., 500.] {
        p.clock(1000. + elapsed);
        let frame = pixels(&mut p);
        for i in &points {
            let shadow = (i + 80 * 300) * 4;
            assert_eq!(
                &frame[shadow..shadow + 4],
                [127, 127, 255, 255],
                "the stationary line and moving glyph shadow are tinted once at {elapsed} ms, pixel {i}"
            );
        }
    }
}
