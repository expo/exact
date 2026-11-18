use exact_gpu::{wgpu, Frame, Surface, SurfaceError, Value};

struct Sky(bool);

impl Surface for Sky {
    fn bind(&mut self, inputs: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
        self.0 = matches!(inputs.first(), Some(Value::Bool(true)));
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
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(if self.0 {
                        wgpu::Color::RED
                    } else {
                        wgpu::Color::BLUE
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        false
    }
}

pub fn surface() -> Box<dyn Surface> {
    Box::new(Sky(false))
}
