//! Local and directional lights on the GPU: spot cones, more than sixteen
//! lights, the second directional light and photometric units.
#![cfg(not(target_arch = "wasm32"))]
#[path = "fixture/device.rs"]
mod test_device;
use exact_game::*;
use exact_game_render::WorldSurface;
use exact_gpu::{fixture, Frame, Gpu, Surface, Value};

fn gpu() -> Option<Gpu> {
    test_device::device_or_skip(fixture::device())
}

#[derive(Default, Args)]
struct Scene {
    kind: u32,
}
const SPOT: u32 = 0;
const POINT: u32 = 1;
const MANY: u32 = 2;
const FILL: u32 = 3;
const SPOT_SHADOW: u32 = 4;
const SPOT_UNSHADOWED: u32 = 5;
const POINT_SHADOW: u32 = 6;
const POINT_UNSHADOWED: u32 = 7;
// A top-down orthographic view: 20 m tall, so 18 px per metre at 640 × 360,
// with +X right and +Z down the screen.
const PX: f32 = 18.;
struct Lights;
impl Game for Lights {
    const ID: &'static str = "render-lights";
    type Args = Scene;
    fn setup(w: &mut World, args: &Scene) {
        w.insert_resource(Environment {
            ambient: 0.,
            bloom: None,
            fog: None,
            background: Some([0.; 3].into()),
            ..Default::default()
        });
        w.spawn((
            Transform::default(),
            Mesh::plane(40., 40.),
            Material::rgb(0.6, 0.6, 0.6),
        ));
        w.spawn((
            Transform::at(0., 10., 0.).looking_at(Vec3::ZERO, -Vec3::Z),
            Camera::orthographic(20.),
        ));
        let down = |x: f32, y: f32, z: f32| {
            Transform::at(x, y, z).looking_at(Vec3::new(x, 0., z), Vec3::Z)
        };
        let sun = DirectionalLight {
            illuminance: 0.,
            ..Default::default()
        };
        w.spawn((down(0., 5., 0.), sun));
        match args.kind {
            SPOT => {
                w.spawn((
                    down(0., 4., 0.),
                    SpotLight {
                        intensity: 20_000.,
                        range: 12.,
                        inner: 0.2,
                        outer: 0.3,
                        ..Default::default()
                    },
                ));
            }
            POINT => {
                w.spawn((
                    down(0., 4., 0.),
                    PointLight {
                        intensity: 20_000.,
                        range: 12.,
                        ..Default::default()
                    },
                ));
            }
            MANY => {
                for i in 0..40 {
                    let (x, z) = ((i % 8) as f32 * 2. - 7., (i / 8) as f32 * 2. - 4.);
                    w.spawn((
                        Transform::at(x, 0.5, z),
                        PointLight {
                            intensity: 2_000.,
                            range: 0.9,
                            ..Default::default()
                        },
                    ));
                }
            }
            SPOT_SHADOW | SPOT_UNSHADOWED => {
                let spot = w.spawn((
                    down(0., 4., 0.),
                    SpotLight {
                        intensity: 20_000.,
                        range: 12.,
                        inner: 0.2,
                        outer: 0.3,
                        ..Default::default()
                    },
                ));
                if args.kind == SPOT_SHADOW {
                    w.insert(spot, LightShadows);
                }
                // Halfway down: its shadow on the floor is twice its width.
                w.spawn((
                    Transform {
                        position: Vec3::new(0., 2., 0.),
                        scale: Vec3::new(0.6, 0.1, 0.6),
                        ..Default::default()
                    },
                    Mesh::cube(1.),
                    Material::rgb(0.6, 0.6, 0.6),
                ));
            }
            POINT_SHADOW | POINT_UNSHADOWED => {
                let lamp = w.spawn((
                    Transform::at(0., 2., 0.),
                    PointLight {
                        intensity: 20_000.,
                        range: 8.,
                        ..Default::default()
                    },
                ));
                if args.kind == POINT_SHADOW {
                    w.insert(lamp, LightShadows);
                }
                // Walls on +X and +Z: two cube faces' maps.
                for (x, z, size) in [
                    (2., 0., Vec3::new(0.2, 2., 3.)),
                    (0., 2.5, Vec3::new(3., 2., 0.2)),
                ] {
                    w.spawn((
                        Transform {
                            position: Vec3::new(x, 1., z),
                            scale: size,
                            ..Default::default()
                        },
                        Mesh::cube(1.),
                        Material::rgb(0.6, 0.6, 0.6),
                    ));
                }
            }
            _ => {
                w.spawn((
                    down(0., 5., 0.),
                    DirectionalLight {
                        illuminance: 10_000.,
                        shadows: true,
                        ..Default::default()
                    },
                ));
            }
        }
    }
    fn tick(_: &mut World, _: &Input, _: &Scene) {}
}

fn render(gpu: &Gpu, kind: u32, name: &str) -> (fixture::Pixels, String) {
    let mut surface = WorldSurface::<Lights>::default();
    surface
        .bind(&[Value::Number(f64::from(kind))], None)
        .unwrap();
    let frame = Frame {
        width: 640.,
        height: 360.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    };
    let (pixels, _) = fixture::render(gpu, &mut surface, &frame).unwrap();
    assert!(surface.error().is_none(), "{:?}", surface.error());
    pixels.save(name);
    let state = surface.agent(r#"{"op":"state"}"#).unwrap();
    (pixels, state)
}
// Mean red channel over a 5 × 5 patch at world (x, z) on the floor.
fn at(image: &fixture::Pixels, x: f32, z: f32) -> f64 {
    let (cx, cy) = ((320. + x * PX) as u32, (180. + z * PX) as u32);
    let mut sum = 0.;
    for y in cy - 2..=cy + 2 {
        for x in cx - 2..=cx + 2 {
            sum += f64::from(image.at(x, y)[0]);
        }
    }
    sum / 25.
}

#[test]
fn a_spot_light_lights_its_cone_and_a_point_light_everywhere_in_range() {
    let Some(gpu) = gpu() else { return };
    let (spot, _) = render(&gpu, SPOT, "lights-spot");
    let (point, _) = render(&gpu, POINT, "lights-point");
    // The cone's edge on the floor is 4 m × tan 0.3 ≈ 1.24 m from its axis.
    let values = [
        at(&spot, 0., 0.),
        at(&spot, 0.6, 0.),
        at(&spot, 2.5, 0.),
        at(&point, 2.5, 0.),
    ];
    eprintln!("spot axis, inside, outside; point outside the cone: {values:?}");
    assert!(values[0] > 60. && values[1] > 60., "{values:?}");
    assert_eq!(values[2], 0., "{values:?}");
    assert!(values[3] > 30., "{values:?}");
    // Inside the inner cone a spot equals the point light of the same intensity.
    assert!((values[0] - at(&point, 0., 0.)).abs() <= 1., "{values:?}");
}

#[test]
fn forty_point_lights_all_light_the_floor_and_none_are_dropped() {
    let Some(gpu) = gpu() else { return };
    let (image, state) = render(&gpu, MANY, "lights-forty");
    let mut dark = Vec::new();
    for i in 0..40 {
        let (x, z) = ((i % 8) as f32 * 2. - 7., (i / 8) as f32 * 2. - 4.);
        if at(&image, x, z) < 40. {
            dark.push(i);
        }
        // Between the lights the floor is out of every range.
        assert_eq!(at(&image, x + 1., z + 1.), 0., "light {i}");
    }
    assert!(dark.is_empty(), "unlit under lights {dark:?}");
    assert!(
        state.contains(r#""lights":{"drawn":40,"dropped":0}"#),
        "{state}"
    );
}

#[test]
fn the_second_directional_light_is_an_unshadowed_fill() {
    let Some(gpu) = gpu() else { return };
    let (image, _) = render(&gpu, FILL, "lights-fill");
    // The first light (the sun) is dark; the second lights the floor alone.
    assert!(at(&image, 0., 0.) > 100., "{}", at(&image, 0., 0.));
}

#[test]
fn a_shadowed_spot_light_darkens_the_floor_behind_a_caster() {
    let Some(gpu) = gpu() else { return };
    let (shadowed, _) = render(&gpu, SPOT_SHADOW, "lights-spot-shadow");
    let (open, _) = render(&gpu, SPOT_UNSHADOWED, "lights-spot-unshadowed");
    // 0.45 m off axis: outside the 0.6 m caster, inside its 1.2 m shadow.
    let values = [
        at(&shadowed, 0.45, 0.),
        at(&open, 0.45, 0.),
        at(&shadowed, 0.9, 0.),
    ];
    eprintln!("shadowed, unshadowed, beyond the shadow: {values:?}");
    assert!(values[0] < 5., "{values:?}");
    assert!(values[1] > 60., "{values:?}");
    assert!(values[2] > 40., "{values:?}");
}

#[test]
fn a_shadowed_point_light_casts_through_each_cube_face() {
    let Some(gpu) = gpu() else { return };
    let (shadowed, _) = render(&gpu, POINT_SHADOW, "lights-point-shadow");
    let (open, _) = render(&gpu, POINT_UNSHADOWED, "lights-point-unshadowed");
    // Behind the +X and +Z walls, and the open floor on -X and -Z.
    let behind = [at(&shadowed, 3., 0.), at(&shadowed, 0., 3.5)];
    let unshadowed = [at(&open, 3., 0.), at(&open, 0., 3.5)];
    let away = [at(&shadowed, -3., 0.), at(&shadowed, 0., -3.5)];
    eprintln!("behind {behind:?}; unshadowed {unshadowed:?}; away {away:?}");
    for i in 0..2 {
        assert!(behind[i] < 5., "{behind:?}");
        assert!(unshadowed[i] > 20., "{unshadowed:?}");
        assert!(
            (away[i] - unshadowed[i]).abs() <= 2.,
            "{away:?} {unshadowed:?}"
        );
    }
}

#[derive(Default, Args)]
struct BoundaryScene {
    shadowed: bool,
}
struct PointShadowBoundary;
impl Game for PointShadowBoundary {
    const ID: &'static str = "point-shadow-boundary";
    type Args = BoundaryScene;
    fn setup(w: &mut World, args: &BoundaryScene) {
        w.insert_resource(Environment {
            ambient: 0.,
            bloom: None,
            fog: None,
            ..Default::default()
        });
        w.spawn((
            Transform::default(),
            DirectionalLight {
                illuminance: 0.,
                ..Default::default()
            },
        ));
        w.spawn((
            Transform::default(),
            Mesh::plane(40., 40.),
            Material::rgb(0.6, 0.6, 0.6),
        ));
        let light = w.spawn((
            Transform::at(0., 2., 0.),
            PointLight {
                intensity: 20_000.,
                range: 8.,
                ..Default::default()
            },
        ));
        if args.shadowed {
            w.insert(light, LightShadows);
        }
        // The plate's shadow covers x = 1..7, across the -Y/+X boundary at x = 2.
        w.spawn((
            Transform::at(2., 1., 0.).with_scale([3., 0.05, 3.]),
            Mesh::cube(1.),
            Material::rgb(0.6, 0.6, 0.6),
        ));
        // Below the plate, looking at 18 cm of floor across the boundary.
        w.spawn((
            Transform::at(2., 0.5, 0.).looking_at(Vec3::new(2., 0., 0.), -Vec3::Z),
            Camera::orthographic(0.036),
        ));
    }
    fn tick(_: &mut World, _: &Input, _: &BoundaryScene) {}
}

#[test]
fn a_point_lights_shadow_stays_dark_across_a_cube_face_boundary() {
    let Some(gpu) = gpu() else { return };
    let frame = Frame {
        width: 320.,
        height: 64.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.,
    };
    for shadowed in [false, true] {
        let mut surface = WorldSurface::<PointShadowBoundary>::default();
        surface
            .bind(&BoundaryScene { shadowed }.values(), None)
            .unwrap();
        let (pixels, _) = fixture::render(&gpu, &mut surface, &frame).unwrap();
        assert!(surface.error().is_none(), "{:?}", surface.error());
        let row: Vec<_> = (0..pixels.width)
            .map(|x| pixels.at(x, pixels.height / 2)[0])
            .collect();
        let min = *row.iter().min().unwrap();
        let max = *row.iter().max().unwrap();
        eprintln!("point shadow boundary: shadowed={shadowed}, red={min}..{max}");
        if shadowed {
            assert!(max < 5, "the shadow must cover the whole row: {row:?}");
        } else {
            assert!(
                min > 30,
                "the whole row must receive the point light: {row:?}"
            );
        }
    }
}

#[test]
fn toggling_a_shadowed_flashlight_reuses_its_shadow_layers() {
    use exact_game_render::{shapes, Batch, FrameInput, LightInput, Renderer};
    let Some(gpu) = gpu() else { return };
    let format = exact_gpu::wgpu::TextureFormat::Rgba8Unorm;
    let mut r = Renderer::new(&gpu.device, &gpu.queue, format);
    let (v, i) = shapes::cube();
    let cube = r.add_mesh(&v, &i);
    r.write_transforms_both(0, &[0., 0., 0., 0., 0., 0., 1., 1., 1., 1.])
        .unwrap();
    r.write_materials(0, &[0.5, 0.5, 0.5, 1., 0., 1., 0., 0., 0., 1., 1., 1.])
        .unwrap();
    r.set_batches(&[Batch::new(cube, 0..1)], &[0]).unwrap();
    let texture = gpu
        .device
        .create_texture(&exact_gpu::wgpu::TextureDescriptor {
            label: None,
            size: exact_gpu::wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: exact_gpu::wgpu::TextureDimension::D2,
            format,
            usage: exact_gpu::wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
    let view = texture.create_view(&Default::default());
    let mut creations = Vec::new();
    for intensity in [3., 0., 3., 0., 3.] {
        let lights = [LightInput {
            position: Vec3::new(0., 3., 0.),
            color: Vec3::ONE,
            intensity,
            range: 10.,
            decay: 2.,
            direction: -Vec3::Y,
            cone: Some([0.9, 0.8]),
            shadows: true,
        }];
        let frame = FrameInput {
            view: glam::camera::rh::view::look_at_mat4(Vec3::new(0., 2., 4.), Vec3::ZERO, Vec3::Y),
            lights: &lights,
            sun: None,
            ..Default::default()
        };
        creations.push(r.draw(&view, (32, 32), &frame).texture_creations);
    }
    assert!(creations.windows(2).all(|w| w[0] == w[1]), "{creations:?}");
}

#[derive(Default, Args)]
struct FloorView {
    model: bool,
    orthographic: bool,
    roughness: f32,
    camera_scale: f32,
}
struct Floor;
impl Game for Floor {
    const ID: &'static str = "floor-view-direction";
    const ASSETS: &'static [&'static str] = &["floor.model"];
    type Args = FloorView;
    fn setup(w: &mut World, args: &FloorView) {
        w.insert_resource(Environment {
            zenith: [0.2; 3],
            horizon: [0.2; 3],
            ground: [0.2; 3],
            bloom: None,
            fog: None,
            tone_mapping: ToneMapping::None,
            ..Default::default()
        });
        let floor = w.spawn((
            Transform::default(),
            if args.model {
                Mesh::asset("floor.model")
            } else {
                Mesh::plane(400., 400.)
            },
        ));
        if !args.model {
            w.insert(
                floor,
                Material {
                    roughness: args.roughness,
                    ..Material::rgb(0.5, 0.5, 0.5)
                },
            );
        }
        w.spawn((
            Transform::at(0., 6., 0.)
                .looking_at(Vec3::ZERO, -Vec3::Z)
                .with_scale([1., 1., args.camera_scale]),
            if args.orthographic {
                Camera::orthographic(10.)
            } else {
                Camera {
                    fov_y_degrees: 80.,
                    ..Default::default()
                }
            },
        ));
        w.spawn((
            Transform::at(1., 1., 0.).looking_at(Vec3::ZERO, Vec3::Y),
            DirectionalLight {
                shadows: false,
                ..Default::default()
            },
        ));
    }
    fn tick(_: &mut World, _: &Input, _: &FloorView) {}
}

fn floor_shading(model: bool) {
    use exact_game::asset::{MaterialData, MeshData, Model, Node};
    let Some(gpu) = gpu() else { return };
    let (vertices, indices) = exact_game_render::shapes::plane();
    let bounds = [-200., 0., -200., 200., 0., 200.];
    let mut floor = Model {
        meshes: vec![MeshData {
            positions: vertices
                .iter()
                .flat_map(|v| v.position.map(|x| x * 400.))
                .collect(),
            normals: vertices.iter().flat_map(|v| v.normal).collect(),
            uvs: vertices.iter().flat_map(|v| v.uv).collect(),
            indices,
            bounds,
            ..Default::default()
        }],
        materials: vec![MaterialData {
            base_color: [0.5, 0.5, 0.5, 1.],
            metallic: 0.,
            ..Default::default()
        }],
        nodes: vec![Node {
            mesh: Some(0),
            ..Default::default()
        }],
        bounds,
        ..Default::default()
    };
    let frame = Frame {
        width: 160.,
        height: 100.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.,
    };
    for (orthographic, roughness) in [
        (true, 0.3),
        (true, 0.5),
        (true, 0.8),
        (true, 1.),
        (false, 0.3),
    ] {
        floor.materials[0].roughness = roughness;
        let scales: &[f32] = if orthographic { &[1., 0.5, 2.] } else { &[1.] };
        let mut unscaled = [0; 3];
        for &camera_scale in scales {
            let mut surface =
                WorldSurface::<Floor, exact_game_render::ModelExecutor, true>::default();
            surface.device_ready(exact_gpu::wgpu::Features::empty());
            surface
                .bind(
                    &FloorView {
                        model,
                        orthographic,
                        roughness,
                        camera_scale,
                    }
                    .values(),
                    None,
                )
                .unwrap();
            assert_eq!(surface.assets().requests, ["floor.model"]);
            surface.asset("floor.model", Ok(&exact_game::asset::encode_model(&floor)));
            assert!(
                !surface.sim().unwrap().is_loading(),
                "{}",
                surface.agent(r#"{"op":"state"}"#).unwrap()
            );
            let (pixels, _) = fixture::render(&gpu, &mut surface, &frame).unwrap();
            assert!(surface.error().is_none(), "{:?}", surface.error());
            // The orthographic row samples near x = -6, 0 and +6 metres, clear of edges.
            let row = [20, 80, 140].map(|x| pixels.at(x, 50)[0]);
            let spread = row.iter().max().unwrap() - row.iter().min().unwrap();
            eprintln!(
                "model={model}, orthographic={orthographic}, roughness={roughness}, \
                 camera_scale={camera_scale}: {row:?}"
            );
            assert!(
                row[1] > 20 && row[1] < 240,
                "the lit floor must be visible: {row:?}"
            );
            if orthographic {
                assert!(spread <= 1, "parallel view rays must shade alike: {row:?}");
                if camera_scale == 1. {
                    unscaled = row;
                } else {
                    assert!(
                        row.iter().zip(unscaled).all(|(a, b)| a.abs_diff(b) <= 1),
                        "camera scale must preserve lighting: {unscaled:?} -> {row:?}"
                    );
                }
            } else {
                assert!(
                    spread > 20,
                    "a perspective eye keeps its highlight: {row:?}"
                );
            }
        }
    }
}

#[test]
fn orthographic_primitive_shading_uses_parallel_view_rays() {
    floor_shading(false);
}

#[test]
fn orthographic_model_shading_uses_parallel_view_rays() {
    floor_shading(true);
}

// One spot over the dark floor: `decay`, its intensity, and either straight down
// from `height` (`graze` 0) or from 0.5 m at x = -9, aimed along the floor to x = 9.
#[derive(Default, Args)]
struct Falloff {
    decay: f32,
    intensity: f32,
    height: f32,
    graze: u32,
}
struct FalloffScene;
impl Game for FalloffScene {
    const ID: &'static str = "render-light-falloff";
    type Args = Falloff;
    fn setup(w: &mut World, args: &Falloff) {
        w.insert_resource(Environment {
            ambient: 0.,
            bloom: None,
            fog: None,
            background: Some([0.; 3].into()),
            ..Default::default()
        });
        w.spawn((
            Transform::default(),
            Mesh::plane(40., 40.),
            Material::rgb(0.6, 0.6, 0.6),
        ));
        w.spawn((
            Transform::at(0., 10., 0.).looking_at(Vec3::ZERO, -Vec3::Z),
            Camera::orthographic(20.),
        ));
        let sun = DirectionalLight {
            illuminance: 0.,
            ..Default::default()
        };
        w.spawn((
            Transform::at(0., 5., 0.).looking_at(Vec3::ZERO, Vec3::Z),
            sun,
        ));
        let (pose, inner, outer) = if args.graze == 0 {
            let pose = Transform::at(0., args.height, 0.).looking_at(Vec3::ZERO, Vec3::Z);
            (pose, 0.5, 0.6)
        } else {
            let pose = Transform::at(-9., 0.5, 0.).looking_at(Vec3::new(9., 0., 0.), Vec3::Y);
            (pose, 0.4, 0.5)
        };
        w.spawn((
            pose,
            SpotLight {
                intensity: args.intensity,
                range: 1000.,
                inner,
                outer,
                decay: args.decay,
                ..Default::default()
            },
        ));
    }
    fn tick(_: &mut World, _: &Input, _: &Falloff) {}
}

fn render_falloff(
    gpu: &Gpu,
    decay: f32,
    intensity: f32,
    height: f32,
    graze: bool,
) -> fixture::Pixels {
    let mut surface = WorldSurface::<FalloffScene>::default();
    let args = [decay, intensity, height].map(|n| Value::Number(f64::from(n)));
    surface
        .bind(
            &[
                args[0].clone(),
                args[1].clone(),
                args[2].clone(),
                Value::Number(f64::from(u32::from(graze))),
            ],
            None,
        )
        .unwrap();
    let frame = Frame {
        width: 640.,
        height: 360.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    };
    let (pixels, _) = fixture::render(gpu, &mut surface, &frame).unwrap();
    assert!(surface.error().is_none(), "{:?}", surface.error());
    pixels.save(&format!("lights-decay-{decay}-{height}-{graze}"));
    pixels
}

#[test]
fn a_spot_without_decay_lights_the_floor_by_angle_alone() {
    let Some(gpu) = gpu() else { return };
    // Grazing a long floor from one end: inverse-square leaves the far end dark;
    // without decay only the falling N·L dims it (5× from x = -6 to x = 6).
    let graze = |decay| {
        let image = render_falloff(
            &gpu,
            decay,
            if decay == 0. { 15_000. } else { 140_000. },
            0.,
            true,
        );
        [at(&image, -6., 0.), at(&image, 0., 0.), at(&image, 6., 0.)]
    };
    let (flat, square) = (graze(0.), graze(2.));
    eprintln!("decay 0 at x -6, 0, 6: {flat:?}; decay 2: {square:?}");
    assert!(flat[2] > 25. && flat[0] < 230., "{flat:?}");
    assert!(square[2] < square[0] / 10., "{square:?}");
    // Straight down from 4 m and 8 m: without decay the same angle off the axis
    // is equally bright at either height; with it, 4x darker from twice as far.
    let below = |decay, intensity, height: f32| {
        let image = render_falloff(&gpu, decay, intensity, height, false);
        [at(&image, 0., 0.), at(&image, height * 0.25, 0.)]
    };
    let near = below(0., 1250., 4.);
    let far = below(0., 1250., 8.);
    eprintln!("decay 0 from 4 m {near:?}, 8 m {far:?}");
    assert!(near[0] > 40. && near[0] < 220., "{near:?}");
    assert!((near[0] - far[0]).abs() <= 1., "{near:?} {far:?}");
    // Off the axis the camera's view direction, so the specular, differs a little.
    assert!((near[1] - far[1]).abs() <= 3., "{near:?} {far:?}");
    let square = [below(2., 20_000., 4.), below(2., 20_000., 8.)];
    assert!(square[1][0] < square[0][0] - 20., "{square:?}");
    // The pow path: decay 1 from 4 m matches decay 2 from 2 m on the axis.
    let (linear, squared) = (below(1., 5_000., 4.)[0], below(2., 5_000., 2.)[0]);
    eprintln!("decay 1 from 4 m {linear}, decay 2 from 2 m {squared}");
    assert!((linear - squared).abs() <= 1., "{linear} {squared}");
    // d^4 at 16 m is 65536, past half precision's largest value.
    let (steep, squared) = (below(4., 81_920_000., 16.)[0], below(2., 320_000., 16.)[0]);
    eprintln!("decay 4 from 16 m {steep}, decay 2 from 16 m {squared}");
    assert!(
        squared > 40. && (steep - squared).abs() <= 1.,
        "{steep} {squared}"
    );
}

// The shadowed spot of SPOT_SHADOW, seen 0.5 m tall around the shadow's edge
// at x = 0.6 m, under `Shadows` with these local settings. `filter`: 0 the
// default, 1 PCF-soft, 2 bilinear; `insert` 0 leaves `Shadows` out.
#[derive(Default, Args)]
struct Local {
    resolution: u32,
    filter: u32,
    softness: f32,
    insert: u32,
}
struct LocalScene;
impl Game for LocalScene {
    const ID: &'static str = "render-local-shadow-settings";
    type Args = Local;
    fn setup(w: &mut World, args: &Local) {
        w.insert_resource(Environment {
            ambient: 0.,
            bloom: None,
            fog: None,
            background: Some([0.; 3].into()),
            ..Default::default()
        });
        if args.insert != 0 {
            let bias = |depth_bias, normal_bias| match args.filter {
                1 => ShadowFilter::PcfSoft {
                    depth_bias,
                    normal_bias,
                },
                2 => ShadowFilter::Bilinear {
                    depth_bias,
                    normal_bias,
                },
                _ => ShadowFilter::ReceiverPlane,
            };
            w.insert_resource(Shadows {
                local_resolution: args.resolution,
                local_filter: bias(0.00002, 0.005),
                local_softness: args.softness,
                ..Default::default()
            });
        }
        w.spawn((
            Transform::default(),
            Mesh::plane(40., 40.),
            Material::rgb(0.6, 0.6, 0.6),
        ));
        w.spawn((
            Transform::at(0.6, 10., 0.).looking_at(Vec3::new(0.6, 0., 0.), -Vec3::Z),
            Camera::orthographic(0.5),
        ));
        w.spawn((
            Transform::at(0., 4., 0.).looking_at(Vec3::ZERO, Vec3::Z),
            SpotLight {
                intensity: 20_000.,
                range: 12.,
                // Full intensity across the view: the cone's edge is at 1.15 m.
                inner: 0.28,
                outer: 0.3,
                ..Default::default()
            },
            LightShadows,
        ));
        w.spawn((
            Transform {
                position: Vec3::new(0., 2., 0.),
                scale: Vec3::new(0.6, 0.1, 0.6),
                ..Default::default()
            },
            Mesh::cube(1.),
            Material::rgb(0.6, 0.6, 0.6),
        ));
    }
    // A lower rung would cap the 2048² layers this compares.
    fn quality(_: &Local) -> Quality {
        Quality::High
    }
    fn tick(_: &mut World, _: &Input, _: &Local) {}
}

// The middle row's red channel across the edge, and its penumbra: the pixels
// between a tenth and nine tenths of the lit floor. The caster's own top fills
// x < 0.3 m, the first 104 pixels.
fn local_edge(
    gpu: &Gpu,
    resolution: u32,
    filter: u32,
    softness: f32,
    insert: bool,
) -> (Vec<u8>, usize) {
    let mut surface = WorldSurface::<LocalScene>::default();
    surface
        .bind(
            &[
                Value::Number(f64::from(resolution)),
                Value::Number(f64::from(filter)),
                Value::Number(f64::from(softness)),
                Value::Number(f64::from(u32::from(insert))),
            ],
            None,
        )
        .unwrap();
    let frame = Frame {
        width: 640.,
        height: 360.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    };
    let (pixels, _) = fixture::render(gpu, &mut surface, &frame).unwrap();
    assert!(surface.error().is_none(), "{:?}", surface.error());
    pixels.save(&format!(
        "lights-local-{resolution}-{filter}-{softness}-{insert}"
    ));
    let row: Vec<u8> = (0..640).map(|x| pixels.at(x, 180)[0]).collect();
    let lit = f32::from(row[450]);
    let penumbra = row[150..]
        .iter()
        .filter(|&&v| f32::from(v) > lit * 0.1 && f32::from(v) < lit * 0.9)
        .count();
    (row, penumbra)
}

#[test]
fn local_shadow_settings_size_and_filter_a_spot_lights_shadow() {
    let Some(gpu) = gpu() else { return };
    let (unsaid, default) = local_edge(&gpu, 0, 0, 0., false);
    let (inserted, _) = local_edge(&gpu, 1024, 0, 1., true);
    assert_eq!(unsaid, inserted, "the defaults are today's map and filter");
    // Shadow inside, lit floor outside, in every filter.
    assert!(
        unsaid[200] < 5 && unsaid[450] > 60,
        "{} {}",
        unsaid[200],
        unsaid[450]
    );
    let (_, sharp) = local_edge(&gpu, 2048, 0, 1., true);
    let (_, soft) = local_edge(&gpu, 1024, 0, 3., true);
    let (pcf_row, pcf_soft) = local_edge(&gpu, 2048, 1, 1., true);
    let (bilinear_row, bilinear) = local_edge(&gpu, 2048, 2, 1., true);
    eprintln!(
        "penumbra px: default {default}, 2048 {sharp}, softness 3 {soft}, \
         PCF-soft {pcf_soft}, bilinear {bilinear}"
    );
    assert!(sharp < default, "2048² is sharper: {sharp} {default}");
    assert!(soft > default * 2, "softness 3 is softer: {soft} {default}");
    assert!(bilinear < pcf_soft, "{bilinear} {pcf_soft}");
    for row in [pcf_row, bilinear_row] {
        assert!(row[200] < 5 && row[450] > 60, "{} {}", row[200], row[450]);
    }
}

// Counts allocations only: that the forward passes then sample the new array
// (`shadow_sample_stale`) is not read back here.
#[test]
fn a_new_local_resolution_replaces_the_layers() {
    use exact_game_render::{shapes, Batch, FrameInput, LightInput, LocalShadows, Renderer};
    let Some(gpu) = gpu() else { return };
    let format = exact_gpu::wgpu::TextureFormat::Rgba8Unorm;
    let mut r = Renderer::new(&gpu.device, &gpu.queue, format);
    let (v, i) = shapes::cube();
    let cube = r.add_mesh(&v, &i);
    r.write_transforms_both(0, &[0., 0., 0., 0., 0., 0., 1., 1., 1., 1.])
        .unwrap();
    r.write_materials(0, &[0.5, 0.5, 0.5, 1., 0., 1., 0., 0., 0., 1., 1., 1.])
        .unwrap();
    r.set_batches(&[Batch::new(cube, 0..1)], &[0]).unwrap();
    let texture = gpu
        .device
        .create_texture(&exact_gpu::wgpu::TextureDescriptor {
            label: None,
            size: exact_gpu::wgpu::Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: exact_gpu::wgpu::TextureDimension::D2,
            format,
            usage: exact_gpu::wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
    let view = texture.create_view(&Default::default());
    // A point light's six layers: at 4096² they do not fit, and it draws unshadowed.
    let lights = [LightInput {
        position: Vec3::new(0., 3., 0.),
        color: Vec3::ONE,
        intensity: 3.,
        range: 10.,
        shadows: true,
        ..Default::default()
    }];
    let mut creations = Vec::new();
    for resolution in [1024, 1024, 2048, 2048, 4096, 2048, 1024] {
        let frame = FrameInput {
            view: glam::camera::rh::view::look_at_mat4(Vec3::new(0., 2., 4.), Vec3::ZERO, Vec3::Y),
            lights: &lights,
            local_shadows: LocalShadows {
                resolution,
                ..Default::default()
            },
            sun: None,
            ..Default::default()
        };
        creations.push(r.draw(&view, (32, 32), &frame).texture_creations);
    }
    let made: Vec<_> = creations.windows(2).map(|w| w[1] - w[0]).collect();
    assert_eq!(made, [0, 1, 0, 0, 0, 1], "{creations:?}");
}

/// A quad of one model material. Without `layers`, the unlit test's material
/// (`MaterialData::unlit`, issue #100): a colour of its base plus its
/// emission, which lights and the view change when it is lit. With them
/// (issue #98), a rough red base: plain (1), under a clear coat (2) or with a
/// sheen (3); or a black base emitting red, plain (4) or under a coat (5).
#[derive(Default, Args)]
struct Quad {
    mode: u32,
    light: u32,
    view: u32,
    unlit: bool,
    layers: u32,
}
struct UnlitQuad;
impl Game for UnlitQuad {
    const ID: &'static str = "render-unlit-quad";
    type Args = Quad;
    fn setup(w: &mut World, args: &Quad) {
        use exact_game::asset::{AlphaMode, MaterialData, MeshData, Model, Node};
        w.insert_resource(Environment {
            background: Some([0.; 3].into()),
            bloom: None,
            fog: None,
            tone_mapping: ToneMapping::None,
            ..Default::default()
        });
        let mode = [AlphaMode::Opaque, AlphaMode::Mask, AlphaMode::Blend][args.mode as usize];
        let mesh = MeshData {
            positions: vec![-1., -1., 0., 1., -1., 0., 1., 1., 0., -1., 1., 0.],
            normals: [0., 0., 1.].repeat(4),
            uvs: vec![0.; 8],
            indices: vec![0, 1, 2, 0, 2, 3],
            bounds: [-1., -1., 0., 1., 1., 0.],
            ..Default::default()
        };
        let quad = w
            .generated_model(
                "quad.model",
                Model {
                    bounds: mesh.bounds,
                    meshes: vec![mesh],
                    materials: vec![if args.layers == 0 {
                        MaterialData {
                            base_color: [0.2, 0.2, 0.2, 0.8],
                            metallic: 0.,
                            roughness: 0.4,
                            emissive: [0.5, 0.4, 0.3],
                            alpha_mode: mode,
                            double_sided: true,
                            unlit: args.unlit,
                            ..Default::default()
                        }
                    } else {
                        MaterialData {
                            base_color: if args.layers < 4 {
                                [0.5, 0.02, 0.02, 1.]
                            } else {
                                [0., 0., 0., 1.]
                            },
                            emissive: if args.layers < 4 {
                                [0.; 3]
                            } else {
                                [0.8, 0., 0.]
                            },
                            metallic: 0.,
                            roughness: 1.,
                            alpha_mode: mode,
                            clearcoat: if matches!(args.layers, 2 | 5) { 1. } else { 0. },
                            clearcoat_roughness: 0.05,
                            sheen_color: if args.layers == 3 { [1.; 3] } else { [0.; 3] },
                            sheen_roughness: 0.5,
                            ..Default::default()
                        }
                    }],
                    nodes: vec![Node {
                        mesh: Some(0),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
        w.spawn((Transform::default(), quad));
        let eye = [
            Vec3::new(0., 0., 4.),
            Vec3::new(2.5, 0.8, 3.),
            Vec3::new(3.9, 0., 0.9),
        ][args.view as usize];
        w.spawn((
            Transform::at(eye.x, eye.y, eye.z).looking_at(Vec3::ZERO, Vec3::Y),
            Camera::default(),
        ));
        let toward =
            |from: Vec3| Transform::at(from.x, from.y, from.z).looking_at(Vec3::ZERO, Vec3::Y);
        let sun = DirectionalLight {
            illuminance: if args.light == 0 { 0. } else { 20_000. },
            ..Default::default()
        };
        // A sun grazing the quad from the side (or behind the eye, 3), then a
        // fill and a point light.
        let from = if args.light == 3 {
            eye
        } else {
            Vec3::new(5., 0.5, 0.3)
        };
        w.spawn((toward(from), sun));
        if args.light == 2 {
            w.spawn((
                toward(Vec3::new(-1., 2., 3.)),
                DirectionalLight {
                    color: [0.6, 0.7, 1.],
                    illuminance: 8_000.,
                    shadows: false,
                },
            ));
            w.spawn((
                Transform::at(0.5, 0.5, 1.),
                PointLight {
                    intensity: 3_000.,
                    range: 5.,
                    ..Default::default()
                },
            ));
        }
    }
    fn tick(_: &mut World, _: &Input, _: &Quad) {}
}

#[test]
fn an_unlit_quads_pixels_change_with_no_light_or_view() {
    let Some(gpu) = gpu() else { return };
    let centre = |mode: u32, light: u32, view: u32, unlit: bool| -> [u8; 4] {
        quad_centre(&gpu, [mode, light, view], unlit, 0)
    };
    let srgb = |linear: f32| {
        let v = if linear <= 0.0031308 {
            linear * 12.92
        } else {
            1.055 * linear.powf(1. / 2.4) - 0.055
        };
        (v * 255.).round() as i32
    };
    for mode in 0..3 {
        // Blended, the quad's alpha mixes it with the black background.
        let alpha = if mode == 2 { 0.8 } else { 1. };
        let expected = [0.7, 0.6, 0.5].map(|c| srgb(c * alpha));
        let unlit = centre(mode, 0, 0, true);
        for c in 0..3 {
            assert!(
                (i32::from(unlit[c]) - expected[c]).abs() <= 1,
                "mode {mode}: {unlit:?}, expected {expected:?}"
            );
        }
        for (light, view) in [(1, 0), (2, 1)] {
            assert_eq!(
                centre(mode, light, view, true),
                unlit,
                "mode {mode} light {light} view {view}"
            );
        }
        // The closest lit material is changed by the lights.
        assert_ne!(
            centre(mode, 0, 0, false),
            centre(mode, 2, 1, false),
            "mode {mode}"
        );
    }
}

/// The centre pixel of a `UnlitQuad` frame, `[mode, light, view]` as `Quad`'s.
fn quad_centre(gpu: &Gpu, [mode, light, view]: [u32; 3], unlit: bool, layers: u32) -> [u8; 4] {
    use exact_game_render::ModelExecutor;
    let mut surface = WorldSurface::<UnlitQuad, ModelExecutor, true>::default();
    let [mode, light, view, layers] =
        [mode, light, view, layers].map(|v| Value::Number(f64::from(v)));
    surface
        .bind(&[mode, light, view, Value::Bool(unlit), layers], None)
        .unwrap();
    surface.device_ready(exact_gpu::wgpu::Features::empty());
    surface.prepare_assets(
        &gpu.device,
        &gpu.queue,
        exact_gpu::wgpu::TextureFormat::Rgba8Unorm,
    );
    let frame = Frame {
        width: 64.,
        height: 64.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    };
    let (pixels, _) = fixture::render(gpu, &mut surface, &frame).unwrap();
    assert!(surface.error().is_none(), "{:?}", surface.error());
    pixels.at(32, 32)
}

/// A clear coat adds a sharp highlight over a rough base and darkens emission
/// under it, and a sheen adds light at grazing angles (`MaterialData::clearcoat`, `sheen_color`; issue #98).
#[test]
fn a_clear_coat_and_a_sheen_add_their_lobes_to_a_rough_base() {
    let Some(gpu) = gpu() else { return };
    let green = |view: u32, layers: u32| quad_centre(&gpu, [0, 3, view], false, layers)[1];
    let (plain, coated) = (green(0, 1), green(0, 2));
    let (grazing, sheen) = (green(2, 1), green(2, 3));
    assert!(
        coated > plain.saturating_add(60),
        "coat: {plain} -> {coated}"
    );
    assert!(
        sheen > grazing.saturating_add(20),
        "sheen: {grazing} -> {sheen}"
    );
    // The coat's Fresnel darkens emission under it, at a grazing view most.
    let red = |layers: u32| quad_centre(&gpu, [0, 0, 2], false, layers)[0];
    let (bare, coated) = (red(4), red(5));
    assert!(coated + 10 < bare, "emission: {bare} -> {coated}");
}
