//! A surface's post-submit map can complete without another queue submission.
#![cfg(not(target_arch = "wasm32"))]

use exact_gpu::{fixture, wgpu, Frame, Surface, SurfaceError, Value};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use wgpu::util::DeviceExt;

struct Status {
    source: wgpu::Buffer,
    staging: wgpu::Buffer,
    copied: bool,
    pending: Option<mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
    value: Option<u32>,
}

impl Surface for Status {
    fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
        Ok(())
    }

    fn render(
        &mut self,
        _: &Frame,
        device: &wgpu::Device,
        _: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        _: &wgpu::TextureView,
        _: wgpu::TextureFormat,
    ) -> bool {
        if self.pending.is_some() {
            device.poll(wgpu::PollType::Poll).expect("poll status");
        }
        if let Some(rx) = &self.pending {
            if let Ok(result) = rx.try_recv() {
                result.expect("status map");
                let view = self.staging.get_mapped_range(..).expect("status bytes");
                self.value = Some(u32::from_le_bytes(view[..4].try_into().unwrap()));
                drop(view);
                self.staging.unmap();
                self.pending = None;
            }
        } else if self.value.is_none() {
            encoder.copy_buffer_to_buffer(&self.source, 0, &self.staging, 0, 4);
            self.copied = true;
        }
        self.value.is_none()
    }

    fn submitted(&mut self) {
        if std::mem::take(&mut self.copied) {
            let (tx, rx) = mpsc::channel();
            self.staging
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |r| {
                    let _ = tx.send(r);
                });
            self.pending = Some(rx);
        }
    }
}

#[test]
fn a_surface_polls_its_post_submit_map_until_it_can_rest() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut status = Status {
        source: gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("status"),
                contents: &37u32.to_le_bytes(),
                usage: wgpu::BufferUsages::COPY_SRC,
            }),
        staging: gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("status readback"),
            size: 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        copied: false,
        pending: None,
        value: None,
    };
    let frame = Frame {
        width: 1.,
        height: 1.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.,
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
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
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    assert!(status.render(
        &frame,
        &gpu.device,
        &gpu.queue,
        &mut encoder,
        &view,
        target.format()
    ));
    gpu.queue.submit([encoder.finish()]);
    status.submitted();

    // No further submit, capture or host sync can deliver the callback for us.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        if !status.render(
            &frame,
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &view,
            target.format(),
        ) {
            break;
        }
        assert!(Instant::now() < deadline, "the status map never completed");
        std::thread::park_timeout(Duration::from_millis(1));
    }
    assert_eq!(status.value, Some(37));
    assert!(status.pending.is_none());
}
