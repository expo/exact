//! A frame uniform: each frame's bytes reach that frame's draw, across the
//! ring's slots, as the queue's write would.

use exact_gpu::{fixture, wgpu, FrameUniform};

const TEXT: &str = "struct U { c: vec4<f32> }\n@group(0) @binding(0) var<uniform> u: U;\n@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {\n  let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));\n  return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);\n}\n@fragment fn fs() -> @location(0) vec4<f32> { return u.c; }\n";

#[test]
fn each_frame_draws_its_own_bytes() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let device = &gpu.device;
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(TEXT.into()),
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let mut uniform = FrameUniform::new(device, 16, "t");
    #[cfg(target_vendor = "apple")]
    assert!(uniform.slots() > 1, "Metal writes a ring in place");
    let groups: Vec<_> = (0..uniform.slots())
        .map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.buffer(i).as_entire_binding(),
                }],
            })
        })
        .collect();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    for frame in 0..9u8 {
        let red = f32::from(frame) / 8.0;
        let bytes: Vec<u8> = [red, 0.5, 0.25, 1.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let slot = uniform.write(&gpu.queue, &bytes);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &groups[slot], &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([encoder.finish()]);
        let px = fixture::read(&gpu, &texture).unwrap();
        assert_eq!(
            px.at(0, 0),
            [(red * 255.0).round() as u8, 128, 64, 255],
            "frame {frame}"
        );
    }
}

#[test]
fn dynamic_offsets_select_each_steps_uniform_slot() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let device = &gpu.device;
    // The entries reflection emits for this shader.
    const STEP: wgpu::BindGroupLayoutEntry = wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: std::num::NonZeroU64::new(8),
        },
        count: None,
    };
    const OUTPUT: wgpu::BindGroupLayoutEntry = wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: std::num::NonZeroU64::new(8),
        },
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("step"),
        entries: &[exact_gpu::dynamic_offset(STEP), OUTPUT],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("step"),
        source: wgpu::ShaderSource::Wgsl(
            "struct Step { dt: f32, index: u32 }
            @group(0) @binding(0) var<uniform> s: Step;
            @group(0) @binding(1) var<storage, read_write> output: array<f32, 2>;
            @compute @workgroup_size(1) fn step() { output[s.index] = s.dt; }"
                .into(),
        ),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("step"),
        compilation_options: Default::default(),
        cache: None,
    });
    let slot_bytes = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(256);
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: slot_bytes * 2,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 8,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 8,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform,
                    offset: 0,
                    size: std::num::NonZeroU64::new(8),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    for (index, dt) in [3.0_f32, 7.0].into_iter().enumerate() {
        let mut bytes = [0; 8];
        bytes[..4].copy_from_slice(&dt.to_le_bytes());
        bytes[4..].copy_from_slice(&(index as u32).to_le_bytes());
        gpu.queue
            .write_buffer(&uniform, slot_bytes * index as u64, &bytes);
    }
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        for index in 0..2 {
            pass.set_bind_group(0, &group, &[(slot_bytes * index) as u32]);
            pass.dispatch_workgroups(1, 1, 1);
        }
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 8);
    gpu.queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    let (send, receive) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        send.send(result).unwrap()
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive.recv().unwrap().unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    let values: Vec<_> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|value| f32::from_le_bytes(*value))
        .collect();
    assert_eq!(values, [3.0, 7.0]);
}
