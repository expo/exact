//! The readback helper (`exact_gpu::fixture`): a surface's frame comes back
//! as tightly packed RGBA whatever the copy's row padding or the texture's
//! channel order, refusals name what was refused, and the PPM is a PPM.
//! The GPU-backed tests skip, saying so, without an adapter.

use exact_gpu::fixture::{self, Pixels};
use exact_gpu::wgpu;
use exact_gpu::{Frame, Gpu, Surface, SurfaceError, Value};

/// Removes its directory when dropped, so a failing test leaves nothing behind.
struct TempDir(std::path::PathBuf);
impl std::ops::Deref for TempDir {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}
impl AsRef<std::path::Path> for TempDir {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Clears to one color.
struct Fill([f64; 4]);

impl Surface for Fill {
    fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
        Ok(())
    }

    fn render(
        &mut self,
        _: &Frame,
        _: &wgpu::Device,
        _: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        _: wgpu::TextureFormat,
    ) -> bool {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: self.0[0],
                        g: self.0[1],
                        b: self.0[2],
                        a: self.0[3],
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        false
    }
}

fn device() -> Option<Gpu> {
    fixture::device_or_skip(fixture::device())
}

#[test]
fn a_package_can_allocate_a_storage_buffer_over_256_mib() {
    let requirements = [exact_gpu::grants::Requirements {
        package: "compute-grid",
        surfaces: &["grid"],
        required_limits: &[("maxStorageBufferBindingSize", 536870912)],
        ..Default::default()
    }];
    let Some(gpu) = fixture::device_or_skip(fixture::device_with_requirements(&requirements))
    else {
        return;
    };
    let size = 256 * 1024 * 1024 + 256;
    if requirements[0]
        .check(&gpu.adapter.limits(), gpu.adapter.features())
        .is_err()
    {
        let grant = exact_gpu::grants::negotiate(
            gpu.adapter.limits(),
            gpu.adapter.features(),
            &requirements,
        );
        assert_eq!(grant.refused.len(), 1);
        return;
    }
    assert!(gpu.device.limits().max_storage_buffer_binding_size >= size);
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("package compute buffer"),
        size,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let error = exact_gpu::block_on(scope.pop());
    assert!(error.is_none(), "{error:?}");
    assert_eq!(buffer.size(), size);
}

#[test]
fn an_unavailable_package_does_not_disable_other_surfaces_on_the_device() {
    use exact_gpu::grants::Requirements;
    static REQUIREMENTS: &[Requirements] = &[Requirements {
        package: "too-large",
        surfaces: &["large"],
        required_limits: &[("maxBufferSize", 1 << 50)],
        optional_limits: &[],
        required_features: &[],
        optional_features: &[],
    }];
    static REGISTRY: exact_gpu::Registry = exact_gpu::Registry {
        surfaces: &[
            ("large", 0, || Box::new(Fill([1.0, 0.0, 0.0, 1.0]))),
            ("small", 0, || Box::new(Fill([0.0, 1.0, 0.0, 1.0]))),
        ],
        shaders: &[],
    };
    let Some(gpu) = fixture::device_or_skip(fixture::device_with_requirements(REQUIREMENTS)) else {
        return;
    };
    let available = gpu.adapter.limits().max_buffer_size;
    let mut module = exact_gpu::Module::new(&REGISTRY).with_requirements(REQUIREMENTS);
    let blocked = module.create_headless("large").unwrap();
    assert!(module.bind(blocked, &[], None));
    module.set_gpu(gpu);
    assert!(module.create_headless("large").is_none());
    let error = module.take_error();
    assert!(error.contains(&available.to_string()), "{error}");
    for word in ["too-large", "maxBufferSize", "1125899906842624"] {
        assert!(error.contains(word), "{error}");
    }
    let id = module.create_headless("small").unwrap();
    assert!(module.bind(id, &[], None));
    let frame = Frame {
        width: 2.0,
        height: 2.0,
        scale: 1.0,
        now_ms: 0.0,
        seekable: false,
        period_ms: 0.0,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    };
    assert!(module.readback(blocked, &frame).is_none());
    assert!(module.take_error().contains("maxBufferSize"));
    let (pixels, _) = module.readback(id, &frame).unwrap();
    assert_eq!(pixels.at(0, 0), [0, 255, 0, 255]);
}

#[test]
fn offscreen_module_admits_device_before_its_first_readback() {
    struct Ready(bool);
    impl Surface for Ready {
        fn device_ready(&mut self, _: wgpu::Features, limits: wgpu::Limits) {
            assert!(limits.max_buffer_size >= 256 * 1024 * 1024);
            self.0 = true;
        }
        fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn render(
            &mut self,
            frame: &Frame,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            encoder: &mut wgpu::CommandEncoder,
            target: &wgpu::TextureView,
            format: wgpu::TextureFormat,
        ) -> bool {
            assert!(
                self.0,
                "offscreen canvases need the same device admission as presented canvases"
            );
            Fill([0.2, 0.4, 0.6, 1.0]).render(frame, device, queue, encoder, target, format)
        }
    }
    static REGISTRY: exact_gpu::Registry = exact_gpu::Registry {
        surfaces: &[("ready", 0, || Box::new(Ready(false)))],
        shaders: &[],
    };
    let Some(gpu) = device() else { return };
    let mut module = exact_gpu::Module::new(&REGISTRY);
    module.set_gpu(gpu);
    let id = module.create_headless("ready").unwrap();
    assert!(module.bind(id, &[], None));
    let frame = Frame {
        width: 3.,
        height: 2.,
        scale: 1.,
        now_ms: 0.,
        seekable: false,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    };
    let (pixels, wants) = module.readback(id, &frame).unwrap();
    assert!(!wants);
    assert_eq!(pixels.at(2, 1), [51, 102, 153, 255]);
}

fn texture(gpu: &Gpu, format: wgpu::TextureFormat, layers: u32) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 3,
            height: 2,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

#[test]
fn rgba_readback_preserves_row_layout_channel_order_and_ppm() {
    let Some(gpu) = device() else { return };
    let mut fill = Fill([0.2, 0.4, 0.6, 1.0]);
    // Three wide: a 12-byte row the copy pads to 256.
    let frame = Frame {
        width: 3.0,
        height: 2.0,
        scale: 1.0,
        now_ms: 0.0,
        children_generation: 0,
        seekable: false,
        period_ms: 0.0,
        shader_generation: exact_gpu::shaders::shader_generation(),
        headroom: 1.0,
    };
    let (px, wants) = fixture::render(&gpu, &mut fill, &frame).unwrap();
    assert!(!wants);
    assert_eq!((px.width, px.height, px.data.len()), (3, 2, 24));
    assert_eq!(px.at(2, 1), [51, 102, 153, 255]);
    assert_eq!(px.count(|p| p == [51, 102, 153, 255]), 6);
    let ppm = px.ppm();
    assert_eq!(&ppm[..11], b"P6\n3 2\n255\n");
    assert_eq!(ppm.len(), 11 + 18);

    let bgra = texture(&gpu, wgpu::TextureFormat::Bgra8Unorm, 1);
    let view = bgra.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    fill.render(
        &frame,
        &gpu.device,
        &gpu.queue,
        &mut encoder,
        &view,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    gpu.queue.submit([encoder.finish()]);
    let px = fixture::read(&gpu, &bgra).unwrap();
    assert_eq!(px.at(0, 0), [51, 102, 153, 255], "swizzled to RGBA");
}

#[test]
fn other_formats_and_array_textures_are_refused_by_name() {
    let Some(gpu) = device() else { return };
    let e = fixture::read(&gpu, &texture(&gpu, wgpu::TextureFormat::R32Float, 1)).unwrap_err();
    assert!(e.contains("R32Float") && e.contains("not an RGBA8"), "{e}");
    let e = fixture::read(&gpu, &texture(&gpu, wgpu::TextureFormat::Rgba8Unorm, 2)).unwrap_err();
    assert!(e.contains("2 layers"), "{e}");
}

#[test]
#[should_panic(expected = "(3, 0) is off a 3x2 picture")]
fn a_pixel_off_the_picture_panics() {
    let px = Pixels {
        width: 3,
        height: 2,
        data: vec![0; 24],
    };
    px.at(3, 0);
}

#[test]
fn save_creates_the_directory_it_is_pointed_at() {
    let dir =
        TempDir(std::env::temp_dir().join(format!("exact-gpu-fixture-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&dir);
    let px = Pixels {
        width: 1,
        height: 1,
        data: vec![1, 2, 3, 4],
    };
    // Process-wide; the only reader is `save`, and this is the file's one setter.
    std::env::set_var("EXACT_GPU_OUT", &*dir);
    let path = px.save("one");
    std::env::remove_var("EXACT_GPU_OUT");
    assert_eq!(path, Some(dir.join("one.ppm")));
    assert_eq!(
        std::fs::read(dir.join("one.ppm")).unwrap(),
        b"P6\n1 1\n255\n\x01\x02\x03"
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(px.save("none"), None, "unset: nothing written");
}

#[test]
fn only_no_adapter_is_a_skip() {
    // The policy is given, not read from EXACT_REQUIRED, so this holds in the lane too.
    assert_eq!(fixture::device_or(Ok(7), false), Some(7));
    assert_eq!(
        fixture::device_or::<()>(Err("no adapter: unavailable".into()), false),
        None
    );
    for reason in [
        "no device: unsupported limits",
        "device lost",
        "shader validation failed",
    ] {
        assert!(
            std::panic::catch_unwind(|| fixture::device_or::<()>(Err(reason.into()), false))
                .is_err()
        );
    }
}

#[test]
fn a_required_gpu_fails_when_no_adapter_is_found() {
    assert_eq!(fixture::device_or(Ok(7), true), Some(7));
    assert!(std::panic::catch_unwind(|| fixture::device_or::<()>(
        Err("no adapter: unavailable".into()),
        true
    ))
    .is_err());
}

/// LLP 1139.003 D2: a fixture's device (a test's, or a headless world's look
/// frame's) never chooses the process's GPU tier; only a module's own load
/// does, so worlds bound before and after it resolve the same rung.
#[test]
fn a_fixture_device_does_not_choose_the_process_tier() {
    let _ = fixture::device();
    assert!(!exact_gpu::tier::chosen());
    assert_eq!(exact_gpu::tier(), exact_gpu::GpuTier::High);
}
