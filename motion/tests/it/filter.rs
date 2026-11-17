//! `filter` under `transition` (LLP 1003.002), held to Chrome 157: a 1 s
//! transition read at 250 ms by seeking `getAnimations()[0].currentTime`.
//! Matched lists move function by function, a shorter list is padded with
//! initial values, an easing's overshoot is clamped to each function's
//! range, and a pair whose functions differ in order changes at once.

use exact_motion::{Engine, FilterKind, FilterStep, FilterValue, Transitions, Value};

const NODE: u64 = 4;

fn f(steps: &[(FilterKind, f64)]) -> FilterValue {
    FilterValue(steps.iter().map(|(k, a)| FilterStep::new(*k, *a)).collect())
}

fn shadow(dx: f64, dy: f64, blur: f64, rgba: [u8; 4]) -> FilterValue {
    let c = Value::rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
    FilterValue(vec![FilterStep::shadow(dx, dy, blur, c, c)])
}

/// The list presented `at` seconds into a transition from `from` to `to`.
fn at(transition: &str, from: FilterValue, to: FilterValue, at: f64) -> Option<FilterValue> {
    let mut e = Engine::new();
    e.set_transitions(NODE, Transitions::parse(transition).unwrap())
        .unwrap();
    e.observe_filter(NODE, Some(from), true);
    e.observe_filter(NODE, Some(to), false);
    e.advance(at).unwrap();
    e.presented_filter(NODE)
}

fn args(v: &FilterValue) -> Vec<(FilterKind, f64)> {
    v.0.iter().map(|s| (s.kind, s.args[0])).collect()
}

fn close(got: &[(FilterKind, f64)], want: &[(FilterKind, f64)]) {
    assert_eq!(got.len(), want.len(), "{got:?} against {want:?}");
    for (g, w) in got.iter().zip(want) {
        assert_eq!(g.0, w.0);
        assert!((g.1 - w.1).abs() < 1e-3, "{got:?} against {want:?}");
    }
}

const LINEAR: &str = "filter 1s linear";
const OVER: &str = "filter 1s cubic-bezier(.5,-1,.5,2)";
use FilterKind::*;

#[test]
fn single_functions_move_and_clamp_as_chrome_does() {
    type List = &'static [(FilterKind, f64)];
    let cases: [(List, List, f64, f64); 4] = [
        (&[(Blur, 12.0)], &[(Blur, 0.0)], 9.0, 14.1879),
        (&[(Brightness, 0.5)], &[(Brightness, 2.0)], 0.875, 0.226516),
        (&[(HueRotate, 0.0)], &[(HueRotate, 360.0)], 90.0, -65.6361),
        (&[(Grayscale, 0.0)], &[(Grayscale, 1.0)], 0.25, 0.0),
    ];
    for (from, to, linear, over) in cases {
        let kind = from[0].0;
        let got = at(LINEAR, f(from), f(to), 0.25).expect("running");
        close(&args(&got), &[(kind, linear)]);
        let got = at(OVER, f(from), f(to), 0.25).expect("running");
        close(&args(&got), &[(kind, over)]);
    }
}

#[test]
fn none_and_a_shorter_list_are_padded_with_initial_values() {
    let got = at(LINEAR, FilterValue::default(), f(&[(Blur, 10.0)]), 0.25).unwrap();
    close(&args(&got), &[(Blur, 2.5)]);
    let got = at(OVER, FilterValue::default(), f(&[(Blur, 10.0)]), 0.25).unwrap();
    close(&args(&got), &[(Blur, 0.0)]);
    let got = at(LINEAR, f(&[(Blur, 10.0)]), FilterValue::default(), 0.25).unwrap();
    close(&args(&got), &[(Blur, 7.5)]);
    let to = f(&[(Blur, 8.0), (Grayscale, 1.0)]);
    let got = at(LINEAR, f(&[(Blur, 4.0)]), to.clone(), 0.25).unwrap();
    close(&args(&got), &[(Blur, 5.0), (Grayscale, 0.25)]);
    let got = at(OVER, f(&[(Blur, 4.0)]), to, 0.25).unwrap();
    close(&args(&got), &[(Blur, 3.27071), (Grayscale, 0.0)]);
    // Every function from `none`, at its own initial value.
    let to = f(&[
        (Brightness, 2.0),
        (Contrast, 3.0),
        (Saturate, 0.0),
        (Opacity, 0.2),
        (Sepia, 1.0),
        (Invert, 1.0),
    ]);
    let got = at(LINEAR, FilterValue::default(), to.clone(), 0.25).unwrap();
    close(
        &args(&got),
        &[
            (Brightness, 1.25),
            (Contrast, 1.5),
            (Saturate, 0.75),
            (Opacity, 0.8),
            (Sepia, 0.25),
            (Invert, 0.25),
        ],
    );
    let got = at(OVER, FilterValue::default(), to, 0.25).unwrap();
    close(
        &args(&got),
        &[
            (Brightness, 0.817678),
            (Contrast, 0.635355),
            (Saturate, 1.18232),
            (Opacity, 1.0),
            (Sepia, 0.0),
            (Invert, 0.0),
        ],
    );
}

#[test]
fn a_drop_shadow_moves_as_a_shadow_from_transparent() {
    let to = shadow(4.0, 6.0, 8.0, [255, 0, 0, 255]);
    let got = at(LINEAR, FilterValue::default(), to.clone(), 0.25).unwrap();
    let s = got.0[0];
    assert_eq!(s.kind, DropShadow);
    assert_eq!(s.args, [1.0, 1.5, 2.0]);
    assert_eq!(s.light.to_rgba8(), [255, 0, 0, 64]);
    let got = at(OVER, FilterValue::default(), to, 0.25).unwrap();
    let s = got.0[0];
    assert!((s.args[0] + 0.72929).abs() < 1e-3 && (s.args[1] + 1.09393).abs() < 1e-3);
    assert_eq!(s.args[2], 0.0, "a shadow's blur is clamped at 0");
    assert_eq!(s.light.to_rgba8(), [0, 0, 0, 0]);
    // rgb(0,128,0) to blue overshoots green and clamps blue: rgb(0,151,0).
    let got = at(
        OVER,
        shadow(4.0, 6.0, 8.0, [0, 128, 0, 255]),
        shadow(0.0, 0.0, 0.0, [0, 0, 255, 255]),
        0.25,
    )
    .unwrap();
    assert_eq!(got.0[0].light.to_rgba8(), [0, 151, 0, 255]);
    let got = at(
        LINEAR,
        shadow(4.0, 6.0, 8.0, [255, 0, 0, 255]),
        shadow(0.0, 0.0, 0.0, [0, 0, 255, 255]),
        0.25,
    )
    .unwrap();
    assert_eq!(got.0[0].light.to_rgba8(), [191, 0, 64, 255]);
}

#[test]
fn functions_in_another_order_change_at_once() {
    let from = f(&[(Blur, 4.0), (Grayscale, 1.0)]);
    let to = f(&[(Grayscale, 0.0), (Blur, 8.0)]);
    assert_eq!(at(LINEAR, from, to, 0.25), None, "the row shows");
}

#[test]
fn a_change_back_reverses_and_an_uncovered_row_stops_it() {
    let mut e = Engine::new();
    e.set_transitions(NODE, Transitions::parse(LINEAR).unwrap())
        .unwrap();
    e.observe_filter(NODE, Some(f(&[(Blur, 12.0)])), true);
    e.observe_filter(NODE, Some(f(&[(Blur, 0.0)])), false);
    e.advance(0.25).unwrap();
    // Back to 12 px from 9 px: CSS's shortened reversal, 250 ms long.
    e.observe_filter(NODE, Some(f(&[(Blur, 12.0)])), false);
    e.advance(0.375).unwrap();
    close(&args(&e.presented_filter(NODE).unwrap()), &[(Blur, 10.5)]);
    assert!((e.settle_time().unwrap() - 0.5).abs() < 1e-9);
    // A filter no declaration covers forgets the track: the row shows.
    e.observe_filter(NODE, None, false);
    assert_eq!(e.presented_filter(NODE), None);
}

#[test]
fn a_spring_plays_its_curve_from_rest() {
    let p = at(
        "filter -exact-spring(1, 100, 10)",
        f(&[(Blur, 0.0)]),
        f(&[(Blur, 10.0)]),
        0.1,
    )
    .expect("running");
    assert!(p.0[0].args[0] > 0.0 && p.0[0].args[0] < 12.0);
}

#[test]
fn a_known_node_without_a_filter_moves_from_none_and_a_new_one_starts_nothing() {
    let mut e = Engine::new();
    e.set_transitions(NODE, Transitions::parse(LINEAR).unwrap())
        .unwrap();
    // New: its first filter is where it starts.
    e.observe_filter(NODE, Some(f(&[(Blur, 10.0)])), true);
    assert_eq!(e.presented_filter(NODE), None);
    e.remove(NODE);
    // Known, with `none` kept as nothing: a filter now moves from `none`.
    e.set_transitions(NODE, Transitions::parse(LINEAR).unwrap())
        .unwrap();
    e.observe_filter(NODE, Some(FilterValue::default()), true);
    e.observe_filter(NODE, Some(f(&[(Blur, 10.0)])), false);
    e.advance(0.25).unwrap();
    close(&args(&e.presented_filter(NODE).unwrap()), &[(Blur, 2.5)]);
    // Back to `none`: it reverses, then settles, and keeps nothing.
    e.observe_filter(NODE, Some(FilterValue::default()), false);
    e.advance(1.0).unwrap();
    assert_eq!(e.presented_filter(NODE), None);
    e.observe_filter(NODE, Some(FilterValue::default()), false);
    e.set_transitions(NODE, Transitions::default()).unwrap();
    assert!(e.at_rest(NODE), "a settled `none` keeps nothing");
}

#[test]
fn currentcolor_is_a_share_that_moves_against_another_colour() {
    let red = shadow(0.0, 0.0, 0.0, [255, 0, 0, 255]);
    let current = FilterValue(vec![FilterStep::current_shadow(0.0, 0.0, 0.0)]);
    let half = at(LINEAR, red.clone(), current.clone(), 0.5).unwrap().0[0];
    assert_eq!(half.current, 0.5);
    assert_eq!(half.light.to_rgba8(), [255, 0, 0, 128], "half of red's own");
    // Two `currentcolor` shadows are the same filter whatever `color` is.
    assert_eq!(
        current,
        FilterValue(vec![FilterStep::current_shadow(0.0, 0.0, 0.0)])
    );
    assert_ne!(red, current);
}
