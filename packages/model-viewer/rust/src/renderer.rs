use crate::{
    camera::Camera,
    pick::Clip,
    presentation::Presentation,
    styles::{Effective, Occlusion},
};
use exact_gpu::{
    wgpu::{self, util::DeviceExt},
    SurfaceError,
};
use exact_model_gltf::{math::*, scene::Scene};

pub fn samples(flags: wgpu::TextureFormatFeatureFlags) -> u32 {
    if flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4) {
        4
    } else {
        1
    }
}
fn floats(values: impl IntoIterator<Item = f32>) -> Vec<u8> {
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}
fn buffer(
    device: &wgpu::Device,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage,
    })
}

pub struct Renderer {
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    uniform: wgpu::Buffer,
    styles: wgpu::Buffer,
    group: wgpu::BindGroup,
    solid: wgpu::RenderPipeline,
    faded: wgpu::RenderPipeline,
    translucent: wgpu::RenderPipeline,
    pub presentation: Presentation,
    depth: wgpu::TextureView,
    msaa: Option<wgpu::TextureView>,
    pub size: (u32, u32),
    pub format: wgpu::TextureFormat,
    pub samples: u32,
}
impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        scene: &Scene,
        size: (u32, u32),
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Result<Self, SurfaceError> {
        let limits = device.limits();
        for (name, bytes) in [
            ("vertices", scene.vertices.len() as u64 * 32),
            ("indices", scene.indices.len() as u64 * 4),
            ("primitive styles", scene.primitives.len() as u64 * 32),
        ] {
            if bytes > limits.max_buffer_size
                || (name == "primitive styles"
                    && bytes > u64::from(limits.max_storage_buffer_binding_size))
            {
                return Err(SurfaceError(format!(
                    "{name} exceed the device buffer limit"
                )));
            }
        }
        let vertices: Vec<u8> = scene
            .vertices
            .iter()
            .flat_map(|v| {
                let mut bytes = floats(v.p.into_iter().chain(v.n));
                bytes.extend(v.node.to_le_bytes());
                bytes.extend(v.primitive.to_le_bytes());
                bytes
            })
            .collect();
        let indices: Vec<u8> = scene.indices.iter().flat_map(|v| v.to_le_bytes()).collect();
        let vertex = buffer(
            device,
            "model vertices",
            if vertices.is_empty() {
                &[0; 32]
            } else {
                &vertices
            },
            wgpu::BufferUsages::VERTEX,
        );
        let index = buffer(
            device,
            "model indices",
            if indices.is_empty() {
                &[0; 4]
            } else {
                &indices
            },
            wgpu::BufferUsages::INDEX,
        );
        let uniform = buffer(
            device,
            "model camera",
            &[0; 112],
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let styles = buffer(
            device,
            "model primitive styles",
            &vec![0; scene.primitives.len().max(1) * 32],
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model viewer"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model viewer"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: styles.as_entire_binding(),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model viewer"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("model viewer"),
            source: wgpu::ShaderSource::Wgsl(include_str!("model.wgsl").into()),
        });
        let make = |entry: &str, depth: bool, compare, cull| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3, 1 => Float32x3, 2 => Uint32, 3 => Uint32
                        ],
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(depth),
                    depth_compare: Some(compare),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: sample_count,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: Presentation::FORMAT,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let solid = make("fragment", true, wgpu::CompareFunction::Less, None);
        let faded = make("faded", false, wgpu::CompareFunction::Greater, None);
        let translucent = make(
            "fragment",
            false,
            wgpu::CompareFunction::Less,
            Some(wgpu::Face::Back),
        );
        let texture = |format, label| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let depth = texture(wgpu::TextureFormat::Depth32Float, "model depth");
        let msaa = (sample_count > 1).then(|| texture(Presentation::FORMAT, "model MSAA"));
        let presentation = Presentation::new(device, size, format);
        Ok(Self {
            vertex,
            index,
            uniform,
            styles,
            group,
            solid,
            faded,
            translucent,
            depth,
            presentation,
            msaa,
            size,
            format,
            samples: sample_count,
        })
    }
    pub fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        camera: &Camera,
        effective: &[Effective],
        clip: Clip,
    ) {
        let aspect = self.size.0 as f32 / self.size.1 as f32;
        let uniform = floats(
            camera
                .matrix(aspect)
                .into_iter()
                .flatten()
                .chain(camera.pose.eye())
                .chain([0.])
                .chain(clip.position)
                .chain([0.])
                .chain(clip.normal)
                .chain([0.]),
        );
        queue.write_buffer(&self.uniform, 0, &uniform);
        let styles = floats(
            effective
                .iter()
                .flat_map(|s| s.color.into_iter().chain([s.occluded_opacity, 0., 0., 0.])),
        );
        if !styles.is_empty() {
            queue.write_buffer(&self.styles, 0, &styles);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("model viewer"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.msaa.as_ref().unwrap_or(&self.presentation.color),
                depth_slice: None,
                resolve_target: self.msaa.as_ref().map(|_| &self.presentation.color),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_vertex_buffer(0, self.vertex.slice(..));
        pass.set_index_buffer(self.index.slice(..), wgpu::IndexFormat::Uint32);
        let draw = |pass: &mut wgpu::RenderPass<'_>, i: usize| {
            pass.draw_indexed(scene.primitives[i].indices.clone(), 0, 0..1)
        };
        pass.set_pipeline(&self.solid);
        for (i, s) in effective.iter().enumerate() {
            if s.visible && s.occlusion == Occlusion::Occluder {
                draw(&mut pass, i);
            }
        }
        pass.set_pipeline(&self.faded);
        for (i, s) in effective.iter().enumerate() {
            if s.visible && s.occlusion == Occlusion::Revealed && s.occluded_opacity > 0. {
                draw(&mut pass, i);
            }
        }
        pass.set_pipeline(&self.solid);
        for (i, s) in effective.iter().enumerate() {
            if s.visible && s.occlusion != Occlusion::Occluder && s.opaque() {
                draw(&mut pass, i);
            }
        }
        let mut translucent: Vec<_> = effective
            .iter()
            .enumerate()
            .filter(|(_, s)| s.visible && !s.opaque())
            .map(|(i, _)| {
                let centre = scene.nodes[scene.primitives[i].node]
                    .bounds
                    .map_or(scene.centre, |b| b.centre());
                (
                    dot(sub(centre, camera.pose.eye()), camera.pose.basis().2),
                    i,
                )
            })
            .collect();
        translucent.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        pass.set_pipeline(&self.translucent);
        for (_, i) in translucent {
            draw(&mut pass, i);
        }
        drop(pass);
    }
}

pub fn clear(encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
}
