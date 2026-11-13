use super::*;

fn lowered(m: &mut Motion, now: f64) -> Vec<f64> {
    let mut out = Vec::new();
    m.lower(now, &mut out).unwrap();
    out
}

#[test]
fn a_released_hold_springs_home_as_frames_once() {
    let mut m = Motion::new();
    assert!(m.transitions(7, "translate -exact-spring(180, 12, 1)"));
    m.observe(7, [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0], None, 0.0)
        .unwrap();
    assert!(
        lowered(&mut m, 0.0).is_empty(),
        "a first observation is taken as it is"
    );
    let (serial, value) = m
        .begin(7, Property::Translate, Value::new(0.0, 0.0), 0.1)
        .unwrap()
        .unwrap();
    assert_eq!(value, Value::new(0.0, 0.0));
    for i in 1..6 {
        assert!(m
            .update(
                serial,
                Value::new(20.0 * i as f64, 0.0),
                0.1 + 0.016 * i as f64
            )
            .unwrap());
    }
    assert!(m.measured(serial, 0.18).x > 500.0);
    let velocity = m.measured(serial, 0.18);
    assert!(m.end(serial, Some(velocity), 0.18).unwrap());
    assert!(!m.live(serial));
    let ops = lowered(&mut m, 0.18);
    assert_eq!(
        ops[..4],
        [1.0, 7.0, 0.0, 0.18],
        "a translate spring on 7 at 0.18 s"
    );
    let n = ops[6] as usize;
    assert_eq!(ops.len(), 7 + 4 * n);
    assert_eq!(
        ops[ops.len() - 4..],
        [0.0, 0.0, 0.0, 0.0],
        "the last frame is the target"
    );
    assert!(
        lowered(&mut m, 0.2).is_empty(),
        "an unchanged spring is not compiled again"
    );
    assert!(m.settle_time().is_some());
}

#[test]
fn an_unknown_node_takes_no_hold_and_a_removed_one_forgets_its_holds() {
    let mut m = Motion::new();
    assert!(m
        .begin(3, Property::Translate, Value::ZERO, 0.0)
        .unwrap()
        .is_none());
    m.observe(3, [5.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0], None, 0.0)
        .unwrap();
    let (serial, _) = m
        .begin(3, Property::Translate, Value::new(5.0, 0.0), 0.0)
        .unwrap()
        .unwrap();
    assert_eq!(m.held(serial), Some((3, Property::Translate)));
    m.remove(3);
    assert!(!m.live(serial));
    assert!(!m.update(serial, Value::ZERO, 0.1).unwrap());
}

#[test]
fn a_new_target_under_an_eased_transition_lowers_nothing() {
    let mut m = Motion::new();
    assert!(m.transitions(1, "translate 300ms ease-out"));
    m.observe(1, [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0], None, 0.0)
        .unwrap();
    m.observe(1, [100.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0], None, 0.5)
        .unwrap();
    assert!(
        lowered(&mut m, 0.5).is_empty(),
        "the browser plays CSS transitions"
    );
    assert!(
        !m.transitions(1, "translate bogus"),
        "refused text is no transition"
    );
}

#[test]
fn declaration_only_changes_cancel_browser_springs_before_their_end() {
    for (next, covered) in [
        ("", true),
        ("unset", true),
        ("all 0s", true),
        ("none", false),
        ("scale 1s", false),
    ] {
        for delay in ["", " 0s 500ms"] {
            let mut m = Motion::new();
            assert!(m.transitions(7, &format!("translate -exact-spring(){delay}")));
            let mut values = [0., 0., 0., 0., 1., 0., 1.];
            m.observe(7, values, None, 0.).unwrap();
            lowered(&mut m, 0.);
            values[0] = 100.;
            m.observe(7, values, None, 0.).unwrap();
            assert_eq!(lowered(&mut m, 0.)[0], 1.);
            let end = m.settle_time();
            assert!(m.transitions(7, next));
            m.observe(7, values, None, 0.1).unwrap();
            let ops = lowered(&mut m, 0.1);
            if covered {
                assert!(ops.is_empty(), "{next}: {ops:?}");
                assert_eq!(m.settle_time(), end);
                assert!(lowered(&mut m, end.unwrap()).is_empty());
            } else {
                assert_eq!(ops, [2., 7., 0.], "{next}{delay}");
                assert_eq!(m.settle_time(), None);
                assert!(lowered(&mut m, 0.2).is_empty());
            }
        }
    }
}

#[test]
fn a_pan_releases_at_its_speed_once() {
    let mut m = Motion::new();
    for i in 0..6 {
        m.pan_sample(
            7,
            i == 0,
            100.0 + 20.0 * i as f64,
            50.0,
            1000.0 + 16.0 * i as f64,
        );
    }
    let v = m.pan_release(7, 1080.0);
    assert!((v.x - 1250.0).abs() < 1.0, "{v:?}");
    assert_eq!(m.pan_release(7, 1080.0), Value::ZERO);
}

#[test]
fn a_held_height_springs_to_its_new_target() {
    let mut m = Motion::new();
    assert!(m.transitions(4, "height -exact-spring(300, 30, 1)"));
    m.height(4, 400.0, 0.0).unwrap();
    let (serial, value) = m
        .begin(4, Property::Height, Value::scalar(400.0), 0.1)
        .unwrap()
        .unwrap();
    assert_eq!(value, Value::scalar(400.0));
    assert!(m.update(serial, Value::scalar(300.0), 0.2).unwrap());
    m.height(4, 640.0, 0.2).unwrap();
    assert!(m.end(serial, Some(Value::ZERO), 0.2).unwrap());
    let ops = lowered(&mut m, 0.2);
    assert_eq!(ops[..3], [1.0, 4.0, 4.0], "a height spring on 4");
    assert_eq!(ops[ops.len() - 4], 640.0, "to the new target");
    assert!(m.retire_height(4));
}

#[test]
fn a_transform_pair_moves_together_and_ends_apart() {
    let mut m = Motion::new();
    assert!(m.transitions(
        9,
        "translate -exact-spring(200, 20, 1), scale -exact-spring(200, 20, 1)"
    ));
    m.observe(9, [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0], None, 0.0)
        .unwrap();
    let (t, s, v) = m.begin_pair(9, [0.0, 0.0, 1.0], 0.1).unwrap().unwrap();
    assert_eq!(v, [0.0, 0.0, 1.0]);
    assert!(m.update_pair(t, [30.0, 10.0, 2.0], 0.2).unwrap());
    assert_eq!(m.held(t), Some((9, Property::Translate)));
    assert_eq!(m.held(s), Some((9, Property::Scale)));
    assert!(m.end(t, Some(Value::ZERO), 0.2).unwrap());
    assert!(
        !m.update_pair(t, [0.0, 0.0, 1.0], 0.3).unwrap(),
        "half a pair is no pair"
    );
    assert!(m.end(s, None, 0.3).unwrap());
}

#[test]
fn authored_translate_velocity_is_per_axis_and_only_changes_a_new_target() {
    let initial = [0., 0., 0., 0., 1., 0., 1.];
    let target = [100., 100., 0., 0., 1., 0., 1.];
    for velocity in [
        Value::new(800., -600.),
        Value::new(-800., 600.),
        Value::ZERO,
    ] {
        let mut m = Motion::new();
        assert!(m.transitions(1, "translate -exact-spring(260, 24, 1)"));
        m.observe(1, initial, None, 0.).unwrap();
        m.observe(1, initial, Some(velocity), 0.).unwrap();
        assert_eq!(
            m.engine.spring_descriptor(1, Property::Translate),
            None,
            "a release at an unchanged settled target starts no spring"
        );
        m.observe(1, target, Some(velocity), 0.).unwrap();
        let spring = m.engine.spring_descriptor(1, Property::Translate).unwrap();
        assert_eq!(spring.velocity, velocity);
        m.observe(1, target, Some(Value::new(999., 999.)), 0.02)
            .unwrap();
        assert_eq!(
            m.engine.spring_descriptor(1, Property::Translate),
            Some(spring),
            "velocity alone never restarts it"
        );
        let mut next = target;
        next[0] = -100.;
        m.observe(1, next, None, 0.04).unwrap();
        let inherited = m.engine.spring_descriptor(1, Property::Translate).unwrap();
        assert_ne!(
            inherited.velocity,
            Value::ZERO,
            "omission preserves motion on interruption"
        );
        next[1] = -100.;
        m.observe(1, next, Some(Value::ZERO), 0.05).unwrap();
        assert_eq!(
            m.engine
                .spring_descriptor(1, Property::Translate)
                .unwrap()
                .velocity,
            Value::ZERO,
            "cancel can explicitly stop inherited velocity"
        );
        m.remove(1);
        m.observe(1, initial, None, 0.06).unwrap();
        assert!(
            m.engine.spring_descriptor(1, Property::Translate).is_none(),
            "a recreated node starts at rest"
        );
    }
    let mut m = Motion::new();
    m.observe(1, initial, None, 0.).unwrap();
    m.observe(1, target, Some(Value::new(800., -600.)), 0.01)
        .unwrap();
    assert_eq!(
        m.engine.value(1, Property::Translate),
        Some(Value::new(100., 100.))
    );
    assert!(
        m.engine.spring_descriptor(1, Property::Translate).is_none(),
        "no transition means no release spring"
    );
}

#[test]
fn enabling_a_spring_releases_nonzero_velocity_at_an_unchanged_endpoint() {
    let initial = [0., 0., 0., 0., 1., 0., 1.];
    for velocity in [
        Value::new(800., -600.),
        Value::new(-800., 600.),
        Value::ZERO,
    ] {
        let mut m = Motion::new();
        m.transitions(1, "none");
        m.observe(1, initial, None, 0.).unwrap();
        m.transitions(1, "translate -exact-spring(260, 24, 1)");
        m.observe(1, initial, Some(velocity), 0.).unwrap();
        if velocity == Value::ZERO {
            assert_eq!(m.engine.spring_descriptor(1, Property::Translate), None);
            continue;
        }
        let spring = m.engine.spring_descriptor(1, Property::Translate).unwrap();
        assert_eq!(spring.velocity, velocity);
        m.observe(1, initial, Some(Value::new(999., 999.)), 0.001)
            .unwrap();
        assert_eq!(
            m.engine.spring_descriptor(1, Property::Translate),
            Some(spring)
        );
        let shown = m.engine.value(1, Property::Translate).unwrap();
        assert_eq!(shown.x.is_sign_positive(), velocity.x.is_sign_positive());
        assert_eq!(shown.y.is_sign_positive(), velocity.y.is_sign_positive());
        let frames = lowered(&mut m, 0.001);
        assert_eq!(
            &frames[frames.len() - 4..],
            &[0.; 4],
            "returns to the same endpoint"
        );
    }
}
