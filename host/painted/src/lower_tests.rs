use super::*;
use exact_kernel::{motion::motion_node, MonospaceMeasurer, TransformDragBinding, ViewId};
use exact_motion::Value;
use exact_runner::Event;

fn playing(transition: &str) -> (Host<()>, TransformDragBinding, ViewId) {
    let source = r#"component App
  state x = 0
  state y = 0
  state zoom = 1
  state transition = "TRANSITION"
  action geometry(w: number, h: number, pw: number, ph: number)
    x = x
  action release(x: number, y: number, s: number, vx: number, vy: number, vs: number)
    x = x
  action move
    x = 100
    y = -40
    zoom = 3
  action snap
    transition = "none"
    x = 50
    y = -20
    zoom = 2
  view
    box width=400 height=500
      box width=200 height=150 overflow="hidden"
        box id="target" testId="target" width="100%" height="100%" box-sizing="border-box" translate=`${x}px ${y}px` scale=zoom transition=transition
          box testId="handle" height=30 transformDragFor="target" transformgeometry=geometry transformrelease=release
      button testId="move" press=move
        text "Move"
      button testId="snap" press=snap
        text "Snap"
"#
    .replace("TRANSITION", transition);
    let (mut h, error) = Host::boot(
        &contract::compile(&source).unwrap().encode(),
        (),
        Box::new(MonospaceMeasurer::default()),
        400.,
        500.,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    h.lowering = true;
    let handle = h.kernel().find_by_test_id("handle")[0];
    let binding = h.transform_drag_binding(handle).unwrap();
    let target = h.kernel().node_by_key(binding.target).unwrap().id;
    let button = h.kernel().find_by_test_id("move")[0];
    let button = h.kernel().node_by_key(button).unwrap().id;
    assert!(h.dispatch_at(button, Event::Press, 0.).is_none());
    assert!(h
        .engine
        .is_played(motion_node(binding.target), Property::Translate));
    assert_eq!(h.lowered_mask(binding.target), LOWER_TRANSFORM);
    assert_eq!(h.presented(target).translate, (100., -40.));
    (h, binding, target)
}

#[test]
fn holds_retire_reader_tracks_before_presentation() {
    let (mut h, binding, target) = playing("translate 1s linear, scale 1s linear");
    let node = motion_node(binding.target);
    let epoch = h.lowered_epoch;
    let held = h
        .hold_begin(target, Property::Translate, 250.)
        .unwrap()
        .unwrap();
    assert_eq!(held.value, Value::new(25., -10.));
    assert!(!h.engine.is_played(node, Property::Translate));
    assert_eq!(
        h.played[&node].len(),
        1,
        "only scale still belongs to the reader"
    );
    assert_eq!(h.played[&node][0].0, Property::Scale);
    assert_eq!(h.lowered_mask(binding.target), LOWER_TRANSFORM);
    assert!(h.lowered_changed[&node] > epoch);

    let epoch = h.lowered_epoch;
    let held = h
        .hold_begin(target, Property::Scale, 250.)
        .unwrap()
        .unwrap();
    assert_eq!(held.value, Value::scalar(1.5));
    assert!(!h.played.contains_key(&node));
    assert_eq!(h.lowered_mask(binding.target), 0);
    assert!(h.lowered_changed[&node] > epoch);
    assert_eq!(h.presented(target).translate, (25., -10.));
    assert_eq!(h.presented(target).scale, 1.5);
}

#[test]
fn transform_drag_catches_reader_easing() {
    for (transition, at, progress) in [
        ("all 1s linear", 250., 0.25),
        ("all 1s linear 500ms", 250., 0.),
        ("all 1s linear 500ms", 750., 0.25),
        ("all 0s linear 500ms", 250., 0.),
        ("all 1s steps(5, jump-end)", 250., 0.2),
        ("all 1s linear", 1250., 1.),
    ] {
        let (mut h, binding, target) = playing(transition);
        let node = motion_node(binding.target);
        let epoch = h.lowered_epoch;
        let held = h.transform_drag_begin(binding, at).unwrap().unwrap();
        assert_eq!(
            held.translate().value,
            Value::new(100. * progress, -40. * progress),
            "{transition} at {at}"
        );
        assert_eq!(held.scale().value, Value::scalar(1. + 2. * progress));
        assert_eq!(
            h.presented(target).translate,
            ((100. * progress) as f32, (-40. * progress) as f32)
        );
        assert_eq!(h.presented(target).scale, (1. + 2. * progress) as f32);
        assert!(!h.engine.is_played(node, Property::Translate));
        assert!(!h.engine.is_played(node, Property::Scale));
        assert!(!h.played.contains_key(&node));
        assert_eq!(h.lowered_mask(binding.target), 0);
        assert!(h.lowered_changed[&node] > epoch);
    }
}

#[test]
fn transform_drag_catches_reader_spring_frames() {
    let (mut h, binding, target) = playing("translate -exact-spring(300,30,1)");
    let node = motion_node(binding.target);
    let track = &h.played[&node][0].1;
    let PlayedCurve::Frames { duration, values } = &track.curve else {
        panic!("the reader plays a spring as frames");
    };
    let at = (track.start + duration * 5.25 / (values.len() - 1) as f64) * 1000.;
    let expected = values[5].zip(values[6], |a, b| a * 0.75 + b * 0.25);
    let held = h.transform_drag_begin(binding, at).unwrap().unwrap();
    assert!((held.translate().value.x - expected.x).abs() < 1e-9);
    assert!((held.translate().value.y - expected.y).abs() < 1e-9);
    assert_eq!(held.scale().value, Value::scalar(3.));
    assert_eq!(
        h.presented(target).translate,
        (expected.x as f32, expected.y as f32)
    );
    assert!(!h.played.contains_key(&node));
    assert_eq!(h.lowered_mask(binding.target), 0);
}

#[test]
fn snapping_a_played_transition_retires_its_reader_tracks() {
    let (mut h, binding, target) = playing("translate 1s linear, scale 1s linear");
    let node = motion_node(binding.target);
    let epoch = h.lowered_epoch;
    let button = h.kernel().find_by_test_id("snap")[0];
    let button = h.kernel().node_by_key(button).unwrap().id;
    assert!(h.dispatch_at(button, Event::Press, 250.).is_none());
    assert!(!h.engine.is_played(node, Property::Translate));
    assert!(!h.engine.is_played(node, Property::Scale));
    assert!(!h.played.contains_key(&node));
    assert_eq!(h.lowered_mask(binding.target), 0);
    assert!(h.lowered_changed[&node] > epoch);
    assert_eq!(h.presented(target).translate, (50., -20.));
    assert_eq!(h.presented(target).scale, 2.);
}

/// A look naming opacity that arrives while the reader plays the node's
/// opacity transition (a `SetLooks` mid-track: a live design's, a new
/// generation's) takes the track back: the engine samples it from where its
/// curve is, so the look composes on its sample (LLP 1121 D8, stage 9 G2).
#[test]
fn a_look_arriving_mid_track_takes_the_readers_track_back() {
    use exact_kernel::{Op, StyleId, StyleProps, StyleValue, LOOK_HOVER};
    use exact_runner::Timed;
    let source = r#"component App
  state o = 1
  action go
    o = 0.2
  view
    box width=400 height=500
      box testId="target" width=100 height=100 opacity=o transition="opacity 1s linear"
      button testId="go" press=go
        text "Go"
"#;
    let (mut h, error) = Host::boot(
        &contract::compile(source).unwrap().encode(),
        (),
        Box::new(MonospaceMeasurer::default()),
        400.,
        500.,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    h.lowering = true;
    let target_key = h.kernel().find_by_test_id("target")[0];
    let target = h.kernel().node_by_key(target_key).unwrap().id;
    let node = motion_node(target_key);
    let go = h.kernel().find_by_test_id("go")[0];
    let go = h.kernel().node_by_key(go).unwrap().id;
    assert!(h.dispatch_at(go, Event::Press, 0.).is_none());
    assert!(
        h.engine.is_played(node, Property::Opacity),
        "the reader plays it"
    );
    // A quarter of the way, still the reader's: the engine presents its target.
    h.tick(250.);
    assert!(h.engine.is_played(node, Property::Opacity));
    assert_eq!(h.presented(target).opacity, 0.2);
    let epoch = h.lowered_epoch;
    let mut look = StyleProps::default();
    look.set_dynamic(StyleId::Opacity, &StyleValue::Number(0.5))
        .unwrap();
    let root = h.roots()[0];
    let receipt = h
        .runner
        .kernel_mut()
        .apply(
            root,
            0,
            &[Op::SetLooks {
                id: target,
                looks: vec![(LOOK_HOVER, look)],
            }],
        )
        .unwrap();
    assert!(h
        .commit(
            &[Timed {
                at_ms: 250.,
                receipt
            }],
            None
        )
        .is_none());
    assert!(
        !h.engine.is_played(node, Property::Opacity),
        "the engine's again"
    );
    assert!(!h.played.contains_key(&node));
    assert!(h.lowered_changed[&node] > epoch);
    let shown = |h: &Host<()>| h.presented(target).opacity;
    assert!(
        (shown(&h) - 0.8).abs() < 0.02,
        "sampled where the curve is: {}",
        shown(&h)
    );
    // It runs on along its curve, to its target at its own end.
    h.tick(500.);
    assert!(
        (shown(&h) - 0.6).abs() < 0.02,
        "on its curve: {}",
        shown(&h)
    );
    h.tick(1000.);
    assert_eq!(shown(&h), 0.2);
}

/// A track of [`spinner_words`], read back as the reader reads it.
struct Track {
    prop: u32,
    hold: Option<f64>,
    duration: f64,
    keys: Vec<(f64, f64, Option<Easing>)>,
}

fn read_tracks(words: &[u32]) -> Vec<Track> {
    let at = |i: usize| f32::from_bits(words[i]) as f64;
    let easing = |i: &mut usize| match words[*i] {
        0 => {
            *i += 1;
            None
        }
        1 => {
            let e = Easing::CubicBezier {
                x1: at(*i + 1),
                y1: at(*i + 2),
                x2: at(*i + 3),
                y2: at(*i + 4),
            };
            *i += 5;
            Some(e)
        }
        kind => panic!("easing kind {kind}"),
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let (prop, duration) = (words[i], at(i + 5));
        let hold = (words[i + 2] != 0).then(|| at(i + 3));
        assert_eq!((words[i + 1], at(i + 4), words[i + 7]), (0, 0.0, 0));
        assert!(at(i + 6).is_infinite(), "endless");
        i += 9;
        assert!(easing(&mut i).is_none(), "each keyframe names its own");
        let n = words[i] as usize;
        i += 1;
        let mut keys = Vec::new();
        for _ in 0..n {
            let (offset, value) = (at(i), at(i + 1));
            i += 2;
            keys.push((offset, value, easing(&mut i)));
        }
        out.push(Track {
            prop,
            hold,
            duration,
            keys,
        });
    }
    out
}

fn sample(t: &Track, seconds: f64) -> f64 {
    let p = (t.hold.unwrap_or(seconds) / t.duration).fract();
    let k = t.keys.windows(2).find(|k| p < k[1].0).unwrap();
    let local = (p - k[0].0) / (k[1].0 - k[0].0);
    let eased = k[0].2.as_ref().map_or(local, |e| e.progress(local));
    k[0].1 + (k[1].1 - k[0].1) * eased
}

/// Material's indicator moves as Compose's `CircularProgressIndicator`
/// draws it, frame by frame: where its tail is and how long its arc is,
/// from the layer's rotation and dash phase the reader plays.
#[test]
fn the_indicator_tracks_move_as_composes_indicator() {
    let tracks = read_tracks(&spinner_words(false));
    let dash = tracks.iter().find(|t| t.prop == DASH).unwrap();
    let rotate = tracks.iter().find(|t| t.prop == ROTATE).unwrap();
    let ease = Easing::CubicBezier {
        x1: 0.4,
        y1: 0.0,
        x2: 0.2,
        y2: 1.0,
    };
    let degrees = |a: f64| a.rem_euclid(360.0);
    for frame in 0..1200 {
        let ms = frame as f64 * 1000.0 / 60.0;
        // Compose's: the rotations step 216° a cycle, the base turns 286°,
        // the head and then the tail run 290° along the easing.
        let (cycle, local) = ((ms / 1332.0).floor(), ms % 1332.0);
        let step = (cycle % 5.0) * 216.0;
        let base = 286.0 * local / 1332.0;
        let (end, start) = if local < 666.0 {
            (290.0 * ease.progress(local / 666.0), 0.0)
        } else {
            (290.0, 290.0 * ease.progress((local - 666.0) / 666.0))
        };
        let tail = degrees(step + base + start);
        let s = ms / 1000.0;
        let played = degrees(sample(rotate, s));
        let gap = (played - tail).abs();
        assert!(
            gap.min(360.0 - gap) < 0.05,
            "{ms} ms: tail {played}, Compose {tail}"
        );
        let arc = (1.0 - sample(dash, s)) * 360.0;
        assert!(
            (arc - (end - start)).abs() < 0.05,
            "{ms} ms: sweep {arc}, Compose {}",
            end - start
        );
    }
}

/// With the device's animations removed the indicator stands still at its
/// longest arc, where its drawing already is; the tracks change with the
/// setting both ways, so a host sends them again then and only then.
#[test]
fn the_indicator_stands_still_while_animations_are_removed() {
    let (mut h, error) = Host::boot(
        &contract::compile("component App\n  view\n    progress aria-label=\"Loading\"\n")
            .unwrap()
            .encode(),
        (),
        Box::new(MonospaceMeasurer::default()),
        400.,
        500.,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    assert!(h.spinner_tracks().is_empty(), "no reader plays layers");
    h.lowering = true;
    let moving = h.spinner_tracks();
    assert_eq!(moving, h.spinner_tracks(), "the same at every paint");
    assert!(read_tracks(&moving).iter().all(|t| t.hold.is_none()));
    let reduce = |h: &mut Host<()>, on: bool| {
        let mut preferences = h.runner().viewport().preferences;
        preferences.reduced_motion = on;
        assert!(h.set_preferences(preferences).is_none());
    };
    reduce(&mut h, true);
    let still = h.spinner_tracks();
    assert_ne!(still, moving);
    let tracks = read_tracks(&still);
    for (prop, value) in [(ROTATE, 0.0), (DASH, SPINNER_REST)] {
        let t = tracks.iter().find(|t| t.prop == prop).unwrap();
        assert!(t.hold.is_some(), "held");
        for s in [0.0, 0.4, 3.0] {
            assert!((sample(t, s) - value).abs() < 1e-4, "{prop} at {s} s");
        }
    }
    reduce(&mut h, false);
    assert_eq!(h.spinner_tracks(), moving);
}

/// A look's `color` leaves the node's colour track to the reader: this host
/// paints no look's text colour, so nothing composes on the engine's sample.
/// A look's background still takes its track.
#[test]
fn a_looks_color_leaves_the_colour_track_to_the_reader() {
    let source = r##"component App
  view
    box testId="t" color="#000000"
      :hover
        color = "#ff0000"
        background-color = "#00ff00"
      text "a"
"##;
    let (h, error) = Host::boot(
        &contract::compile(source).unwrap().encode(),
        (),
        Box::new(MonospaceMeasurer::default()),
        400.,
        500.,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let node = h
        .kernel()
        .node_by_key(h.kernel().find_by_test_id("t")[0])
        .unwrap();
    assert!(!crate::host::press::paints_over(&node, Property::Color));
    assert!(crate::host::press::paints_over(
        &node,
        Property::BackgroundColor
    ));
}
