//! Per-instance scalar factors match authored factors and retain model geometry.
use super::*;
use exact_game::{asset::*, *};

const FACTORS: [[f32; 4]; 4] = [
    [0.2, 0.0, 0.0, 0.2],
    [0.8, 1.0, 0.0, 0.6],
    [0.18, 0.7, 0.85, 0.07],
    [0.55, 0.25, 0.4, 0.65],
];
#[derive(Default, Args)]
struct Look {
    baked: bool,
    coated: bool,
    mapped: bool,
    #[live]
    case: u32,
    #[live]
    enabled: bool,
    pair: bool,
    /// The die's override: 0 every factor, 1 a coat alone, 2 a zero coat, 3 none.
    #[live]
    coat: u32,
}
struct Die;
impl Game for Die {
    const ID: &'static str = "scalar-look";
    type Args = Look;
    fn setup(w: &mut World, a: &Look) {
        w.insert_resource(Environment {
            fog: None,
            bloom: None,
            tone_mapping: ToneMapping::None,
            ..Default::default()
        });
        let pose = if a.pair {
            Transform {
                scale: Vec3::splat(0.4),
                ..Transform::at(-0.9, 0., 0.)
            }
        } else {
            Transform::default()
        };
        w.spawn_named("die", (pose, Mesh::asset("die.model")));
        if a.pair {
            w.spawn_named(
                "other",
                (
                    Transform {
                        scale: Vec3::splat(0.4),
                        ..Transform::at(0.9, 0., 0.)
                    },
                    Mesh::asset("die.model"),
                ),
            );
        }
        w.spawn((Transform::at(0., 0., 4.), Camera::default()));
        w.spawn((
            Transform::at(2., 3., 4.).looking_at(Vec3::ZERO, Vec3::Y),
            DirectionalLight {
                illuminance: 3000.,
                shadows: false,
                ..Default::default()
            },
        ));
    }
    fn tick(_: &mut World, _: &Input, _: &Look) {}
    fn present(p: &mut Present<'_>, a: &Look) {
        p.cosmetic_model("die.model", || {
            let mut model = super::cosmetics::quad("unused.tex");
            let material = &mut model.materials[0];
            *material = MaterialData {
                base_color: [0.12, 0.06, 0.02, 1.],
                metallic: 0.33,
                roughness: 0.47,
                emissive: [0.1, 0.02, 0.01],
                clearcoat: if a.coated { 0.8 } else { 0. },
                clearcoat_roughness: 0.3,
                metallic_roughness_texture: a.mapped.then_some(0),
                ..Default::default()
            };
            if a.baked {
                let [roughness, metallic, clearcoat, clearcoat_roughness] =
                    FACTORS[a.case as usize];
                material.roughness = roughness;
                material.metallic = metallic;
                material.clearcoat = clearcoat;
                material.clearcoat_roughness = clearcoat_roughness;
            }
            model.textures = if a.mapped {
                vec!["surface.tex".into()]
            } else {
                vec![]
            };
            model
        })
        .unwrap();
        if !a.baked && a.enabled {
            for (name, case) in [("die", a.case), ("other", 3)] {
                let Some(entity) = p.named(name) else {
                    continue;
                };
                let [roughness, metallic, clearcoat, clearcoat_roughness] = FACTORS[case as usize];
                let coat = |clearcoat| MaterialOverride {
                    clearcoat: Some(clearcoat),
                    ..Default::default()
                };
                let look = match (name, a.coat) {
                    ("die", 1) => coat(0.8),
                    ("die", 2) => coat(0.),
                    ("die", 3) => continue,
                    _ => MaterialOverride {
                        roughness: Some(roughness),
                        metallic: Some(metallic),
                        clearcoat: Some(clearcoat),
                        clearcoat_roughness: Some(clearcoat_roughness),
                        ..Default::default()
                    },
                };
                p.insert(entity, MaterialOverrides(vec![look]));
            }
        }
    }
}
fn values(baked: bool, coated: bool, mapped: bool, case: u32, enabled: bool) -> [Value; 7] {
    [
        Value::Bool(baked),
        Value::Bool(coated),
        Value::Bool(mapped),
        Value::Number(case.into()),
        Value::Bool(enabled),
        Value::Bool(false),
        Value::Number(0.into()),
    ]
}
fn surface(args: [Value; 7]) -> WorldSurface<Die, crate::ModelExecutor, true> {
    let mut s = WorldSurface::default();
    s.device_ready(exact_gpu::wgpu::Features::empty());
    s.bind(&args, None).unwrap();
    let texel = [0, 128, 192, 255];
    s.asset(
        "surface.tex",
        Ok(&exact_game::bin::to_vec(&TextureData {
            width: 1,
            height: 1,
            mips: vec![texel.to_vec()],
            srgb: false,
            ..Default::default()
        })),
    );
    s
}
fn draw(
    gpu: &Gpu,
    s: &mut WorldSurface<Die, crate::ModelExecutor, true>,
    now: f64,
) -> fixture::Pixels {
    let pixels = fixture::render(gpu, s, &frame(now)).unwrap().0;
    assert!(s.error().is_none(), "{:?}", s.error());
    pixels
}
/// One half of a 64-pixel-wide frame: the die draws left, the other instance right.
fn half(p: &fixture::Pixels, right: bool) -> Vec<[u8; 4]> {
    p.data
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(i, _)| (*i % 64 >= 32) == right)
        .map(|(_, rgba)| *rgba)
        .collect()
}
/// The largest byte difference between two pixel lists.
fn apart(a: &[[u8; 4]], b: &[[u8; 4]]) -> u8 {
    a.iter()
        .flatten()
        .zip(b.iter().flatten())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap()
}
#[test]
fn scalar_overrides_match_baked_factors_with_and_without_textures_or_a_coat() {
    let Some(gpu) = gpu() else { return };
    for mapped in [false, true] {
        for coated in [false, true] {
            let mut first = None;
            for case in 0..FACTORS.len() as u32 {
                let mut overridden = surface(values(false, coated, mapped, case, true));
                let mut baked = surface(values(true, coated, mapped, case, false));
                for now in [0., 20.] {
                    draw(&gpu, &mut overridden, now);
                    draw(&gpu, &mut baked, now);
                }
                let actual = draw(&gpu, &mut overridden, 40.);
                let expected = draw(&gpu, &mut baked, 40.);
                let max_error = actual
                    .data
                    .iter()
                    .zip(&expected.data)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                // Half-precision bare and layered programs can round one byte apart.
                assert!(max_error <= 1,
                    "case {case}, mapped {mapped}, coated {coated}: max byte error {max_error}, center {:?} vs {:?}", actual.at(32,32), expected.at(32,32));
                if let Some(first) = &first {
                    assert!(
                        actual != *first,
                        "the factors must visibly change the material"
                    );
                } else {
                    first = Some(actual);
                }
            }
        }
    }
}
#[test]
fn scalar_animation_reuses_the_layered_variant_and_patches_in_place() {
    let Some(gpu) = gpu() else { return };
    let paired = |case| {
        let mut args = values(false, false, true, case, true);
        args[5] = Value::Bool(true);
        args
    };
    let mut s = surface(paired(2));
    for now in [0., 20., 40.] {
        draw(&gpu, &mut s, now);
    }
    s.renderer_for_test()
        .unwrap()
        .pipelines
        .wait_specializations();
    let before = draw(&gpu, &mut s, 60.);
    let renderer = s.renderer_for_test().unwrap();
    let epoch = renderer.cull.epoch;
    let uploads = renderer.residency_work();
    let model_count = renderer.models.loaded.len();
    assert_eq!(renderer.models.variants.len(), 1);
    for i in 0..16 {
        s.bind(&paired(2 + i % 2), None).unwrap();
        draw(&gpu, &mut s, 80. + f64::from(i) * 20.);
    }
    let after = draw(&gpu, &mut s, 420.);
    let renderer = s.renderer_for_test().unwrap();
    assert_ne!(half(&before, false), half(&after, false));
    assert_eq!(
        half(&before, true),
        half(&after, true),
        "the other instance keeps its factors"
    );
    assert_eq!(renderer.cull.epoch, epoch);
    assert_eq!(renderer.models.variants.len(), 1);
    assert_eq!(renderer.models.loaded.len(), model_count);
    assert_eq!(renderer.residency_work().mesh_uploads, uploads.mesh_uploads);
    assert_eq!(
        renderer.residency_work().texture_uploads,
        uploads.texture_uploads
    );
}
#[test]
fn a_coat_override_set_zeroed_removed_and_set_again_restores_its_pixels() {
    let Some(gpu) = gpu() else { return };
    for coated in [false, true] {
        let step = |coat: u32| {
            let mut args = values(false, coated, false, 0, true);
            args[5] = Value::Bool(true);
            args[6] = Value::Number(coat.into());
            args
        };
        let mut s = surface(step(1));
        let mut now = 0.;
        // Each step draws until its program is compiled, then is read.
        let mut shown = |coat| {
            s.bind(&step(coat), None).unwrap();
            for _ in 0..3 {
                draw(&gpu, &mut s, now);
                now += 20.;
            }
            s.renderer_for_test()
                .unwrap()
                .pipelines
                .wait_specializations();
            now += 20.;
            draw(&gpu, &mut s, now)
        };
        let [set, zeroed, removed, again] = [1, 2, 3, 1].map(&mut shown);
        let die = |p| half(p, false);
        assert_ne!(
            die(&set),
            die(&zeroed),
            "coated {coated}: a zero coat shows"
        );
        assert_eq!(die(&set), die(&again), "coated {coated}: the coat returns");
        // The bare and layered programs can round one byte apart.
        if coated {
            // The override names the authored coat, and a zero coat is not it.
            assert!(apart(&die(&removed), &die(&set)) <= 1);
            assert_ne!(die(&removed), die(&zeroed), "the authored coat returns");
        } else {
            assert!(apart(&die(&removed), &die(&zeroed)) <= 1);
        }
        for (step, pixels) in [
            ("zeroed", &zeroed),
            ("removed", &removed),
            ("again", &again),
        ] {
            assert_eq!(
                half(&set, true),
                half(pixels, true),
                "coated {coated}, {step}: the other instance keeps its coat"
            );
        }
    }
}
