use super::*;
use exact_game::{InputEvent, PointerPan, PointerPhase, Present, Vec2};

const HORIZONTAL: Vec3 = Vec3::new(0.025, 0., 0.);
const VERTICAL: Vec3 = Vec3::new(0., 0., 0.04);
#[derive(Default, exact_game::Resource)]
struct Pan {
    position: Vec2,
    delta: Vec2,
}
struct Panning;
impl Game for Panning {
    const ID: &'static str = "pointer-pan";
    const HZ: u32 = 120;
    type Args = ();
    fn setup(w: &mut World, _: &()) {
        w.insert_resource(Pan::default());
        let camera = w.spawn_named("camera", (Transform::at(0., 2., 8.), Camera::default()));
        w.spawn_named(
            "child",
            (Transform::at(1., 0., -1.), Parent(camera), Mesh::cube(0.1)),
        );
    }
    fn tick(w: &mut World, input: &Input, _: &()) {
        let delta = input
            .pointers()
            .iter()
            .find(|p| p.id == 1)
            .map_or(Vec2::ZERO, |p| p.delta);
        let mut pan = w.resource_mut::<Pan>();
        pan.delta = delta;
        if let Some(p) = input.pointers().iter().find(|p| p.id == 1) {
            pan.position = p.position;
        }
        drop(pan);
        w.get_mut::<Transform>("camera").unwrap().position +=
            HORIZONTAL * delta.x + VERTICAL * delta.y;
    }
    fn present(p: &mut Present<'_>, _: &()) {
        let pan = p.resource::<Pan>().unwrap();
        let sample = PointerPan {
            pointer: 1,
            position: pan.position,
            delta: pan.delta,
            horizontal: HORIZONTAL,
            vertical: VERTICAL,
            ..Default::default()
        };
        drop(pan);
        p.insert(p.named("camera").unwrap(), sample);
    }
}
fn touch(sim: &mut Sim<Panning>, id: u64, phase: PointerPhase, x: f32, dx: f32, at_ms: f64) {
    sim.input(InputEvent::Pointer {
        id,
        phase,
        x,
        y: 0.,
        dx,
        dy: 0.,
        buttons: u32::from(matches!(phase, PointerPhase::Down | PointerPhase::Move)),
        at_ms,
        player: 0,
    });
}

#[test]
fn pointer_pan_shows_latest_contact_at_60_and_120_hz_without_state_changes() {
    for display in [60., 120.] {
        let mut sim = Sim::<Panning>::new(()).unwrap();
        sim.frame_period(1000. / display);
        let mut feed = Feed::default();
        let mut writes = Recording::default();
        feed.feed_to(sim.world(), &mut writes).unwrap();
        sim.advance(0., Clock::Live);
        touch(&mut sim, 1, PointerPhase::Down, 0., 0., 0.);
        touch(&mut sim, 2, PointerPhase::Down, 400., 0., 0.);
        let (mut next_sample, mut x, mut max_error) = (1000. / 240., 0f32, 0f32);
        let mut last_shown = 0.;
        let mut steps = Vec::new();
        let mut ages = Vec::new();
        for n in 1..=300 {
            let now = f64::from(n) * 1000. / display;
            while next_sample <= now {
                let next = if next_sample < 1000. {
                    next_sample * 0.08
                } else if next_sample < 1300. {
                    80.
                } else if next_sample < 2300. {
                    80. - (next_sample - 1300.) * 0.08
                } else {
                    0.
                } as f32;
                touch(&mut sim, 1, PointerPhase::Move, next, next - x, next_sample);
                touch(&mut sim, 2, PointerPhase::Move, 900., 500., next_sample);
                x = next;
                next_sample += 1000. / 240.;
            }
            sim.advance_with(now, Clock::Live, |w, left| {
                if left < 2 {
                    feed.feed_to(w, &mut writes).unwrap();
                }
            });
            let hash = sim.world().hash();
            let save = sim.save().unwrap();
            let sample = sim.frame_pointer(feed.pointer_pan().unwrap()).unwrap();
            ages.push(now - sample.at_ms);
            feed.pointer_sample(Some(sample));
            let frame = feed.frame(sim.world(), sim.alpha(), 1.);
            max_error = max_error.max((frame.camera_position.x - x * HORIZONTAL.x).abs());
            let shown = frame.camera_position.x / HORIZONTAL.x;
            if (200. ..900.).contains(&now) || (1500. ..2200.).contains(&now) {
                let sign = if now < 1000. { 1. } else { -1. };
                steps.push(f64::from((shown - last_shown) * sign));
            }
            last_shown = shown;
            if let Some(child) = frame
                .attachments
                .iter()
                .find(|a| a.entity == sim.world().named("child").unwrap())
            {
                assert!(
                    ((frame.view * child.matrix).w_axis.truncate() - Vec3::new(1., 0., -1.))
                        .length()
                        < 1e-4
                );
            }
            assert_eq!(sim.world().hash(), hash);
            assert_eq!(sim.save().unwrap(), save);
        }
        let mean = steps.iter().sum::<f64>() / steps.len() as f64;
        let stddev =
            (steps.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / steps.len() as f64).sqrt();
        let age = ages.iter().copied().fold(0., f64::max);
        println!("synthetic {display} Hz display, 120 Hz world, 240 Hz samples: max latest-sample error {:.6} pt; frame-step stddev {stddev:.6} pt; max sample age {age:.3} ms", max_error / HORIZONTAL.x);
        assert!(max_error / HORIZONTAL.x < 0.002, "{max_error}");
    }
}

#[test]
fn pointer_pan_keeps_release_motion_and_does_not_jump_on_a_reused_contact() {
    let mut sim = Sim::<Panning>::new(()).unwrap();
    let mut feed = Feed::default();
    let mut writes = Recording::default();
    feed.feed_to(sim.world(), &mut writes).unwrap();
    sim.advance(0., Clock::Seekable);
    touch(&mut sim, 1, PointerPhase::Down, 10., 0., 1.);
    touch(&mut sim, 1, PointerPhase::Move, 14., 4., 2.);
    touch(&mut sim, 1, PointerPhase::Up, 16., 2., 3.);
    sim.advance_with(10., Clock::Seekable, |w, _| {
        feed.feed_to(w, &mut writes).unwrap()
    });
    feed.pointer_sample(sim.frame_pointer(1));
    assert!(
        (feed.frame(sim.world(), sim.alpha(), 1.).camera_position.x - 6. * HORIZONTAL.x).abs()
            < 1e-5
    );
    touch(&mut sim, 1, PointerPhase::Down, 200., 0., 11.);
    touch(&mut sim, 1, PointerPhase::Move, 201., 1., 12.);
    assert!(!sim.frame_pointer(1).unwrap().current_contact);
    sim.advance_with(20., Clock::Seekable, |w, _| {
        feed.feed_to(w, &mut writes).unwrap()
    });
    feed.pointer_sample(sim.frame_pointer(1));
    assert!(
        (feed.frame(sim.world(), sim.alpha(), 1.).camera_position.x - 7. * HORIZONTAL.x).abs()
            < 1e-5
    );
}

#[test]
fn recreating_the_feed_does_not_replay_the_last_ticks_pan() {
    let mut sim = Sim::<Panning>::new(()).unwrap();
    sim.advance(0., Clock::Seekable);
    touch(&mut sim, 1, PointerPhase::Down, 0., 0., 1.);
    touch(&mut sim, 1, PointerPhase::Move, 10., 10., 2.);
    sim.advance(10., Clock::Seekable);
    let mut feed = Feed::default();
    feed.feed_to(sim.world(), &mut Recording::default())
        .unwrap();
    feed.pointer_sample(sim.frame_pointer(1));
    assert!(
        (feed.frame(sim.world(), sim.alpha(), 1.).camera_position.x - 10. * HORIZONTAL.x).abs()
            < 1e-6
    );
}

#[test]
fn world_surface_supplies_pointer_sample_and_traces_the_drawn_camera() {
    use exact_gpu::{fixture, Frame, Surface};
    let Some(gpu) = crate::test_device::device_or_skip(fixture::device()) else {
        return;
    };
    let mut surface = crate::WorldSurface::<Panning>::default();
    surface.bind(&[], None).unwrap();
    let mut frame = Frame {
        width: 64.,
        height: 64.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.,
    };
    fixture::render(&gpu, &mut surface, &frame).unwrap();
    surface.input(&exact_gpu::InputEvent::Pointer {
        id: 1,
        phase: exact_gpu::PointerPhase::Down,
        x: 0.,
        y: 0.,
        dx: 0.,
        dy: 0.,
        kind: exact_gpu::PointerKind::Touch,
        buttons: 1,
        at_ms: 1.,
    });
    surface
        .agent(r#"{"op":"state","trace":{"entity":"camera","frames":1}}"#)
        .unwrap();
    frame.now_ms = 10.;
    fixture::render(&gpu, &mut surface, &frame).unwrap();
    surface.input(&exact_gpu::InputEvent::Pointer {
        id: 1,
        phase: exact_gpu::PointerPhase::Move,
        x: 10.,
        y: 0.,
        dx: 10.,
        dy: 0.,
        kind: exact_gpu::PointerKind::Touch,
        buttons: 1,
        at_ms: 11.,
    });
    frame.now_ms = 12.; // The second simulation tick is not due yet.
    fixture::render(&gpu, &mut surface, &frame).unwrap();
    let trace: serde_json::Value =
        serde_json::from_str(&surface.agent(r#"{"op":"state","trace":"read"}"#).unwrap()).unwrap();
    let row = trace["trace"]["frames"].as_array().unwrap();
    assert_eq!(trace["trace"]["stride"], 37);
    for (value, expected) in row[32..37].iter().zip([1., 10., 0., 11., 1.]) {
        assert_eq!(value.as_f64(), Some(expected));
    }
    assert!((row[5].as_f64().unwrap() - 10. * f64::from(HORIZONTAL.x)).abs() < 1e-6);
    assert_eq!(
        surface
            .sim()
            .unwrap()
            .get::<Transform>("camera")
            .unwrap()
            .position
            .x,
        0.
    );
}

#[test]
fn inactive_mouse_look_keeps_authored_pitch_and_invalid_pan_does_not_poison_it() {
    let mut world = World::new(60, 0);
    let rotation = Quat::from_rotation_x(0.8);
    world.spawn((
        Transform {
            rotation,
            ..Transform::default()
        },
        Camera::default(),
        exact_game::MouseLook {
            yaw_per_point: 0.01,
            pitch_per_point: 0.,
            pitch_limit: 0.5,
        },
        PointerPan {
            pointer: 1,
            horizontal: Vec3::splat(f32::NAN),
            ..Default::default()
        },
    ));
    world.propagate();
    let mut feed = Feed::default();
    feed.feed_to(&world, &mut Recording::default()).unwrap();
    let frame = feed.frame(&world, 1., 1.);
    assert!(
        (frame.view.inverse().transform_vector3(-Vec3::Z) - rotation * -Vec3::Z).length() < 1e-6
    );
    feed.unshown_motion(Vec2::new(10., 0.));
    feed.pointer_sample(Some(exact_game::PointerSample {
        id: 1,
        position: Vec2::ONE,
        phase: PointerPhase::Move,
        buttons: 1,
        at_ms: 1.,
        current_contact: true,
    }));
    let frame = feed.frame(&world, 1., 1.);
    assert!(frame.view.is_finite());
    assert!(frame.view.inverse().transform_vector3(-Vec3::Z).x.abs() > 0.01);
}

#[test]
fn mouse_look_turns_the_drawn_camera_and_its_children_about_the_eye() {
    let mut w = World::new(60, 0);
    let look = exact_game::MouseLook {
        yaw_per_point: -0.01,
        pitch_per_point: -0.01,
        pitch_limit: 0.5,
    };
    let camera = w.spawn((Transform::at(1., 2., 3.), Camera::default(), look));
    let gun = w.spawn((
        Transform::at(0.3, -0.2, -0.5),
        Parent(camera),
        Mesh::cube(0.1),
    ));
    w.propagate();
    let mut f = Feed::default();
    let mut r = Recording::default();
    f.feed_to(&w, &mut r).unwrap();
    assert!(
        f.frame(&w, 1., 1.).attachments.is_empty(),
        "no motion, no turn"
    );
    // 50 points left: half a radian of yaw, drawn only.
    f.unshown_motion(exact_game::Vec2::new(-50., 0.));
    let frame = f.frame(&w, 1., 1.);
    let eye = frame.view.inverse();
    assert!(frame.camera_position.distance(Vec3::new(1., 2., 3.)) < 1e-5);
    let forward = Quat::from_rotation_y(0.5) * -Vec3::Z;
    assert!((eye.transform_vector3(-Vec3::Z) - forward).length() < 1e-5);
    // The viewmodel turns with it: it stays put in the camera's view.
    let held = frame.attachments.iter().find(|a| a.entity == gun).unwrap();
    let in_view = (frame.view * held.matrix).w_axis.truncate();
    assert!((in_view - Vec3::new(0.3, -0.2, -0.5)).length() < 1e-5);
    // Pitch stops at the game's limit, and each frame's motion is its own.
    f.unshown_motion(exact_game::Vec2::new(0., -1000.));
    let up = f
        .frame(&w, 1., 1.)
        .view
        .inverse()
        .transform_vector3(-Vec3::Z);
    assert!((up.y - exact_game::math::sin(0.5)).abs() < 1e-4);
    assert!(f.frame(&w, 1., 1.).attachments.is_empty());
}

#[test]
fn pointer_pan_bounds_limit_the_world_camera_and_move_its_children() {
    let mut w = World::new(60, 0);
    let camera = w.spawn((
        Transform::at(1., 2., 3.),
        Camera::default(),
        PointerPan {
            pointer: 1,
            horizontal: Vec3::X,
            vertical: Vec3::Z,
            bounds: Some([Vec3::new(-2., 2., -4.), Vec3::new(2., 2., 4.)]),
            ..Default::default()
        },
    ));
    let child = w.spawn((Transform::at(0., 0., -1.), Parent(camera), Mesh::cube(0.1)));
    w.propagate();
    let hash = w.hash();
    let save = w.save();
    let mut feed = Feed::default();
    feed.feed_to(&w, &mut Recording::default()).unwrap();
    for (position, expected) in [
        (Vec2::splat(100.), Vec3::new(2., 2., 4.)),
        (Vec2::splat(-100.), Vec3::new(-2., 2., -4.)),
        (Vec2::new(0.5, -0.5), Vec3::new(1.5, 2., 2.5)),
    ] {
        feed.pointer_sample(Some(exact_game::PointerSample {
            id: 1,
            position,
            phase: PointerPhase::Move,
            buttons: 1,
            at_ms: 1.,
            current_contact: true,
        }));
        let frame = feed.frame(&w, 0.5, 1.);
        assert!(frame.camera_position.distance(expected) < 1e-5);
        let moved = frame
            .attachments
            .iter()
            .find(|a| a.entity == child)
            .unwrap();
        assert!(((frame.view * moved.matrix).w_axis.truncate() - Vec3::NEG_Z).length() < 1e-5);
    }
    assert_eq!(w.hash(), hash);
    assert_eq!(w.save(), save);
}

#[test]
fn pointer_pan_ignores_invalid_bounds_and_allows_open_axes() {
    for bounds in [
        [Vec3::splat(f32::NAN), Vec3::ONE],
        [Vec3::ONE, Vec3::ZERO],
        [Vec3::INFINITY, Vec3::INFINITY],
        [Vec3::NEG_INFINITY, Vec3::NEG_INFINITY],
        [Vec3::NEG_INFINITY, Vec3::INFINITY],
    ] {
        let mut w = World::new(60, 0);
        w.spawn((
            Transform::default(),
            Camera::default(),
            PointerPan {
                pointer: 1,
                horizontal: Vec3::X,
                bounds: Some(bounds),
                ..Default::default()
            },
        ));
        w.propagate();
        let mut feed = Feed::default();
        feed.feed_to(&w, &mut Recording::default()).unwrap();
        feed.pointer_sample(Some(exact_game::PointerSample {
            id: 1,
            position: Vec2::X,
            phase: PointerPhase::Move,
            buttons: 1,
            at_ms: 1.,
            current_contact: true,
        }));
        assert_eq!(feed.frame(&w, 1., 1.).camera_position, Vec3::X);
    }
}

#[test]
fn pointer_pan_bounds_leave_a_camera_with_no_unshown_motion_where_its_tick_put_it() {
    let mut w = World::new(60, 0);
    w.spawn((
        Transform::at(5., 0., 0.),
        Camera::default(),
        PointerPan {
            pointer: 1,
            horizontal: Vec3::X,
            bounds: Some([Vec3::splat(-2.), Vec3::splat(2.)]),
            ..Default::default()
        },
    ));
    w.propagate();
    let mut feed = Feed::default();
    feed.feed_to(&w, &mut Recording::default()).unwrap();
    // No sample: the limits apply to the frame's own translation only.
    assert_eq!(feed.frame(&w, 1., 1.).camera_position, Vec3::X * 5.);
    feed.pointer_sample(Some(exact_game::PointerSample {
        id: 1,
        position: Vec2::ZERO,
        phase: PointerPhase::Move,
        buttons: 1,
        at_ms: 1.,
        current_contact: true,
    }));
    assert_eq!(feed.frame(&w, 1., 1.).camera_position, Vec3::X * 5.);
}
