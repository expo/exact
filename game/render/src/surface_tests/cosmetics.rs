//! A look changed at any time (issues #88, #89, #90): a die's skin is a
//! texture named per instance, its shape a cosmetic model; each change keeps
//! the old look until the new one can draw, and releases what is no longer shown.
use super::*;
use exact_game::asset::{AlphaMode, MaterialData, MeshData, Model, Node, TextureData};

#[derive(Default, exact_game::Args)]
struct Look {
    /// 0: the model's own texture; otherwise the skin's atlas.
    #[live]
    skin: u32,
    /// 0: the square shape; 1: the round one; 2: one whose texture fails.
    #[live]
    shape: u32,
}
struct Dice;
impl Game for Dice {
    const ID: &'static str = "cosmetic-looks";
    type Args = Look;
    fn setup(w: &mut World, _: &Look) {
        w.spawn_named("die", (Transform::default(), Mesh::asset("square.model")));
        w.spawn((Transform::at(0., 0., 4.), Camera::default()));
    }
    fn tick(w: &mut World, input: &Input, look: &Look) {
        let shape = ["square.model", "round.model", "broken.model"][look.shape as usize];
        if !matches!(&*w.require::<Mesh>("die"), Mesh::Asset(name) if name == shape) {
            *w.require_mut::<Mesh>("die") = Mesh::asset(shape);
        }
        if input.messages().iter().any(|m| m == "import") {
            w.invalidate_presentation();
        }
    }
    fn present(p: &mut exact_game::Present<'_>, look: &Look) {
        let die = p.named("die").unwrap();
        let Mesh::Asset(shape) = (*p.require::<Mesh>(die)).clone() else {
            unreachable!()
        };
        let texture = match shape.as_str() {
            "round.model" => "yellow.tex",
            "broken.model" => "broken.tex",
            _ => "red.tex",
        };
        p.cosmetic_model(&shape, || quad(texture)).unwrap();
        if look.skin != 0 {
            let atlas = ["", "blue.tex", "green.tex"][look.skin as usize];
            p.insert(
                die,
                exact_game::MaterialOverrides(vec![exact_game::MaterialOverride {
                    material: 0,
                    textures: exact_game::TextureSlots {
                        base_color: Some(atlas.into()),
                        ..Default::default()
                    },
                    ..Default::default()
                }]),
            );
        }
    }
}
/// A square facing the camera, sampling `texture`.
pub(super) fn quad(texture: &str) -> Model {
    let mesh = MeshData {
        positions: vec![-2., -2., 0., 2., -2., 0., 2., 2., 0., -2., 2., 0.],
        normals: [0., 0., 1.].repeat(4),
        uvs: vec![0., 1., 1., 1., 1., 0., 0., 0.],
        indices: vec![0, 1, 2, 0, 2, 3],
        bounds: [-2., -2., 0., 2., 2., 0.],
        ..Default::default()
    };
    Model {
        bounds: mesh.bounds,
        meshes: vec![mesh],
        materials: vec![MaterialData {
            base_color_texture: Some(0),
            metallic: 0.,
            alpha_mode: AlphaMode::Opaque,
            double_sided: true,
            ..Default::default()
        }],
        textures: vec![texture.into()],
        nodes: vec![Node {
            mesh: Some(0),
            ..Default::default()
        }],
        ..Default::default()
    }
}
/// A solid 4×4 sRGB texture with its mips, as a delivered `.tex` file.
pub(super) fn solid(rgb: [u8; 3]) -> Vec<u8> {
    let texel = [rgb[0], rgb[1], rgb[2], 255];
    exact_game::bin::to_vec(&TextureData {
        width: 4,
        height: 4,
        mips: [16, 4, 1].map(|n| texel.repeat(n)).to_vec(),
        srgb: true,
        ..Default::default()
    })
}
/// The centre pixel's dominant channel: 0 red, 1 green, 2 blue, 3 yellow.
pub(super) fn hue(pixels: &fixture::Pixels) -> usize {
    let [r, g, b, _] = pixels.at(32, 32);
    let (r, g, b) = (u32::from(r), u32::from(g), u32::from(b));
    assert!(r.max(g).max(b) > 20, "the die draws: {r} {g} {b}");
    if r > 2 * b && g > 2 * b && r.abs_diff(g) * 4 < r {
        3
    } else if r > 2 * g.max(b) {
        0
    } else if g > 2 * r.max(b) {
        1
    } else {
        assert!(b > 2 * r.max(g), "{r} {g} {b}");
        2
    }
}

#[test]
fn a_skin_and_a_shape_change_keep_the_old_look_until_the_new_one_draws() {
    let Some(gpu) = gpu() else { return };
    let mut s = WorldSurface::<Dice, crate::ModelExecutor, true>::default();
    s.device_ready(exact_gpu::wgpu::Features::empty());
    let bind = |s: &mut WorldSurface<Dice, crate::ModelExecutor, true>, skin: u32, shape: u32| {
        s.bind(
            &[Value::Number(skin.into()), Value::Number(shape.into())],
            None,
        )
        .unwrap();
    };
    bind(&mut s, 0, 0);
    let mut now = 0.;
    let mut draw = |s: &mut WorldSurface<Dice, crate::ModelExecutor, true>| {
        now += 50.;
        hue(&fixture::render(&gpu, s, &frame(now)).unwrap().0)
    };
    let deliver = |s: &mut WorldSurface<Dice, crate::ModelExecutor, true>, name, rgb| {
        s.asset(name, Ok(&solid(rgb)));
    };
    // The cosmetic model is never fetched: only its texture is.
    assert_eq!(s.assets().requests, ["red.tex"]);
    deliver(&mut s, "red.tex", [255, 0, 0]);
    assert_eq!(draw(&mut s), 0);
    assert!(s.sim().unwrap().world().model("square.model").is_none());

    // A skin whose atlas is not resident: the old look stays until it lands.
    bind(&mut s, 1, 0);
    assert_eq!(draw(&mut s), 0);
    assert_eq!(s.assets().requests, ["blue.tex"]);
    assert_eq!(draw(&mut s), 0);
    deliver(&mut s, "blue.tex", [0, 0, 255]);
    assert_eq!(draw(&mut s), 2);
    assert!(
        s.assets().retired.is_empty(),
        "the model keeps its own texture"
    );

    // Another skin: the drawn atlas is held until the next one draws, then released.
    bind(&mut s, 2, 0);
    assert_eq!(draw(&mut s), 2);
    let changes = s.assets();
    assert_eq!(changes.requests, ["green.tex"]);
    assert!(changes.retired.is_empty(), "{:?}", changes.retired);
    deliver(&mut s, "green.tex", [0, 255, 0]);
    assert_eq!(draw(&mut s), 1);
    let retired = s.assets().retired;
    assert!(retired.iter().any(|n| n.contains("blue")), "{retired:?}");
    assert!(!s.render.as_ref().unwrap().0.models.textures["blue.tex"].active);

    // Back to the model's own texture, resident: it draws at once, and nothing
    // shows the skin any more.
    bind(&mut s, 0, 0);
    assert_eq!(draw(&mut s), 0);
    let retired = s.assets().retired;
    assert!(retired.iter().any(|n| n.contains("green")), "{retired:?}");

    // A shape whose texture fails: the die keeps drawing the square, which
    // stays resident.
    bind(&mut s, 0, 2);
    assert_eq!(draw(&mut s), 0);
    assert_eq!(s.assets().requests, ["broken.tex"]);
    s.asset("broken.tex", Err(AssetError::Failed("gone".into())));
    assert_eq!(draw(&mut s), 0);
    assert!(s.assets().retired.is_empty());
    assert_eq!(draw(&mut s), 0);

    // A shape still in flight: the die keeps drawing the square.
    bind(&mut s, 0, 1);
    assert_eq!(draw(&mut s), 0);
    let changes = s.assets();
    assert_eq!(changes.requests, ["yellow.tex"]);
    // Nothing shows the broken shape any more; the square stands in, held.
    let mut retired = changes.retired;
    retired.sort();
    assert_eq!(retired, ["broken.model", "broken.tex"]);
    assert_eq!(draw(&mut s), 0);
    deliver(&mut s, "yellow.tex", [255, 255, 0]);
    assert_eq!(draw(&mut s), 3);
    // Nothing shows the square any more: it and its texture retire.
    let names = s.assets().retired.join(" ");
    for name in ["square.model", "red"] {
        assert!(names.contains(name), "{name}: {names}");
    }
    // No draw uses a skin's material any more: it was reclaimed.
    assert!(s.render.as_ref().unwrap().0.models.variants.is_empty());
    let sim = s.sim().unwrap();
    assert_eq!(
        sim.presentation_models()
            .map(|(n, _)| n)
            .collect::<Vec<_>>(),
        ["round.model"]
    );
}

/// A ribbon's tail scrolled per frame (issue #101): a blended quad whose base
/// colour texture is red on its left half and blue on its right.
#[derive(Default, exact_game::Args)]
struct Scroll {
    /// 0: no override; otherwise the UV offset is (step − 1) / 2 along u.
    #[live]
    step: u32,
}
struct Ribbon;
impl Game for Ribbon {
    const ID: &'static str = "scrolled-ribbon";
    type Args = Scroll;
    fn setup(w: &mut World, _: &Scroll) {
        w.spawn_named(
            "ribbon",
            (Transform::default(), Mesh::asset("ribbon.model")),
        );
        w.spawn((Transform::at(0., 0., 4.), Camera::default()));
    }
    fn tick(_: &mut World, _: &Input, _: &Scroll) {}
    fn present(p: &mut exact_game::Present<'_>, scroll: &Scroll) {
        let ribbon = p.named("ribbon").unwrap();
        p.cosmetic_model("ribbon.model", || {
            let mut model = quad("tail.tex");
            model.materials[0].alpha_mode = AlphaMode::Blend;
            model
        })
        .unwrap();
        if scroll.step != 0 {
            let u = (scroll.step - 1) as f32 / 2.;
            p.insert(
                ribbon,
                exact_game::MaterialOverrides(vec![exact_game::MaterialOverride {
                    material: 0,
                    uv_offset: Some([u, 0.]),
                    ..Default::default()
                }]),
            );
        }
    }
}

#[test]
fn a_blended_material_s_uv_offset_scrolls_its_texture_in_place() {
    let Some(gpu) = gpu() else { return };
    let mut s = WorldSurface::<Ribbon, crate::ModelExecutor, true>::default();
    s.device_ready(exact_gpu::wgpu::Features::empty());
    s.bind(&[Value::Number(0u32.into())], None).unwrap();
    // Red then blue columns, sampled nearest, wrapping.
    let red = [255, 0, 0, 255];
    let blue = [0, 0, 255, 255];
    let row = [red, red, blue, blue].concat();
    s.asset(
        "tail.tex",
        Ok(&exact_game::bin::to_vec(&TextureData {
            width: 4,
            height: 4,
            mips: vec![row.repeat(4), [red, blue].concat().repeat(2), red.to_vec()],
            srgb: true,
            filter: [exact_game::asset::Filter::Nearest; 3],
            ..Default::default()
        })),
    );
    let mut now = 0.;
    let mut draw = |s: &mut WorldSurface<Ribbon, crate::ModelExecutor, true>, step: u32| {
        s.bind(&[Value::Number(step.into())], None).unwrap();
        now += 50.;
        fixture::render(&gpu, s, &frame(now)).unwrap().0
    };
    // The left and right quarters' hues.
    let sides = |p: &fixture::Pixels| {
        let at = |x| {
            let [r, _, b, _] = p.at(x, 32);
            assert!(r.max(b) > 20, "the ribbon draws at {x}");
            if r > b {
                0
            } else {
                2
            }
        };
        (at(20), at(44))
    };
    let plain = draw(&mut s, 0);
    assert_eq!(sides(&plain), (0, 2));
    // An offset of zero adds a neutral look entry: the same bytes.
    let zero = draw(&mut s, 1);
    assert!(plain == zero, "a zero offset draws as no offset");
    let epoch = s.render.as_ref().unwrap().0.cull.epoch;
    // Half a turn swaps the halves; a turn and a half wraps to the same.
    assert_eq!(sides(&draw(&mut s, 2)), (2, 0));
    assert_eq!(sides(&draw(&mut s, 4)), (2, 0));
    // A whole turn is where it started.
    assert!(draw(&mut s, 3) == plain);
    assert_eq!(
        s.render.as_ref().unwrap().0.cull.epoch,
        epoch,
        "a changed offset patches the instance in place"
    );
}

#[derive(Default, exact_game::Args)]
struct Shaded {
    /// 0: lit; 1: unlit; 2: under a clear coat.
    shading: u32,
    /// Use blue for the model's own texture.
    blue: bool,
    /// An instance override replaces the base colour texture with blue.
    skin: bool,
    /// Request an undelivered model after the first look draws.
    #[live]
    pending: bool,
}
struct ShadedSkin;
impl Game for ShadedSkin {
    const ID: &'static str = "cosmetic-shaded-skin";
    type Args = Shaded;
    fn setup(w: &mut World, _: &Shaded) {
        w.insert_resource(exact_game::Environment {
            background: Some([0.; 3].into()),
            ambient: 0.,
            bloom: None,
            fog: None,
            tone_mapping: exact_game::ToneMapping::None,
            ..Default::default()
        });
        w.spawn_named("die", (Transform::default(), Mesh::asset("die.model")));
        w.spawn((Transform::at(0., 0., 4.), Camera::default()));
        // Light faces the quad so that the coat's highlight is visible.
        w.spawn((
            Transform::at(0., 0., 1.).looking_at(Vec3::ZERO, Vec3::Y),
            exact_game::DirectionalLight {
                illuminance: 1_000.,
                shadows: false,
                ..Default::default()
            },
        ));
    }
    fn tick(w: &mut World, _: &Input, args: &Shaded) {
        if args.pending {
            *w.require_mut::<Mesh>("die") = Mesh::asset("pending.model");
        }
    }
    fn present(p: &mut exact_game::Present<'_>, args: &Shaded) {
        let die = p.named("die").unwrap();
        p.cosmetic_model("die.model", || {
            let mut model = quad(if args.blue { "blue.tex" } else { "red.tex" });
            let material = &mut model.materials[0];
            material.unlit = args.shading == 1;
            if args.shading >= 2 {
                material.clearcoat = 1.;
                material.clearcoat_roughness = 0.2;
                if args.shading >= 3 {
                    material.metallic_roughness_texture = Some(1);
                    material.clearcoat_mask_from_alpha = args.shading == 3;
                    model.textures.push("mask.tex".into());
                }
            }
            model
        })
        .unwrap();
        if args.skin {
            p.insert(
                die,
                exact_game::MaterialOverrides(vec![exact_game::MaterialOverride {
                    material: 0,
                    textures: exact_game::TextureSlots {
                        base_color: Some("blue.tex".into()),
                        ..Default::default()
                    },
                    ..Default::default()
                }]),
            );
        }
    }
}

/// A texture override keeps the material's shading, including while its model
/// stands in for a replacement that has not arrived.
#[test]
fn an_instance_skin_keeps_an_unlit_or_coated_materials_shading() {
    let Some(gpu) = gpu() else { return };
    let centre = |shading: u32, blue: bool, skin: bool| -> [u8; 4] {
        let mut s = WorldSurface::<ShadedSkin, crate::ModelExecutor, true>::default();
        s.device_ready(exact_gpu::wgpu::Features::empty());
        s.bind(
            &[
                Value::Number(shading.into()),
                Value::Bool(blue),
                Value::Bool(skin),
                Value::Bool(false),
            ],
            None,
        )
        .unwrap();
        let mut requests = s.assets().requests;
        let mut now = 0.;
        for _ in 0..3 {
            if requests.is_empty() {
                break;
            }
            for name in &requests {
                if name == "mask.tex" {
                    s.asset(
                        name,
                        Ok(&exact_game::bin::to_vec(&TextureData {
                            width: 4,
                            height: 4,
                            srgb: false,
                            mips: [16, 4, 1].map(|n| [255, 255, 255, 0].repeat(n)).to_vec(),
                            ..Default::default()
                        })),
                    );
                    continue;
                }
                let rgb = match name.as_str() {
                    "blue.tex" => [0, 0, 255],
                    "red.tex" => [255, 0, 0],
                    _ => panic!("unexpected asset: {name}"),
                };
                s.asset(name, Ok(&solid(rgb)));
            }
            now += 50.;
            fixture::render(&gpu, &mut s, &frame(now)).unwrap();
            requests = s.assets().requests;
        }
        assert!(requests.is_empty(), "pending assets: {requests:?}");
        now += 50.;
        let (pixels, _) = fixture::render(&gpu, &mut s, &frame(now)).unwrap();
        assert!(s.error().is_none(), "{:?}", s.error());
        if skin {
            let models = &s.render.as_ref().unwrap().0.models;
            let [variant] = models.variants.values().collect::<Vec<_>>()[..] else {
                panic!("one skinned material: {:?}", models.variants.keys());
            };
            let material = &models.materials[variant.0];
            assert_eq!(material.layered, shading >= 2);
            s.bind(
                &[
                    Value::Number(shading.into()),
                    Value::Bool(blue),
                    Value::Bool(skin),
                    Value::Bool(true),
                ],
                None,
            )
            .unwrap();
            now += 50.;
            let (fallback, _) = fixture::render(&gpu, &mut s, &frame(now)).unwrap();
            assert!(s.error().is_none(), "{:?}", s.error());
            assert_eq!(s.assets().requests, ["pending.model"]);
            assert_eq!(fallback.data, pixels.data, "fallback shading {shading}");
        }
        pixels.at(32, 32)
    };
    let mut drawn = Vec::new();
    for shading in 0..3 {
        let own = centre(shading, true, false);
        assert!(
            own[2] > 20 && own[2] > own[0].max(own[1]),
            "shading {shading}: {own:?}"
        );
        assert_eq!(centre(shading, false, true), own, "shading {shading}");
        drawn.push(own);
    }
    assert_ne!(drawn[1], drawn[0], "unlit draws apart from lit");
    assert_ne!(drawn[2], drawn[0], "the coat draws apart from no coat");
    assert_eq!(
        centre(3, true, false),
        drawn[0],
        "zero mask removes the coat"
    );
    assert_eq!(
        centre(4, true, false),
        drawn[2],
        "alpha stays unused unless opted in"
    );
    assert_eq!(
        centre(3, false, true),
        drawn[0],
        "mask survives texture override and fallback"
    );
}

#[test]
fn a_new_picture_waits_for_a_shown_texture_not_yet_delivered() {
    let Some(gpu) = gpu() else { return };
    let mut s = WorldSurface::<Dice, crate::ModelExecutor, true>::default();
    s.device_ready(exact_gpu::wgpu::Features::empty());
    let live = |now| Frame {
        seekable: false,
        ..frame(now)
    };
    s.bind(
        &[Value::Number(0u32.into()), Value::Number(0u32.into())],
        None,
    )
    .unwrap();
    s.clock(false);
    s.asset("red.tex", Ok(&solid([255, 0, 0])));
    let mut now = 0.;
    while s.ready_work.is_none() {
        assert!(now < 10_000., "a live frame drew");
        now += 20.;
        fixture::render(&gpu, &mut s, &live(now)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    // An import shows a skin whose bytes have not arrived.
    s.bind(
        &[Value::Number(1u32.into()), Value::Number(0u32.into())],
        None,
    )
    .unwrap();
    s.input(&exact_gpu::InputEvent::Message {
        text: "import".into(),
        at_ms: now + 1.,
    });
    for _ in 0..5 {
        now += 20.;
        fixture::render(&gpu, &mut s, &live(now)).unwrap();
        assert!(
            !s.frame_ready(),
            "a shown texture not delivered cannot present"
        );
        assert!(s.ready_work.is_none());
    }
    assert!(s.assets().requests.contains(&"blue.tex".to_owned()));
    s.asset("blue.tex", Ok(&solid([0, 0, 255])));
    let mut complete = None;
    for _ in 0..500 {
        now += 20.;
        let pixels = fixture::render(&gpu, &mut s, &live(now)).unwrap().0;
        if s.frame_ready() {
            complete = Some(pixels);
            break;
        }
        std::thread::yield_now();
    }
    assert_eq!(hue(&complete.expect("the new picture became ready")), 2);
}
