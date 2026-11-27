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
    assert_eq!(held.value, Value::new(1.5, 1.5));
    assert!(!h.played.contains_key(&node));
    assert_eq!(h.lowered_mask(binding.target), 0);
    assert!(h.lowered_changed[&node] > epoch);
    assert_eq!(h.presented(target).translate, (25., -10.));
    assert_eq!(h.presented(target).scale.0, 1.5);
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
        assert_eq!(
            held.scale().value,
            Value::new(1. + 2. * progress, 1. + 2. * progress)
        );
        assert_eq!(
            h.presented(target).translate,
            ((100. * progress) as f32, (-40. * progress) as f32)
        );
        assert_eq!(h.presented(target).scale.0, (1. + 2. * progress) as f32);
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
    assert_eq!(held.scale().value, Value::new(3., 3.));
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
    assert_eq!(h.presented(target).scale.0, 2.);
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

/// A scalar track decoded from the layer stream.
struct Track {
    prop: u32,
    hold: Option<f64>,
    duration: f64,
    iterations: f64,
    easing: Option<Easing>,
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
        let iterations = at(i + 6);
        i += 9;
        let track_easing = easing(&mut i);
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
            iterations,
            easing: track_easing,
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

#[test]
fn lowered_translate_keyframes_and_transitions_resolve_the_current_box() {
    const END: &str = "calc(10px - 50%) calc(25% - 4px)";
    for transition in [false, true] {
        let motion = if transition {
            format!(r#"translate=(moving ? "{END}" : "0px") transition="translate 1s linear""#)
        } else {
            r#"animation=(moving ? "slide 1s linear" : "none") animation-timeline=Transport"#
                .to_owned()
        };
        let source = format!(
            r#"timeline Transport
keyframes slide
  from translate="0px"
  to translate="{END}"
component App
  state moving = false
  action go
    moving = true
  view
    column
      box testId="strip" width=200 height=100 {motion}
      button testId="start" press=go
        text "Start"
"#
        );
        let (mut h, error) = Host::boot(
            &crate::test_plan(&source),
            (),
            Box::new(MonospaceMeasurer::default()),
            400.,
            500.,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        h.lowering = true;
        h.engine.set_lowered_properties(&LOWERED);
        let strip = h.kernel().find_by_test_id("strip")[0];
        let button = h.kernel().find_by_test_id("start")[0];
        let id = h.kernel().node_by_key(button).unwrap().id;
        assert!(h.dispatch_at(id, Event::Press, 0.).is_none());
        assert_eq!(h.lowered_mask(strip), LOWER_TRANSFORM);
        for (size, expected) in [((200., 100.), [-90., 21.]), ((360., 40.), [-170., 6.])] {
            let tracks = read_tracks(&h.layer_tracks(strip, &Presented::IDENTITY, size));
            for (code, end) in [TX, TY].into_iter().zip(expected) {
                let keys = &tracks.iter().find(|t| t.prop == code).unwrap().keys;
                assert_eq!(keys.first().unwrap().1, 0.);
                assert_eq!(keys.last().unwrap().1, end);
            }
        }
    }
}

/// Material's indicator moves as Compose's `CircularProgressIndicator`
/// draws it, frame by frame: where its tail is and how long its arc is,
/// from the layer's rotation and dash phase the reader plays.
#[test]
fn the_indicator_tracks_move_as_composes_indicator() {
    let tracks = read_tracks(&spinner_words(false));
    let dash = tracks.iter().find(|t| t.prop == DASH).unwrap();
    let rotate = tracks.iter().find(|t| t.prop == ROTATE).unwrap();
    assert!(tracks
        .iter()
        .all(|t| t.iterations.is_infinite() && t.easing.is_none()));
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

/// A press's scale multiplies the node's own where the engine samples it
/// (`Host::presented`), so the reader, which plays the node's scale without
/// it, takes none of the node's transform.
#[test]
fn a_press_scale_keeps_its_nodes_transform_from_the_reader() {
    for (press, lowered) in [("", LOWER_TRANSFORM), (" -exact-press-scale=0.5", 0)] {
        let source = format!("keyframes grow\n  to scale=2\ncomponent App\n  state n = 0\n  action go\n    n = n + 1\n  view\n    box press=go testId=\"t\" width=120 height=60 animation=\"grow 1s linear both\"{press}\n");
        let (mut h, error) = Host::boot(
            &contract::compile(&source).unwrap().encode(),
            (),
            Box::new(MonospaceMeasurer::default()),
            300.,
            300.,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        h.lowering = true;
        let key = h.kernel().find_by_test_id("t")[0];
        h.engine.set_lowered_properties(&LOWERED);
        let mut sync = MotionSync::default();
        h.kernel().motion_sync_node(key, &mut sync);
        h.lower_eligibility(&sync);
        assert_eq!(h.lowered_mask(key), lowered, "{press:?}");
    }
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

/// A blended SVG element is drawn alone into an island and blended onto
/// what is below it (LLP 1055.000 D19): a layer the reader moves holds no
/// backdrop to blend with, so the engine samples its animation. The same
/// element unblended is the reader's.
#[test]
fn a_blended_svg_backdrop_is_sampled_not_the_readers() {
    for (blend, clip, played) in [
        ("normal", "none", true),
        ("multiply", "none", false),
        ("normal", "path('M0 0H50V100H0Z')", false),
    ] {
        for backdrop in [false, true] {
            let foreground_blend = if backdrop { "normal" } else { blend };
            let sibling_blend = if backdrop { blend } else { "normal" };
            let source = format!(
                r##"component App
  state o = 1
  action go
    o = 0.2
  view
    box width=400 height=500
      svg width=100 height=100
        circle testId="target" cx=50 cy=50 r=40 fill="#3366cc" opacity=o transition="opacity 1s linear" mix-blend-mode="{foreground_blend}" clip-path="{clip}"
        circle cx=70 cy=50 r=20 fill="#cc6633" mix-blend-mode="{sibling_blend}"
      button testId="go" press=go
        text "Go"
"##
            );
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
            let target = h.kernel().find_by_test_id("target")[0];
            let go = h.kernel().find_by_test_id("go")[0];
            let go = h.kernel().node_by_key(go).unwrap().id;
            assert!(h.dispatch_at(go, Event::Press, 0.).is_none());
            let node = motion_node(target);
            assert_eq!(
                h.engine.is_played(node, Property::Opacity),
                played,
                "{blend}"
            );
            let mask = if played { LOWER_OPACITY } else { 0 };
            assert_eq!(h.lowered_mask(target), mask, "{blend}");
        }
    }
}

#[test]
fn a_new_blend_reclaims_a_running_backdrop_without_restarting_it() {
    for reference in 0..3 {
        for animation in [false, true] {
            let motion = if animation {
                "animation=motion"
            } else {
                "opacity=o transition=\"opacity 1s linear\""
            };
            let definition = if reference == 1 {
                "      svg width=0 height=0\n        defs\n          rect id=\"ink\" width=50 height=50 fill=\"#fd2f7c\" mix-blend-mode=blend"
            } else {
                ""
            };
            let foreground = if reference == 1 {
                "use href=\"#ink\""
            } else {
                "rect width=50 height=50 fill=\"#fd2f7c\" mix-blend-mode=blend"
            };
            let mut source = format!(
                r##"keyframes fade
  from opacity=1
  to opacity=0.2
component App
  state o = 1
  state motion = "none"
  state blend = "normal"
  action go
    o = 0.2
    motion = "fade 1s linear infinite"
  action overprint
    blend = "multiply"
  view
    column
{definition}
      svg width=100 height=100
        rect testId="target" width=100 height=100 fill="#34f1ec" {motion}
        {foreground}
      button testId="go" press=go
        text "Go"
      button testId="blend" press=overprint
        text "Blend"
"##
            );
            if reference == 2 {
                source = source.replace("rect testId=\"target\"", "rect id=\"ink\" testId=\"target\"")
                    .replace("        rect width=50 height=50 fill=\"#fd2f7c\" mix-blend-mode=blend",
                        "      svg width=100 height=100\n        rect width=100 height=100 fill=\"#34f1ec\"\n        use href=\"#ink\" mix-blend-mode=blend");
            }
            let (mut h, error) = Host::boot(
                &contract::compile(&source).unwrap().encode(),
                (),
                Box::new(MonospaceMeasurer::default()),
                200.,
                200.,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            h.lowering = true;
            h.engine.set_lowered_properties(&LOWERED);
            let target = h.kernel().find_by_test_id("target")[0];
            let id = h.kernel().node_by_key(target).unwrap().id;
            let node = motion_node(target);
            let go = h
                .kernel()
                .node_by_key(h.kernel().find_by_test_id("go")[0])
                .unwrap()
                .id;
            assert!(h.dispatch_at(go, Event::Press, 0.).is_none());
            assert_eq!(h.lowered_mask(target), LOWER_OPACITY);
            h.tick(250.);
            let epoch = h.lowered_epoch;
            let blend = h
                .kernel()
                .node_by_key(h.kernel().find_by_test_id("blend")[0])
                .unwrap()
                .id;
            assert!(h.dispatch_at(blend, Event::Press, 250.).is_none());
            assert_eq!(h.lowered_mask(target), 0);
            assert!(!h.engine.is_played(node, Property::Opacity));
            assert!(h.lowered_changed[&node] > epoch);
            assert!((h.presented(id).opacity - 0.8).abs() < 0.01);
            h.tick(500.);
            assert!((h.presented(id).opacity - 0.6).abs() < 0.01);
            if animation {
                assert!(h.engine.node_sampled(node));
                h.tick(1250.);
                assert!((h.presented(id).opacity - 0.8).abs() < 0.01);
            } else {
                h.tick(1000.);
                assert!((h.presented(id).opacity - 0.2).abs() < 0.01);
            }
        }
    }
}

#[test]
fn axis_keyframes_use_the_spatial_painter_under_the_native_clock() {
    for (frame, row, spatial) in [
        ("-90deg x", "0", true),
        ("-90", "0deg x", true),
        ("-90deg z", "0", false),
    ] {
        let source = format!("keyframes fold\n  to rotate=\"{frame}\"\ncomponent App\n  view\n    view testId=\"fold\" width=120 height=60 rotate=\"{row}\" animation=\"fold 1s linear both\"\n");
        let (mut h, error) = Host::boot(
            &contract::compile(&source).unwrap().encode(),
            (),
            Box::new(MonospaceMeasurer::default()),
            300.,
            300.,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        h.lowering = true;
        let key = h.kernel().find_by_test_id("fold")[0];
        let id = h.kernel().node_by_key(key).unwrap().id;
        h.engine.set_lowered_properties(&LOWERED);
        let mut sync = MotionSync::default();
        h.kernel().motion_sync_node(key, &mut sync);
        h.lower_eligibility(&sync);
        assert_eq!(
            h.lowered_mask(key),
            if spatial { 0 } else { LOWER_TRANSFORM }
        );
        if !spatial {
            continue;
        }
        assert!(h.advance(500.).is_none());
        let shown = h.presented(id);
        assert_eq!(shown.rotate, -45.0);
        assert_eq!(shown.rotate_axis.0, [1.0, 0.0, 0.0]);
        assert_eq!(
            h.lowered_mask(key),
            0,
            "a 2D reader track cannot draw this fold"
        );
        assert!(h.advance(1000.).is_none());
        assert_eq!(h.presented(id).rotate, -90.0);
    }
}

#[test]
fn a_transition_from_a_spatial_axis_stays_with_the_painter() {
    let source = "component App\n  state on = false\n  action go\n    on = true\n  view\n    column\n      button \"Go\" testId=\"go\" press=go\n      view testId=\"box\" width=120 height=60 rotate=(on ? \"45deg z\" : \"90deg x\") transition=\"rotate 1s linear\"\n";
    let (mut h, error) = Host::boot(
        &contract::compile(source).unwrap().encode(),
        (),
        Box::new(MonospaceMeasurer::default()),
        300.,
        300.,
    )
    .unwrap();
    assert!(error.is_none());
    h.lowering = true;
    let key = h.kernel().find_by_test_id("box")[0];
    let go = h
        .kernel()
        .node_by_key(h.kernel().find_by_test_id("go")[0])
        .unwrap()
        .id;
    h.dispatch_at(go, Event::Press, 0.);
    h.advance(500.);
    assert!(!h.engine.is_played(motion_node(key), Property::Rotate));
    assert_eq!(h.lowered_mask(key), 0);
    let v = h.engine.value(motion_node(key), Property::Rotate).unwrap();
    let expected = exact_motion::rotation::interpolate(
        exact_motion::rotation::parse("x 90deg").unwrap(),
        exact_motion::rotation::parse("z 45deg").unwrap(),
        0.5,
    );
    assert!((v.x - expected.x).abs() < 1e-8, "{v:?}");
}

#[test]
fn fade_keyframes_stay_sampled_during_a_transition_back_to_z() {
    let source = r#"keyframes fade
  from opacity=1
  to opacity=0
component App
  state on = false
  action go
    on = true
  view
    column
      button "Go" testId="go" press=go
      view testId="box" width=120 height=60 rotate=(on ? "z 0deg" : "x 60deg") transition="rotate 1s linear" animation="fade 1s linear both"
"#;
    let (mut h, error) = Host::boot(
        &contract::compile(source).unwrap().encode(),
        (),
        Box::new(MonospaceMeasurer::default()),
        300.,
        300.,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    h.lowering = true;
    h.engine.set_lowered_properties(&LOWERED);
    let key = h.kernel().find_by_test_id("box")[0];
    let id = h.kernel().node_by_key(key).unwrap().id;
    let mut sync = MotionSync::default();
    h.kernel().motion_sync_node(key, &mut sync);
    h.lower_eligibility(&sync);
    let go = h
        .kernel()
        .node_by_key(h.kernel().find_by_test_id("go")[0])
        .unwrap()
        .id;
    assert!(h.dispatch_at(go, Event::Press, 0.).is_none());
    assert_eq!(h.lowered_mask(key), 0);
    assert!(h.advance(500.).is_none());
    assert_eq!(h.presented(id).opacity, 0.5);
    assert_eq!(h.lowered_mask(key), 0);
}

#[test]
fn negative_z_literal_and_bound_rows_present_the_same_inherited_rotation() {
    for row in ["\"0 0 -1 0deg\"", "rotation"] {
        let source = format!("keyframes spin\n  to rotate=60\ncomponent App\n  state rotation = \"0 0 -1 0deg\"\n  view\n    view testId=\"box\" rotate={row} animation=\"spin 1s linear both\"\n");
        let (mut h, error) = Host::boot(
            &contract::compile(&source).unwrap().encode(),
            (),
            Box::new(MonospaceMeasurer::default()),
            300.,
            300.,
        )
        .unwrap();
        assert!(error.is_none());
        let key = h.kernel().find_by_test_id("box")[0];
        let id = h.kernel().node_by_key(key).unwrap().id;
        assert!(h.advance(500.).is_none());
        assert_eq!(h.presented(id).rotate, -30.);
        assert!(h.advance(1000.).is_none());
        assert_eq!(h.presented(id).rotate, -60.);
    }
}

#[test]
fn computed_svg_animation_names_are_refused_after_resolution() {
    for (row, frame) in [
        ("0deg", "-90deg x"),
        ("0deg x", "60"),
        ("10deg x", "60deg z"),
    ] {
        let source = format!("keyframes fold\n  to rotate=\"{frame}\"\ncomponent App\n  state animation = \"fold 1s linear both\"\n  view\n    svg width=100 height=100\n      rect width=50 height=50 rotate=\"{row}\" animation=animation\n");
        let result = Host::boot(
            &contract::compile(&source).unwrap().encode(),
            (),
            Box::new(MonospaceMeasurer::default()),
            300.,
            300.,
        );
        assert!(matches!(result, Err(ref e) if format!("{e:?}").contains("SpatialSvgAnimation")));
    }
}

#[test]
fn spatial_folds_keep_fades_sampled_and_reclaim_reader_tracks() {
    use crate::presenter::{PainterChoice, Presenter};
    for (from, to, fold_at) in [("x 60deg", "z 0deg", 0.), ("z 0deg", "x 60deg", 250.)] {
        let source = format!(
            r##"component App
  state folded = false
  state faded = false
  action fold
    folded = true
  action fade
    faded = true
  view
    box width=200 height=200 background-color="#ffffff"
      box testId="box" margin=50 width=100 height=100 background-color="#ff0000" rotate=(folded ? "{to}" : "{from}") opacity=(faded ? 0 : 1) transition="rotate 1s linear, opacity 1s linear"
      button "Fold" testId="fold" press=fold
      button "Fade" testId="fade" press=fade
"##
        );
        let (mut p, error) = Presenter::boot_with(
            &contract::compile(&source).unwrap().encode(),
            (),
            (200., 200.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.host_mut().lowering = true;
        let key = p.host().kernel().find_by_test_id("box")[0];
        let id = p.host().kernel().node_by_key(key).unwrap().id;
        let button = |p: &Presenter<()>, name| {
            p.host()
                .kernel()
                .node_by_key(p.host().kernel().find_by_test_id(name)[0])
                .unwrap()
                .id
        };
        p.frame();
        p.tap(button(&p, "fade")).unwrap();
        if fold_at > 0. {
            assert!(p
                .host()
                .engine
                .is_played(motion_node(key), Property::Opacity));
            p.tick(fold_at);
        }
        p.tap(button(&p, "fold")).unwrap();
        assert!(!p
            .host()
            .engine
            .is_played(motion_node(key), Property::Opacity));
        assert_eq!(p.host().lowered_mask(key), 0);
        p.tick(500.);
        assert!((p.host().presented(id).opacity - 0.5).abs() < 1e-5);
        assert!(p.host().presented(id).rotate_axis.is_3d());
        let c = p.frame().pixel(100, 100).unwrap().demultiply();
        assert_eq!(c.red(), 255);
        assert!(
            (i16::from(c.green()) - 128).abs() <= 1,
            "{from} -> {to}: {c:?}"
        );
        assert_eq!(c.green(), c.blue());
    }
}

/// A boot's play waits for the first present, which [`Host::layer_tracks`]
/// sends the reader as a held track; the present starts it and owes the
/// reader its running track (LLP 1003.001 D9).
#[test]
fn a_boots_play_waits_for_the_present_and_its_start_owes_a_picture() {
    let previous = exact_runner::page::set_awaits_present(true);
    let (mut h, error) = Host::boot(
        &contract::compile(
            "keyframes enter\n  from opacity=0\n  to opacity=1\ncomponent App\n  view\n    box testId=\"logo\" width=10 height=10 animation=\"enter 400ms linear both\"\n",
        )
        .unwrap()
        .encode(),
        (),
        Box::new(MonospaceMeasurer::default()),
        400.,
        500.,
    )
    .unwrap();
    exact_runner::page::set_awaits_present(previous);
    assert!(error.is_none(), "{error:?}");
    let node = motion_node(h.kernel().find_by_test_id("logo")[0]);
    let play = |h: &Host<()>| h.engine.animation_plays(node)[0].clone();
    assert_eq!(play(&h).pending, Some(0.0), "waits for the present");
    let epoch = h.lowered_epoch;
    let (paint, _) = h.picture_presented(h.engine.sample_time(), 300.);
    assert!(paint, "the started play owes the reader its track");
    assert!(h.lowered_changed[&node] > epoch);
    assert_eq!((play(&h).pending, play(&h).start), (None, 0.3));
}

#[test]
fn per_axis_scale_reader_tracks_keep_both_axes_and_underlying_values() {
    for transition in [false, true] {
        let motion = if transition {
            r#"scale=(moving ? "3 0" : "1 0.5") transition="scale 1s linear""#
        } else {
            r#"scale="1 0.5" animation=(moving ? "flip 1s linear both" : "none")"#
        };
        let source = format!(
            r#"keyframes flip
  to scale="3 0"
component App
  state moving = false
  action go
    moving = true
  view
    column
      box testId="target" width=100 height=100 {motion}
      button testId="go" press=go
        text "Go"
"#
        );
        let (mut h, error) = Host::boot(
            &crate::test_plan(&source),
            (),
            Box::new(MonospaceMeasurer::default()),
            400.,
            500.,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        h.lowering = true;
        h.engine.set_lowered_properties(&LOWERED);
        let go = h.kernel().find_by_test_id("go")[0];
        let go = h.kernel().node_by_key(go).unwrap().id;
        assert!(h.dispatch_at(go, Event::Press, 0.).is_none());
        let key = h.kernel().find_by_test_id("target")[0];
        let base = Presented {
            scale: (1., 0.5),
            ..Presented::IDENTITY
        };
        let tracks = read_tracks(&h.layer_tracks(key, &base, (100., 100.)));
        let x = tracks.iter().find(|t| t.prop == SCALE).unwrap();
        let y = tracks.iter().find(|t| t.prop == SCALE_Y).unwrap();
        assert_eq!((sample(x, 0.5), sample(y, 0.5)), (2., 0.25));
    }
}
