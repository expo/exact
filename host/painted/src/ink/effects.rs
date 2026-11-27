//! Composition stays on the device; only finish reads pixels. @ref LLP 1149 D2.5, D2.7.
use super::clips::Tile;
use super::*;
use exact_ink::instance::{Clip, Scissor};

/// The width of each of the three box blurs that stand for a Gaussian of `sigma`
/// pixels (Filter Effects 1, feGaussianBlur). An even width is the first two of
/// its three; the third is one wider.
fn box_width(sigma: f32) -> u32 {
    let d = sigma * 3. * (2. * std::f32::consts::PI).sqrt() / 4. + 0.5;
    d.floor().max(1.) as u32
}

fn boxes(sigma: f32) -> [u32; 3] {
    let d = box_width(sigma);
    [d, d, d + (d + 1) % 2]
}

/// The variance of a box blur of each width after the other, in pixels squared.
fn variance(widths: [u32; 3]) -> f32 {
    widths.iter().map(|w| (w * w - 1) as f32 / 12.).sum()
}

/// What averaging `factor` pixels into one and enlarging the result with linear
/// filtering add to a blur's variance, in the pixels reduced.
fn resampled(factor: f32) -> f32 {
    if factor == 1. {
        0.
    } else {
        (factor * factor - 1.) / 12. + factor * factor / 6.
    }
}

/// The widest blur run on unreduced pixels. A blur's cost is its pixels times its
/// taps, so each halving cuts it eightfold. The sigma left after halving is over
/// 4 pixels: under that the box widths are too coarse to hold the CPU painter's
/// blur within three levels, and stripes near the reduced grid's pitch flicker as
/// they move.
const HALVE_ABOVE: f32 = 8.;

/// A backdrop blur of `sigma` device pixels: the times to halve the backdrop, and
/// the box blurs to run on what is left, as the pixels each reaches to either side.
/// Under two pixels the one pass is a Gaussian's, with `sigma` its own.
struct Blur {
    halvings: u32,
    /// The pixels averaged into one of those blurred, along an axis.
    reduced: f32,
    sigma: f32,
    passes: [(f32, f32); 3],
    count: usize,
}

fn blur(sigma: f32) -> Blur {
    // The CPU painter halves above 12 (`exact_svg_raster::backdrop_blur`) and takes
    // its boxes from what is left. Its blur's variance is the one to draw.
    let (mut reference, mut factor) = (sigma, 1.);
    while reference > 12. {
        reference *= 0.5;
        factor *= 2.;
    }
    let wanted = factor * factor * variance(boxes(reference)) + resampled(factor);
    let (mut sigma, mut halvings, mut reduced) = (sigma, 0, 1.);
    while sigma > HALVE_ABOVE {
        sigma *= 0.5;
        halvings += 1;
        reduced *= 2.;
    }
    if sigma < 2. {
        let r = (3. * sigma).ceil();
        return Blur {
            halvings,
            reduced,
            sigma,
            passes: [(r, r); 3],
            count: 1,
        };
    }
    let mut widths = boxes(sigma);
    if reduced != factor {
        // Halved further than the CPU painter, the formula's widths would draw a
        // narrower blur: take the three nearest its variance. Two even widths or
        // none keep the blur centred.
        let wanted = (wanted - resampled(reduced)) / (reduced * reduced);
        let near = box_width(sigma);
        let mut best = f32::INFINITY;
        for w in near.saturating_sub(4).max(1)..=near + 4 {
            let candidates: &[[u32; 3]] = if w % 2 == 1 {
                &[[w, w, w], [w, w, w + 2], [w, w + 2, w + 2]]
            } else {
                &[[w, w, w - 1], [w, w, w + 1]]
            };
            for &candidate in candidates {
                let off = (variance(candidate) - wanted).abs();
                if off < best {
                    (best, widths) = (off, candidate);
                }
            }
        }
    }
    // An even box reaches a pixel further to one side: the first two take turns.
    let radii = |at: usize| {
        let (w, r) = (widths[at], (widths[at] / 2) as f32);
        match (w % 2, at) {
            (1, _) => (r, r),
            (_, 0) => (r, r - 1.),
            _ => (r - 1., r),
        }
    };
    Blur {
        halvings,
        reduced,
        sigma,
        passes: [radii(0), radii(1), radii(2)],
        count: 3,
    }
}

fn sampling_reach(filter: &exact_kernel::style::BackdropFilter, scale: f32) -> f32 {
    filter
        .0
        .iter()
        .map(|op| {
            let exact_kernel::style::BackdropOp::Blur(sigma) = *op else {
                return 0.;
            };
            let sigma = sigma * scale;
            if !sigma.is_finite() || sigma <= 0. {
                return 0.;
            }
            let blur = blur(sigma);
            let factor = blur.reduced;
            let passes = blur.passes.iter().take(blur.count);
            let radius: f32 = passes.map(|(l, r)| l.max(*r)).sum();
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
    pub(super) fn end_frame(&mut self) -> Result<TextureId> {
        let target = self.flush()?;
        self.images.end(&mut self.device)?;
        self.pool.end(&mut self.device)?;
        Ok(target)
    }

    pub(super) fn flush(&mut self) -> Result<TextureId> {
        let drawn = self.draw(false)?;
        Ok(drawn.expect("a pass that is not a group's last").texture)
    }

    /// Draw what the encoder holds into the target being drawn. A group's first pass
    /// makes its target: the frame's size, or, where the pass is the group's `last`,
    /// about the size of what it draws, and none where it draws nothing.
    pub(super) fn draw(&mut self, last: bool) -> Result<Option<Tile>> {
        if let Some(e) = &self.error {
            return Err(Error::Device(e.clone()));
        }
        let started = Instant::now();
        self.glyphs.flush(&mut self.device)?;
        let frame = self.encoder.take_frame();
        let viewport = Scissor::full(self.size);
        let mut drawn = None;
        if let Some(group) = self
            .groups
            .last_mut()
            .filter(|group| group.can_bound_composite())
        {
            group.drawn = frame.draw_bounds(group.drawn, viewport);
            drawn = Some(group.drawn);
        }
        // These read a texture at the frame's coordinates, which a smaller target moves.
        let positioned = flags::LAYER_MASK | flags::REPLACE | flags::BLEND_MASK;
        let fits = last
            && self.draw_target.is_none()
            && !frame.instances.iter().any(|i| i.flags & positioned != 0);
        let tile = match drawn.filter(|_| fits) {
            Some(drawn) => {
                self.draw_clear = None;
                // Keep the neighboring texels used by linear filtering.
                let region = drawn.map(|d| d.outset(1).intersect(viewport));
                region.map(|r| self.render_tile(frame, r)).transpose()?
            }
            None => {
                let texture = match self.draw_target {
                    Some(t) => t,
                    None => {
                        let t = self.pool.take(&mut self.device)?;
                        self.draw_target = Some(t);
                        t
                    }
                };
                let clear = self.draw_clear.take();
                debug_assert!(drawn.is_none() || clear.is_none() || clear == Some([0.; 4]));
                self.device.render(&frame, texture, clear)?;
                Some(Tile {
                    texture,
                    region: viewport,
                })
            }
        };
        self.layer_ms += started.elapsed().as_secs_f64() * 1000.;
        Ok(tile)
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

    fn destination(&mut self, region: Scissor) -> Result<Tile> {
        let target = self.flush()?;
        let started = Instant::now();
        let size = [region.width, region.height];
        let copy = self.pool.take_sized(&mut self.device, size)?;
        self.device
            .copy_region(target, [region.x, region.y], copy, [0, 0], size)?;
        self.layer_ms += started.elapsed().as_secs_f64() * 1000.;
        Ok(Tile {
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
        self.emit_bounded(i, b, bounds)
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
        let texture = self.images.pixmap(&mut self.device, pixels, true)?;
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

    /// A target for the frame's pixels in `region`, which is whole pixels.
    fn region_target(&mut self, region: [f32; 4]) -> Result<Tile> {
        let [x, y, width, height] = region.map(|v| v as u32);
        let (texture, [width, height]) =
            self.pool.take_fitting(&mut self.device, [width, height])?;
        Ok(Tile {
            texture,
            region: Scissor {
                x,
                y,
                width,
                height,
            },
        })
    }

    /// A target of `size` whose pixels are a reduced grid's, from that grid's origin.
    fn reduced_target(&mut self, size: [u32; 2]) -> Result<Tile> {
        Ok(Tile {
            texture: self.pool.take_sized(&mut self.device, size)?,
            region: Scissor::full(size),
        })
    }

    /// Draw `kind` of `source` over `area` into `target`. `area` is in the
    /// coordinates the two tiles' regions are in.
    fn effect(
        &mut self,
        source: Tile,
        area: [f32; 4],
        kind: Kind,
        param: [f32; 4],
        target: Tile,
    ) -> Result<Tile> {
        let started = Instant::now();
        let to = target.region;
        let mut e = Encoder::new(to.width, to.height);
        // The shader reads `area` and the pixel it draws in the target's own coordinates.
        let [x, y, width, height] = area;
        let mut i = Instance::new(
            kind,
            Brush::Solid,
            [x - to.x as f32, y - to.y as f32, width, height],
        );
        i.param = param;
        i.uv = [
            to.x as f32 - source.region.x as f32,
            to.y as f32 - source.region.y as f32,
            1.,
            1.,
        ];
        e.push(
            i,
            Bindings {
                image: source.texture,
                ..Default::default()
            },
        )
        .map_err(|_| Error::Invalid("effect clip".into()))?;
        self.device
            .render(&e.finish(), target.texture, Some([0.; 4]))?;
        self.layer_ms += started.elapsed().as_secs_f64() * 1000.;
        Ok(target)
    }

    /// Draw the part `uv` of `source` over `area`, which is in the coordinates of
    /// `target`'s region. `reduce` averages the source pixels under each pixel
    /// drawn; otherwise the source is filtered.
    fn resample(
        &mut self,
        source: Tile,
        area: [f32; 4],
        uv: [f32; 4],
        target: Tile,
        reduce: bool,
    ) -> Result<Tile> {
        let to = target.region;
        let mut encoder = Encoder::new(to.width, to.height);
        let kind = if reduce {
            Kind::Downsample
        } else {
            Kind::Layer
        };
        let mut i = Instance::new(kind, Brush::Image, area);
        i.uv = uv;
        encoder
            .push(
                i.translated([-(to.x as f32), -(to.y as f32)]),
                Bindings {
                    image: source.texture,
                    ..Default::default()
                },
            )
            .map_err(|_| Error::Invalid("resample clip".into()))?;
        self.device
            .render(&encoder.finish(), target.texture, Some([0.; 4]))?;
        Ok(target)
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
                    let target = self.region_target(region)?;
                    let param = [amount, 0., 0., 0.];
                    source = self.effect(source, region, Kind::ColorPass, param, target)?;
                }
                exact_kernel::style::BackdropOp::Blur(sigma) => {
                    let sigma = sigma * scale;
                    if !sigma.is_finite() || sigma <= 0. {
                        continue;
                    }
                    let blur = blur(sigma);
                    // The blur's grid: the frame's pixels, or `region` reduced.
                    let mut size = None;
                    let mut area = region;
                    let mut uv = [
                        (region[0] - source.region.x as f32) / source.region.width as f32,
                        (region[1] - source.region.y as f32) / source.region.height as f32,
                        region[2] / source.region.width as f32,
                        region[3] / source.region.height as f32,
                    ];
                    // Halving bounds each area pass to at most nine source pixels.
                    for _ in 0..blur.halvings {
                        let half = [
                            (area[2] / 2.).ceil().max(1.) as u32,
                            (area[3] / 2.).ceil().max(1.) as u32,
                        ];
                        let next = [0., 0., half[0] as f32, half[1] as f32];
                        let target = self.reduced_target(half)?;
                        source = self.resample(source, next, uv, target, true)?;
                        size = Some(half);
                        area = next;
                        uv = [0., 0., 1., 1.];
                    }
                    for vertical in [0., 1.] {
                        for &(left, right) in blur.passes.iter().take(blur.count) {
                            let target = match size {
                                Some(size) => self.reduced_target(size)?,
                                None => self.region_target(region)?,
                            };
                            let gaussian = if blur.count == 1 { blur.sigma } else { 0. };
                            let param = [left, right, vertical, gaussian];
                            source = self.effect(source, area, Kind::Blur, param, target)?;
                        }
                    }
                    if size.is_some() {
                        let target = self.region_target(region)?;
                        source = self.resample(source, region, [0., 0., 1., 1.], target, false)?;
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
        self.emit_bounded(i, bindings, visible)
    }
}
