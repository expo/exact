use exact_game::{asset::MeshData, Mesh, Transform, World};

fn triangle() -> MeshData {
    MeshData {
        positions: vec![0., 0., 0., 0., 0., 1., 1., 0., 0.],
        normals: vec![0., 1., 0., 0., 1., 0., 0., 1., 0.],
        uvs: vec![0.; 6],
        colors: vec![1., 0., 0., 1., 0., 1., 0., 1., 0., 0., 1., 1.],
        indices: vec![0, 1, 2],
        bounds: [0., 0., 0., 1., 0., 1.],
        ..Default::default()
    }
}
#[test]
fn generated_mesh_is_shared_immutable_and_saved_by_identity() {
    let mut w = World::new(60, 0);
    let mesh = w.generated("rock.model", triangle()).unwrap();
    for _ in 0..100 {
        w.spawn((mesh.clone(), Transform::default()));
    }
    let original = w.model("rock.model").unwrap() as *const _;
    let revision = w.model_revision();
    w.generated("rock.model", triangle()).unwrap();
    assert_eq!(revision, w.model_revision());
    assert_eq!(original, w.model("rock.model").unwrap() as *const _);
    let saved = w.save();
    assert!(!saved.windows(9).any(|s| s == b"positions"));
    let mut fresh = World::new(60, 0);
    fresh.register::<Mesh>();
    fresh.register::<Transform>();
    assert!(fresh
        .load(&saved)
        .unwrap_err()
        .to_string()
        .contains("rock.model"));
    fresh.generated("rock.model", triangle()).unwrap();
    fresh.load(&saved).unwrap();
    assert_eq!(fresh.hash(), w.hash());
    let mut different = triangle();
    different.positions[0] = 0.25;
    let mut changed = World::new(60, 0);
    changed.register::<Mesh>();
    changed.register::<Transform>();
    changed.generated("rock.model", different.clone()).unwrap();
    assert!(changed
        .load(&saved)
        .unwrap_err()
        .to_string()
        .contains("rock.model"));
    assert!(w
        .generated("rock.model", different)
        .unwrap_err()
        .contains("rock.model"));
}
#[test]
fn generated_mesh_refuses_bad_colors_and_upload_ranges() {
    for bad in [0, 1, 2] {
        let mut mesh = triangle();
        match bad {
            0 => {
                mesh.colors.pop();
            }
            1 => mesh.colors[0] = f32::NAN,
            _ => mesh.indices[0] = 100,
        }
        assert!(World::new(60, 0).generated("bad.model", mesh).is_err());
    }
}

#[test]
fn a_megabyte_of_vertices_does_not_enter_the_save() {
    let mut small = World::new(60, 7);
    let mesh = small.generated("terrain.model", triangle()).unwrap();
    small.spawn((Transform::default(), mesh));
    let mut data = triangle();
    data.positions = data.positions.repeat(40_000);
    data.normals = data.normals.repeat(40_000);
    data.uvs = data.uvs.repeat(40_000);
    data.colors.clear();
    let mut large = World::new(60, 7);
    let mesh = large.generated("terrain.model", data).unwrap();
    large.spawn((Transform::default(), mesh));
    assert!(large.save().len().abs_diff(small.save().len()) < 10);
}

#[test]
fn identity_is_required_even_when_the_saved_world_has_no_geometry() {
    let empty = World::new(60, 0).save();
    let mut w = World::new(60, 0);
    w.generated("terrain.model", triangle()).unwrap();
    assert!(w
        .load(&empty)
        .unwrap_err()
        .to_string()
        .contains("terrain.model"));
}

#[test]
fn setup_geometry_remains_resident_before_its_first_entity() {
    struct Late;
    impl exact_game::Game for Late {
        const ID: &'static str = "late-generated";
        type Args = ();
        fn setup(w: &mut World, _: &()) {
            w.generated("rock.model", triangle()).unwrap();
        }
        fn tick(w: &mut World, _: &exact_game::Input, _: &()) {
            if w.tick() == 1 {
                w.spawn((Transform::default(), Mesh::asset("rock.model")));
            }
        }
    }
    let mut sim = exact_game::Sim::<Late>::new(()).unwrap();
    assert!(sim.take_assets().is_empty());
    sim.run(1000. / 60.);
    assert!(sim.take_assets().is_empty());
    assert!(sim.world().model("rock.model").is_some());
    assert!(sim.save().is_ok());
}

#[test]
fn setup_reconstructs_generated_geometry_and_drops_old_identities() {
    use exact_game::{Args, Game, Input, Sim, Value};
    #[derive(Default, Args)]
    struct Options {
        seed: u32,
    }
    struct Seeded;
    impl Game for Seeded {
        const ID: &'static str = "seeded-generated";
        type Args = Options;
        fn setup(w: &mut World, args: &Options) {
            if args.seed < 2 {
                let mut mesh = triangle();
                mesh.positions[0] = args.seed as f32 * 0.25;
                let mesh = w.generated("rock.model", mesh).unwrap();
                w.spawn((Transform::default(), mesh));
            }
        }
        fn tick(_: &mut World, _: &Input, _: &Options) {}
    }
    let mut sim = Sim::<Seeded>::new(Options::default()).unwrap();
    let original = sim.save().unwrap();
    sim.bind(&[Value::Number(1.)], None).unwrap();
    assert_eq!(sim.world().query::<&Mesh>().iter().count(), 1);
    assert_eq!(
        sim.world().model("rock.model").unwrap().meshes[0].positions[0],
        0.25
    );
    assert!(sim.take_assets().is_empty());
    let changed = sim.save().unwrap();
    assert!(sim
        .restore(&original)
        .unwrap_err()
        .to_string()
        .contains("rock.model"));
    assert_eq!(sim.save().unwrap(), changed, "refused restore is atomic");
    sim.bind(&[Value::Number(0.)], None).unwrap();
    sim.restore(&original).unwrap();
    assert_eq!(sim.save().unwrap(), original);

    sim.bind(&[Value::Number(2.)], None).unwrap();
    assert!(sim.world().model("rock.model").is_none());
    assert!(sim.take_assets().is_empty());
    assert!(sim.presentation_models().next().is_none());
    let fresh = Sim::<Seeded>::new(Options { seed: 2 }).unwrap();
    assert_eq!(sim.world().save(), fresh.world().save());
    sim.restore(&fresh.save().unwrap()).unwrap();
}

/// A generated model's identity is in every save of a world that holds it.
/// Material names (LLP 1139.004 D3) are informational: model record 2 writes
/// no name for an unnamed material, so the identity, and the saves, of
/// existing generated models stay as they were.
#[test]
fn an_unnamed_material_keeps_a_generated_models_identity() {
    let mut w = World::new(60, 0);
    w.generated("rock.model", triangle()).unwrap();
    let model = w.model("rock.model").unwrap();
    assert_eq!(model.materials.len(), 1);
    assert_eq!(exact_game::hash::of(model), 9794033405689772318);
}

/// The material record's name rule is the record's alone: present when not
/// empty, absent meaning unnamed, assigned on every read. The other fields
/// keep the codec's lossless encoding, negative zero included, and the hash
/// tells a named material from an unnamed one.
#[test]
fn the_material_record_round_trips_its_name_and_every_bit() {
    use exact_game::asset::MaterialData;
    use exact_game::{bin, hash};
    let named = MaterialData {
        name: "Hull".into(),
        metallic: -0.0,
        roughness: 0.0,
        ..Default::default()
    };
    let back: MaterialData = bin::from_slice(&bin::to_vec(&named)).unwrap();
    assert_eq!(back.name, "Hull");
    assert_eq!(back.metallic.to_bits(), (-0.0f32).to_bits());
    assert_eq!(back.roughness.to_bits(), 0.0f32.to_bits());
    // A stale value never survives a read of a record without a name.
    let mut dirty = named.clone();
    bin::read_into(&bin::to_vec(&MaterialData::default()), &mut dirty).unwrap();
    assert_eq!(dirty.name, "");
    let unnamed = MaterialData {
        name: String::new(),
        ..named.clone()
    };
    assert_ne!(hash::of(&named), hash::of(&unnamed));
    assert_ne!(
        hash::of(&named),
        hash::of(&MaterialData {
            name: "Hul".into(),
            ..named.clone()
        })
    );
    // Only the name is optional: the derive has no attribute that drops a field by value.
    assert_ne!(
        hash::of(&MaterialData {
            metallic: 1.0,
            roughness: 0.0,
            ..unnamed.clone()
        }),
        hash::of(&MaterialData {
            metallic: 0.0,
            roughness: 1.0,
            ..unnamed.clone()
        })
    );
}

/// A baked `.model` file is versioned (`asset::MODEL_RECORD`); a file of an
/// older bake or another record is refused by name, never read as unnamed.
#[test]
fn a_model_of_another_record_is_refused_by_name() {
    use exact_game::asset::{decode_model, encode_model, Model, MODEL_RECORD};
    let model = Model::default();
    let file = encode_model(&model);
    assert_eq!(&file[..8], b"EXMODEL\x02");
    assert_eq!(MODEL_RECORD, 2);
    decode_model(&file).unwrap();
    let old = exact_game::bin::to_vec(&model);
    let error = decode_model(&old).unwrap_err();
    assert!(
        error.contains("baked before model records") && error.contains("bake art/ again"),
        "{error}"
    );
    let mut later = file.clone();
    later[7] = 3;
    let error = decode_model(&later).unwrap_err();
    assert!(
        error.contains("model record 3") && error.contains("reads record 2"),
        "{error}"
    );
}

/// The shading fields are written as one optional field, only when one
/// differs from its default: a material without them keeps its bytes (the
/// record keeps its number, and the baked fixture
/// `bake/tests/fixtures/crate.model` and the generated identity above still
/// match), and one with them round-trips and hashes apart.
#[test]
fn the_material_record_writes_its_shading_only_when_it_is_not_the_default() {
    use exact_game::asset::MaterialData;
    use exact_game::{bin, hash};
    let lit = MaterialData::default();
    let unlit = MaterialData {
        unlit: true,
        ..Default::default()
    };
    let bytes = bin::to_vec(&lit);
    assert!(!bytes.windows(7).any(|w| w == b"shading"));
    let back: MaterialData = bin::from_slice(&bin::to_vec(&unlit)).unwrap();
    assert!(back.unlit);
    assert_ne!(hash::of(&lit), hash::of(&unlit));
    // A stale value never survives a read of a record without shading.
    let mut dirty = MaterialData {
        clearcoat: 1.0,
        sheen_color: [0.5; 3],
        ..unlit.clone()
    };
    bin::read_into(&bytes, &mut dirty).unwrap();
    assert!(!dirty.unlit && dirty.clearcoat == 0.0 && dirty.sheen_color == [0.0; 3]);
    // Every shading field is written together: two materials that differ in
    // which field holds a value hash apart, and a negative zero survives.
    let coat = MaterialData {
        clearcoat: 0.5,
        ..Default::default()
    };
    let rough = MaterialData {
        clearcoat_roughness: 0.5,
        ..Default::default()
    };
    assert_ne!(hash::of(&coat), hash::of(&rough));
    let layered = MaterialData {
        clearcoat: 1.0,
        clearcoat_roughness: 0.05,
        sheen_color: [0.2, 0.3, 0.4],
        sheen_roughness: -0.0,
        ..Default::default()
    };
    let back: MaterialData = bin::from_slice(&bin::to_vec(&layered)).unwrap();
    assert_eq!(
        (back.clearcoat, back.clearcoat_roughness, back.sheen_color),
        (1.0, 0.05, [0.2, 0.3, 0.4])
    );
    assert_eq!(back.sheen_roughness.to_bits(), (-0.0f32).to_bits());
    let negative = MaterialData {
        clearcoat: -0.0,
        ..Default::default()
    };
    let back: MaterialData = bin::from_slice(&bin::to_vec(&negative)).unwrap();
    assert_eq!(back.clearcoat.to_bits(), (-0.0f32).to_bits());
}

#[test]
fn each_shading_field_changes_a_generated_models_identity() {
    let mut original = World::new(60, 0);
    original.generated("rock.model", triangle()).unwrap();
    let model = original.model("rock.model").unwrap().clone();
    let saved = original.save();
    for field in [
        "unlit",
        "clearcoat",
        "clearcoat_roughness",
        "sheen_color",
        "sheen_roughness",
    ] {
        let mut changed = model.clone();
        let material = &mut changed.materials[0];
        match field {
            "unlit" => material.unlit = true,
            "clearcoat" => material.clearcoat = 0.5,
            "clearcoat_roughness" => material.clearcoat_roughness = 0.5,
            "sheen_color" => material.sheen_color = [0.5; 3],
            "sheen_roughness" => material.sheen_roughness = 0.5,
            _ => unreachable!(),
        }
        assert_ne!(
            exact_game::hash::of(&model),
            exact_game::hash::of(&changed),
            "{field}"
        );
        let mut world = World::new(60, 0);
        world.generated_model("rock.model", changed).unwrap();
        let error = world.load(&saved).unwrap_err().to_string();
        assert!(
            error.contains("rock.model") && error.contains("identity differs"),
            "{field}: {error}"
        );
    }
}

/// A complete record (a tuning table's JSON) names every material field, as the
/// derive's records must; only the name may be absent. A short record and
/// trailing input are refused.
#[test]
fn a_complete_material_record_requires_every_field_but_its_name() {
    use exact_game::asset::{MaterialData, Table};
    use exact_game::{bin, json};
    let table = Table::of::<MaterialData>("material.json");
    let error = table.decode("{}").err().unwrap().to_string();
    assert!(
        error.contains("missing field") && error.contains("base_color"),
        "{error}"
    );
    let full = json::to_string(&MaterialData::default()).unwrap();
    assert!(!full.contains("\"name\""), "{full}");
    table.decode(&full).unwrap();
    let named = json::to_string(&MaterialData {
        name: "Hull".into(),
        ..Default::default()
    })
    .unwrap();
    table.decode(&named).unwrap();
    let short = full.replacen("\"metallic\":1.0,", "", 1);
    assert_ne!(short, full);
    let error = table.decode(&short).err().unwrap().to_string();
    assert!(
        error.contains("missing field") && error.contains("metallic"),
        "{error}"
    );
    let mut trailing = bin::to_vec(&MaterialData::default());
    trailing.push(0);
    assert!(bin::from_slice::<MaterialData>(&trailing).is_err());
}

/// A generated model sampling `soil-<theme>.tex`, which `Game::ASSETS` does not
/// declare: the texture is fetched on first sight, the model never is.
#[derive(Default, exact_game::Args)]
struct Theme {
    theme: u32,
}
struct Tiled;
impl exact_game::Game for Tiled {
    const ID: &'static str = "textured-generated";
    type Args = Theme;
    fn setup(w: &mut World, args: &Theme) {
        use exact_game::asset::{MaterialData, Model, Node};
        let mesh = w
            .generated_model(
                "tile.model",
                Model {
                    bounds: triangle().bounds,
                    meshes: vec![triangle()],
                    materials: vec![MaterialData {
                        base_color_texture: Some(0),
                        ..Default::default()
                    }],
                    textures: vec![format!("soil-{}.tex", args.theme)],
                    nodes: vec![Node {
                        mesh: Some(0),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
        w.spawn((Transform::default(), mesh));
    }
    fn tick(_: &mut World, _: &exact_game::Input, _: &Theme) {}
}
// The bake's tracked golden: a fresh checkout has no baked game art.
const SOIL: &[u8] = include_bytes!("../../bake/tests/fixtures/crate/0-srgb-straight.tex");
fn tile_state(sim: &mut exact_game::Sim<Tiled>) -> String {
    let state = sim.agent(r#"{"op":"state"}"#);
    let at = state
        .find(r#""name":"tile.model""#)
        .expect("tile.model state");
    state[at..].split('}').next().unwrap().to_owned()
}

#[test]
fn a_generated_model_requests_only_its_textures_and_draws_once_they_arrive() {
    let mut sim = exact_game::Sim::<Tiled>::new(Theme::default()).unwrap();
    assert_eq!(sim.take_assets(), ["soil-0.tex"]);
    // A lost device reopens pending names; the model is still not a payload.
    sim.invalidate_device_assets();
    assert_eq!(sim.take_assets(), ["soil-0.tex"]);
    assert!(tile_state(&mut sim).contains("Pending"));
    sim.asset("soil-0.tex", Some(SOIL)).unwrap();
    assert!(tile_state(&mut sim).contains("Loaded"));
    sim.asset_prepared("soil-0.tex", Ok(()));
    sim.asset_prepared("tile.model", Ok(()));
    assert!(sim.model_prepared("tile.model"));
    sim.save().unwrap();

    // A lost device unprepares every name and redelivers the texture; the
    // model is still never fetched.
    assert_eq!(sim.invalidate_device_assets(), ["soil-0.tex"]);
    assert!(!sim.model_prepared("tile.model"));
    assert_eq!(sim.take_assets(), ["soil-0.tex"]);
    sim.asset("soil-0.tex", Some(SOIL)).unwrap();
    sim.asset_prepared("soil-0.tex", Ok(()));
    sim.asset_prepared("tile.model", Ok(()));
    assert!(sim.model_prepared("tile.model"));

    // Another theme replaces the model under the same name: only its texture is fetched.
    sim.bind(&[exact_game::Value::Number(1.)], None).unwrap();
    assert!(tile_state(&mut sim).contains("Pending"));
    assert_eq!(sim.take_assets(), ["soil-1.tex"]);
    sim.asset("soil-1.tex", Some(SOIL)).unwrap();
    assert!(tile_state(&mut sim).contains("Loaded"));
    assert!(sim.take_assets().is_empty());
}

#[test]
fn a_headless_load_fetches_a_generated_models_textures_and_a_failed_one_names_it() {
    let mut fetched = Vec::new();
    let mut sim = exact_game::Sim::<Tiled>::new(Theme::default()).unwrap();
    sim.load_assets(|name: &str| {
        fetched.push(name.to_owned());
        match name {
            "soil-0.tex" => Ok(SOIL.to_vec()),
            _ => Err(format!("`{name}` is not a payload")),
        }
    })
    .unwrap();
    assert_eq!(fetched, ["soil-0.tex"]);
    assert!(tile_state(&mut sim).contains("Loaded"));

    let mut sim = exact_game::Sim::<Tiled>::new(Theme::default()).unwrap();
    let error = sim
        .load_assets(|name: &str| Err::<Vec<u8>, _>(format!("`{name}` is missing")))
        .unwrap_err()
        .to_string();
    let state = tile_state(&mut sim);
    assert!(
        state.contains("Failed") && state.contains("asset `soil-0.tex`: `soil-0.tex` is missing"),
        "{state}"
    );
    assert!(!error.contains("`tile.model` is missing"), "{error}");
}

/// A die whose skin a tick picks after setup: `Game::present` supplies each
/// skin's model when a `Mesh` shows it (issue #88).
struct Dice;
fn skin(name: &str) -> exact_game::asset::Model {
    use exact_game::asset::{MaterialData, Model, Node};
    let mut mesh = triangle();
    mesh.positions[0] = name.len() as f32 * 0.01;
    Model {
        bounds: [0., 0., 0., 1., 0., 1.],
        meshes: vec![mesh],
        materials: vec![MaterialData {
            base_color_texture: Some(0),
            ..Default::default()
        }],
        textures: vec![name.replace(".model", ".tex")],
        nodes: vec![Node {
            mesh: Some(0),
            ..Default::default()
        }],
        ..Default::default()
    }
}
impl exact_game::Game for Dice {
    const ID: &'static str = "cosmetic-dice";
    type Args = ();
    fn setup(w: &mut World, _: &()) {
        w.spawn_named("die", (Transform::default(), Mesh::asset("classic.model")));
    }
    fn tick(w: &mut World, _: &exact_game::Input, _: &()) {
        if w.tick() == 30 {
            *w.require_mut::<Mesh>("die") = Mesh::asset("walnut.model");
        }
    }
    fn present(p: &mut exact_game::Present<'_>, _: &()) {
        let mut names = Vec::new();
        p.for_each::<Mesh>(|_, m| {
            if let Mesh::Asset(name) = m {
                names.push(name.clone());
            }
        });
        for name in names {
            p.cosmetic_model(&name, || skin(&name)).unwrap();
        }
    }
}

#[test]
fn a_cosmetic_model_registers_from_present_after_setup_and_retires_unshown() {
    let mut sim = exact_game::Sim::<Dice>::new(()).unwrap();
    // The model has no payload: only its texture is fetched.
    assert_eq!(sim.take_assets(), ["classic.tex"]);
    assert!(sim.world().model("classic.model").is_none());
    // Its texture still in flight never gates a save: the save is the same.
    let early = sim.save().unwrap();
    sim.asset("classic.tex", Some(SOIL)).unwrap();
    let hash = sim.world().hash();
    let saved = sim.save().unwrap();
    assert_eq!(early, saved);
    assert!(!saved.windows(9).any(|s| s == b"positions"));
    assert!(!saved.windows(13).any(|s| s == b"assetIdentity"));

    sim.run(31. * 1000. / 60.);
    assert_eq!(sim.world().tick(), 31);
    assert!(sim.world().model("walnut.model").is_none());
    let models: Vec<_> = sim.presentation_models().map(|(n, _)| n).collect();
    assert_eq!(models, ["classic.model", "walnut.model"]);
    // No Mesh shows classic any more: it and its texture retire, and walnut's
    // texture is fetched.
    assert_eq!(sim.take_assets(), ["walnut.tex"]);
    let mut retired = sim.take_retired_assets();
    retired.sort();
    assert_eq!(retired, ["classic.model", "classic.tex"]);
    let models: Vec<_> = sim.presentation_models().map(|(n, _)| n).collect();
    assert_eq!(models, ["walnut.model"]);

    // Load order never reaches the simulation: a run whose texture never
    // arrives hashes the same.
    let mut other = exact_game::Sim::<Dice>::new(()).unwrap();
    other.run(31. * 1000. / 60.);
    assert_eq!(other.world().hash(), sim.world().hash());
    assert_ne!(hash, sim.world().hash());

    // A restore keeps presentation residency; present registers what it shows.
    sim.restore(&saved).unwrap();
    assert!(sim.presentation_models().any(|(n, _)| n == "classic.model"));
    assert!(sim.world().model("classic.model").is_none());
    assert_eq!(sim.world().hash(), hash);
}

#[test]
fn a_cosmetic_name_is_immutable_while_registered_and_never_takes_a_declared_name() {
    // The die shows the skin, then a cube, the skin, a cube and the skin
    // again. From tick 7 present builds other content under the same name.
    struct Renamed;
    impl exact_game::Game for Renamed {
        const ID: &'static str = "cosmetic-names";
        type Args = ();
        fn setup(w: &mut World, _: &()) {
            w.generated("rock.model", triangle()).unwrap();
            w.spawn_named("die", (Transform::default(), Mesh::asset("skin.model")));
        }
        fn tick(w: &mut World, _: &exact_game::Input, _: &()) {
            match w.tick() {
                2 | 6 => *w.require_mut::<Mesh>("die") = Mesh::cube(1.),
                4 | 8 => *w.require_mut::<Mesh>("die") = Mesh::asset("skin.model"),
                _ => {}
            }
        }
        fn present(p: &mut exact_game::Present<'_>, _: &()) {
            let tick = p.tick();
            if matches!(*p.require::<Mesh>("die"), Mesh::Asset(_)) {
                let content = if tick < 7 { "a.model" } else { "bb.model" };
                // Resident, the build does not run: its content cannot change.
                p.cosmetic_model("skin.model", || skin(content)).unwrap();
            }
            let error = p
                .cosmetic_model("rock.model", || skin("a.model"))
                .unwrap_err();
            assert!(error.contains("generated"), "{error}");
            let error = p
                .cosmetic_model("skin.tex", || skin("a.model"))
                .unwrap_err();
            assert!(error.contains(".model"), "{error}");
        }
    }
    let mut sim = exact_game::Sim::<Renamed>::new(()).unwrap();
    let first = |sim: &exact_game::Sim<Renamed>| {
        sim.presentation_models()
            .find(|(n, _)| *n == "skin.model")
            .map(|(_, m)| m.meshes[0].positions[0])
    };
    let a = Some(skin("a.model").meshes[0].positions[0]);
    let bb = Some(skin("bb.model").meshes[0].positions[0]);
    let (mut shown_before, mut returns) = (true, 0);
    for tick in 1..=11 {
        sim.run(1000. / 60.);
        assert_eq!(sim.world().tick(), tick);
        sim.take_assets();
        let shown = matches!(*sim.world().require::<Mesh>("die"), Mesh::Asset(_));
        returns += u32::from(shown && !shown_before);
        shown_before = shown;
        // Retired while unshown; registered again with what present builds then.
        let expected = match (shown, returns) {
            (false, _) => None,
            (true, 0 | 1) => a,
            (true, _) => bb,
        };
        assert_eq!(first(&sim), expected, "tick {tick}");
    }
    assert_eq!(returns, 2);
}
