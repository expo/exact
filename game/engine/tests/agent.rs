use exact_game::*;
struct Scene;
impl Game for Scene {
    type Args = ();
    const ID: &'static str = "Scene";
    fn setup(w: &mut World, _: &Self::Args) {
        let child = w.spawn_named("child", Transform::at(0.0, 1.0, 0.0));
        let root = w.spawn_named("root", Transform::at(2.0, 0.0, 0.0));
        w.insert(child, Parent(root));
        w.spawn_named("camera", (Transform::at(0.0, 0.0, 10.0), Camera::default()));
    }
    fn tick(_: &mut World, _: &Input, _: &Self::Args) {}
}
#[test]
fn hierarchy_preorder_under_and_cap() {
    let mut s = Sim::<Scene>::new(()).unwrap();
    assert_eq!(
        s.agent(r#"{"op":"tree","under":"root"}"#),
        r#"{"tick":0,"entities":[{"id":1,"name":"root","parent":null,"depth":0,"components":["Transform"],"tags":[]},{"id":0,"name":"child","parent":1,"depth":1,"components":["Parent","Transform"],"tags":[]}],"truncated":false,"total":2}"#
    );
    assert!(s
        .agent(r#"{"op":"layout","entity":"child"}"#)
        .contains("\"position\":[2.0,1.0,0.0]"));
    for _ in 0..520 {
        s.world_mut().spawn(());
    }
    let tree = s.agent(r#"{"op":"tree"}"#);
    assert_eq!(tree.matches("\"id\":").count(), 512);
    assert!(tree.ends_with("\"truncated\":true,\"total\":523,\"next\":512}"));
    // The forest's 2k-tree world was refused as truncated: pages reach every entity.
    let rest = s.agent(r#"{"op":"tree","from":512,"limit":100}"#);
    assert_eq!(rest.matches("\"id\":").count(), 11);
    assert!(rest.ends_with("\"truncated\":false,\"total\":523}"));
    let all = s.agent(r#"{"op":"tree","limit":100000}"#);
    assert_eq!(all.matches("\"id\":").count(), 523);
    assert!(s
        .agent(r#"{"op":"tree","limit":0}"#)
        .contains("limit must be 1..=100000"));
    assert_eq!(
        s.agent(r#"{"op":"tree","under":"bad"}"#),
        r#"{"tick":0,"error":"no entity named `bad`; `tree world` lists names; add `w.spawn_named(\"bad\", (Transform::default(),));` in setup if intended"}"#
    );
}
#[test]
fn pick_uses_oriented_boxes_and_exact_spheres_and_capsules() {
    let mut s = Sim::<Scene>::new(()).unwrap();
    s.viewport(800.0, 600.0);
    let e = s
        .world_mut()
        .spawn_named("shape", (Transform::default(), Mesh::sphere(1.0)));
    s.world_mut().propagate();
    let pick = |s: &mut Sim<Scene>, x: f32, y: f32| {
        s.agent(&format!("{{\"op\":\"layout\",\"x\":{x},\"y\":{y}}}"))
    };
    assert!(pick(&mut s, 400.0, 300.0).contains("\"distance\":9"));
    // This ray intersects the sphere's box near its corner, but misses the sphere.
    assert!(pick(&mut s, 447.0, 253.0).contains("\"hit\":null"));
    s.world_mut().insert(
        e,
        Mesh::Capsule {
            radius: 0.4,
            height: 1.0,
        },
    );
    assert!(pick(&mut s, 400.0, 300.0).contains("\"distance\":9.6"));
    s.world_mut().insert(e, Mesh::cube(1.0));
    s.world_mut().teleport(
        e,
        Transform {
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
            ..Transform::default().with_scale(Vec3::new(4.0, 0.2, 1.0))
        },
    );
    assert!(pick(&mut s, 400.0, 300.0).contains("\"distance\":9.5"));
    // World AABB includes (1,-1,0); the narrow rotated box does not.
    assert!(pick(&mut s, 450.0, 350.0).contains("\"hit\":null"));
    s.world_mut().insert(e, Visible(false));
    assert!(pick(&mut s, 400.0, 300.0).contains("\"hit\":null"));
}
#[test]
fn projection_refusals_and_lossless_state_are_read_only() {
    let mut s = Sim::<Scene>::new(()).unwrap();
    let e = s.world().named("child").unwrap();
    s.world_mut()
        .teleport(e, Transform::at(0.1234567, 0.0, 0.0));
    let hash = s.world().hash();
    assert!(s
        .agent(r#"{"op":"state","entity":"child"}"#)
        .contains("0.1234567"));
    let position = json::to_string(&s.global_position("child").unwrap()).unwrap();
    assert!(s
        .agent(r#"{"op":"layout","entity":"child"}"#)
        .contains(&format!("\"position\":{position}")));
    assert!(position.contains("2.1234567"), "{position}");
    assert!(s
        .agent(r#"{"op":"layout","x":0,"y":0}"#)
        .contains("needs an active camera and viewport"));
    for request in [
        r#"{"op":"tree","width":0,"height":600}"#,
        r#"{"op":"state","now":"late"}"#,
        r#"{"op":"logs","since":-1}"#,
        r#"{"op":"tree","op":"state"}"#,
        r#"{"op":"tree"} trailing"#,
    ] {
        assert!(s.agent(request).contains("\"error\":"), "{request}");
    }
    assert_eq!(s.world().hash(), hash);
    assert!(s
        .agent(r#"{"op":"layout","entity":"child","width":800,"height":600}"#)
        .contains("\"inFrustum\":true"));
    s.world_mut().teleport(e, Transform::at(0.0, 0.0, 20.0));
    let behind = s.agent(r#"{"op":"layout","entity":"child"}"#);
    assert!(behind.contains("\"behindCamera\":true"));
    assert!(behind.contains("\"inFrustum\":false"));
}

#[test]
fn capsule_pick_from_inside_ignores_internal_cap_surfaces() {
    let mut s = Sim::<Scene>::new(()).unwrap();
    s.viewport(800.0, 600.0);
    let camera = s.world().named("camera").unwrap();
    s.world_mut()
        .teleport(camera, Transform::default().looking_at(Vec3::Y, Vec3::Z));
    s.world_mut().spawn_named(
        "capsule",
        (
            Transform::default(),
            Mesh::Capsule {
                radius: 1.0,
                height: 3.0,
            },
        ),
    );
    s.world_mut().propagate();
    let reply = s.agent(r#"{"op":"layout","x":400,"y":300}"#);
    assert!(reply.contains("\"distance\":1.5"), "{reply}");
}

#[test]
fn busy_is_returned_only_when_requested_and_clock_without_now_does_not_seek() {
    let mut s = Sim::<Scene>::new(()).unwrap();
    s.advance(0., Clock::Seekable);
    s.advance(1000., Clock::Seekable);
    let before = s.world().tick();
    assert!(s
        .agent(r#"{"op":"state","entity":"*","busy":true}"#)
        .contains("\"busy\":"));
    assert!(!s
        .agent(r#"{"op":"state","entity":"*"}"#)
        .contains("\"busy\":"));
    s.agent(r#"{"op":"clock"}"#);
    assert_eq!(s.world().tick(), before);
}
#[test]
fn restore_format_refusal_is_not_double_wrapped() {
    let mut s = Sim::<Scene>::new(()).unwrap();
    let error = s.restore(b"old-format").unwrap_err().to_string();
    assert_eq!(error.matches("restore refused").count(), 1, "{error}");
    assert!(
        error.contains("expected EXSIM v8") && error.contains("no cross-version migration"),
        "{error}"
    );
    assert!(!error.contains("named additions"), "{error}");
}

#[test]
fn cycle_refusal_contains_the_entities_and_parent_ids() {
    let mut s = Sim::<Scene>::new(()).unwrap();
    let w = s.world_mut();
    let a = w.resolve("root").unwrap();
    let b = w.resolve("child").unwrap();
    w.insert(a, Parent(b));
    for op in [r#"{"op":"tree"}"#, r#"{"op":"state","entity":"*"}"#] {
        let reply = s.agent(op);
        assert!(
            reply.contains("root")
                && reply.contains("child")
                && reply.contains("Parent #0")
                && reply.contains("Parent #1"),
            "{reply}"
        );
        assert!(!reply.contains("show Parent components"));
    }
}

#[test]
fn screen_bounds_clip_near_without_changing_saved_state() {
    struct Clipped;
    impl Game for Clipped {
        const ID: &'static str = "clipped-bounds";
        type Args = ();
        fn setup(w: &mut World, _: &()) {
            w.spawn_named(
                "camera",
                (
                    Transform::default(),
                    Camera {
                        near: 1.,
                        far: 100.,
                        fov_y_degrees: 90.,
                        ..Camera::default()
                    },
                ),
            );
            w.spawn_named("box", (Transform::at(0., 0., -1.), Mesh::cube(2.)));
        }
        fn tick(_: &mut World, _: &Input, _: &()) {}
    }
    let mut s = Sim::<Clipped>::new(()).unwrap();
    s.viewport(200., 200.);
    let saved = s.save().unwrap();
    let rect = s.layout("box").unwrap().screen;
    for (a, b) in [rect.x, rect.y, rect.w, rect.h]
        .into_iter()
        .zip([0., 0., 200., 200.])
    {
        assert!((a - b).abs() < 0.0001, "{rect:?}");
    }
    assert_eq!(
        s.save().unwrap(),
        saved,
        "projection cannot change the complete save"
    );
    let entity = s.world().named("box").unwrap();
    s.world_mut().teleport(entity, Transform::at(0., 0., 2.));
    assert!(s.layout("box").is_none());
    // Exactly on the near plane, the rear face still supplies finite bounds.
    s.world_mut().teleport(entity, Transform::at(0., 0., 0.));
    let rect = s.layout("box").unwrap().screen;
    assert!((rect.w - 200.).abs() < 0.0001 && (rect.h - 200.).abs() < 0.0001);
}

#[test]
fn reported_paranoid_mode_and_completed_samples_follow_the_executor() {
    let mut expected = None;
    for (mode, name) in [
        (Paranoid::Off, "0"),
        (Paranoid::Save, "1"),
        (Paranoid::FreshGame, "fresh-game"),
    ] {
        let mut sim = Sim::<Scene>::new(()).unwrap().paranoid(mode);
        let before = sim.agent(r#"{"op":"state"}"#);
        assert!(before.contains(&format!(
            r#""paranoid":{{"mode":"{name}","checked":0,"skipped":0,"owed":false}}"#
        )));
        sim.run(100.0);
        let state = sim.agent(r#"{"op":"state"}"#);
        assert!(state.contains(&format!(
            r#""paranoid":{{"mode":"{name}","checked":{},"skipped":0,"owed":false}}"#,
            if mode == Paranoid::Off { 0 } else { 1 }
        )));
        let bytes = sim.save().unwrap();
        if let Some(expected) = &expected {
            assert_eq!(&bytes, expected);
        } else {
            expected = Some(bytes.clone());
        }
        sim.restore(&bytes).unwrap();
        assert_eq!(
            sim.agent(r#"{"op":"state"}"#).split("\"paranoid\":").nth(1),
            state.split("\"paranoid\":").nth(1)
        );
    }
}

#[derive(Default, Args)]
struct CosmeticLayoutArgs {
    #[live]
    wide: bool,
}
struct CosmeticLayout;
impl Game for CosmeticLayout {
    const ID: &'static str = "cosmetic-layout";
    type Args = CosmeticLayoutArgs;
    fn setup(w: &mut World, _: &Self::Args) {
        let parent = w.spawn(Transform::at(0.5, 0., 0.));
        w.spawn_named(
            "triangle",
            (
                Transform {
                    scale: Vec3::new(1.5, 0.5, 1.),
                    ..Default::default()
                },
                Parent(parent),
                Mesh::asset("small.model"),
            ),
        );
        w.spawn_named(
            "authored",
            (
                Transform::default(),
                Mesh::asset("small.model").bounds([-0.25, -0.25, 0., 0.25, 0.25, 0.]),
            ),
        );
        w.spawn((Transform::at(0., 0., 10.), Camera::orthographic(4.)));
    }
    fn tick(w: &mut World, _: &Input, a: &Self::Args) {
        let name = if a.wide { "wide.model" } else { "small.model" };
        if !matches!(&*w.require::<Mesh>("triangle"), Mesh::Asset(n) if n == name) {
            *w.require_mut::<Mesh>("triangle") = Mesh::asset(name);
        }
    }
    fn present(p: &mut Present<'_>, _: &Self::Args) {
        let mut names = Vec::new();
        p.for_each::<Mesh>(|_, mesh| {
            if let Mesh::Asset(name) = mesh {
                names.push(name.clone());
            }
        });
        for name in names {
            p.cosmetic_model(&name, || {
                let wide = name == "wide.model";
                let (x, bottom, top) = if wide { (2., -1., 2.) } else { (1., 0., 1.) };
                let bounds = [-x, bottom, 0., x, top, 0.];
                asset::Model {
                    meshes: vec![asset::MeshData {
                        positions: vec![-x, bottom, 0., x, bottom, 0., 0., top, 0.],
                        normals: [0., 0., 1.].repeat(3),
                        uvs: vec![0.; 6],
                        indices: vec![0, 1, 2],
                        bounds,
                        ..Default::default()
                    }],
                    materials: vec![asset::MaterialData {
                        base_color_texture: Some(0),
                        ..Default::default()
                    }],
                    textures: vec![name.replace(".model", ".tex")],
                    nodes: vec![asset::Node {
                        mesh: Some(0),
                        ..Default::default()
                    }],
                    bounds,
                    ..Default::default()
                }
            })
            .unwrap();
        }
    }
}
#[test]
fn registered_cosmetic_layout_is_read_only_and_independent_of_texture_arrival() {
    let mut s = Sim::<CosmeticLayout>::new(Default::default()).unwrap();
    s.viewport(200., 200.);
    let hash = s.world().hash();
    let saved = s.save().unwrap();
    let rect = s.layout("triangle").unwrap().screen;
    assert!(
        (rect.w - 150.).abs() < 0.001 && (rect.h - 25.).abs() < 0.001,
        "{rect:?}"
    );
    let reply = s.agent(r#"{"op":"layout","entity":"triangle"}"#);
    assert!(!reply.contains("\"bounds\":null"), "{reply}");
    assert!(
        reply.contains("\"min\":[-1,0,0]") && reply.contains("\"max\":[2,0.5,0]"),
        "{reply}"
    );
    assert!(s.world().model("small.model").is_none());
    assert_eq!(s.world().hash(), hash);
    assert_eq!(s.save().unwrap(), saved);
    let authored = s.layout("authored").unwrap().screen;
    assert!((authored.w - 25.).abs() < 0.001 && (authored.h - 25.).abs() < 0.001);
    assert_eq!(s.take_assets(), ["small.tex"]);
    let texture = bin::to_vec(&asset::TextureData {
        width: 1,
        height: 1,
        mips: vec![vec![255; 4]],
        srgb: true,
        ..Default::default()
    });
    s.asset("small.tex", Some(&texture)).unwrap();
    assert_eq!(s.agent(r#"{"op":"layout","entity":"triangle"}"#), reply);
    assert_eq!(s.world().hash(), hash);
    assert_eq!(s.save().unwrap(), saved);
    s.restore(&saved).unwrap();
    assert_eq!(s.agent(r#"{"op":"layout","entity":"triangle"}"#), reply);
    s.bind(&[Value::Bool(true)], None).unwrap();
    s.run(20.);
    let hash = s.world().hash();
    let saved = s.save().unwrap();
    let rect = s.layout("triangle").unwrap().screen;
    assert!(
        (rect.w - 300.).abs() < 0.001 && (rect.h - 75.).abs() < 0.001,
        "{rect:?}"
    );
    assert!(s.world().model("wide.model").is_none());
    assert_eq!(s.world().hash(), hash);
    assert_eq!(s.save().unwrap(), saved);
}
