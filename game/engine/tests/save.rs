use exact_game::{Component, Data, Resource, Transform, World};

#[derive(Default, Component)]
struct Health {
    hp: u32,
}
#[derive(Default, Resource)]
struct Score(u64);
#[test]
fn round_trip_preserves_ids_names_rng_resources_and_registration_order() {
    let mut w = World::new(144, 8123);
    w.register::<Health>()
        .register::<Transform>()
        .register_resource::<Score>();
    let e = w.spawn_named("fox", (Transform::at(1.0, 2.0, 3.0), Health { hp: 15 }));
    let dead = w.spawn_named("dead", (Health { hp: 4 },));
    w.despawn(dead);
    w.insert_resource(Score(79));
    w.rng().next_u32();
    w.propagate();
    let bytes = w.save();
    let hash = w.hash();
    let mut loaded = World::new(30, 1);
    loaded
        .register_resource::<Score>()
        .register::<Transform>()
        .register::<Health>();
    loaded.load(&bytes).unwrap();
    assert_eq!(hash, loaded.hash());
    assert_eq!(bytes, loaded.save());
    assert_eq!(loaded.named("fox"), Some(e));
    assert_eq!(loaded.name(e), Some("fox"));
    assert_eq!(loaded.resource::<Score>().0, 79);
    assert_eq!(
        loaded.global(e).unwrap().translation,
        exact_game::Vec3::new(1.0, 2.0, 3.0).into()
    );
    for _ in 0..20 {
        assert_eq!(w.rng().next_u32(), loaded.rng().next_u32());
    }
    assert_eq!(w.spawn(()), loaded.spawn(()));
}
#[test]
fn load_failure_is_atomic_and_names_unknown_types() {
    let mut source = World::new(60, 9);
    source.spawn((Health { hp: 7 },));
    let mut target = World::new(30, 2);
    target.spawn_named("keep", ());
    let before = target.save();
    let error = target.load(&source.save()).unwrap_err().to_string();
    assert!(
        error.contains("Health") && error.contains("unregistered"),
        "{error}"
    );
    assert_eq!(before, target.save());
    target.register::<Health>();
    let b = source.save();
    for i in 0..b.len() {
        assert!(target.load(&b[..i]).is_err());
        assert_eq!(before, target.save());
    }
    let mut junk = b.clone();
    junk.push(0);
    assert!(target.load(&junk).is_err());
    let mut bad_version = b;
    bad_version[7] += 1;
    assert!(target.load(&bad_version).is_err());
}
#[test]
fn component_schema_evolves_by_field_name() {
    #[derive(Data)]
    struct Old {
        hp: u32,
        obsolete: Vec<String>,
    }
    impl Default for Old {
        fn default() -> Self {
            Self {
                hp: 50,
                obsolete: vec![],
            }
        }
    }
    impl Component for Old {
        const NAME: &'static str = "Actor";
    }
    #[derive(Data)]
    struct New {
        added: u32,
        hp: u32,
    }
    impl Default for New {
        fn default() -> Self {
            Self { added: 123, hp: 90 }
        }
    }
    impl Component for New {
        const NAME: &'static str = "Actor";
    }
    let mut old = World::new(60, 0);
    let e = old.spawn((Old {
        hp: 7,
        obsolete: vec!["discard".into()],
    },));
    let mut new = World::new(60, 0);
    new.register::<New>();
    new.load(&old.save()).unwrap();
    assert_eq!(new.get::<New>(e).unwrap().hp, 7);
    assert_eq!(new.get::<New>(e).unwrap().added, 123);
    old.load(&new.save()).unwrap();
    assert_eq!(old.get::<Old>(e).unwrap().hp, 7);
    assert!(old.get::<Old>(e).unwrap().obsolete.is_empty());
}
#[test]
fn published_values_and_skipped_fields_are_not_saved() {
    #[derive(Default, Component)]
    struct Cache {
        value: u64,
        #[data(skip)]
        cache: u32,
    }
    let mut w = World::new(60, 0);
    let e = w.spawn((Cache {
        value: 2,
        cache: 42,
    },));
    let hash = w.hash();
    let bytes = w.save();
    w.get_mut::<Cache>(e).unwrap().cache = 17;
    assert_eq!(w.hash(), hash);
    assert_eq!(w.save(), bytes);
    w.load(&bytes).unwrap();
    assert_eq!(w.get::<Cache>(e).unwrap().cache, 0);
    assert_eq!(w.hash(), hash);
}
#[test]
fn engine_scene_components_first_spawned_mid_game_restore_unregistered() {
    // Grow a Garden's restore refused `unregistered component Parent` until the
    // game registered the engine's own component (garden diary, friction 3).
    let mut w = World::new(60, 3);
    let root = w.spawn(Transform::at(1.0, 0.0, 0.0));
    let fruit = w.spawn((
        Transform::at(0.0, 2.0, 0.0),
        exact_game::Parent(root),
        exact_game::Mesh::Sphere { radius: 0.2 },
        exact_game::Material::default(),
        exact_game::Visible(true),
        exact_game::Ambient,
    ));
    // The lights, the viewmodel and mouse look later lanes added restore the same way.
    w.spawn((
        Transform::default(),
        exact_game::SpotLight::default(),
        exact_game::LightShadows,
        exact_game::ViewModel,
        exact_game::MouseLook::default(),
    ));
    w.propagate();
    let mut fresh = World::new(60, 0);
    fresh.load(&w.save()).unwrap();
    assert_eq!(fresh.hash(), w.hash());
    assert_eq!(
        fresh.global(fruit).unwrap().translation,
        exact_game::Vec3::new(1.0, 2.0, 0.0).into()
    );
}

#[test]
fn a_light_at_the_default_decay_keeps_the_encoding_it_had_without_one() {
    use exact_game::data::{bin, hash, json};
    use exact_game::{PointLight, SpotLight};
    // The records as they were before `decay`.
    #[derive(Data, Default)]
    struct Point {
        color: [f32; 3],
        intensity: f32,
        range: f32,
    }
    #[derive(Data, Default)]
    struct Spot {
        color: [f32; 3],
        intensity: f32,
        range: f32,
        inner: f32,
        outer: f32,
    }
    let point = PointLight::default();
    let old = Point {
        color: point.color,
        intensity: point.intensity,
        range: point.range,
    };
    assert_eq!(bin::to_vec(&point), bin::to_vec(&old));
    assert_eq!(hash::of(&point), hash::of(&old));
    let spot = SpotLight::default();
    let old = Spot {
        color: spot.color,
        intensity: spot.intensity,
        range: spot.range,
        inner: spot.inner,
        outer: spot.outer,
    };
    assert_eq!(bin::to_vec(&spot), bin::to_vec(&old));
    assert_eq!(hash::of(&spot), hash::of(&old));
    assert_eq!(PointLight::TYPE_ID, "table:exact_game::scene::PointLight");

    // Another decay is saved and restored, and changes the hash.
    let flat = SpotLight { decay: 0., ..spot };
    assert_ne!(hash::of(&flat), hash::of(&spot));
    let mut w = World::new(60, 0);
    let lamp = w.spawn((Transform::default(), flat));
    let bytes = w.save();
    let mut fresh = World::new(60, 0);
    fresh.load(&bytes).unwrap();
    assert_eq!(fresh.get::<SpotLight>(lamp).unwrap().decay, 0.);
    assert_eq!(fresh.hash(), w.hash());
    // A record without it is the default decay, even over a light that had another.
    let mut patched = flat;
    json::read_into(&json::to_string(&spot).unwrap(), &mut patched).unwrap();
    assert_eq!(patched, spot);
    assert!(!json::to_string(&spot).unwrap().contains("decay"));
    assert!(json::to_string(&flat).unwrap().contains("\"decay\""));
}

#[test]
fn shadows_at_the_default_local_settings_keep_their_encoding() {
    use exact_game::data::{bin, hash};
    use exact_game::{ShadowBox, ShadowFilter, Shadows};
    // The record as it was before the local settings.
    #[derive(Data, Default)]
    struct Old {
        fixed_box: Option<ShadowBox>,
        cache_static: bool,
        resolution: u32,
        distance: f32,
        cascades: u32,
        softness: f32,
        filter: ShadowFilter,
        strength: f32,
    }
    let shadows = Shadows {
        resolution: 4096,
        ..Default::default()
    };
    let old = Old {
        fixed_box: shadows.fixed_box,
        cache_static: shadows.cache_static,
        resolution: shadows.resolution,
        distance: shadows.distance,
        cascades: shadows.cascades,
        softness: shadows.softness,
        filter: shadows.filter,
        strength: shadows.strength,
    };
    assert_eq!(bin::to_vec(&shadows), bin::to_vec(&old));
    assert_eq!(hash::of(&shadows), hash::of(&old));
    let local = Shadows {
        local_resolution: 2048,
        local_filter: ShadowFilter::PcfSoft {
            depth_bias: 0.0001,
            normal_bias: 0.01,
        },
        ..shadows
    };
    assert_ne!(hash::of(&local), hash::of(&shadows));
    assert_eq!(
        bin::from_slice::<Shadows>(&bin::to_vec(&local)).unwrap(),
        local
    );
    let mut w = World::new(60, 0);
    w.insert_resource(local);
    let mut fresh = World::new(60, 0);
    fresh.load(&w.save()).unwrap();
    assert_eq!(*fresh.resource::<Shadows>(), local);
}

#[test]
fn invalid_shadows_fixed_box_refuses_on_load_and_restore() {
    use exact_game::{Game, Input, ShadowBox, Shadows, Sim, Vec3};

    struct Empty;
    impl Game for Empty {
        type Args = ();
        const ID: &'static str = "test.invalid-shadow-box";
        fn setup(_: &mut World, _: &()) {}
        fn tick(_: &mut World, _: &Input, _: &()) {}
    }

    for fixed_box in [
        ShadowBox {
            center: Vec3::ZERO,
            half_width: 0.0,
            half_depth: 10.0,
        },
        ShadowBox {
            center: Vec3::ZERO,
            half_width: -1.0,
            half_depth: 10.0,
        },
        ShadowBox {
            center: Vec3::ZERO,
            half_width: f32::NAN,
            half_depth: 10.0,
        },
        ShadowBox {
            center: Vec3::ZERO,
            half_width: 10.0,
            half_depth: f32::INFINITY,
        },
        ShadowBox {
            center: Vec3::NAN,
            half_width: 10.0,
            half_depth: 10.0,
        },
    ] {
        let mut w = World::new(60, 0);
        w.insert_resource(Shadows {
            fixed_box: Some(fixed_box),
            ..Default::default()
        });
        let world_save = w.save();
        let mut restored = World::new(60, 0);
        let err = restored.load(&world_save).unwrap_err();
        assert!(err.to_string().contains("sun box"), "{err}");

        let mut sim = Sim::<Empty>::new(()).unwrap();
        sim.world_mut().insert_resource(Shadows {
            fixed_box: Some(fixed_box),
            ..Default::default()
        });
        let sim_save = sim.save().unwrap();
        let mut restored_sim = Sim::<Empty>::new(()).unwrap();
        let err = restored_sim.restore(&sim_save).unwrap_err();
        assert!(err.to_string().contains("sun box"), "{err}");
    }

    let mut box_edit = ShadowBox::default();
    assert!(box_edit.set_field("half_width", "0").is_err());
    assert!(box_edit.set_field("half_depth", "-2").is_err());
    assert_eq!(box_edit, ShadowBox::default());
}
