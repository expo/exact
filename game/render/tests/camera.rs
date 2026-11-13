//! The drawn image, inspection and picking share an off-center projection.
#![cfg(not(target_arch = "wasm32"))]
use exact_game::*;
use exact_game_render::WorldSurface;
use exact_gpu::{fixture, Frame, Surface};

#[derive(Default, Args)]
struct Settings {
    shift: f32,
    orthographic: bool,
}
struct Scene;
impl Game for Scene {
    const ID: &'static str = "camera-image-shift";
    type Args = Settings;
    fn setup(w: &mut World, a: &Settings) {
        w.insert_resource(Environment {
            background: Some([0.; 3].into()),
            bloom: None,
            fog: None,
            tone_mapping: ToneMapping::None,
            ..Default::default()
        });
        w.spawn_named(
            "box",
            (
                Transform::at(0., 0., -5.),
                Mesh::cube(0.8),
                Material::rgb(0., 0., 0.).emissive(1., 1., 1.),
            ),
        );
        w.spawn_named(
            "camera",
            (
                Transform::default(),
                Camera {
                    shift: Vec2::new(a.shift, a.shift),
                    ..if a.orthographic {
                        Camera::orthographic(8.)
                    } else {
                        Camera::default()
                    }
                },
            ),
        );
    }
    fn tick(_: &mut World, _: &Input, _: &Settings) {}
}
#[test]
fn a_shift_moves_the_image_and_pick_by_the_same_pixel_distance() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    for orthographic in [false, true] {
        for (width, height) in [(128., 96.), (96., 128.)] {
            let mut centers = Vec::new();
            for shift in [0., 0.25] {
                let mut surface = WorldSurface::<Scene>::default();
                surface
                    .bind(&[Value::Number(shift), Value::Bool(orthographic)], None)
                    .unwrap();
                let frame = Frame {
                    width,
                    height,
                    scale: 1.,
                    now_ms: 0.,
                    seekable: true,
                    period_ms: 0.,
                    children_generation: 0,
                    shader_generation: 0,
                    headroom: 1.,
                };
                let pixels = fixture::render(&gpu, &mut surface, &frame).unwrap().0;
                assert!(surface.error().is_none(), "{:?}", surface.error());
                let mut sum = Vec2::ZERO;
                let mut count = 0.;
                for y in 0..pixels.height {
                    for x in 0..pixels.width {
                        if pixels.at(x, y)[0] > 150 {
                            sum += Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                            count += 1.;
                        }
                    }
                }
                assert!(count > 10., "the white box draws");
                let center = sum / count;
                let sim = surface.sim().unwrap();
                let bounds = sim.layout("box").unwrap().screen;
                assert!(
                    (center - bounds.center()).length() < 1.5,
                    "{center:?} {bounds:?}"
                );
                assert_eq!(
                    sim.pick(center).unwrap().entity,
                    sim.world().named("box").unwrap()
                );
                assert_eq!(
                    *sim.world().require::<Transform>("camera"),
                    Transform::default()
                );
                centers.push(center);
            }
            let expected = Vec2::new(width * 0.125, -height * 0.125);
            assert!(
                (centers[1] - centers[0] - expected).length() < 1.5,
                "{centers:?}"
            );
        }
    }
}
