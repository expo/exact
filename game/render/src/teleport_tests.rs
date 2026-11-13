use super::*;

#[test]
fn teleport_from_preserves_post_landing_motion_for_mesh_camera_and_light() {
    struct Empty;
    impl Game for Empty {
        type Args = ();
        const ID: &'static str = "landing-motion-history";
        fn setup(_: &mut World, _: &()) {}
        fn tick(_: &mut World, _: &Input, _: &()) {}
    }
    let mut sim = Sim::<Empty>::new(()).unwrap();
    let w = sim.world_mut();
    let root = w.spawn((Transform::default(), Mesh::cube(1.)));
    let child = w.spawn((Transform::at(2., 0., 0.), Parent(root), Mesh::cube(1.)));
    w.spawn((Transform::at(0., 0., 8.), Parent(root), Camera::default()));
    w.spawn((Transform::default(), Parent(root), PointLight::default()));
    w.propagate();
    let mut feed = Feed::default();
    let mut recording = Recording::default();
    feed.feed_to(w, &mut recording).unwrap();
    sim.advance(0., Clock::Seekable);
    sim.advance_with(17., Clock::Seekable, |w, _| {
        feed.feed_to(w, &mut recording).unwrap()
    });
    sim.world_mut()
        .teleport_from(root, Transform::at(20., 0., 0.), Transform::at(24., 0., 0.));
    feed.feed_to(sim.world(), &mut recording).unwrap();
    assert_eq!(
        (
            recording.position(root, true).x,
            recording.position(root, false).x
        ),
        (20., 24.)
    );
    assert_eq!(
        (
            recording.position(child, true).x,
            recording.position(child, false).x
        ),
        (22., 26.)
    );
    let frame = feed.frame(sim.world(), 0.25, 1.);
    assert_eq!(frame.camera_position.x, 21.);
    assert_eq!(frame.lights[0].position.x, 21.);
    let saved = sim.world().save();
    sim.advance_with(34., Clock::Seekable, |w, _| {
        feed.feed_to(w, &mut recording).unwrap()
    });
    assert!(sim.world().interpolation_origin(root).is_none());
    let w = sim.world_mut();
    w.load(&saved).unwrap();
    assert!(w.interpolation_origin(root).is_none());
    w.teleport_from(root, Transform::at(40., 0., 0.), Transform::at(44., 0., 0.));
    w.teleport(root, Transform::at(50., 0., 0.));
    assert!(w.interpolation_origin(root).is_none());
    feed.feed_to(w, &mut recording).unwrap();
    assert_eq!(
        (
            recording.position(root, true).x,
            recording.position(root, false).x
        ),
        (50., 50.)
    );
}

/// A socketed prop's owner chain starts a `teleport_from` at its landing, as
/// the owner's own mesh does: through the frame it moves from the landing to
/// where the tick's motion took it.
#[test]
fn a_socket_follows_teleport_from_from_its_landing() {
    struct Rig;
    impl Game for Rig {
        type Args = ();
        const ID: &'static str = "landing-motion-socket";
        fn setup(w: &mut World, _: &()) {
            let mesh = w
                .generated_model("posed.model", crate::test_model::posable_model())
                .unwrap();
            let actor = w.spawn_named("actor", (Transform::default(), mesh));
            w.spawn_named(
                "charm",
                (
                    Transform::default(),
                    exact_game::SocketFollow::new(actor, "hand"),
                ),
            );
        }
        fn tick(_: &mut World, _: &Input, _: &()) {}
    }
    let mut sim = Sim::<Rig>::new(()).unwrap();
    let mut attachments = crate::world::scene::Attachments::default();
    attachments.feed(sim.world(), true, false, None, true);
    sim.advance(0., Clock::Seekable);
    sim.advance_with(17., Clock::Seekable, |w, _| {
        attachments.feed(w, false, true, None, false)
    });
    let actor = sim.world().named("actor").unwrap();
    sim.world_mut().teleport_from(
        actor,
        Transform::at(20., 0., 0.),
        Transform::at(24., 0., 0.),
    );
    attachments.feed(sim.world(), false, false, None, false);
    let charm = sim.world().named("charm").unwrap();
    let mut at = |alpha: f32| {
        attachments.frame(alpha);
        let shown = attachments.output.iter().find(|a| a.entity == charm);
        shown.expect("the charm follows its socket").matrix.w_axis.x
    };
    let (start, middle, end) = (at(0.), at(0.5), at(1.));
    assert!((end - start - 4.).abs() < 1e-4, "{start} → {end}");
    assert!((middle - start - 2.).abs() < 1e-4, "{start} → {middle}");
}
