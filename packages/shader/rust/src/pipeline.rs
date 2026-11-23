use exact_gpu::wgpu;
use exact_shader_uniforms::{AlphaMode, Layout};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};

pub(crate) type Shared = Rc<RefCell<Pipeline>>;
type Validation = Pin<Box<dyn Future<Output = Option<wgpu::Error>>>>;

thread_local! {
    static PIPELINES: RefCell<Vec<Weak<RefCell<Pipeline>>>> = const { RefCell::new(Vec::new()) };
}

pub(crate) struct Pipeline {
    device: wgpu::Device,
    format: wgpu::TextureFormat,
    text: Rc<str>,
    pub alpha_mode: AlphaMode,
    pub render: wgpu::RenderPipeline,
    pub bind_layout: Option<wgpu::BindGroupLayout>,
    validation: Option<Validation>,
    error: Option<String>,
}

impl Pipeline {
    pub fn matches(&self, device: &wgpu::Device, format: wgpu::TextureFormat) -> bool {
        self.device == *device && self.format == format
    }

    pub fn poll(&mut self) {
        if let Some(future) = &mut self.validation {
            if let Poll::Ready(error) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                self.error = error.map(|e| e.to_string());
                self.validation = None;
            }
        }
    }

    pub fn pending(&self) -> bool {
        self.validation.is_some()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

pub(crate) fn get(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    text: Rc<str>,
    layout: Layout,
    alpha_mode: AlphaMode,
) -> Shared {
    PIPELINES.with_borrow_mut(|cache| {
        cache.retain(|p| p.strong_count() != 0);
        if let Some(pipeline) = cache.iter().filter_map(Weak::upgrade).find(|p| {
            let p = p.borrow();
            p.matches(device, format) && p.text == text && p.alpha_mode == alpha_mode
        }) {
            return pipeline;
        }
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let fragment = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader fragment"),
            source: wgpu::ShaderSource::Wgsl(text.as_ref().into()),
        });
        let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader triangle"),
            source: wgpu::ShaderSource::Wgsl(include_str!("triangle.wgsl").into()),
        });
        let bind_layout = layout.name.map(|_| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shader uniforms"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(layout.size as u64),
                    },
                    count: None,
                }],
            })
        });
        let groups = bind_layout.iter().map(Some).collect::<Vec<_>>();
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shader"),
            bind_group_layouts: &groups,
            immediate_size: 0,
        });
        let render = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shader"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex,
                entry_point: None,
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &fragment,
                entry_point: None,
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    // An opaque picture keeps the pass's cleared alpha of one.
                    write_mask: match alpha_mode {
                        AlphaMode::Opaque => wgpu::ColorWrites::COLOR,
                        AlphaMode::Premultiplied => wgpu::ColorWrites::ALL,
                    },
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let pipeline = Rc::new(RefCell::new(Pipeline {
            device: device.clone(),
            format,
            text,
            alpha_mode,
            render,
            bind_layout,
            validation: Some(Box::pin(scope.pop())),
            error: None,
        }));
        cache.push(Rc::downgrade(&pipeline));
        pipeline
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shader;
    use exact_gpu::{fixture, Frame, Surface};

    #[test]
    fn pending_validation_requests_frames_and_replacements_keep_drawing() {
        let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
            return;
        };
        let text =
            b"@fragment fn solid() -> @location(0) vec4f { return vec4f(0.25, 0., 0., 0.); }";
        let frame = Frame {
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
        let mut surface = Shader::default();
        surface
            .bind(&[exact_gpu::Value::str("assets/a.wgsl")], None)
            .unwrap();
        surface.asset("a.wgsl", Ok(text));
        let source = &surface.state.preparing.as_ref().unwrap().source;
        let pipeline = get(
            &gpu.device,
            wgpu::TextureFormat::Rgba8Unorm,
            source.text.clone(),
            source.layout.clone(),
            AlphaMode::Opaque,
        );
        pipeline.borrow_mut().validation = Some(Box::pin(std::future::pending()));
        assert!(fixture::render(&gpu, &mut surface, &frame).unwrap().1);
        assert!(!surface.frame_ready());
        pipeline.borrow_mut().validation = None;
        let expected = fixture::render(&gpu, &mut surface, &frame).unwrap().0;
        assert_eq!(expected.at(2, 2), [64, 0, 0, 255]);

        surface
            .bind(&[exact_gpu::Value::str("assets/b.wgsl")], None)
            .unwrap();
        assert_eq!(surface.generation(), 2);
        assert_eq!(
            fixture::render(&gpu, &mut surface, &frame).unwrap().0,
            expected
        );
        surface.asset(
            "b.wgsl",
            Ok(b"@fragment fn blue() -> @location(0) vec4f { return vec4f(0., 0., 1., 0.); }"),
        );
        // Hold validation across a frame, as the browser's error-scope promise does.
        let source = &surface.state.preparing.as_ref().unwrap().source;
        let replacement = get(
            &gpu.device,
            wgpu::TextureFormat::Rgba8Unorm,
            source.text.clone(),
            source.layout.clone(),
            AlphaMode::Opaque,
        );
        replacement.borrow_mut().validation = Some(Box::pin(std::future::pending()));
        let (pixels, wants) = fixture::render(&gpu, &mut surface, &frame).unwrap();
        assert_eq!(pixels, expected);
        assert!(wants);
        replacement.borrow_mut().validation = None;
        assert_eq!(
            fixture::render(&gpu, &mut surface, &frame)
                .unwrap()
                .0
                .at(2, 2),
            [0, 0, 255, 255]
        );
        assert!(!surface.preparing());
    }

    #[test]
    fn an_alpha_mode_switch_blends_the_shown_picture_until_its_replacement_draws() {
        let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
            return;
        };
        let frame = Frame {
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
        let bind = |surface: &mut Shader, mode| {
            let src = exact_gpu::Value::str("assets/a.wgsl");
            let inputs = [
                src,
                exact_gpu::Value::Number(0.),
                exact_gpu::Value::Unit,
                exact_gpu::Value::str(mode),
            ];
            surface.bind(&inputs, None).unwrap();
        };
        let mut surface = Shader::default();
        bind(&mut surface, "premultiplied");
        surface.asset(
            "a.wgsl",
            Ok(b"@fragment fn half() -> @location(0) vec4f { return vec4f(0.25, 0., 0., 0.5); }"),
        );
        let blended = fixture::render(&gpu, &mut surface, &frame).unwrap().0;
        assert!(
            blended.at(2, 2)[3].abs_diff(128) <= 1,
            "{:?}",
            blended.at(2, 2)
        );
        assert!(!surface.opaque());

        bind(&mut surface, "opaque");
        let source = &surface.state.preparing.as_ref().unwrap().source;
        let replacement = get(
            &gpu.device,
            wgpu::TextureFormat::Rgba8Unorm,
            source.text.clone(),
            source.layout.clone(),
            AlphaMode::Opaque,
        );
        replacement.borrow_mut().validation = Some(Box::pin(std::future::pending()));
        let (pixels, wants) = fixture::render(&gpu, &mut surface, &frame).unwrap();
        assert_eq!(pixels, blended);
        assert!(wants);
        assert!(!surface.opaque());
        replacement.borrow_mut().validation = None;
        let pixels = fixture::render(&gpu, &mut surface, &frame).unwrap().0;
        assert_eq!(pixels.at(2, 2)[3], 255);
        assert!(surface.opaque());
    }
}
