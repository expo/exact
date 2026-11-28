//! Reusable native capture storage. A canvas keeps one target and staging
//! buffer; map completion is still synchronous, so no frame can overwrite it.
use crate::*;

pub(crate) struct Readback {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    rows: Option<Rows>,
}

impl Readback {
    fn new(gpu: &Gpu, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("canvas readback"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        Self {
            texture,
            view,
            rows: None,
        }
    }
}

/// A copy scheduled before presentation. Only a pixel read maps the buffer.
#[derive(Clone)]
pub(crate) struct Rows {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    row: u32,
    sample_flags: u32,
}

impl Rows {
    fn stride(format: wgpu::TextureFormat) -> u32 {
        format
            .block_copy_size(None)
            .expect("an uncompressed color target")
    }

    fn new(gpu: &Gpu, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        let stride = Self::stride(format);
        let row = (width * stride).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canvas readback staging"),
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            width,
            height,
            format,
            buffer,
            row,
            sample_flags: 0,
        }
    }

    fn copy(&self, encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture) {
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.row),
                    rows_per_image: None,
                },
            },
            texture.size(),
        );
    }

    fn read(&self, gpu: &Gpu, out: &mut [u8]) -> Result<(), String> {
        let slice = self.buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let result = (|| {
            gpu.device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|e| format!("poll: {e:?}"))?;
            rx.recv_timeout(std::time::Duration::from_secs(10))
                .map_err(|_| "the map callback did not run".to_string())?
                .map_err(|e| format!("map: {e:?}"))?;
            let mapped = slice
                .get_mapped_range()
                .map_err(|e| format!("map: {e:?}"))?;
            let width = (self.width * Self::stride(self.format)) as usize;
            let bgra = matches!(
                self.format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            );
            for (dst, src) in out
                .chunks_exact_mut(width)
                .zip(mapped.chunks_exact(self.row as usize))
            {
                if bgra {
                    for (d, s) in std::iter::zip(dst.as_chunks_mut::<4>().0, src.as_chunks::<4>().0)
                    {
                        d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
                    }
                } else {
                    dst.copy_from_slice(&src[..width]);
                }
            }
            Ok(())
        })();
        self.buffer.unmap();
        result
    }

    pub(crate) fn capture(
        previous: Option<&Rows>,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) -> Self {
        let (w, h, format) = (texture.width(), texture.height(), texture.format());
        let rows = previous
            .filter(|s| (s.width, s.height, s.format) == (w, h, format))
            .cloned()
            .unwrap_or_else(|| Self::new(gpu, w, h, format));
        rows.copy(encoder, texture);
        rows
    }
    /// The compositor interprets these bytes using the configuration of this frame.
    pub(crate) fn presented(mut self, primaries: ColorSpace, opaque: bool) -> Self {
        self.sample_flags = u32::from(self.format == wgpu::TextureFormat::Rgba16Float)
            | (u32::from(primaries == ColorSpace::DisplayP3) << 1)
            | (u32::from(opaque) << 2);
        self
    }
}

impl Module {
    pub(crate) fn sampled_frame(&mut self, id: u32) -> Option<([u32; 3], Vec<u8>)> {
        self.settle(id);
        self.check_device();
        let inst = self.instances.get(&id)?;
        if inst.bound && inst.surface.is_empty() {
            return Some(([1, 1, 0], vec![0; 4]));
        }
        let rows = inst.sampled.as_ref()?;
        let mut bytes =
            vec![0; (rows.width * Rows::stride(rows.format)) as usize * rows.height as usize];
        let result = rows.read(self.gpu.as_ref()?, &mut bytes);
        let size = [rows.width, rows.height, rows.sample_flags];
        if let Err(error) = result {
            return self.fail(error);
        }
        Some((size, bytes))
    }

    /// Render and capture directly into caller-owned RGBA bytes, reusing GPU
    /// storage until the size, format, or device changes, or the surface clears.
    pub fn readback_into(&mut self, id: u32, frame: &Frame, out: &mut [u8]) -> Option<bool> {
        self.render_readback(id, frame, Some(out))
    }

    // Without a drawable, an agent frame is presented to its offscreen target.
    // Keep its negotiated format, primaries and headroom for the sample.
    pub(crate) fn render_readback(
        &mut self,
        id: u32,
        frame: &Frame,
        out: Option<&mut [u8]>,
    ) -> Option<bool> {
        if let Some(error) = self.refused_instance(id) {
            return self.fail(error);
        }
        self.check_device();
        #[cfg(windows)]
        if let Some(window) = self.window.as_mut().filter(|w| w.canvas == id) {
            window.rested = None;
        }
        if let Some(why) = shaders::missing(self.registry.shaders) {
            self.error = why;
            return None;
        }
        self.flush();
        let scale = if out.is_none() {
            frame.scale * self.render_scale(id)
        } else {
            frame.scale
        };
        let gpu = self.gpu.as_ref()?;
        let Some(inst) = self.instances.get_mut(&id) else {
            self.error = "no such canvas".into();
            return None;
        };
        if !inst.bound {
            return None;
        }
        let format = inst
            .config
            .as_ref()
            .map_or_else(|| inst.offscreen_format(), crate::view_format);
        if out.is_some()
            && !matches!(
                format,
                wgpu::TextureFormat::Rgba8Unorm
                    | wgpu::TextureFormat::Rgba8UnormSrgb
                    | wgpu::TextureFormat::Bgra8Unorm
                    | wgpu::TextureFormat::Bgra8UnormSrgb
            )
        {
            self.error = format!("{format:?} is not an RGBA8 or BGRA8 format");
            return None;
        }
        let present = out.is_none();
        let frame = Frame {
            scale,
            seekable: self.seekable,
            period_ms: self.period_ms,
            children_generation: inst.children_generation,
            shader_generation: shaders::shader_generation(),
            headroom: if out.is_none() && inst.hdr {
                inst.headroom
            } else {
                1.
            },
            ..*frame
        };
        let (width, height) = frame.pixels();
        let len = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if let Some(out) = out.as_ref().filter(|out| out.len() < len) {
            self.error = format!("readback: {} bytes for {} pixels", out.len(), len / 4);
            return None;
        }
        if inst.surface.is_empty() {
            let had_wake = inst.wake.take();
            if inst.surface.is_empty() {
                if let Some(out) = out {
                    out[..len].fill(0);
                }
                inst.input_dirty = false;
                inst.dirty = false;
                inst.drain();
                return Some(inst.wake.is_woken());
            }
            if had_wake {
                inst.wake.rearm();
            }
        }
        let mut capture_target = ColorTarget::readback(format);
        let presentation_target = inst.config.as_ref().map_or(capture_target, ColorTarget::of);
        if present {
            capture_target.space = presentation_target.space;
            // Hidden windows can bind new alpha before configuring their next drawable.
            capture_target.alpha = if inst.config.is_some() {
                target::alpha_mode(inst.surface.opaque(), &inst.alpha_modes)
            } else if inst.surface.opaque() {
                wgpu::CompositeAlphaMode::Opaque
            } else {
                capture_target.alpha
            };
        }
        let restore = |inst: &mut crate::Instance| {
            if !present && presentation_target.space != ColorSpace::Srgb {
                inst.surface.color_space_granted(presentation_target.space);
            }
            if !inst.bound || (inst.config.is_some() && inst.destination != presentation_target) {
                inst.set_color_target(presentation_target)?;
            }
            Ok::<_, SurfaceError>(())
        };
        // Byte readback always reports sRGB. A sampled presentation keeps the drawable's primaries.
        if !present {
            inst.surface.color_space_granted(ColorSpace::Srgb);
        }
        if inst.destination != capture_target {
            if let Err(e) = inst.set_color_target(capture_target) {
                let _ = restore(inst);
                self.error = e.0;
                return None;
            }
        }
        inst.surface.prepare_assets(&gpu.device, &gpu.queue, format);
        if inst.readback.as_ref().is_none_or(|r| {
            r.texture.width() != width
                || r.texture.height() != height
                || r.texture.format() != format
        }) {
            inst.readback = Some(Readback::new(gpu, width, height, format));
        }
        let target = inst.readback.as_mut().unwrap();
        let mut recording = crate::frame::Recording::begin(&gpu.device);
        inst.wake.take();
        let wants = inst.surface.render(
            &frame,
            &gpu.device,
            &gpu.queue,
            &mut recording.encoder,
            &target.view,
            format,
        );
        let failure = inst.surface.take_error();
        let ready = inst.surface.frame_presentable();
        let sampled = if present {
            (ready && failure.is_none()).then(|| {
                Rows::capture(
                    inst.sampled.as_ref(),
                    gpu,
                    &mut recording.encoder,
                    &target.texture,
                )
                .presented(
                    capture_target.space,
                    capture_target.alpha == wgpu::CompositeAlphaMode::Opaque,
                )
            })
        } else {
            target.rows = Some(Rows::capture(
                target.rows.as_ref(),
                gpu,
                &mut recording.encoder,
                &target.texture,
            ));
            None
        };
        let commands = match recording.finish() {
            Ok(commands) => commands,
            Err(e) => {
                if let Err(e) = restore(inst) {
                    self.error = e.0;
                    return None;
                }
                inst.drain();
                self.error = failure
                    .or_else(|| inst.surface.take_error())
                    .map_or(e, |SurfaceError(own)| own);
                return None;
            }
        };
        let (_, submit_ms) =
            crate::performance::time(inst.performance.enabled(), || gpu.queue.submit([commands]));
        if let Some(ms) = submit_ms {
            inst.performance.submitted(ms);
        }
        inst.surface.submitted();
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos"))]
        self.pipeline_cache.save_metal();
        let result = match out {
            Some(out) => inst
                .readback
                .as_ref()
                .unwrap()
                .rows
                .as_ref()
                .unwrap()
                .read(gpu, &mut out[..len]),
            None => Ok(()),
        };
        if let Err(e) = restore(inst) {
            self.error = e.0;
            return None;
        }
        inst.drain();
        if let Some(SurfaceError(e)) = failure.or_else(|| inst.surface.take_error()) {
            self.error = e;
            return None;
        }
        match result {
            Ok(()) => {
                if let Some(sampled) = sampled {
                    inst.sampled = Some(sampled);
                }
                inst.input_dirty = inst.surface.input_pending();
                inst.dirty = present && !ready;
                // A canvas with no target of its own is shown by its host
                // composing these pixels (Linux's display baseline), and a
                // seekable agent drive falls back to readback when its window
                // is occluded on macOS: a complete frame is presented. A live
                // canvas with a target is only being captured.
                if (inst.presentation.is_none() || self.seekable)
                    && !inst.has_ring()
                    && inst.surface.frame_ready()
                {
                    inst.presented.now();
                }
                let wants = wants || (!self.seekable && !inst.surface.frame_ready());
                Some(wants || (present && !ready) || inst.wake.is_woken())
            }
            Err(e) => {
                self.error = e;
                None
            }
        }
    }
}

/// Enable copies of presented drawables for canvases this agent host creates.
pub fn enable_frame_samples() {
    native::with(|m| m.frame_samples = true);
}

/// Render an offscreen presentation, or the caller's nested-canvas capture.
pub fn capture(
    id: u32,
    width: f32,
    height: f32,
    scale: f32,
    now_ms: f64,
    out: Option<&mut [u8]>,
) -> u32 {
    let frame = Frame {
        width,
        height,
        scale,
        now_ms,
        children_generation: 0,
        seekable: false,
        period_ms: 0.0,
        shader_generation: 0,
        headroom: 1.0,
    };
    match native::with(|m| m.render_readback(id, &frame, out)).flatten() {
        Some(true) => 2,
        Some(false) => 0,
        None if native::with(|m| m.instances.contains_key(&id) && m.gpu().is_none())
            .unwrap_or(false) =>
        {
            3
        }
        None => 1,
    }
}

/// Pixel dimensions and tightly packed top-down RGBA rows in the configured layer's color space.
pub fn sampled_frame(id: u32) -> Option<([u32; 3], Vec<u8>)> {
    native::with(|m| m.sampled_frame(id)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Solid(bool);
    impl Surface for Solid {
        fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn render(
            &mut self,
            frame: &Frame,
            _: &wgpu::Device,
            _: &wgpu::Queue,
            encoder: &mut wgpu::CommandEncoder,
            view: &wgpu::TextureView,
            _: wgpu::TextureFormat,
        ) -> bool {
            self.0 = frame.now_ms > 31.;
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: frame.now_ms / 255.,
                            g: 0.,
                            b: 1.,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            false
        }
        fn frame_ready(&self) -> bool {
            self.0
        }
        fn frame_presentable(&self) -> bool {
            true
        }
    }
    static REGISTRY: Registry = Registry {
        surfaces: &[("solid", 0, || Box::new(Solid(false)))],
        shaders: &[],
    };

    #[test]
    fn reusable_capture_handles_unaligned_rows_resize_and_short_buffers() {
        let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
            return;
        };
        let mut module = Module::new(&REGISTRY);
        module.set_gpu(gpu);
        let id = module.create_headless("solid").unwrap();
        module.bind(id, &[], None);
        let mut frame = Frame {
            width: 3.,
            height: 2.,
            scale: 1.,
            now_ms: 31.,
            seekable: false,
            period_ms: 0.,
            children_generation: 0,
            shader_generation: 0,
            headroom: 1.,
        };
        for (width, red) in [(3., 31.), (3., 95.), (65., 191.), (1., 255.)] {
            frame.width = width;
            frame.now_ms = red;
            let len = width as usize * 2 * 4;
            let mut output = vec![77; len + 4];
            assert_eq!(
                module.readback_into(id, &frame, &mut output),
                Some(red == 31.)
            );
            assert!(output[..len]
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [red as u8, 0, 255, 255]));
            assert_eq!(&output[len..], &[77; 4], "caller suffix remains untouched");
            let mut short = vec![77; len - 1];
            assert!(module.readback_into(id, &frame, &mut short).is_none());
            assert!(short.iter().all(|&byte| byte == 77), "refusal is atomic");
        }
        module.lose_device();
        assert!(module.instances[&id].readback.is_none());
    }
    #[derive(Default)]
    struct Board {
        fill: wgpu::Color,
        binds: u32,
        srgb_view: bool,
        refuse_next: bool,
        destination: ColorTarget,
    }
    impl Surface for Board {
        fn bind(&mut self, values: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
            if std::mem::take(&mut self.refuse_next) {
                return Err(SurfaceError("board refused the target".into()));
            }
            self.fill = self.destination.clear(json::color(&values[0], "board")?);
            self.binds += 1;
            Ok(())
        }
        fn agent(&mut self, command: &str) -> Option<String> {
            self.refuse_next = command == "refuse next bind";
            Some(self.binds.to_string())
        }
        fn srgb_target(&self) -> bool {
            self.srgb_view
        }
        fn color_target(&mut self, destination: ColorTarget) {
            self.destination = destination;
        }
        fn render(
            &mut self,
            _: &Frame,
            _: &wgpu::Device,
            _: &wgpu::Queue,
            encoder: &mut wgpu::CommandEncoder,
            view: &wgpu::TextureView,
            _: wgpu::TextureFormat,
        ) -> bool {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.fill),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            false
        }
    }

    #[test]
    fn bound_board_colours_clear_both_sdr_attachments_to_the_view_bytes() {
        static BOARDS: Registry = Registry {
            surfaces: &[
                ("unorm", 1, || Box::<Board>::default()),
                ("srgb", 1, || {
                    Box::new(Board {
                        srgb_view: true,
                        ..Default::default()
                    })
                }),
            ],
            shaders: &[],
        };
        let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
            return;
        };
        let mut module = Module::new(&BOARDS);
        module.set_gpu(gpu);
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
        for name in ["unorm", "srgb"] {
            let id = module.create_headless(name).unwrap();
            for (rgb, a, want) in [
                ([236, 236, 239], 1., [236, 236, 239, 255]),
                ([20, 20, 22], 1., [20, 20, 22, 255]),
                ([236, 100, 40], 0.5, [118, 50, 20, 128]),
            ] {
                let [r, g, b] =
                    rgb.map(|v| exact_color::srgb_to_linear(f64::from(v) / 255.) as f32);
                assert!(module.bind_json(id, &format!(r#"[{{"color":[{r},{g},{b},{a}]}}]"#), None));
                let (pixels, _) = module.readback(id, &frame).unwrap();
                assert_eq!(pixels.data, want, "{name}");
            }
            assert_eq!(module.agent(id, "binds").as_deref(), Some("3"));
            module.agent(id, "refuse next bind");
            let inst = module.instances.get_mut(&id).unwrap();
            let other = ColorTarget::readback(wgpu::TextureFormat::Rgba16Float);
            assert!(inst.set_color_target(other).is_err());
            inst.set_color_target(other).unwrap();
            inst.set_color_target(other).unwrap();
            module.detach(id);
            assert_eq!(module.agent(id, "binds").as_deref(), Some("4"));
            assert_eq!(
                module.readback(id, &frame).unwrap().0.data,
                [118, 50, 20, 128]
            );
            assert_eq!(module.agent(id, "binds").as_deref(), Some("5"));
        }
    }
}
