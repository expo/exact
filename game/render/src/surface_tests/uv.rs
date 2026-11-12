//! UV scrolling on blended base and emission textures, including unlit materials.
use super::gpu;
use crate::{ModelExecutor, WorldSurface};
use exact_game::{asset::*, *};
use exact_gpu::{fixture, Frame, Surface, Value};

#[derive(Default, Args)]
struct Settings {
    #[live]
    mode: u32,
    #[live]
    offset: f32,
    #[live]
    animate: bool,
    #[live]
    interpolate: bool,
}
struct Ribbon;
impl Game for Ribbon {
    const ID: &'static str = "uv-api-repro";
    const HZ: u32 = 10;
    type Args = Settings;
    fn setup(w: &mut World, a: &Settings) {
        w.insert_resource(Environment {
            fog: None,
            bloom: None,
            tone_mapping: ToneMapping::None,
            ..Default::default()
        });
        w.spawn_named(
            "ribbon",
            (
                Transform::default(),
                Mesh::asset(format!("ribbon-{}.model", a.mode)),
            ),
        );
        w.spawn((Transform::at(0.0, 0.0, 4.0), Camera::default()));
        w.spawn(DirectionalLight {
            illuminance: 100_000.0,
            ..Default::default()
        });
    }
    fn tick(w: &mut World, _: &Input, a: &Settings) {
        *w.require_mut::<Mesh>("ribbon") = Mesh::asset(format!("ribbon-{}.model", a.mode));
        if a.animate {
            w.busy("UV animation");
        }
    }
    fn present(p: &mut Present<'_>, a: &Settings) {
        let e = p.named("ribbon").unwrap();
        let name = format!("ribbon-{}.model", a.mode);
        p.cosmetic_model(&name, || quad(a.mode)).unwrap();
        p.insert(
            e,
            MaterialOverrides(vec![MaterialOverride {
                uv_offset: Some([
                    if a.animate {
                        p.seconds() as f32 * 4.0
                    } else {
                        a.offset
                    },
                    0.0,
                ]),
                uv_offset_previous: (a.animate && a.interpolate)
                    .then(|| [(p.seconds() as f32 - 0.1) * 4.0, 0.0]),
                ..Default::default()
            }]),
        );
    }
}
fn quad(mode: u32) -> Model {
    let bounds = [-1.0, -1.0, 0.0, 1.0, 1.0, 0.0];
    Model {
        meshes: vec![MeshData {
            positions: vec![
                -1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0, 1.0, 0.0, -1.0, 1.0, 0.0,
            ],
            normals: [0.0, 0.0, 1.0].repeat(4),
            uvs: vec![0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0],
            indices: vec![0, 1, 2, 0, 2, 3],
            bounds,
            ..Default::default()
        }],
        materials: vec![MaterialData {
            unlit: mode != 2,
            base_color: if mode == 0 {
                [1.0; 4]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            },
            base_color_texture: (mode == 0).then_some(0),
            emissive: if mode == 1 || mode == 2 {
                [1.0; 3]
            } else {
                [0.0; 3]
            },
            emissive_texture: (mode == 1 || mode == 2).then_some(0),
            alpha_mode: AlphaMode::Blend,
            ..Default::default()
        }],
        textures: if mode == 3 {
            vec![]
        } else {
            vec!["tail.tex".into()]
        },
        nodes: vec![Node {
            mesh: Some(0),
            ..Default::default()
        }],
        bounds,
        ..Default::default()
    }
}
fn surface(
    mode: u32,
    animate: bool,
    interpolate: bool,
) -> WorldSurface<Ribbon, ModelExecutor, true> {
    let mut s = WorldSurface::default();
    s.device_ready(exact_gpu::wgpu::Features::empty());
    s.bind(
        &[
            Value::Number(mode.into()),
            Value::Number(0.0),
            Value::Bool(animate),
            Value::Bool(interpolate),
        ],
        None,
    )
    .unwrap();
    let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255]);
    s.asset(
        "tail.tex",
        Ok(&exact_game::bin::to_vec(&TextureData {
            width: 4,
            height: 4,
            mips: vec![
                [red, red, blue, blue].concat().repeat(4),
                [red, blue].concat().repeat(2),
                red.to_vec(),
            ],
            srgb: true,
            filter: [Filter::Nearest; 3],
            ..Default::default()
        })),
    );
    s
}
fn draw(
    gpu: &exact_gpu::Gpu,
    s: &mut WorldSurface<Ribbon, ModelExecutor, true>,
    now_ms: f64,
) -> fixture::Pixels {
    let frame = Frame {
        width: 64.0,
        height: 64.0,
        scale: 1.0,
        now_ms,
        seekable: true,
        period_ms: 0.0,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    };
    let image = fixture::render(gpu, s, &frame).unwrap().0;
    assert!(s.error().is_none(), "{:?}", s.error());
    image
}
#[test]
fn base_colour_uv_offset_scrolls_and_wraps() {
    let Some(gpu) = gpu() else { return };
    let mut s = surface(0, false, false);
    for now in [0.0, 100.0, 200.0] {
        draw(&gpu, &mut s, now);
    }
    let a = draw(&gpu, &mut s, 300.0);
    s.bind(
        &[
            Value::Number(0.0),
            Value::Number(0.5),
            Value::Bool(false),
            Value::Bool(false),
        ],
        None,
    )
    .unwrap();
    let b = draw(&gpu, &mut s, 400.0);
    assert!(a != b);
    s.bind(
        &[
            Value::Number(0.0),
            Value::Number(1.5),
            Value::Bool(false),
            Value::Bool(false),
        ],
        None,
    )
    .unwrap();
    assert!(b == draw(&gpu, &mut s, 500.0));
    s.bind(
        &[
            Value::Number(0.0),
            Value::Number(1.6),
            Value::Bool(false),
            Value::Bool(false),
        ],
        None,
    )
    .unwrap();
    assert!(
        b != draw(&gpu, &mut s, 600.0),
        "the fixture sees a .1-repeat move"
    );
}
#[test]
fn unlit_emission_uv_offset_scrolls_like_the_lit_material() {
    let Some(gpu) = gpu() else { return };
    for mode in [2, 1] {
        let mut s = surface(mode, false, false);
        for now in [0.0, 100.0, 200.0] {
            draw(&gpu, &mut s, now);
        }
        let a = draw(&gpu, &mut s, 300.0);
        s.bind(
            &[
                Value::Number(mode.into()),
                Value::Number(0.5),
                Value::Bool(false),
                Value::Bool(false),
            ],
            None,
        )
        .unwrap();
        let b = draw(&gpu, &mut s, 400.0);
        assert!(a != b, "mode {mode}: the emission texture should scroll");
    }
}
#[test]
fn uv_animation_changes_between_world_ticks() {
    let Some(gpu) = gpu() else { return };
    // At 4 repeats/s, the 25 ms interval below should move .1 repeat.
    // Check that this fixture can see that move before blaming tick timing.
    let mut control = surface(0, false, false);
    for now in [0.0, 100.0, 200.0] {
        draw(&gpu, &mut control, now);
    }
    control
        .bind(
            &[
                Value::Number(0.0),
                Value::Number(3.0),
                Value::Bool(false),
                Value::Bool(false),
            ],
            None,
        )
        .unwrap();
    let expected_a = draw(&gpu, &mut control, 300.0);
    control
        .bind(
            &[
                Value::Number(0.0),
                Value::Number(3.1),
                Value::Bool(false),
                Value::Bool(false),
            ],
            None,
        )
        .unwrap();
    assert!(
        expected_a != draw(&gpu, &mut control, 400.0),
        "the expected between-frame move must be visible"
    );
    for interpolate in [false, true] {
        let mut s = surface(0, true, interpolate);
        for i in 0..=8 {
            draw(&gpu, &mut s, i as f64 * 100.0);
        }
        let a = draw(&gpu, &mut s, 850.0);
        let tick = s.sim().unwrap().world().tick();
        let hash = s.sim().unwrap().world().hash();
        let epoch = s.renderer_for_test().unwrap().cull.epoch;
        let alpha = s.sim().unwrap().alpha();
        let b = draw(&gpu, &mut s, 875.0);
        assert_eq!(tick, s.sim().unwrap().world().tick());
        assert!(s.sim().unwrap().alpha() > alpha);
        assert_eq!(hash, s.sim().unwrap().world().hash());
        assert_eq!(epoch, s.renderer_for_test().unwrap().cull.epoch);
        assert!(
            a != draw(&gpu, &mut s, 905.0),
            "the next tick must visibly advance the UV"
        );
        assert_eq!(
            a == b,
            !interpolate,
            "only an opted-in head advances between ticks"
        );
    }
}
#[test]
fn unlit_black_has_no_light_residue() {
    let Some(gpu) = gpu() else { return };
    let mut s = surface(3, false, false);
    for now in [0.0, 100.0, 200.0] {
        draw(&gpu, &mut s, now);
    }
    let image = draw(&gpu, &mut s, 300.0);
    assert_eq!(&image.at(32, 32)[..3], &[0, 0, 0]);
}
