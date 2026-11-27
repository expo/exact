//! A mip level fills the next attachment. Its source view determines both sizes.
use super::Texture;
use ::wgpu as gpu;

pub(super) fn pipeline(
    device: &gpu::Device,
    shader: &gpu::ShaderModule,
    cache: Option<&gpu::PipelineCache>,
) -> gpu::RenderPipeline {
    device.create_render_pipeline(&gpu::RenderPipelineDescriptor {
        label: Some("ink mip"),
        layout: None,
        vertex: gpu::VertexState {
            module: shader,
            entry_point: Some("vs_mip"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: gpu::PrimitiveState {
            topology: gpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        fragment: Some(gpu::FragmentState {
            module: shader,
            entry_point: Some("fs_mip"),
            compilation_options: Default::default(),
            targets: &[Some(gpu::ColorTargetState {
                format: gpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: gpu::ColorWrites::ALL,
            })],
        }),
        multisample: Default::default(),
        depth_stencil: None,
        multiview_mask: None,
        cache,
    })
}

pub(super) fn encode(
    device: &gpu::Device,
    pipeline: &gpu::RenderPipeline,
    encoder: &mut gpu::CommandEncoder,
    texture: &Texture,
) {
    let levels = texture.desc.levels();
    if levels == 1 {
        return;
    }
    let layout = pipeline.get_bind_group_layout(0);
    let view = |level| {
        texture.raw.create_view(&gpu::TextureViewDescriptor {
            base_mip_level: level,
            mip_level_count: Some(1),
            ..Default::default()
        })
    };
    let mut source = view(0);
    for level in 1..levels {
        let target = view(level);
        let group = device.create_bind_group(&gpu::BindGroupDescriptor {
            label: Some("ink mip"),
            layout: &layout,
            entries: &[gpu::BindGroupEntry {
                binding: 5,
                resource: gpu::BindingResource::TextureView(&source),
            }],
        });
        let mut pass = encoder.begin_render_pass(&gpu::RenderPassDescriptor {
            label: Some("ink mip"),
            color_attachments: &[Some(gpu::RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: gpu::Operations {
                    load: gpu::LoadOp::Clear(gpu::Color::TRANSPARENT),
                    store: gpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..4, 0..1);
        drop(pass);
        source = target;
    }
}
