use exact_gpu::{
    json, wgpu, ColorSpace, ColorTarget, Frame, Registry, Surface, SurfaceError, Value,
};

#[derive(Default)]
struct Board {
    float: bool,
    plain: bool,
    rgba: [[f32; 4]; 2],
    target: ColorTarget,
    pipeline: Option<(wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup)>,
    binds: u32,
    frames: u32,
}

impl Surface for Board {
    fn color_space(&self) -> ColorSpace {
        ColorSpace::DisplayP3
    }
    fn high_dynamic_range(&self) -> bool {
        self.float
    }
    fn device_ready(&mut self, _: wgpu::Features, _: wgpu::Limits) {
        self.pipeline = None;
    }
    fn color_target(&mut self, target: ColorTarget) {
        if self.target.format != target.format {
            self.pipeline = None;
        }
        self.target = target;
    }
    fn bind(&mut self, inputs: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
        if !self.plain {
            self.rgba = [
                json::color(&inputs[0], "quad")?,
                json::color(&inputs[1], "clear")?,
            ];
        }
        self.binds += 1;
        Ok(())
    }
    fn agent(&mut self, _: &str) -> Option<String> {
        Some(format!(
            r#"{{"world":{{"frames":{},"binds":{},"target":"{:?}"}}}}"#,
            self.frames, self.binds, self.target
        ))
    }
    fn render(
        &mut self,
        _: &Frame,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> bool {
        self.frames += 1;
        let fill = if self.plain {
            wgpu::Color {
                r: 0.3,
                g: 0.2,
                b: 0.15,
                a: 0.5,
            }
        } else {
            self.target.clear(self.rgba[1])
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(fill),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        if !self.plain {
            let (pipeline, buffer, group) = self.pipeline.get_or_insert_with(|| {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: None,
                    source: wgpu::ShaderSource::Wgsl(
                        r#"
@group(0) @binding(0) var<uniform> color: vec4f;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let xy = array<vec2f, 4>(vec2f(-.75,-.75), vec2f(.75,-.75), vec2f(-.75,.75), vec2f(.75,.75));
    return vec4f(xy[i], 0., 1.);
}
@fragment fn fs() -> @location(0) vec4f { return color; }
"#
                        .into(),
                    ),
                });
                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: None,
                    layout: None,
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleStrip,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                });
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 16,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                });
                (pipeline, buffer, group)
            });
            let color = self.target.fragment(self.rgba[0]);
            let bytes = color.map(f32::to_le_bytes);
            queue.write_buffer(buffer, 0, bytes.as_flattened());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &*group, &[]);
            pass.draw(0..4, 0..1);
        }
        self.frames < 120
    }
}
static REGISTRY: Registry = Registry {
    surfaces: &[
        ("board", 2, || Box::<Board>::default()),
        ("boardFloat", 2, || {
            Box::new(Board {
                float: true,
                ..Default::default()
            })
        }),
        ("literal", 0, || {
            Box::new(Board {
                plain: true,
                ..Default::default()
            })
        }),
    ],
    shaders: &[],
};
exact_gpu::module!(REGISTRY);
