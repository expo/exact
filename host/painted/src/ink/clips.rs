use super::*;
use exact_ink::instance::{Clip, CoverageClip, Frame, Scissor};
use exact_ink::path::Coverage;

/// A target that holds the frame's pixels in `region`, the first of them at its
/// origin. It can be larger than what was drawn into it, and is clear there.
#[derive(Clone, Copy)]
pub(super) struct Tile {
    pub(super) texture: TextureId,
    pub(super) region: Scissor,
}

pub(super) struct Group {
    parent: Encoder,
    target: Option<TextureId>,
    clear: Option<[f64; 4]>,
    opacity: f32,
    pub(super) transform: Option<Transform>,
    pub(super) drawn: Option<Scissor>,
}

impl Group {
    pub(super) fn can_bound_composite(&self) -> bool {
        self.transform.is_none_or(|ts| ts.is_identity()) && self.opacity.is_finite()
    }
}

impl Ink {
    pub(super) fn open_group(&mut self, opacity: f32) {
        let next = self.encoder.fork();
        let parent = std::mem::replace(&mut self.encoder, next);
        self.groups.push(Group {
            parent,
            target: self.draw_target.take(),
            clear: self.draw_clear.replace([0.; 4]),
            opacity,
            transform: None,
            drawn: None,
        });
    }

    /// Flatten overflowing clips into one mask, so every child sees their intersection.
    pub(super) fn combined_clip(&mut self, mask: Option<Coverage>) -> Result<CoverageClip> {
        let viewport = Scissor::full(self.size);
        let bounds = self.encoder.clip_bounds();
        let bounds = mask.map_or(bounds, |mask| {
            bounds.intersect(Scissor::from_rect(mask.rect))
        });
        if bounds.width == 0 || bounds.height == 0 {
            // Nothing shows through an empty clip, so no mask is drawn for it.
            return Ok(CoverageClip {
                texture: TextureId::default(),
                rect: bounds.rect(),
                uv: [0.; 4],
            });
        }
        // Keep the texels next to the mask bounds for linear filtering.
        let region = bounds.outset(1).intersect(viewport);
        let mut encoder = self.encoder.fork();
        let mask_in_first_draw = mask.is_some_and(|mask| {
            if encoder.push_clip(Clip::Coverage(mask_clip(mask))).is_ok() {
                true
            } else {
                encoder.pop_clip();
                false
            }
        });
        // Outset, so the fill's own antialiased corners fall outside the target.
        let [_, _, width, height] = viewport.rect();
        let cover = [-2., -2., width + 4., height + 4.];
        encoder
            .push(
                Instance::new(Kind::RoundedRect, Brush::Solid, cover),
                Bindings::default(),
            )
            .map_err(|_| Error::Invalid("parent clip mask".into()))?;
        let parent = self.render_mask(encoder, region)?;
        let tile = if let Some(mask) = mask.filter(|_| !mask_in_first_draw) {
            let mut encoder = Encoder::new(self.size[0], self.size[1]);
            let mut instance = Instance::new(Kind::Layer, Brush::Image, parent.region.rect());
            instance.set_mask(mask_clip(mask));
            encoder
                .push(
                    instance,
                    Bindings {
                        image: parent.texture,
                        mask: mask.entry.texture,
                        ..Default::default()
                    },
                )
                .map_err(|_| Error::Invalid("clip intersection".into()))?;
            self.render_mask(encoder, region)?
        } else {
            parent
        };
        let [x, y, width, height] = tile.region.rect();
        Ok(CoverageClip {
            texture: tile.texture,
            rect: bounds.rect(),
            uv: [
                (bounds.x as f32 - x) / width,
                (bounds.y as f32 - y) / height,
                bounds.width as f32 / width,
                bounds.height as f32 / height,
            ],
        })
    }

    fn render_mask(&mut self, encoder: Encoder, region: Scissor) -> Result<Tile> {
        let tile = self.render_tile(encoder.finish(), region)?;
        exact_ink::counters::add(exact_ink::counters::Counter::ClipMasks, 1);
        Ok(tile)
    }

    /// Draw the part of `frame` inside `region` into a clear target of about the
    /// region's size. `frame` holds nothing that reads a texture by the frame's
    /// coordinates (`LAYER_MASK`, `REPLACE` or a blend).
    pub(super) fn render_tile(&mut self, mut frame: Frame, region: Scissor) -> Result<Tile> {
        let size = [region.width, region.height];
        let (texture, [width, height]) = self.pool.take_fitting(&mut self.device, size)?;
        let offset = [-(region.x as f32), -(region.y as f32)];
        for instance in &mut frame.instances {
            *instance = instance.translated(offset);
        }
        for batch in &mut frame.batches {
            let scissor = batch.scissor.intersect(region);
            batch.scissor = Scissor {
                x: scissor.x - region.x,
                y: scissor.y - region.y,
                ..scissor
            };
        }
        self.device.render(&frame, texture, Some([0.; 4]))?;
        Ok(Tile {
            texture,
            region: Scissor {
                width,
                height,
                ..region
            },
        })
    }

    pub(super) fn open_clip_group(&mut self, mask: Coverage) -> Result<()> {
        let clip = self.combined_clip(Some(mask))?;
        self.encoder.push_combined_clip(clip);
        Ok(())
    }

    pub(super) fn close_group(&mut self) -> Result<()> {
        self.close_masked_group(None)
    }

    pub(super) fn close_masked_group(&mut self, alpha_mask: Option<TextureId>) -> Result<()> {
        if self.groups.is_empty() {
            return Ok(());
        }
        // The mask is read by the frame's coordinates, so its layer has the frame's.
        let layer = self.draw(alpha_mask.is_none())?;
        let group = self.groups.pop().unwrap();
        let can_bound = group.can_bound_composite();
        self.encoder = group.parent;
        self.draw_target = group.target;
        self.draw_clear = group.clear;
        let Some(layer) = layer else {
            return Ok(());
        };
        let mut instance = Instance::new(Kind::Layer, Brush::Image, layer.region.rect());
        instance.param[0] = group.opacity;
        if let Some(ts) = group.transform {
            instance.xform = affine(ts);
        }
        let mut bindings = Bindings {
            image: layer.texture,
            ..Default::default()
        };
        if let Some(mask) = alpha_mask {
            instance.flags |= flags::LAYER_MASK;
            bindings.color = mask;
        }
        let viewport = Scissor::full(self.size);
        let bounds = if can_bound {
            let Some(drawn) = group.drawn else {
                return Ok(());
            };
            // Keep the neighboring texels used by linear filtering.
            drawn.outset(1).intersect(viewport)
        } else {
            viewport
        };
        self.encoder
            .push_unclipped_in(instance, bindings, bounds)
            .map_err(|_| Error::Invalid("group composition".into()))
    }

    pub(super) fn clip(&mut self, shape: &Shape, ts: Transform) -> Result<()> {
        let ts = self.record_transform(ts);
        if self.record(rows::Marker::Clip(*shape, ts)) {
            return Ok(());
        }
        if shape.rect.2 <= 0. || shape.rect.3 <= 0. {
            self.encoder
                .push_clip(Clip::Scissor(Scissor {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                }))
                .map_err(|_| Error::Invalid("empty clip".into()))?;
            self.clips.push(());
            return Ok(());
        }
        let dev = self.device_transform(ts);
        if let Some(clip) = simple(shape, dev, self.size) {
            if self.encoder.push_clip(clip).is_ok() {
                self.clips.push(());
                return Ok(());
            }
            self.encoder.pop_clip();
        }
        let coverage = match shape_path(shape) {
            Some(path) => exact_ink::path::coverage(
                &mut self.paths,
                &mut self.device,
                &path,
                FillRule::Winding,
                &[],
                dev,
                Some(self.size),
            )?,
            None => None,
        };
        if let Some(mask) = coverage {
            // Keep the same clip slots available to children as the flattened mask.
            if self.encoder.is_scissor_only() && dev.kx == 0. && dev.ky == 0. {
                self.encoder.push_combined_clip(mask_clip(mask));
            } else {
                self.open_clip_group(mask)?;
            }
            self.clips.push(());
        } else {
            self.encoder
                .push_clip(Clip::Scissor(Scissor {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                }))
                .map_err(|_| Error::Invalid("empty clip".into()))?;
            self.clips.push(());
        }
        Ok(())
    }
}

fn simple(shape: &Shape, dev: Transform, size: [u32; 2]) -> Option<Clip> {
    if !circular(shape)
        || !dev.is_finite()
        || dev.kx != 0.
        || dev.ky != 0.
        || dev.sx == 0.
        || dev.sy == 0.
    {
        return None;
    }
    let (x, y, w, h) = shape.rect;
    let (x0, y0, x1, y1) = (
        x * dev.sx + dev.tx,
        y * dev.sy + dev.ty,
        (x + w) * dev.sx + dev.tx,
        (y + h) * dev.sy + dev.ty,
    );
    let edges = [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)];
    if !shape.rounded() && edges.iter().all(|e| e.fract() == 0.) {
        let x = edges[0].clamp(0., size[0] as f32) as u32;
        let y = edges[1].clamp(0., size[1] as f32) as u32;
        return Some(Clip::Scissor(Scissor {
            x,
            y,
            width: (edges[2].clamp(0., size[0] as f32) as u32).saturating_sub(x),
            height: (edges[3].clamp(0., size[1] as f32) as u32).saturating_sub(y),
        }));
    }
    if shape.rounded() && dev.sx.abs() != dev.sy.abs() {
        return None;
    }
    let mut radii = shape.radii.map(|r| r.0 * dev.sx.abs());
    if dev.sx < 0. {
        radii.swap(0, 1);
        radii.swap(3, 2);
    }
    if dev.sy < 0. {
        radii.swap(0, 3);
        radii.swap(1, 2);
    }
    Some(Clip::Rounded(RoundedClip {
        rect: [edges[0], edges[1], edges[2] - edges[0], edges[3] - edges[1]],
        radii,
    }))
}

pub(super) fn mask_clip(mask: Coverage) -> CoverageClip {
    CoverageClip {
        texture: mask.entry.texture,
        rect: mask.rect,
        uv: mask.entry.uv,
    }
}
