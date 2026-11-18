//! Apple's captured children are premultiplied sRGB bytes (LLP 1014 D5).
use crate::hotspots::Hotspots;
use exact_gpu::{
    wgpu::{self, util::DeviceExt},
    Frame,
};

pub struct Labels {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
}
impl Labels {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model hotspots"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("model hotspots"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("model hotspots"),
            source: wgpu::ShaderSource::Wgsl(include_str!("labels.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model hotspots"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("model hotspots"), layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader, entry_point: Some("vertex"), compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 20, step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32],
                })],
            },
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: Some("fragment"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }), multiview_mask: None, cache: None,
        });
        Self {
            format,
            layout,
            sampler,
            pipeline,
        }
    }
}
impl Hotspots {
    pub fn has_captured_labels(&self) -> bool {
        self.children
            .iter()
            .any(|c| c.texture.is_some() && c.placement.is_some_and(|p| !p.hidden))
    }
    pub fn device_lost(&mut self) {
        self.gpu = None;
        for c in &mut self.children {
            c.texture = None;
            c.group = None;
        }
    }
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        frame: &Frame,
        format: wgpu::TextureFormat,
    ) {
        let mut order: Vec<_> = self
            .children
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                c.placement
                    .filter(|p| !p.hidden && c.texture.is_some())
                    .map(|p| (i, p.depth))
            })
            .collect();
        if order.is_empty() {
            return;
        }
        order.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        if self.gpu.as_ref().is_none_or(|gpu| gpu.format != format) {
            self.gpu = Some(Labels::new(device, format));
            for c in &mut self.children {
                c.group = None;
            }
        }
        let gpu = self.gpu.as_ref().unwrap();
        let mut bytes = Vec::with_capacity(order.len() * 6 * 20);
        for &(i, _) in &order {
            let c = &mut self.children[i];
            let p = c.placement.unwrap();
            let [x, y] = [p.homography[2], p.homography[5]];
            for [u, v] in [[0., 0.], [1., 0.], [0., 1.], [0., 1.], [1., 0.], [1., 1.]] {
                for value in [
                    2. * (x + u * c.frame[2]) / frame.width - 1.,
                    1. - 2. * (y + v * c.frame[3]) / frame.height,
                    u,
                    v,
                    c.opacity,
                ] {
                    bytes.extend(value.to_le_bytes());
                }
            }
            c.group.get_or_insert_with(|| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("model hotspot"),
                    layout: &gpu.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                c.texture.as_ref().unwrap(),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&gpu.sampler),
                        },
                    ],
                })
            });
        }
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("model hotspot quads"),
            contents: &bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("model hotspots"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&gpu.pipeline);
        pass.set_vertex_buffer(0, vertices.slice(..));
        for (j, &(i, _)) in order.iter().enumerate() {
            pass.set_bind_group(0, self.children[i].group.as_ref().unwrap(), &[]);
            pass.draw(j as u32 * 6..(j as u32 + 1) * 6, 0..1);
        }
    }
}
