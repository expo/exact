//! Copy the logical image to the display at its mounted orientation.
use crate::Rotation;
use ::wgpu::*;

pub(super) struct Blit {
    pipeline: RenderPipeline,
    layout: BindGroupLayout,
}

impl Blit {
    pub(super) fn new(device: &Device, format: TextureFormat) -> Self {
        let shader = device.create_shader_module(include_wgsl!("../../../shaders/scanout.wgsl"));
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Linux panel copy"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let layout = pipeline.get_bind_group_layout(0);
        Self { pipeline, layout }
    }

    pub(super) fn copy(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        source: &TextureView,
        target: &TextureView,
        rotation: Rotation,
    ) {
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Linux panel image"),
            layout: &self.layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(source),
            }],
        });
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Linux panel copy"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        let turn = u32::from(rotation.degrees() / 90);
        pass.draw(0..4, turn..turn + 1);
    }
}
