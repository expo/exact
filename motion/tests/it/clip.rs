//! A box's `clip-path` under `transition` (LLP 1003.003): two `path()` clips
//! with the same fill rule and commands move number by number, any other
//! pair changes at once, and a clip the kernel stops sending stops.

use exact_motion::{ClipValue, Engine, PathCommand, PathValue, Property, Transitions};

const NODE: u64 = 7;

fn clip(even_odd: bool, cmds: &[(u8, &[f64])]) -> ClipValue {
    let path = PathValue(
        cmds.iter()
            .map(|(verb, args)| {
                let mut a = [0.0; 7];
                a[..args.len()].copy_from_slice(args);
                PathCommand {
                    verb: *verb,
                    args: a,
                }
            })
            .collect(),
    );
    ClipValue { even_odd, path }
}

/// A balloon's body and tail, the tail grown (`tip` 48) or collapsed to its
/// origin (`tip` 40).
fn balloon(tip: f64) -> ClipValue {
    clip(
        false,
        &[
            (b'M', &[0.0, 0.0]),
            (b'H', &[120.0]),
            (b'V', &[40.0]),
            (b'H', &[0.0]),
            (b'Z', &[]),
            (b'M', &[104.0, 40.0]),
            (b'L', &[120.0, 40.0]),
            (b'L', &[120.0, tip]),
            (b'Z', &[]),
        ],
    )
}

fn engine(transition: &str) -> Engine {
    let mut e = Engine::new();
    e.set_transitions(NODE, Transitions::parse(transition).unwrap())
        .unwrap();
    e.observe_clip(NODE, Some(balloon(48.0)));
    e
}

fn shown(e: &Engine) -> Option<String> {
    e.presented_clip(NODE).map(|c| c.to_css())
}

#[test]
fn a_matched_clip_moves_under_clip_path_and_all_and_settles_on_the_row() {
    exact_motion::link_transitions();
    for transition in ["clip-path 400ms linear", "all 400ms linear"] {
        let mut e = engine(transition);
        assert_eq!(shown(&e), None, "first seen: no transition");
        e.observe_clip(NODE, Some(balloon(40.0)));
        e.advance(0.2).unwrap();
        assert_eq!(
            shown(&e).as_deref(),
            Some("path('M0 0H120V40H0ZM104 40L120 40L120 44Z')"),
            "the tail half way to its origin"
        );
        assert!(e.frame().iter().any(|p| p.property == Property::ClipPath));
        e.advance(0.4).unwrap();
        assert_eq!(shown(&e), None, "settled: the row shows");
        assert!(e.quiescent());
    }
}

#[test]
fn a_clip_that_does_not_interpolate_changes_at_once() {
    exact_motion::link_transitions();
    let lines = clip(
        false,
        &[
            (b'M', &[0.0, 0.0]),
            (b'L', &[120.0, 0.0]),
            (b'L', &[120.0, 40.0]),
            (b'Z', &[]),
        ],
    );
    let horizontal = clip(
        false,
        &[
            (b'M', &[0.0, 0.0]),
            (b'H', &[120.0]),
            (b'L', &[120.0, 40.0]),
            (b'Z', &[]),
        ],
    );
    let mut even_odd = lines.clone();
    even_odd.even_odd = true;
    for next in [horizontal, even_odd] {
        let mut e = Engine::new();
        e.set_transitions(NODE, Transitions::parse("clip-path 400ms linear").unwrap())
            .unwrap();
        e.observe_clip(NODE, Some(lines.clone()));
        e.observe_clip(NODE, Some(next));
        assert_eq!(
            shown(&e),
            None,
            "`H` against `L`, or another fill rule: at once"
        );
        assert!(!e.is_active(NODE, Property::ClipPath));
    }
}

#[test]
fn a_clip_the_kernel_stops_sending_stops_its_transition() {
    exact_motion::link_transitions();
    let mut e = engine("clip-path 400ms linear");
    e.observe_clip(NODE, Some(balloon(40.0)));
    e.advance(0.1).unwrap();
    assert!(e.is_active(NODE, Property::ClipPath));
    e.frame();
    // `none`, a basic shape, or a box that stopped being rendered (D6)
    e.observe_clip(NODE, None);
    assert!(!e.is_active(NODE, Property::ClipPath));
    assert!(
        e.frame().iter().any(|p| p.property == Property::ClipPath),
        "the cancellation presents the row, so a host that drew the run draws the row"
    );
    assert_eq!(shown(&e), None);
    // Its next clip is first seen: at once
    e.observe_clip(NODE, Some(balloon(48.0)));
    assert_eq!(shown(&e), None);
}

#[test]
fn a_change_back_reverses_from_where_the_clip_is() {
    exact_motion::link_transitions();
    let mut e = engine("clip-path 400ms linear");
    e.observe_clip(NODE, Some(balloon(40.0)));
    e.advance(0.1).unwrap();
    e.observe_clip(NODE, Some(balloon(48.0)));
    assert_eq!(
        shown(&e).as_deref(),
        Some("path('M0 0H120V40H0ZM104 40L120 40L120 46Z')"),
        "from a quarter of the way"
    );
    // CSS's shortening: a quarter of the way back takes a quarter of the time
    assert_eq!(e.settle_time(), Some(0.2));
}
