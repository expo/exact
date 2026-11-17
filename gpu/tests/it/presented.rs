//! A canvas's `presented` (LLP 1009 D9) on a canvas with no target of its
//! own, which its host shows by composing its read-back pixels.
#![cfg(not(target_arch = "wasm32"))]

use exact_gpu::{fixture, wgpu, Frame, Module, Registry, Surface, SurfaceError, Value};

/// Clears its target; its generation is its first input, and it publishes
/// `{"score":…}` with it. `Pending` never finishes a frame.
#[derive(Default)]
struct Picture {
    generation: u64,
    pending: bool,
    publish: bool,
}

impl Surface for Picture {
    fn bind(&mut self, inputs: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
        if let Some(Value::Number(n)) = inputs.first() {
            self.generation = *n as u64;
            self.publish = true;
        }
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
                    load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        false
    }
    fn frame_ready(&self) -> bool {
        !self.pending
    }
    fn published(&mut self) -> Option<String> {
        std::mem::take(&mut self.publish).then(|| format!(r#"{{"score":{}}}"#, self.generation))
    }
    fn generation(&self) -> u64 {
        self.generation
    }
    fn restore(&mut self, _: &[u8], _: exact_gpu::Restore) -> Result<(), String> {
        Ok(())
    }
}

static REGISTRY: Registry = Registry {
    surfaces: &[
        ("picture", 0, || Box::<Picture>::default()),
        ("pending", 0, || {
            Box::new(Picture {
                pending: true,
                ..Default::default()
            })
        }),
    ],
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

fn module() -> Option<Module> {
    let gpu = fixture::device_or_skip(fixture::device())?;
    let mut m = Module::new(&REGISTRY);
    m.set_gpu(gpu);
    Some(m)
}

#[test]
fn a_read_back_canvas_is_presented_by_its_complete_frames_until_its_picture_changes() {
    let Some(mut m) = module() else { return };
    let id = m.create_headless("picture").unwrap();
    assert!(m.bind(id, &[], None));
    assert_eq!(m.take_published(id), None, "nothing published or shown");
    assert!(m.readback(id, &FRAME).is_some());
    assert_eq!(
        m.take_published(id).as_deref(),
        Some(r#"{"presented":true}"#)
    );
    assert!(m.readback(id, &FRAME).is_some());
    assert_eq!(m.take_published(id), None, "a second frame says nothing");
    // A new picture: the surface's own record, and not presented until it is.
    assert!(m.bind(id, &[Value::Number(1.)], None));
    assert_eq!(
        m.take_published(id).as_deref(),
        Some(r#"{"score":1,"presented":false}"#)
    );
    assert!(m.readback(id, &FRAME).is_some());
    assert_eq!(
        m.take_published(id).as_deref(),
        Some(r#"{"score":1,"presented":true}"#)
    );
    assert!(m.restore(id, &[], exact_gpu::Restore::Open));
    assert_eq!(
        m.take_published(id).as_deref(),
        Some(r#"{"presented":false}"#),
        "the restored state's record is its own"
    );
}

#[test]
fn an_incomplete_frame_is_not_presented() {
    let Some(mut m) = module() else { return };
    let id = m.create_headless("pending").unwrap();
    assert!(m.bind(id, &[], None));
    assert!(m.readback(id, &FRAME).is_some());
    assert_eq!(m.take_published(id), None);
}
