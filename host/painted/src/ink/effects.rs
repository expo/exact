//! Composition stays on the device; only finish reads pixels. @ref LLP 1149 D2.5, D2.7.
use super::*;
use exact_ink::instance::{Clip, Scissor};

#[derive(Clone, Copy)]
struct Snapshot {
    texture: TextureId,
    region: Scissor,
}

fn blur_passes(sigma: f32) -> impl Iterator<Item = (f32, f32)> {
    let (passes, count) = if sigma < 2. {
        let r = (3. * sigma).ceil();
        ([(r, r); 3], 1)
    } else {
        let d = (sigma * 3. * (2. * std::f32::consts::PI).sqrt() / 4. + 0.5)
            .floor()
            .max(1.) as u32;
        let r = (d / 2) as f32;
        (
            if d % 2 == 1 {
                [(r, r); 3]
            } else {
                [(r, r - 1.), (r - 1., r), (r, r)]
            },
            3,
        )
    };
    passes.into_iter().take(count)
}

fn sampling_reach(filter: &exact_kernel::style::BackdropFilter, scale: f32) -> f32 {
    filter
        .0
        .iter()
        .map(|op| {
            let exact_kernel::style::BackdropOp::Blur(sigma) = *op else {
                return 0.;
            };
            let mut sigma = sigma * scale;
            if !sigma.is_finite() || sigma <= 0. {
                return 0.;
            }
            let mut factor = 1.;
            while sigma > 12. {
                sigma *= 0.5;
                factor *= 2.;
            }
            let radius: f32 = blur_passes(sigma).map(|(l, r)| l.max(r)).sum();
            // Each reduction covers at most `factor` input pixels. The final
            // bilinear enlargement reaches one more reduced pixel on either side.
            if factor > 1. {
                (radius + 2.) * factor
            } else {
                radius
            }
        })
        .sum()
}

impl Ink {
    pub(super) fn flush(&mut self) -> Result<TextureId> {
        if let Some(e) = &self.error {
            return Err(Error::Device(e.clone()));
        }
        let started = Instant::now();
        let target = match self.draw_target {
            Some(t) => t,
            None => {
                let t = self.pool.take(&mut self.device)?;
                self.draw_target = Some(t);
                t
            }
        };
        self.glyphs.flush(&mut self.device)?;
        let frame = self.encoder.take_frame();
        self.device.render(&frame, target, self.draw_clear.take())?;
        self.layer_ms += started.elapsed().as_secs_f64() * 1000.;
        Ok(target)
    }

    pub(super) fn masked(
        &mut self,
        shape: &Shape,
        mask: &std::result::Result<GradientPaint, [u8; 4]>,
        ts: Transform,
    ) -> Result<()> {
        let saved = std::mem::replace(&mut self.encoder, Encoder::new(self.size[0], self.size[1]));
        match mask {
            Ok(g) => self.fill_gradient(shape, g, ts),
            Err(c) => self.fill(shape, *c, ts),
        }
        let frame = std::mem::replace(&mut self.encoder, saved).finish();
        let started = Instant::now();
        let target = self.pool.take(&mut self.device)?;
        self.device.render(&frame, target, Some([0.; 4]))?;
        self.layer_ms += started.elapsed().as_secs_f64() * 1000.;
        self.close_masked_group(Some(target))
    }

    fn destination(&mut self, region: Scissor) -> Result<Snapshot> {
        let target = self.flush()?;
        let started = Instant::now();
        let size = [region.width, region.height];
        let copy = self.pool.take_sized(&mut self.device, size)?;
        self.device
            .copy_region(target, [region.x, region.y], copy, [0, 0], size)?;
        self.layer_ms += started.elapsed().as_secs_f64() * 1000.;
        Ok(Snapshot {
            texture: copy,
            region,
        })
    }

    fn emit_bounded(&mut self, i: Instance, b: Bindings, bounds: Scissor) -> Result<()> {
        self.encoder
            .push_clip(Clip::Scissor(bounds))
            .map_err(|_| Error::Invalid("effect scissor".into()))?;
        let result = self.emit(i, b);
        self.encoder.pop_clip();
        result
    }

    pub(super) fn blend_instance(&mut self, mut i: Instance, mut b: Bindings) -> Result<()> {
        if i.flags & flags::BLEND_MASK == 0 || !self.rows.capture.is_empty() {
            return self.emit(i, b);
        }
        let bounds = self.encoder.bounds(&i);
        if bounds.width == 0 || bounds.height == 0 {
            return Ok(());
        }
        let destination = self.destination(bounds)?;
        i.param = [bounds.x as f32, bounds.y as f32, 0., 0.];
        b.destination = destination.texture;
        self.emit_bounded(i, b, bounds)?;
        // A following blend must see this result, including within an opacity root.
        self.flush()?;
        Ok(())
    }

    pub(super) fn blended_image(
        &mut self,
        pixels: &Arc<Pixmap>,
        dst: Rect4,
        ts: Transform,
        mode: u8,
    ) -> Result<()> {
        if dst.2 <= 0. || dst.3 <= 0. {
            return Ok(());
        }
        let texture = self.images.pixmap(&mut self.device, pixels)?;
        let mut i = Instance::new(Kind::RoundedRect, Brush::Image, rect(dst));
        i.xform = affine(self.device_transform(ts));
        i.flags |= u32::from(mode.min(15)) << flags::BLEND_SHIFT;
        self.blend_instance(
            i,
            Bindings {
                image: texture,
                ..Default::default()
            },
        )?;
        Ok(())
    }

    fn effect(
        &mut self,
        source: Snapshot,
        region: [f32; 4],
        kind: Kind,
        param: [f32; 4],
    ) -> Result<Snapshot> {
        self.effect_sized(source, region, kind, param, self.size)
    }

    fn effect_sized(
        &mut self,
        source: Snapshot,
        region: [f32; 4],
        kind: Kind,
        param: [f32; 4],
        size: [u32; 2],
    ) -> Result<Snapshot> {
        let started = Instant::now();
        let target = self.pool.take_sized(&mut self.device, size)?;
        let mut e = Encoder::new(size[0], size[1]);
        let mut i = Instance::new(kind, Brush::Solid, region);
        i.param = param;
        i.uv = [-(source.region.x as f32), -(source.region.y as f32), 1., 1.];
        e.push(
            i,
            Bindings {
                image: source.texture,
                ..Default::default()
            },
        )
        .map_err(|_| Error::Invalid("effect clip".into()))?;
        self.device.render(&e.finish(), target, Some([0.; 4]))?;
        self.layer_ms += started.elapsed().as_secs_f64() * 1000.;
        Ok(Snapshot {
            texture: target,
            region: Scissor {
                x: 0,
                y: 0,
                width: size[0],
                height: size[1],
            },
        })
    }

    fn resample(
        &mut self,
        source: Snapshot,
        region: [f32; 4],
        uv: [f32; 4],
        size: [u32; 2],
        area: bool,
    ) -> Result<Snapshot> {
        let target = self.pool.take_sized(&mut self.device, size)?;
        let mut encoder = Encoder::new(size[0], size[1]);
        let mut i = Instance::new(
            if area { Kind::Downsample } else { Kind::Layer },
            Brush::Image,
            region,
        );
        i.uv = uv;
        encoder
            .push(
                i,
                Bindings {
                    image: source.texture,
                    ..Default::default()
                },
            )
            .map_err(|_| Error::Invalid("resample clip".into()))?;
        self.device
            .render(&encoder.finish(), target, Some([0.; 4]))?;
        Ok(Snapshot {
            texture: target,
            region: Scissor {
                x: 0,
                y: 0,
                width: size[0],
                height: size[1],
            },
        })
    }

    pub(super) fn backdrop(
        &mut self,
        shape: &Shape,
        filter: &exact_kernel::style::BackdropFilter,
        ts: Transform,
    ) -> Result<()> {
        let dev = self.device_transform(ts);
        let Some(path) = shape_path(shape).and_then(|p| p.transform(dev)) else {
            return Ok(());
        };
        if dev.invert().is_none() {
            return Ok(());
        }
        let b = path.bounds();
        let x = b.left().floor().max(0.);
        let y = b.top().floor().max(0.);
        let right = b.right().ceil().min(self.size[0] as f32);
        let bottom = b.bottom().ceil().min(self.size[1] as f32);
        if right <= x || bottom <= y {
            return Ok(());
        }
        let region = [x, y, right - x, bottom - y];
        let Some((mut i, mut bindings, _)) = self.shape_instance(shape, ts)? else {
            return Ok(());
        };
        let visible = self.encoder.bounds(&i);
        if visible.width == 0 || visible.height == 0 {
            return Ok(());
        }
        let scale = (dev.sx * dev.sy - dev.kx * dev.ky).abs().sqrt();
        // Replacement samples the filtered texture bilinearly.
        let reach = (sampling_reach(filter, scale) + 1.).min(self.size[0].max(self.size[1]) as f32);
        let [vx, vy, vw, vh] = visible.rect();
        let samples =
            Scissor::from_rect([vx - reach, vy - reach, vw + reach * 2., vh + reach * 2.])
                .intersect(Scissor::from_rect(region));
        // The replacement also reads the unfiltered destination at the quad's
        // antialiased fringe, outside the filter's mirrored sampling domain.
        let left = visible.x.min(samples.x);
        let top = visible.y.min(samples.y);
        let original = self.destination(Scissor {
            x: left,
            y: top,
            width: (visible.x + visible.width).max(samples.x + samples.width) - left,
            height: (visible.y + visible.height).max(samples.y + samples.height) - top,
        })?;
        let mut source = original;
        for op in &filter.0 {
            match *op {
                exact_kernel::style::BackdropOp::Saturate(amount) => {
                    source = self.effect(source, region, Kind::ColorPass, [amount, 0., 0., 0.])?;
                }
                exact_kernel::style::BackdropOp::Blur(sigma) => {
                    let mut sigma = sigma * scale;
                    if !sigma.is_finite() || sigma <= 0. {
                        continue;
                    }
                    let mut size = self.size;
                    let mut area = region;
                    let mut uv = [
                        (region[0] - source.region.x as f32) / source.region.width as f32,
                        (region[1] - source.region.y as f32) / source.region.height as f32,
                        region[2] / source.region.width as f32,
                        region[3] / source.region.height as f32,
                    ];
                    let reduced = sigma > 12.;
                    // Halving bounds each area pass to at most nine source pixels.
                    while sigma > 12. {
                        size = [
                            (area[2] / 2.).ceil().max(1.) as u32,
                            (area[3] / 2.).ceil().max(1.) as u32,
                        ];
                        let next = [0., 0., size[0] as f32, size[1] as f32];
                        source = self.resample(source, next, uv, size, true)?;
                        sigma *= 0.5;
                        area = next;
                        uv = [0., 0., 1., 1.];
                    }
                    for vertical in [0., 1.] {
                        for (left, right) in blur_passes(sigma) {
                            source = self.effect_sized(
                                source,
                                area,
                                Kind::Blur,
                                [left, right, vertical, if sigma < 2. { sigma } else { 0. }],
                                size,
                            )?;
                        }
                    }
                    if reduced {
                        source =
                            self.resample(source, region, [0., 0., 1., 1.], self.size, false)?;
                    }
                }
            }
        }
        i.kind_brush = (i.kind_brush & 65535) | ((Brush::Image as u32) << 16);
        i.flags |= flags::REPLACE;
        i.param = [
            original.region.x as f32,
            original.region.y as f32,
            source.region.x as f32,
            source.region.y as f32,
        ];
        bindings.image = source.texture;
        bindings.destination = original.texture;
        self.emit_bounded(i, bindings, visible)?;
        self.flush()?;
        Ok(())
    }
}
