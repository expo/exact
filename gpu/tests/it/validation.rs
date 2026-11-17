//! A surface whose commands fail wgpu's validation is refused with an error,
//! and the app keeps running (#658): wgpu validates a pass when its encoder
//! finishes, and its default handler for an error no scope catches aborts.
#![cfg(not(target_arch = "wasm32"))]

use exact_gpu::{fixture, wgpu, Frame, Module, Registry, Surface, SurfaceError, Value};

/// Draws with a pipeline made for a single-channel target into its own
/// target, which the pass refuses. `Some(false)` draws with none: a frame
/// that is valid. Records its submits.
#[derive(Default)]
pub struct Misfit {
    pipeline: Option<wgpu::RenderPipeline>,
    valid: bool,
    pub submitted: u32,
}

const WGSL: &str = "@vertex fn vs() -> @builtin(position) vec4<f32> { return vec4<f32>(0.0); }
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }";

impl Surface for Misfit {
    fn bind(&mut self, inputs: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
        self.valid = matches!(inputs.first(), Some(Value::Bool(true)));
        Ok(())
    }
    fn render(
        &mut self,
        _: &Frame,
        device: &wgpu::Device,
        _: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        _: wgpu::TextureFormat,
    ) -> bool {
        let pipeline = self.pipeline.get_or_insert_with(|| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(WGSL.into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("misfit"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::TextureFormat::R8Unorm.into())],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("misfit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        if !self.valid {
            pass.set_pipeline(pipeline);
            pass.draw(0..3, 0..1);
        }
        true
    }
    fn submitted(&mut self) {
        self.submitted += 1;
    }
}

static REGISTRY: Registry = Registry {
    surfaces: &[("misfit", 0, || Box::<Misfit>::default())],
    shaders: &[],
};

const FRAME: Frame = Frame {
    width: 4.,
    height: 4.,
    scale: 1.,
    now_ms: 0.,
    seekable: true,
    period_ms: 0.,
    children_generation: 0,
    shader_generation: 0,
    headroom: 1.,
};

#[test]
fn a_read_back_frame_that_fails_validation_is_refused_and_the_next_one_reads() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut m = Module::new(&REGISTRY);
    m.set_gpu(gpu);
    let id = m.create_headless("misfit").unwrap();
    assert!(m.bind(id, &[], None));
    for _ in 0..3 {
        assert!(m.readback(id, &FRAME).is_none(), "refused");
        let error = m.take_error();
        assert!(error.contains("misfit"), "{error}");
    }
    assert!(m.flush(), "nothing reached the uncaptured handler");
    assert!(m.bind(id, &[Value::Bool(true)], None));
    let (pixels, _) = m.readback(id, &FRAME).expect("a valid frame reads");
    assert_eq!(
        &pixels.data[..4],
        &[0, 0, 255, 255],
        "{:?}",
        &pixels.data[..4]
    );
    let other = m.create_headless("misfit").unwrap();
    assert!(m.bind(other, &[Value::Bool(true)], None));
    assert!(m.readback(other, &FRAME).is_some(), "and another canvas");
    assert_eq!(m.take_error(), "");
}

/// An error outside every scope the module holds is kept for the next flush
/// to report, where wgpu's default handler would abort.
#[test]
fn an_error_no_scope_catches_is_reported_by_the_next_flush() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut m = Module::new(&REGISTRY);
    m.set_gpu(gpu);
    let _ = m
        .gpu()
        .unwrap()
        .device
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("unmappable"),
            size: 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::MAP_WRITE,
            mapped_at_creation: false,
        });
    assert!(!m.flush());
    let error = m.take_error();
    assert!(error.contains("unmappable"), "{error}");
    assert!(m.flush(), "reported once");
}
