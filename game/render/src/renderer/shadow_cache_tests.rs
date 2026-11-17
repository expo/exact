use super::*;
use crate::{Environment, FrameInput, Renderer, ShadowBox, Shadows, Sun};
use exact_game::{asset::*, *};
use exact_gpu::fixture;

#[test]
fn fixed_sun_box_is_independent_of_camera_and_projection() {
    let mut f = FrameInput {
        sun: Some(Sun {
            direction: Vec3::new(1., -2., 0.5),
            color: Vec3::ONE,
            illuminance: 1.,
            shadows: None,
        }),
        ..Default::default()
    };
    let settings = Shadows {
        fixed_box: Some(ShadowBox::default()),
        ..Default::default()
    };
    let a = Cascades::new(&f, settings);
    f.view = glam::camera::rh::view::look_at_mat4(Vec3::new(4., 5., 6.), Vec3::ZERO, Vec3::Y);
    f.proj = glam::camera::rh::proj::directx::perspective(1., 1.7, 0.2, 300.);
    let b = Cascades::new(&f, settings);
    assert_eq!(a.count, 1);
    assert_eq!(a.matrices, b.matrices);
    assert_eq!(a.texels, b.texels);
    f.sun.as_mut().unwrap().direction = Vec3::new(-1., -2., 0.5);
    assert_ne!(a.matrices, Cascades::new(&f, settings).matrices);
}

/// Every layer of the sun's shadow map, as stored.
fn map_depth(gpu: &exact_gpu::Gpu, r: &Renderer) -> Vec<f32> {
    let texture = r.shadows.as_ref().unwrap().view.texture();
    let (side, layers) = (texture.width(), texture.depth_or_array_layers());
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(side * side * layers * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side * 4),
                rows_per_image: Some(side),
            },
        },
        texture.size(),
    );
    gpu.queue.submit([encoder.finish()]);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap();
    data.chunks(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}

struct Scene {
    w: World,
    r: Renderer,
    feed: crate::Feed,
    target: wgpu::Texture,
    frame: FrameInput<'static>,
}

fn scene(gpu: &exact_gpu::Gpu) -> Scene {
    let mut w = World::new(60, 1);
    let (vertices, indices) = crate::shapes::cube();
    let model = Model {
        meshes: vec![MeshData {
            positions: vertices.iter().flat_map(|v| v.position).collect(),
            normals: vertices.iter().flat_map(|v| v.normal).collect(),
            uvs: vertices.iter().flat_map(|v| v.uv).collect(),
            indices,
            bounds: [-0.5, -0.5, -0.5, 0.5, 0.5, 0.5],
            ..Default::default()
        }],
        nodes: vec![Node {
            mesh: Some(0),
            ..Default::default()
        }],
        materials: vec![MaterialData::default()],
        bounds: [-0.5, -0.5, -0.5, 0.5, 0.5, 0.5],
        ..Default::default()
    };
    let mut r = Renderer::configured(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Rgba8Unorm,
        SceneSamples::One,
        true,
    );
    for (name, transform) in [
        (
            "floor.model",
            Transform {
                scale: Vec3::new(12., 0.2, 12.),
                ..Transform::at(0., -0.1, 0.)
            },
        ),
        ("box.model", Transform::at(0., 1., 0.)),
    ] {
        let mesh = w.generated_model(name, model.clone()).unwrap();
        w.spawn_named(name, (transform, mesh));
        r.prepare_model(name, &model).unwrap();
    }
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let frame = FrameInput {
        view: glam::camera::rh::view::look_at_mat4(Vec3::new(4., 6., 8.), Vec3::ZERO, Vec3::Y),
        proj: glam::camera::rh::proj::directx::orthographic(-5., 5., -5., 5., 0.1, 40.),
        camera_position: Vec3::new(4., 6., 8.),
        sun: Some(Sun {
            direction: Vec3::new(1., -2., 0.3),
            color: Vec3::ONE,
            illuminance: 2.,
            shadows: Some(Shadows {
                resolution: 256,
                cache_static: true,
                fixed_box: Some(ShadowBox {
                    half_width: 10.,
                    ..Default::default()
                }),
                ..Default::default()
            }),
        }),
        environment: Environment {
            fog: None,
            bloom: None,
            sun_disc: 0.,
            ..Default::default()
        },
        ..Default::default()
    };
    Scene {
        w,
        r,
        feed: Default::default(),
        target,
        frame,
    }
}

fn look(
    gpu: &exact_gpu::Gpu,
    r: &mut Renderer,
    target: &wgpu::Texture,
    frame: &FrameInput<'_>,
) -> (Vec<u8>, crate::tells::Drawn) {
    let state = |r: &Renderer| {
        r.shadows.as_ref().and_then(|s| s.cached.as_ref()).map(|c| {
            (
                c.signature.clone(),
                c.scratch.clone(),
                c.groups.clone(),
                c.valid,
                c.dirty,
                c.encoded.get(),
            )
        })
    };
    let saved = state(r);
    let view = target.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    r.arm_probe(true);
    r.encode_hooked(
        &mut encoder,
        &view,
        (128, 128),
        frame,
        &mut (),
        None,
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    let records = r.probe.as_ref().unwrap().records.take();
    let facts = r.probe_facts();
    let readback = r.probe_copies(&mut encoder, target, &facts);
    gpu.queue.submit([encoder.finish()]);
    r.submitted();
    r.end_look();
    readback.map();
    assert!(readback.ready(&gpu.device).unwrap());
    let read = readback.read().unwrap();
    assert_eq!(state(r), saved, "a look must leave cache state unchanged");
    let drawn = crate::look::drawn(&records, &facts, &read);
    (read.rgba, drawn)
}

fn observed_live(
    gpu: &exact_gpu::Gpu,
    r: &mut Renderer,
    target: &wgpu::Texture,
    frame: &FrameInput<'_>,
) -> (Vec<u8>, u64) {
    let view = target.create_view(&Default::default());
    r.draw(&view, (128, 128), frame);
    let live = fixture::read(gpu, target).unwrap();
    let texels = map_depth(gpu, r).iter().filter(|&&d| d < 1.).count() as u64;
    let (pixels, drawn) = look(gpu, r, target, frame);
    assert_eq!(
        drawn.sun_texels,
        crate::tells::Measure::Taken(texels),
        "the oracle must count the maps the live frame used"
    );
    assert!(pixels == live.data, "the look must show the live shadows");
    let judged = crate::tells::judge(&crate::tells::FrameStats::of(&pixels), &drawn);
    assert_eq!(
        judged.tells.iter().any(|t| t.name == "no-shadow-casters"),
        texels == 0
    );
    (pixels, texels)
}

#[test]
fn look_observes_missing_and_stale_cached_casters() {
    let Some(gpu) = crate::test_device::device_or_skip(fixture::device()) else {
        return;
    };
    for seed in [true, false] {
        let Scene {
            mut w,
            mut r,
            mut feed,
            target,
            frame,
        } = scene(&gpu);
        w.propagate();
        feed.feed(&w, &mut r).unwrap();
        // The floor receives the box's shadow without masking its texel count.
        let floor = r.models.loaded["floor.model"].meshes.clone();
        for batch in &mut r.batches {
            if floor.contains(&batch.mesh) {
                batch.casts_shadows = false;
            }
        }
        let view = target.create_view(&Default::default());
        r.draw(&view, (128, 128), &frame);
        r.shadows.as_mut().unwrap().seed = seed;
        r.shadows.as_mut().unwrap().cached = None;
        let (healthy, n) = observed_live(&gpu, &mut r, &target, &frame);
        assert!(n > 0);

        // Omit the cached caster while keeping its signature and validity.
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        for layer in &r.shadows.as_ref().unwrap().cached.as_ref().unwrap().layers {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: layer,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
        }
        gpu.queue.submit([encoder.finish()]);
        let (missing, n) = observed_live(&gpu, &mut r, &target, &frame);
        assert_eq!(n, 0);
        assert!(healthy != missing);

        r.shadows.as_mut().unwrap().cached.as_mut().unwrap().valid = false;
        let (repaired, _) = observed_live(&gpu, &mut r, &target, &frame);
        assert!(healthy == repaired);

        let mut stale = r.shadows.as_mut().unwrap().cached.take().unwrap();
        let cube = w.named("box.model").unwrap();
        w.get_mut::<Transform>(cube).unwrap().position.x = 50.;
        w.propagate();
        feed.feed(&w, &mut r).unwrap();
        r.draw(&view, (128, 128), &frame);
        let cache = r.shadows.as_mut().unwrap().cached.as_mut().unwrap();
        // Keep the old depth under the new signature, as a failed refresh would.
        std::mem::swap(&mut cache.texture, &mut stale.texture);
        std::mem::swap(&mut cache.layers, &mut stale.layers);
        std::mem::swap(&mut cache.seed, &mut stale.seed);
        let (ghost, n) = observed_live(&gpu, &mut r, &target, &frame);
        assert!(n > 0, "the oracle must expose the retained stale caster");
        r.shadows.as_mut().unwrap().cached.as_mut().unwrap().valid = false;
        let (repaired, n) = observed_live(&gpu, &mut r, &target, &frame);
        assert_eq!(n, 0);
        assert!(ghost != repaired, "the stale shadow must be visible");
    }
}

#[test]
fn look_preserves_unbuilt_invalid_and_incompatible_caches() {
    let Some(gpu) = crate::test_device::device_or_skip(fixture::device()) else {
        return;
    };
    let Scene {
        mut w,
        mut r,
        mut feed,
        target,
        frame: mut f,
    } = scene(&gpu);
    w.propagate();
    feed.feed(&w, &mut r).unwrap();
    // A look before the first live frame must neither build nor validate a cache.
    let (uncached, _) = look(&gpu, &mut r, &target, &f);
    let (cached, _) = observed_live(&gpu, &mut r, &target, &f);
    assert!(uncached == cached);
    assert!(r.shadows.as_ref().unwrap().cached.as_ref().unwrap().dirty);
    let (_, drawn) = look(&gpu, &mut r, &target, &f);
    assert_eq!(drawn.sun_casters, None, "reused depth has no caster count");

    // A camera-dependent sun map cannot reuse depth from another look camera.
    f.sun.as_mut().unwrap().shadows.as_mut().unwrap().fixed_box = None;
    let view = target.create_view(&Default::default());
    r.draw(&view, (128, 128), &f);
    r.draw(&view, (128, 128), &f);
    let live_view = f.view;
    let live_position = f.camera_position;
    f.camera_position.x += 5.;
    f.view = glam::camera::rh::view::look_at_mat4(f.camera_position, Vec3::ZERO, Vec3::Y);
    let (other_view, _) = look(&gpu, &mut r, &target, &f);
    let cache = r.shadows.as_mut().unwrap().cached.take();
    let (redrawn, _) = look(&gpu, &mut r, &target, &f);
    assert!(other_view == redrawn);
    r.shadows.as_mut().unwrap().cached = cache;
    f.view = live_view;
    f.camera_position = live_position;
    observed_live(&gpu, &mut r, &target, &f);

    // Discarded live work must not become valid when a look is submitted.
    let cube = w.named("box.model").unwrap();
    w.get_mut::<Transform>(cube).unwrap().position.x = 2.;
    w.propagate();
    feed.feed(&w, &mut r).unwrap();
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    r.encode_hooked(
        &mut encoder,
        &view,
        (128, 128),
        &f,
        &mut (),
        None,
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    drop(encoder);
    assert!(!r.shadows.as_ref().unwrap().cached.as_ref().unwrap().valid);
    look(&gpu, &mut r, &target, &f);
    r.draw(&view, (128, 128), &f);
    assert!(r.shadows.as_ref().unwrap().cached.as_ref().unwrap().dirty);

    let cache = r.shadows.as_mut().unwrap().cached.take();
    r.shadows.as_mut().unwrap().cache_enabled = false;
    f.sun
        .as_mut()
        .unwrap()
        .shadows
        .as_mut()
        .unwrap()
        .cache_static = false;
    let (disabled, _) = observed_live(&gpu, &mut r, &target, &f);
    assert!(r.shadows.as_ref().unwrap().cached.is_none());
    assert!(cache.is_some());
    assert!(!disabled.is_empty());
}

#[test]
fn cached_casters_match_redrawing_after_scene_changes() {
    let Some(gpu) = crate::test_device::device_or_skip(fixture::device()) else {
        return;
    };
    let Scene {
        mut w,
        mut r,
        mut feed,
        target,
        frame: mut f,
    } = scene(&gpu);
    let cube = w.named("box.model").unwrap();
    let view = target.create_view(&Default::default());
    for step in 0..8 {
        match step {
            1 => {
                f.camera_position.x += 1.;
                f.view =
                    glam::camera::rh::view::look_at_mat4(f.camera_position, Vec3::ZERO, Vec3::Y);
            }
            2 => {
                w.get_mut::<Transform>(cube).unwrap().position.x = 2.;
            }
            3 => {
                w.insert(cube, Opacity(0.5));
            }
            4 => {
                w.remove::<Opacity>(cube);
                w.insert(cube, Visible(false));
            }
            5 => {
                w.remove::<Visible>(cube);
                f.sun.as_mut().unwrap().direction.x = -1.;
            }
            6 => {
                f.sun.as_mut().unwrap().shadows.as_mut().unwrap().resolution = 512;
            }
            7 => {
                f.sun.as_mut().unwrap().shadows.as_mut().unwrap().fixed_box = None;
            }
            _ => {}
        }
        w.propagate();
        feed.feed(&w, &mut r).unwrap();
        f.sun
            .as_mut()
            .unwrap()
            .shadows
            .as_mut()
            .unwrap()
            .cache_static = true;
        let cold = r.draw(&view, (128, 128), &f);
        let warm = r.draw(&view, (128, 128), &f);
        assert!(
            r.shadows.as_ref().unwrap().cached.is_some(),
            "cache active at {step}"
        );
        assert!(
            warm.draws < cold.draws,
            "cache reuses at {step}: {} vs {}",
            cold.draws,
            warm.draws
        );
        if step == 0 {
            f.camera_position.x += 0.1;
            f.view = glam::camera::rh::view::look_at_mat4(f.camera_position, Vec3::ZERO, Vec3::Y);
            let moved_camera = r.draw(&view, (128, 128), &f);
            assert_eq!(
                warm.draws, moved_camera.draws,
                "camera motion keeps the fixed map"
            );
            assert!(!r.shadows.as_ref().unwrap().cached.as_ref().unwrap().dirty);
        }
        observed_live(&gpu, &mut r, &target, &f);
        let cached = fixture::read(&gpu, &target).unwrap();
        // The cache copied into the map where a device cannot seed it: the
        // same depth, to the bit, and the same image.
        let seeded = map_depth(&gpu, &r);
        let copy = |r: &mut Renderer, seed| {
            let shadows = r.shadows.as_mut().unwrap();
            shadows.seed = seed;
            shadows.cached = None;
        };
        copy(&mut r, false);
        r.draw(&view, (128, 128), &f);
        r.draw(&view, (128, 128), &f);
        assert!(r.shadows.as_ref().unwrap().cached.is_some());
        let copied = map_depth(&gpu, &r);
        assert!(
            seeded
                .iter()
                .zip(&copied)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
            "step {step}: seeded and copied depth differ"
        );
        assert_eq!(cached.data, fixture::read(&gpu, &target).unwrap().data);
        copy(&mut r, true);
        r.draw(&view, (128, 128), &f);
        r.draw(&view, (128, 128), &f);
        f.sun
            .as_mut()
            .unwrap()
            .shadows
            .as_mut()
            .unwrap()
            .cache_static = false;
        r.draw(&view, (128, 128), &f);
        let normal = fixture::read(&gpu, &target).unwrap();
        for y in 0..128 {
            for x in 0..128 {
                for (a, b) in cached.at(x, y).into_iter().zip(normal.at(x, y)) {
                    assert!(a.abs_diff(b) <= 1, "step {step}, pixel {x},{y}: {a} vs {b}");
                }
            }
        }
        if step == 0 {
            let sun = f.sun.take();
            r.draw(&view, (128, 128), &f);
            let no_sun = fixture::read(&gpu, &target).unwrap();
            f.sun = sun;
            assert!(
                (0..128)
                    .flat_map(|y| (0..128).map(move |x| (x, y)))
                    .filter(|&(x, y)| normal.at(x, y)[0].abs_diff(no_sun.at(x, y)[0]) > 20)
                    .count()
                    > 100
            );
        }
    }
    // An encoded but discarded frame cannot make a replacement cache valid.
    f.sun
        .as_mut()
        .unwrap()
        .shadows
        .as_mut()
        .unwrap()
        .cache_static = true;
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    r.encode_hooked(
        &mut encoder,
        &view,
        (128, 128),
        &f,
        &mut (),
        None,
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert!(!r.shadows.as_ref().unwrap().cached.as_ref().unwrap().valid);
    drop(encoder);
    r.draw(&view, (128, 128), &f);
    assert!(r.shadows.as_ref().unwrap().cached.as_ref().unwrap().dirty);
    // Public transform writes bypass Feed's CPU histories; never reuse them.
    f.sun
        .as_mut()
        .unwrap()
        .shadows
        .as_mut()
        .unwrap()
        .cache_static = true;
    r.draw(&view, (128, 128), &f);
    r.write_transforms_both(cube.index(), &[0., 2., 0., 0., 0., 0., 1., 1., 1., 1.])
        .unwrap();
    r.draw(&view, (128, 128), &f);
    assert!(r.shadows.as_ref().unwrap().cached.is_none());
}

/// A shadow distance at or before the camera's near plane shadows nothing the
/// camera sees: the frame draws with no cascades instead of asserting (an
/// abort, under the games' `panic = "abort"`).
#[test]
fn a_shadow_distance_inside_the_near_plane_draws_no_shadow() {
    let Some(gpu) = crate::test_device::device_or_skip(fixture::device()) else {
        return;
    };
    let mut r = Renderer::configured(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Rgba8Unorm,
        SceneSamples::One,
        true,
    );
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    for (near, distance) in [(0.1, 0.05), (80., 60.)] {
        let f = FrameInput {
            view: glam::camera::rh::view::look_at_mat4(
                Vec3::new(0., 100., 100.),
                Vec3::ZERO,
                Vec3::Y,
            ),
            proj: glam::camera::rh::proj::directx::perspective(1., 1., near, 400.),
            sun: Some(Sun {
                direction: Vec3::new(1., -2., 0.5),
                color: Vec3::ONE,
                illuminance: 1.,
                shadows: Some(Shadows {
                    distance,
                    ..Default::default()
                }),
            }),
            ..Default::default()
        };
        assert!(Cascades::reach(&f, f.sun.unwrap().shadows.unwrap()).is_none());
        r.draw(&view, (32, 32), &f);
        assert!(r.shadows.is_none(), "near {near}, distance {distance}");
    }
}
