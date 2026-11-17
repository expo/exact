use exact_game::{Transform, Vec3, World};
use exact_game_physics::{self as physics, Body, Collider, Shape};

#[test]
fn heightfield_mesh_and_cylinder_use_world_entities() {
    for shape in [
        Shape::Heightfield {
            rows: 2,
            cols: 2,
            heights: vec![0.0; 4],
            scale: Vec3::new(10.0, 1.0, 10.0),
        },
        Shape::Mesh {
            vertices: vec![
                Vec3::new(-5.0, 0.0, -5.0),
                Vec3::new(-5.0, 0.0, 5.0),
                Vec3::new(5.0, 0.0, -5.0),
                Vec3::new(5.0, 0.0, 5.0),
            ],
            indices: vec![[0, 1, 2], [2, 1, 3]],
        },
        Shape::Cylinder {
            radius: 2.0,
            height: 1.0,
        },
    ] {
        let mut w = World::new(60, 0);
        physics::register(&mut w);
        let floor = w.spawn((
            Transform::default(),
            Collider {
                shape: shape.clone(),
                ..Collider::default()
            },
        ));
        let hit = physics::raycast(&w, Vec3::new(0.2, 3.0, 0.3), -Vec3::Y, 5.0, u32::MAX).unwrap();
        assert_eq!(hit.entity, floor);
        let expected = if matches!(shape, Shape::Cylinder { .. }) {
            2.5
        } else {
            3.0
        };
        assert!((hit.distance - expected).abs() < 0.001);
        let box_id = w.spawn((
            Transform::at(0.2, 2.0, 0.3),
            Body::default(),
            Collider::default(),
        ));
        for _ in 0..360 {
            physics::step(&mut w);
        }
        assert!(w.get::<Body>(box_id).unwrap().asleep);
        assert!((w.get::<Transform>(box_id).unwrap().position.y - (3.5 - expected)).abs() < 0.01);
        let saved = w.save();
        let mut restored = World::new(60, 0);
        physics::register(&mut restored);
        restored.load(&saved).unwrap();
        assert_eq!(w.hash(), restored.hash());
        physics::step(&mut w);
        physics::step(&mut restored);
        assert_eq!(w.hash(), restored.hash());
    }
}
#[test]
fn ccd_is_replaced_by_serialized_bodies_and_kept_by_patches() {
    use exact_game::{bin, json};
    let on = Body {
        ccd: true,
        ..Body::default()
    };
    assert!(json::to_string(&Body::default())
        .unwrap()
        .contains("\"ccd\":false"));
    assert!(json::to_string(&on).unwrap().contains("\"ccd\":true"));
    for patch in [r#"{"damping":0.1}"#, "{}"] {
        let mut body = on.clone();
        json::read_into(patch, &mut body).unwrap();
        assert!(body.ccd, "{patch}");
    }
    let saved = json::to_string(&Body {
        damping: 0.1,
        ..Body::default()
    })
    .unwrap();
    assert!(!json::from_str::<Body>(&saved).unwrap().ccd);
    let mut decoded = on.clone();
    json::read_into(&saved, &mut decoded).unwrap();
    assert!(!decoded.ccd, "a serialized default body must turn CCD off");
    let mut decoded = on.clone();
    bin::read_into(&bin::to_vec(&Body::default()), &mut decoded).unwrap();
    assert!(!decoded.ccd, "a binary default body must turn CCD off");
    for body in [Body::default(), on] {
        let mut w = World::new(60, 0);
        physics::register(&mut w);
        let e = w.spawn((Transform::default(), body.clone()));
        let mut restored = World::new(60, 0);
        physics::register(&mut restored);
        restored.load(&w.save()).unwrap();
        assert_eq!(*restored.get::<Body>(e).unwrap(), body);
        assert_eq!(restored.hash(), w.hash());
        assert_eq!(bin::from_slice::<Body>(&bin::to_vec(&body)).unwrap(), body);
    }
}
// A fast body is swept against static colliders by default, but against a thin
// dynamic one only with `ccd`, set at spawn or later.
#[test]
fn ccd_stops_a_fast_body_tunnelling_through_a_thin_dynamic_one() {
    for ccd in [None, Some(0), Some(1)] {
        let mut w = World::new(60, 0);
        physics::register(&mut w);
        let wall = w.spawn((
            Transform::at(5.0, 0.0, 0.0),
            Body {
                gravity: 0.0,
                mass: 100.0,
                ..Body::default()
            },
            Collider {
                shape: Shape::Box {
                    half: Vec3::new(0.01, 1.0, 1.0),
                },
                ..Collider::default()
            },
        ));
        let bullet = w.spawn((
            Transform::default(),
            Body {
                velocity: Vec3::X * 120.0,
                gravity: 0.0,
                ccd: ccd == Some(0),
                ..Body::default()
            },
            Collider {
                shape: Shape::Sphere { radius: 0.05 },
                ..Collider::default()
            },
        ));
        if ccd == Some(1) {
            physics::step(&mut w);
            let mut b = w.get_mut::<Body>(bullet).unwrap();
            exact_game::Data::set_field(&mut *b, "ccd", "true").unwrap();
        }
        for _ in 0..5 {
            physics::step(&mut w);
        }
        let x = |e| w.get::<Transform>(e).unwrap().position.x;
        assert_eq!(x(bullet) > x(wall), ccd.is_none(), "{ccd:?}: {}", x(bullet));
    }
}
#[test]
fn fast_rigid_body_does_not_tunnel_and_queries_are_read_only() {
    let mut w = World::new(60, 0);
    physics::register(&mut w);
    w.spawn((
        Transform::at(1.0, 0.0, 0.0),
        Collider {
            shape: Shape::Box {
                half: Vec3::new(0.01, 10.0, 10.0),
            },
            ..Collider::default()
        },
    ));
    let e = w.spawn((
        Transform::default(),
        Body {
            velocity: Vec3::X * 200.0,
            gravity: 0.0,
            ..Body::default()
        },
        Collider {
            shape: Shape::Sphere { radius: 0.1 },
            ..Collider::default()
        },
    ));
    for _ in 0..10 {
        physics::step(&mut w);
        assert!(w.get::<Transform>(e).unwrap().position.x < 1.0);
    }
    let hash = w.hash();
    physics::raycast(&w, Vec3::ZERO, Vec3::X, 10.0, u32::MAX);
    physics::overlap(&w, &Shape::default(), Transform::default(), u32::MAX);
    assert_eq!(hash, w.hash());
    w.remove::<Collider>(e);
    physics::step(&mut w);
    w.remove::<Body>(e);
    w.insert(e, Collider::default());
    physics::step(&mut w);
    assert_eq!(
        physics::raycast(&w, Vec3::new(-1.0, 0.0, 0.0), Vec3::X, 10.0, u32::MAX)
            .unwrap()
            .entity,
        e
    );
}

#[test]
fn colliders_match_primitive_dimensions_and_plane_top() {
    use exact_game::Mesh;
    for (mesh, half_height) in [
        (Mesh::cube(2.0), 1.0),
        (Mesh::cuboid(Vec3::new(2.0, 3.0, 4.0)), 1.5),
        (Mesh::sphere(0.6), 0.6),
        (Mesh::capsule(0.4, 1.8), 0.9),
        (Mesh::cylinder(0.5, 1.2), 0.6),
        (Mesh::plane(40.0, 20.0), 0.0),
    ] {
        let mut w = World::new(60, 0);
        physics::register(&mut w);
        w.spawn((Transform::default(), Collider::of(&mesh)));
        let hit = physics::raycast(&w, Vec3::new(0.0, 3.0, 0.0), -Vec3::Y, 5.0, u32::MAX).unwrap();
        assert!(
            (hit.point.y - half_height).abs() < 1e-5,
            "{mesh:?}: {hit:?}"
        );
        if matches!(mesh, Mesh::Plane { .. }) {
            let bottom =
                physics::raycast(&w, Vec3::new(0.0, -1.0, 0.0), Vec3::Y, 2.0, u32::MAX).unwrap();
            assert!((bottom.point.y + 0.01).abs() < 1e-5);
        }
    }
}

#[test]
fn plane_is_a_dynamic_box_and_side_pick_matches_the_physics_slab() {
    use exact_game::{Camera, Game, Input, Mesh, Sim};
    struct Plane;
    impl Game for Plane {
        type Args = ();
        const ID: &'static str = "dynamic-plane";
        fn setup(w: &mut World, _: &()) {
            physics::register(w);
            w.spawn_named(
                "plane",
                (
                    Transform::default(),
                    Mesh::plane(2.0, 2.0),
                    Collider::of(&Mesh::plane(2.0, 2.0)),
                    Body {
                        gravity: 0.0,
                        velocity: Vec3::X,
                        ..Default::default()
                    },
                ),
            );
            w.spawn((
                Transform::at(2.0, -0.005, 0.0).looking_at(Vec3::new(0.0, -0.005, 0.0), Vec3::Y),
                Camera::default(),
            ));
        }
        fn tick(w: &mut World, _: &Input, _: &()) {
            physics::step(w);
        }
    }
    let mut s = Sim::<Plane>::new(()).unwrap();
    assert!(
        matches!(s.world().get::<Collider>("plane").unwrap().shape, Shape::Box { half } if half == Vec3::new(1.0, 0.005, 1.0))
    );
    let hit = physics::raycast(
        s.world(),
        Vec3::new(2.0, -0.005, 0.0),
        -Vec3::X,
        5.0,
        u32::MAX,
    )
    .unwrap();
    assert!((hit.distance - 1.0).abs() < 1e-6);
    let pick = s.agent(r#"{"op":"layout","x":400,"y":300,"width":800,"height":600}"#);
    assert!(
        pick.contains("\"name\":\"plane\"") && pick.contains("\"distance\":1"),
        "{pick}"
    );
    let bounds = s.agent(r#"{"op":"layout","entity":"plane"}"#);
    assert!(
        bounds.contains("\"min\":[-1,-0.01,-1]") && bounds.contains("\"max\":[1,0,1]"),
        "{bounds}"
    );
    s.run(100.0);
    assert!(s.world().get::<Transform>("plane").unwrap().position.x > 0.09);
}
