//! Motion starts at the first frame that shows it (LLP 1003.001): with the
//! rule on, a curve an author's commit begins waits at its start until the
//! next presented frame, which starts it there; inputs keep their own clock.

use exact_motion::{
    Change, Easing, Engine, HoldEnd, Property, SpringConfig, TimingFunction, Transition,
    TransitionProperty, Transitions, Value,
};

const NODE: u64 = 7;

fn engine(seconds: f64, timing: TimingFunction) -> Engine {
    let mut e = Engine::new();
    e.set_start_on_frame(true, 0.0).unwrap();
    e.set_transitions(
        NODE,
        Transitions(vec![Transition::new(
            TransitionProperty::All,
            seconds,
            timing,
        )]),
    )
    .unwrap();
    observe(&mut e, 0.0);
    e
}

fn linear() -> TimingFunction {
    TimingFunction::Easing(Easing::Linear)
}

fn observe(e: &mut Engine, value: f64) {
    e.observe(Change {
        node: NODE,
        property: Property::Opacity,
        value: Value::scalar(value),
        velocity: None,
    })
    .unwrap();
}

fn opacity(e: &Engine) -> f64 {
    e.value(NODE, Property::Opacity).unwrap().x
}

#[test]
fn a_curve_begun_between_frames_starts_at_the_next_frame() {
    let mut e = engine(1.0, linear());
    e.advance(0.004).unwrap();
    observe(&mut e, 1.0);
    // The commit's own work and the wait for the frame show nothing.
    e.advance(0.012).unwrap();
    assert_eq!(opacity(&e), 0.0);
    e.present_frame(0.030, 0.030).unwrap();
    assert_eq!(opacity(&e), 0.0, "the first frame shows the start");
    e.advance(0.040).unwrap();
    e.present_frame(0.050, 0.050).unwrap();
    assert!((opacity(&e) - 0.020).abs() < 1e-12, "then one interval");
    assert_eq!(e.settle_time(), Some(1.030));
}

#[test]
fn a_curve_on_a_node_not_yet_shown_starts_at_the_first_frame_after_it_shows() {
    let mut e = engine(0.1, linear());
    observe(&mut e, 1.0);
    e.set_unshown([NODE]);
    // Frames present while the node is prepared off screen: its curve waits.
    for frame in [0.016, 0.100, 0.200] {
        e.advance(frame).unwrap();
        e.present_frame(frame, frame).unwrap();
        assert_eq!(opacity(&e), 0.0, "at {frame}");
    }
    e.set_unshown([]);
    e.present_frame(0.216, 0.216).unwrap();
    assert_eq!(opacity(&e), 0.0, "the frame that shows it starts it");
    e.present_frame(0.266, 0.266).unwrap();
    assert!((opacity(&e) - 0.5).abs() < 1e-9);
}

#[test]
fn a_pending_curve_is_not_moved_or_retired_by_another_commit() {
    let mut e = engine(0.005, linear());
    observe(&mut e, 1.0);
    e.advance(0.010).unwrap();
    assert!(!e.quiescent(), "a 5 ms curve still waits for its frame");
    assert_eq!(opacity(&e), 0.0);
    e.present_frame(0.016, 0.016).unwrap();
    assert_eq!(opacity(&e), 0.0);
    e.present_frame(0.0185, 0.0185).unwrap();
    assert!((opacity(&e) - 0.5).abs() < 1e-9);
}

#[test]
fn an_interruption_before_the_frame_starts_from_where_it_stood() {
    let mut e = engine(1.0, linear());
    observe(&mut e, 1.0);
    e.advance(0.010).unwrap();
    observe(&mut e, 0.5);
    e.present_frame(0.020, 0.020).unwrap();
    e.present_frame(0.520, 0.520).unwrap();
    assert!(
        (opacity(&e) - 0.25).abs() < 1e-9,
        "0 → 0.5 over 1 s, half way"
    );
}

#[test]
fn a_delay_is_kept_and_a_negative_delay_keeps_its_skip() {
    for (delay, at) in [(0.1, 0.0), (-0.25, 0.25)] {
        let mut e = Engine::new();
        e.set_start_on_frame(true, 0.0).unwrap();
        let mut t = Transition::new(TransitionProperty::All, 1.0, linear());
        t.delay = delay;
        e.set_transitions(NODE, Transitions(vec![t])).unwrap();
        observe(&mut e, 0.0);
        observe(&mut e, 1.0);
        e.advance(0.010).unwrap();
        e.present_frame(0.020, 0.020).unwrap();
        assert!((opacity(&e) - at).abs() < 1e-9, "delay {delay}");
        e.present_frame(0.520, 0.520).unwrap();
        assert!((opacity(&e) - (at + (0.5 - delay.max(0.0)).max(0.0))).abs() < 1e-9);
    }
}

#[test]
fn input_keeps_its_own_clock_behind_the_frame() {
    let mut e = engine(1.0, linear());
    e.present_frame(1.000, 1.000).unwrap();
    // A touch whose time is before the frame the display just presented.
    let held = e
        .begin_hold(NODE, Property::Opacity, 0.990, None)
        .unwrap()
        .unwrap();
    assert!(e
        .update_hold(held.token, 0.995, Value::scalar(0.3))
        .unwrap());
    assert!(e.end_hold(held.token, 0.998, HoldEnd::Cancel).unwrap());
}

#[test]
fn a_fling_keeps_its_release_instant_and_a_cancel_waits() {
    let spring = TimingFunction::Spring(SpringConfig::default());
    let release = |end: HoldEnd, on: bool| {
        let mut e = engine(0.0, spring.clone());
        e.set_start_on_frame(on, 0.0).unwrap();
        observe(&mut e, 1.0);
        e.advance(0.5).unwrap();
        if on {
            e.present_frame(0.5, 0.5).unwrap();
        }
        let held = e
            .begin_hold(NODE, Property::Opacity, 0.5, None)
            .unwrap()
            .unwrap();
        e.update_hold(held.token, 0.51, Value::scalar(0.2)).unwrap();
        if on {
            // The display is already past the release when it arrives.
            e.present_frame(0.525, 0.525).unwrap();
        }
        e.end_hold(held.token, 0.52, end).unwrap();
        e.advance(0.53).unwrap();
        if on {
            e.present_frame(0.54, 0.54).unwrap();
        } else {
            e.advance(0.54).unwrap();
        }
        opacity(&e)
    };
    let fling = HoldEnd::Release {
        velocity: Value::scalar(3.0),
    };
    assert_eq!(
        release(fling, true),
        release(fling, false),
        "v·Δt from the release"
    );
    let cancel = release(HoldEnd::Cancel, true);
    assert!(
        (cancel - 0.2).abs() < 1e-12,
        "a cancel's first frame is its start: {cancel}"
    );
}

#[test]
fn an_exit_ends_its_own_plays_after_the_frame_starts_them() {
    let mut e = engine(1.0, linear());
    let exit = crate::keyframed("fade 300ms @keyframes fade{to{opacity:0}}").unwrap();
    observe(&mut e, 1.0);
    e.advance(0.010).unwrap();
    e.play_exit(NODE, &exit).unwrap();
    assert_eq!(e.exit_end(NODE, 1), None, "no destroy while it waits");
    e.present_frame(0.020, 0.020).unwrap();
    assert_eq!(e.exit_end(NODE, 1), Some(0.320));
    assert_eq!(e.curve_start(NODE, Property::Opacity), Some(0.020));
}

#[test]
fn a_lowered_play_keeps_the_clock_until_its_frame() {
    let mut e = Engine::new();
    e.set_start_on_frame(true, 0.0).unwrap();
    e.set_lowered(true);
    let spin = crate::keyframed("spin 1s @keyframes spin{to{opacity:0}}").unwrap();
    e.set_animations(NODE, &spin).unwrap();
    e.advance(0.010).unwrap();
    assert!(!e.quiescent(), "the display link runs to start it");
    assert_eq!(e.animation_plays(NODE)[0].local(0.010), 0.0);
    assert_eq!(e.present_frame(0.020, 0.020).unwrap(), vec![NODE]);
    assert_eq!(e.animation_plays(NODE)[0].start, 0.020);
    assert!(e.quiescent(), "Core Animation plays it from here");
}

#[test]
fn joins_to_an_idle_clock_before_the_frame_move_with_its_origin() {
    let row = || {
        crate::keyframed(
            "pulse 800ms infinite alternate @keyframes pulse{from{opacity:0.4}to{opacity:1}}",
        )
        .unwrap()
    };
    let mut e = Engine::new();
    e.set_start_on_frame(true, 0.0).unwrap();
    for node in [3, 4] {
        e.set_animation_clock(node, Some("Pulse"));
    }
    e.advance(0.010).unwrap();
    e.set_animations(3, &row()).unwrap();
    e.advance(0.015).unwrap();
    e.set_animations(4, &row()).unwrap();
    assert!(!e.quiescent());
    e.present_frame(0.030, 0.030).unwrap();
    let starts: Vec<f64> = [3, 4].map(|n| e.animation_plays(n)[0].start).to_vec();
    assert_eq!(starts, vec![0.030, 0.030], "one phase, from the frame");
}

#[test]
fn turning_the_rule_off_starts_what_waits_there() {
    let mut e = engine(1.0, linear());
    observe(&mut e, 1.0);
    e.advance(0.010).unwrap();
    e.set_start_on_frame(false, 0.025).unwrap();
    e.advance(0.525).unwrap();
    assert!((opacity(&e) - 0.5).abs() < 1e-9);
}

#[test]
fn a_takeover_before_a_curves_begin_starts_it_at_its_begin_and_forgets_frames() {
    let mut e = engine(1.0, linear());
    e.present_frame(0.050, 0.050).unwrap();
    e.advance(0.020).unwrap();
    observe(&mut e, 1.0); // begins at the sampling time, 0.050
    e.set_start_on_frame(false, 0.030).unwrap();
    assert!(!e.starts_on_frame());
    assert_eq!(e.sample_time(), 0.030, "the agent's clock alone from here");
    e.advance(0.550).unwrap();
    assert!(
        (opacity(&e) - 0.5).abs() < 1e-9,
        "started at its begin, not stranded"
    );
}

fn pulse(text: &str) -> exact_motion::Animations {
    crate::keyframed(&format!(
        "{text} @keyframes pulse{{from{{opacity:0.4}}to{{opacity:1}}}}"
    ))
    .unwrap()
}

#[test]
fn a_lone_member_resumed_keeps_its_clocks_phase_with_the_rule_on() {
    let mut e = Engine::new();
    e.set_start_on_frame(true, 0.0).unwrap();
    e.set_animation_clock(3, Some("Pending"));
    e.set_animations(3, &pulse("pulse 1s infinite")).unwrap();
    e.present_frame(0.016, 0.016).unwrap();
    e.advance(0.4).unwrap();
    e.set_animations(3, &pulse("pulse 1s infinite paused"))
        .unwrap();
    e.advance(7.9).unwrap();
    e.set_animations(3, &pulse("pulse 1s infinite running"))
        .unwrap();
    assert_eq!(
        e.animation_plays(3)[0].start,
        7.016,
        "paused, it kept the clock busy"
    );
    assert_eq!(
        e.animation_plays(3)[0].pending,
        None,
        "a running clock's phase"
    );
}

#[test]
fn a_join_to_an_origin_waiting_for_the_frame_waits_with_it() {
    let mut e = Engine::new();
    e.set_start_on_frame(true, 0.0).unwrap();
    e.set_animations(4, &pulse("pulse 1s infinite paused"))
        .unwrap();
    e.advance(10.3).unwrap();
    e.set_animation_clock(4, Some("Pending"));
    e.advance(10.31).unwrap();
    e.set_animation_clock(5, Some("Pending"));
    e.set_animations(5, &pulse("pulse 1s infinite")).unwrap();
    assert!(!e.quiescent(), "the origin waits for a frame");
    e.present_frame(10.33, 10.33).unwrap();
    assert_eq!(
        e.animation_plays(5)[0].start,
        10.33,
        "the joiner starts with the origin"
    );
}

#[test]
fn a_member_born_after_a_late_frame_waits_for_the_next_and_keeps_the_phase() {
    let mut e = Engine::new();
    e.set_start_on_frame(true, 0.0).unwrap();
    for node in [3, 4] {
        e.set_animation_clock(node, Some("Pending"));
    }
    e.set_animations(3, &pulse("pulse 1s infinite")).unwrap();
    e.advance(0.020).unwrap();
    e.set_animations(4, &pulse("pulse 1s infinite")).unwrap();
    // A tick for the 16 ms frame reaches the engine after the second join.
    e.present_frame(0.016, 0.016).unwrap();
    assert_eq!(e.animation_plays(3)[0].start, 0.016);
    assert!(
        e.animation_plays(4)[0].pending.is_some(),
        "born after that frame"
    );
    e.present_frame(0.033, 0.033).unwrap();
    assert_eq!(e.animation_plays(4)[0].start, 0.016, "the origin's phase");
}

#[test]
fn the_takeover_samples_at_its_own_instant() {
    let mut e = engine(1.0, linear());
    observe(&mut e, 1.0);
    e.present_frame(0.016, 0.016).unwrap(); // running from 0.016
    e.present_frame(0.216, 0.216).unwrap(); // a frame ahead of the wall
    e.advance(0.150).unwrap();
    e.set_start_on_frame(false, 0.200).unwrap();
    assert!(
        (opacity(&e) - 0.184).abs() < 1e-9,
        "at the takeover, not the forgotten frame"
    );
}

#[test]
fn a_curve_ending_between_the_takeover_and_the_last_frame_is_still_running() {
    let mut e = engine(0.2, linear());
    observe(&mut e, 1.0);
    e.present_frame(0.016, 0.016).unwrap(); // ends at 0.216
    e.present_frame(0.210, 0.210).unwrap(); // a frame ahead of the wall
    e.advance(0.150).unwrap();
    e.set_start_on_frame(false, 0.200).unwrap();
    assert!(!e.quiescent(), "still running at the takeover");
    assert!((opacity(&e) - 0.92).abs() < 1e-9);
}

#[test]
fn two_plays_of_one_node_born_around_a_late_frame_share_the_origins_phase() {
    let mut e = Engine::new();
    e.set_start_on_frame(true, 0.0).unwrap();
    e.set_animation_clock(3, Some("Pending"));
    e.set_animations(3, &pulse("pulse 1s infinite")).unwrap();
    e.advance(0.020).unwrap();
    let both = crate::keyframed(
        "pulse 1s infinite, glow 1s infinite @keyframes pulse{from{opacity:0.4}to{opacity:1}} @keyframes glow{from{scale:0.9}to{scale:1}}",
    )
    .unwrap();
    e.set_animations(3, &both).unwrap();
    e.present_frame(0.016, 0.016).unwrap();
    assert_eq!(e.animation_plays(3)[0].start, 0.016);
    e.present_frame(0.033, 0.033).unwrap();
    assert_eq!(
        e.animation_plays(3)[1].start,
        0.016,
        "the origin's phase, not its own frame"
    );
}

#[test]
fn an_exit_appended_to_a_clock_member_starts_at_its_own_frame() {
    let mut e = Engine::new();
    e.set_start_on_frame(true, 0.0).unwrap();
    e.set_animation_clock(3, Some("Pending"));
    e.set_animations(3, &pulse("pulse 1s infinite")).unwrap();
    e.advance(0.020).unwrap();
    let exit = crate::keyframed("fade 300ms @keyframes fade{to{opacity:0}}").unwrap();
    e.play_exit(3, &exit).unwrap();
    e.present_frame(0.016, 0.016).unwrap();
    assert_eq!(e.animation_plays(3)[0].start, 0.016);
    e.present_frame(0.033, 0.033).unwrap();
    let end = e.exit_end(3, 1).unwrap();
    assert!(
        (end - 0.333).abs() < 1e-9,
        "an exit never joins the clock: {end}"
    );
}

// LLP 1003.001 D9: a present starts what its picture carries, at the present.

#[test]
fn a_present_starts_only_what_its_picture_carries() {
    let mut e = engine(1.0, linear());
    e.advance(0.010).unwrap();
    observe(&mut e, 1.0); // begins at 0.010; the picture produced at 0.016 carries it
    e.advance(0.020).unwrap();
    e.set_transitions(
        NODE + 1,
        Transitions(vec![Transition::new(
            TransitionProperty::All,
            1.0,
            linear(),
        )]),
    )
    .unwrap();
    let other = |e: &mut Engine, value| {
        e.observe(Change {
            node: NODE + 1,
            property: Property::Opacity,
            value: Value::scalar(value),
            velocity: None,
        })
        .unwrap()
    };
    other(&mut e, 0.0);
    other(&mut e, 1.0); // born at 0.020, after that picture
                        // The picture's present is heard at 0.300: it starts the first curve there.
    e.present_frame(0.016, 0.300).unwrap();
    assert_eq!(e.curve_start(NODE, Property::Opacity), Some(0.300));
    assert_eq!(
        e.curve_start(NODE + 1, Property::Opacity),
        None,
        "not in that picture"
    );
    // An older picture's present, heard late, starts nothing.
    e.present_frame(0.012, 0.400).unwrap();
    assert_eq!(e.curve_start(NODE + 1, Property::Opacity), None);
    // The next picture carries it, and its present starts it.
    e.present_frame(0.310, 0.330).unwrap();
    assert_eq!(e.curve_start(NODE + 1, Property::Opacity), Some(0.330));
    e.advance(0.400).unwrap();
    assert!(
        (opacity(&e) - 0.100).abs() < 1e-12,
        "timed from its present"
    );
}

#[test]
fn a_release_starts_each_hold_at_its_commit_and_keeps_shown() {
    let mut e = engine(1.0, linear());
    e.present_frame(0.005, 0.005).unwrap();
    e.advance(0.010).unwrap();
    observe(&mut e, 1.0);
    assert_eq!(e.held_since(), Some(0.010));
    assert_eq!(e.held_deadline(), Some(0.010 + exact_motion::HOLD_DEADLINE));
    e.advance(0.500).unwrap();
    e.release_held(f64::INFINITY).unwrap();
    assert_eq!(e.curve_start(NODE, Property::Opacity), Some(0.010));
    assert!((opacity(&e) - 0.490).abs() < 1e-12, "timed from its commit");
    assert_eq!(e.held_since(), None);
    assert_eq!(e.sample_time(), 0.500, "shown is kept, not forgotten");
}

#[test]
fn an_overdue_hold_is_released_at_the_deadline() {
    let mut e = engine(10.0, linear());
    e.advance(0.010).unwrap();
    observe(&mut e, 1.0);
    e.advance(1.0).unwrap();
    assert!(e.release_overdue(1.0).unwrap().is_empty());
    assert_eq!(e.curve_start(NODE, Property::Opacity), None);
    e.advance(2.010).unwrap();
    e.release_overdue(2.010).unwrap();
    assert_eq!(e.curve_start(NODE, Property::Opacity), Some(0.010));
}

#[test]
fn nothing_is_held_while_nothing_will_present() {
    let mut e = engine(1.0, linear());
    e.advance(0.010).unwrap();
    observe(&mut e, 1.0);
    // The page hides: the hold is released at its commit.
    e.advance(0.100).unwrap();
    e.set_presentable(false).unwrap();
    assert_eq!(e.curve_start(NODE, Property::Opacity), Some(0.010));
    // A commit while hidden starts at the commit.
    e.advance(0.200).unwrap();
    observe(&mut e, 0.0);
    assert_eq!(e.curve_start(NODE, Property::Opacity), Some(0.200));
    // Shown again: holds resume.
    e.set_presentable(true).unwrap();
    e.advance(0.300).unwrap();
    observe(&mut e, 1.0);
    assert_eq!(e.curve_start(NODE, Property::Opacity), None);
    assert!(e.starts_on_frame());
}

#[test]
fn the_web_starts_one_curve_at_its_ready_time() {
    let mut e = engine(1.0, linear());
    e.advance(0.010).unwrap();
    observe(&mut e, 1.0);
    assert!(
        !e.present_curve(NODE, Property::Scale, 0.050).unwrap(),
        "nothing held there"
    );
    assert!(e.present_curve(NODE, Property::Opacity, 0.050).unwrap());
    assert_eq!(e.curve_start(NODE, Property::Opacity), Some(0.050));
    assert!(
        !e.present_curve(NODE, Property::Opacity, 0.060).unwrap(),
        "started once"
    );
    assert_eq!(e.held_since(), None);
}

#[test]
fn an_idle_clock_origin_waits_for_the_present_or_its_release() {
    let pulse = || {
        crate::keyframed(
            "pulse 800ms infinite alternate @keyframes pulse{from{opacity:0.4}to{opacity:1}}",
        )
        .unwrap()
    };
    for release in [false, true] {
        let mut e = Engine::new();
        e.set_start_on_frame(true, 0.0).unwrap();
        e.advance(0.010).unwrap();
        e.set_animation_clock(NODE, Some("Beat"));
        e.set_animations(NODE, &pulse()).unwrap();
        assert_eq!(e.held_since(), Some(0.010));
        e.advance(0.100).unwrap();
        if release {
            e.release_held(f64::INFINITY).unwrap();
            assert_eq!(e.animation_plays(NODE)[0].start, 0.010, "at its commit");
        } else {
            e.present_frame(0.050, 0.200).unwrap();
            assert_eq!(e.animation_plays(NODE)[0].start, 0.200, "at the present");
        }
        assert_eq!(e.held_since(), None);
    }
}

#[test]
fn cancelling_pending_tracks_or_taking_a_hold_leaves_no_stale_pending_keys() {
    use exact_motion::{FilterKind, FilterStep, FilterValue, PathCommand, PathValue};

    // Path transition cancelled to None and to a non-interpolable shape before present_frame.
    let cmd = |verb: u8, x: f64, y: f64| PathCommand {
        verb,
        args: [x, y, 0.0, 0.0, 0.0, 0.0, 0.0],
    };
    let line = |x: f64| PathValue(vec![cmd(b'M', 0.0, 0.0), cmd(b'L', x, 0.0)]);
    let tri = PathValue(vec![
        cmd(b'M', 0.0, 0.0),
        cmd(b'L', 10.0, 0.0),
        cmd(b'L', 5.0, 5.0),
    ]);
    for cancel_to in [None, Some(tri)] {
        let mut e = engine(1.0, linear());
        e.observe_path(NODE, Some(line(10.0)));
        e.observe_path(NODE, Some(line(20.0)));
        assert!(e.held_since().is_some());
        e.set_unshown([NODE]);
        e.observe_path(NODE, cancel_to);
        e.observe_path(NODE, None);
        e.set_transitions(NODE, Transitions(Vec::new())).unwrap();
        e.present_frame(0.016, 0.016).unwrap();
        assert_eq!(e.held_since(), None);
        assert!(e.at_rest(NODE), "cancelled path must leave node at rest");
    }

    // Filter transition cancelled to None and to a non-interpolable list before present_frame.
    let blur = |r: f64| FilterValue(vec![FilterStep::new(FilterKind::Blur, r)]);
    let sep = FilterValue(vec![FilterStep::new(FilterKind::Sepia, 1.0)]);
    for cancel_to in [None, Some(sep)] {
        let mut e = engine(1.0, linear());
        e.observe_filter(NODE, Some(blur(0.0)), true);
        e.observe_filter(NODE, Some(blur(8.0)), false);
        assert!(e.held_since().is_some());
        e.set_unshown([NODE]);
        e.observe_filter(NODE, cancel_to, false);
        e.observe_filter(NODE, None, false);
        e.set_transitions(NODE, Transitions(Vec::new())).unwrap();
        e.present_frame(0.016, 0.016).unwrap();
        assert_eq!(e.held_since(), None);
        assert!(e.at_rest(NODE), "cancelled filter must leave node at rest");
    }

    // Single hold and transform hold taking over pending transitions clear self.pending.
    let mut e = engine(1.0, linear());
    observe(&mut e, 1.0);
    assert!(e.held_since().is_some());
    e.set_unshown([NODE]);
    let hold = e
        .begin_hold(NODE, Property::Opacity, 0.01, Some(Value::scalar(1.0)))
        .unwrap()
        .unwrap();
    assert!(e.end_hold(hold.token, 0.02, HoldEnd::Cancel).unwrap());
    e.set_transitions(NODE, Transitions(Vec::new())).unwrap();
    e.present_frame(0.03, 0.03).unwrap();
    assert_eq!(e.held_since(), None);
    assert!(
        e.at_rest(NODE),
        "settled hold takeover must leave node at rest"
    );

    let mut e = engine(1.0, linear());
    for (p, v0, v1) in [
        (Property::Translate, Value::ZERO, Value::new(10.0, 0.0)),
        (Property::Scale, Value::new(1.0, 1.0), Value::new(2.0, 2.0)),
    ] {
        e.observe(Change {
            node: NODE,
            property: p,
            value: v0,
            velocity: None,
        })
        .unwrap();
        e.observe(Change {
            node: NODE,
            property: p,
            value: v1,
            velocity: None,
        })
        .unwrap();
    }
    assert!(e.held_since().is_some());
    e.set_unshown([NODE]);
    let pair = e
        .begin_transform_hold(
            NODE,
            0.01,
            Some([Value::new(10.0, 0.0), Value::new(2.0, 2.0)]),
        )
        .unwrap()
        .unwrap();
    assert!(e
        .end_hold(pair.translate().token, 0.02, HoldEnd::Cancel)
        .unwrap());
    assert!(e
        .end_hold(pair.scale().token, 0.02, HoldEnd::Cancel)
        .unwrap());
    e.set_transitions(NODE, Transitions(Vec::new())).unwrap();
    e.present_frame(0.03, 0.03).unwrap();
    assert_eq!(e.held_since(), None);
    assert!(
        e.at_rest(NODE),
        "settled transform hold must leave node at rest"
    );
}

#[test]
fn timeline_unbind_while_unpresentable_does_not_hold() {
    use exact_motion::NamedTimeline;

    let mut e = Engine::new();
    e.set_start_on_frame(true, 0.0).unwrap();
    e.set_animations(NODE, &pulse("pulse 1s infinite")).unwrap();
    e.set_animation_timeline(
        NODE,
        Some((
            NamedTimeline::Scroll {
                scroller: 1,
                x: false,
            },
            [0.0, 100.0],
        )),
    );
    e.set_presentable(false).unwrap();
    e.advance(0.25).unwrap();
    e.set_animation_timeline(NODE, None);
    assert_eq!(e.animation_plays(NODE)[0].pending, None);
    assert_eq!(e.held_since(), None);
    assert_eq!(e.held_deadline(), None);
}

#[test]
fn release_overdue_releases_all_overdue_pending_clocks() {
    let mut e = Engine::new();
    e.set_start_on_frame(true, 0.0).unwrap();
    // Paused plays create pending idle clock origins with no pending plays of their own.
    for (node, name, at) in [(10, "A", 0.0), (11, "B", 0.5), (12, "C", 1.0)] {
        e.advance(at).unwrap();
        e.set_animations(node, &pulse("pulse 1s infinite paused"))
            .unwrap();
        e.set_animation_clock(node, Some(name));
    }
    assert_eq!(e.held_since(), Some(0.0));
    e.advance(2.6).unwrap();
    e.release_overdue(2.6).unwrap();
    assert_eq!(
        e.held_since(),
        Some(1.0),
        "both 0.0 and 0.5 are overdue at 2.6; 1.0 remains pending"
    );
    // Live joiners on A and B start immediately; a joiner on C still waits.
    for (node, name, waits) in [(20, "A", false), (21, "B", false), (22, "C", true)] {
        e.set_animation_clock(node, Some(name));
        e.set_animations(node, &pulse("pulse 1s infinite")).unwrap();
        assert_eq!(
            e.animation_plays(node)[0].pending.is_some(),
            waits,
            "clock {name}"
        );
    }
}
