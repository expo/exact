//! One fragment shader, with uniforms declared by its WGSL (LLP 1181). It is
//! opaque unless the canvas writes `alphaMode="premultiplied"`.

mod binding;
mod pipeline;
mod state;
mod timeline;

use exact_gpu::{
    wgpu, AssetChanges, AssetError, Frame, FrameUniform, Surface, SurfaceError, Value,
};
use exact_shader_uniforms::{AlphaMode, Builtins, Layout};
use pipeline::Shared;
use std::rc::Rc;

pub fn surface() -> Box<dyn Surface> {
    Box::<Shader>::default()
}

#[derive(Default)]
struct Shader {
    arguments: binding::Binding,
    reduced_motion: bool,
    timeline: timeline::Timeline,
    changes: AssetChanges,
    state: state::State,
    report: Option<SurfaceError>,
    published_error: Option<String>,
}

struct Source {
    text: Rc<str>,
    layout: Layout,
}

struct Picture {
    generation: u64,
    source: Rc<Source>,
    binding: binding::Binding,
    pipeline: Option<Shared>,
    uniform: Option<Uniform>,
}

struct Uniform {
    buffer: FrameUniform,
    groups: Vec<wgpu::BindGroup>,
}

impl Shader {
    fn running(&self) -> bool {
        self.state.picture.as_ref().is_some_and(|p| {
            let binding = if self.state.current(p) {
                &self.arguments
            } else {
                &p.binding
            };
            p.source.layout.members.iter().any(|m| m.name == "time")
                && !binding.values.iter().any(|(name, _)| name == "time")
                && binding.play.unwrap_or(!self.reduced_motion)
        })
    }

    fn fail(&mut self, message: impl std::fmt::Display) {
        let message = message.to_string();
        self.report = Some(SurfaceError(format!("{}: {message}", self.arguments.src)));
        self.state.fail(message);
    }

    fn prepare(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        if let Some(pending) = &mut self.state.preparing {
            if let Err(error) = pending.prepare(device, format) {
                self.fail(error);
            }
        }
        if let Some(picture) = &mut self.state.picture {
            if let Err(error) = picture.prepare(device, format) {
                self.fail(error);
            }
        }
    }
}

impl Picture {
    fn prepare(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Result<bool, String> {
        if self
            .pipeline
            .as_ref()
            .is_none_or(|p| !p.borrow().matches(device, format))
        {
            self.pipeline = Some(pipeline::get(
                device,
                format,
                self.source.text.clone(),
                self.source.layout.clone(),
                self.binding.alpha_mode,
            ));
            self.uniform = None;
        }
        let pipeline = self.pipeline.as_ref().unwrap().clone();
        let mut p = pipeline.borrow_mut();
        p.poll();
        if let Some(error) = p.error() {
            return Err(error.to_string());
        }
        if p.pending() || self.uniform.is_some() {
            return Ok(!p.pending());
        }
        if let Some(layout) = &p.bind_layout {
            let buffer =
                FrameUniform::new(device, self.source.layout.size.max(16), "shader uniforms");
            let groups = (0..buffer.slots())
                .map(|slot| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("shader uniforms"),
                        layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buffer.buffer(slot).as_entire_binding(),
                        }],
                    })
                })
                .collect();
            self.uniform = Some(Uniform { buffer, groups });
        }
        Ok(true)
    }
}

impl Surface for Shader {
    fn arguments(&self) -> Vec<(&'static str, Value)> {
        exact_shader_uniforms::ARGUMENTS
            .into_iter()
            .zip([
                Value::str(""),
                Value::Number(0.0),
                Value::Unit,
                Value::str("opaque"),
            ])
            .collect()
    }

    fn rest_arguments(&self) -> bool {
        true
    }

    fn bind(&mut self, inputs: &[Value], at_ms: Option<f64>) -> Result<(), SurfaceError> {
        let next = binding::Binding::read(inputs)?;
        self.state.check_binding(&next)?;
        if next.src != self.arguments.src {
            if !self.arguments.src.is_empty() {
                self.changes
                    .retired
                    .push(self.arguments.src[7..].to_string());
            }
            if !next.src.is_empty() {
                self.changes.requests.push(next.src[7..].to_string());
            }
            self.state.set_source(&next.src);
            self.report = None;
        }
        if next != self.arguments {
            self.state.generation += 1;
        }
        self.arguments = next;
        self.state.bind(&self.arguments);
        self.timeline.play(self.running(), at_ms);
        Ok(())
    }

    fn max_fps(&self) -> u32 {
        self.arguments.max_fps
    }

    fn reduced_motion(&mut self, reduce: bool) {
        self.reduced_motion = reduce;
        self.timeline.play(self.running(), None);
    }

    fn assets(&mut self) -> AssetChanges {
        std::mem::take(&mut self.changes)
    }

    fn accepts_asset_update(&self, name: &str) -> bool {
        self.arguments.src.strip_prefix("assets/") == Some(name)
    }

    fn asset(&mut self, name: &str, bytes: Result<&[u8], AssetError>) {
        if !self.accepts_asset_update(name) {
            return;
        }
        let read = || -> Result<Source, String> {
            let bytes = bytes.map_err(|e| format!("could not load shader: {e:?}"))?;
            let text =
                std::str::from_utf8(bytes).map_err(|e| format!("expected UTF-8 WGSL: {e}"))?;
            exact_gpu::shaders::validate_shader(name, text, None)?;
            let layout = exact_shader_uniforms::parse(text).map_err(|e| e.to_string())?;
            Ok(Source {
                text: Rc::from(text),
                layout,
            })
        };
        self.state.asset_arriving();
        match read() {
            Ok(source) => {
                self.state.parsed(&self.arguments.src, source);
                self.state.bind(&self.arguments);
                if let Some(error) = self.state.error() {
                    self.report = Some(SurfaceError(format!("{}: {error}", self.arguments.src)));
                }
            }
            Err(error) => self.fail(error),
        }
    }

    fn prepare_assets(
        &mut self,
        device: &wgpu::Device,
        _: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        self.prepare(device, format);
    }

    fn preparing(&self) -> bool {
        self.state.preparing.is_some()
            || self
                .state
                .picture
                .as_ref()
                .is_some_and(|p| p.pipeline.as_ref().is_some_and(|p| p.borrow().pending()))
    }

    fn render(
        &mut self,
        frame: &Frame,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> bool {
        self.prepare(device, format);
        if self.state.preparing.as_ref().is_some_and(|p| {
            p.pipeline
                .as_ref()
                .is_some_and(|p| !p.borrow().pending() && p.borrow().error().is_none())
        }) {
            self.state.picture = self.state.preparing.take();
            self.timeline.play(self.running(), None);
        }
        self.state.bind_frame(&self.arguments);
        let drawable = self.state.picture.as_ref().is_some_and(|picture| {
            let p = picture.pipeline.as_ref().unwrap().borrow();
            !p.pending() && p.error().is_none()
        });
        if !drawable {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shader fallback"),
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
            return self.preparing();
        }
        let picture = self.state.picture.as_mut().unwrap();
        let pipeline = picture.pipeline.as_ref().unwrap();
        let p = pipeline.borrow();
        let time = self.timeline.at(frame.now_ms);
        let (width, height) = frame.pixels();
        let builtins = Builtins {
            time,
            resolution: [f64::from(width), f64::from(height)],
            device_pixel_ratio: f64::from(frame.scale),
        };
        let bytes = picture
            .binding
            .pack(&picture.source.layout, builtins)
            .expect("accepted shader uniforms");
        let slot = picture
            .uniform
            .as_mut()
            .map(|u| u.buffer.write(queue, &bytes));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shader"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(match p.alpha_mode {
                        AlphaMode::Opaque => wgpu::Color::BLACK,
                        AlphaMode::Premultiplied => wgpu::Color::TRANSPARENT,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&p.render);
        if let Some(slot) = slot {
            pass.set_bind_group(0, &picture.uniform.as_ref().unwrap().groups[slot], &[]);
        }
        pass.draw(0..3, 0..1);
        drop(pass);
        drop(p);
        self.state.drawn();
        self.preparing() || self.running()
    }

    fn frame_ready(&self) -> bool {
        self.state.frame_ready()
    }

    fn frame_presentable(&self) -> bool {
        self.state.frame_presentable()
    }

    fn is_empty(&self) -> bool {
        self.state.is_empty()
    }

    fn opaque(&self) -> bool {
        // An opaque picture is right under either composition, a
        // premultiplied one only under blending. So the canvas blends from
        // before a premultiplied picture's first frame until it is replaced:
        // the host configures the target before `render` swaps pictures.
        let state = &self.state;
        std::iter::once(&self.arguments)
            .chain(
                state
                    .picture
                    .iter()
                    .chain(&state.preparing)
                    .map(|p| &p.binding),
            )
            .all(|binding| binding.alpha_mode == AlphaMode::Opaque)
    }

    fn generation(&self) -> u64 {
        self.state.generation
    }

    fn device_lost(&mut self) {
        for picture in self
            .state
            .picture
            .iter_mut()
            .chain(self.state.preparing.iter_mut())
        {
            picture.pipeline = None;
            picture.uniform = None;
            picture.generation = 0;
        }
        self.state.generation += 1;
    }

    fn take_error(&mut self) -> Option<SurfaceError> {
        self.report.take()
    }

    fn published(&mut self) -> Option<String> {
        let error = self.state.diagnostic();
        if self.published_error.as_ref() == Some(&error) {
            return None;
        }
        self.published_error = Some(error.clone());
        let quoted = exact_gpu::json::strings(&[error]);
        Some(format!("{{\"error\":{}}}", &quoted[1..quoted.len() - 1]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_gpu::fixture;

    #[test]
    fn canvases_share_the_validated_pipeline_and_keep_their_uniforms() {
        let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
            return;
        };
        let text = "struct U { red: f32 }
            @group(0) @binding(0) var<uniform> u: U;
            @fragment fn color() -> @location(0) vec4f { return vec4f(u.red, 0., 0., 0.); }";
        let mut canvases = [Shader::default(), Shader::default()];
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
        for (surface, red) in canvases.iter_mut().zip([0.25_f64, 0.75]) {
            let args = exact_gpu::json::parse_values(&format!(
                "[\"assets/test.wgsl\",0,null,null,[[\"red\",{red}]]]"
            ))
            .unwrap();
            surface.bind(&args, None).unwrap();
            surface.asset("test.wgsl", Ok(text.as_bytes()));
            let (pixels, wants) = fixture::render(&gpu, surface, &frame).unwrap();
            assert_eq!(pixels.at(2, 2), [(red * 255.).round() as u8, 0, 0, 255]);
            assert!(!wants);
        }
        assert!(Rc::ptr_eq(
            canvases[0]
                .state
                .picture
                .as_ref()
                .unwrap()
                .pipeline
                .as_ref()
                .unwrap(),
            canvases[1]
                .state
                .picture
                .as_ref()
                .unwrap()
                .pipeline
                .as_ref()
                .unwrap()
        ));
        assert_ne!(
            canvases[0]
                .state
                .picture
                .as_ref()
                .unwrap()
                .uniform
                .as_ref()
                .unwrap()
                .buffer
                .buffer(0),
            canvases[1]
                .state
                .picture
                .as_ref()
                .unwrap()
                .uniform
                .as_ref()
                .unwrap()
                .buffer
                .buffer(0)
        );
    }
}
