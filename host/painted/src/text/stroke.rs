//! Stroke bands belong to a width snapshot, alongside its glyph ink.

use super::{Paragraph, RunPaint};
use std::collections::VecDeque;
use std::sync::Arc;
use tiny_skia::Pixmap;

const MAX_BYTES: usize = 8 * 1024 * 1024;

struct Band {
    width: f32,
    scale: f32,
    origin: (f32, f32),
    colors: Box<[[u8; 4]]>,
    pixels: Arc<Pixmap>,
}
impl Band {
    fn bytes(&self) -> usize {
        self.colors.len() * 4 + self.pixels.data().len()
    }

    fn matches_origin(&self, origin: (f32, f32), paragraph: &Paragraph) -> bool {
        if self.origin == origin {
            return true;
        }
        let offset = (origin.0 * self.scale, origin.1 * self.scale);
        let old = (self.origin.0 * self.scale, self.origin.1 * self.scale);
        // f32 subtraction can round a fractional move to a whole pixel.
        let delta = (
            f64::from(offset.0) - f64::from(old.0),
            f64::from(offset.1) - f64::from(old.1),
        );
        if delta.0.fract() != 0.0 || delta.1.fract() != 0.0 {
            return false;
        }
        // Even a whole-pixel move can change coverage: y truncates toward
        // zero, and f32 addition can round across a glyph's phase boundary.
        paragraph
            .layout_runs()
            .zip(paragraph.baselines())
            .all(|(line, baseline)| {
                let y = (origin.1 + baseline) * self.scale;
                let old_y = (self.origin.1 + baseline) * self.scale;
                line.glyphs.iter().all(|g| {
                    let (x, bin, y) = g.pixel((offset.0, y), self.scale);
                    let (old_x, old_bin, old_y) = g.pixel((old.0, old_y), self.scale);
                    bin == old_bin
                        && f64::from(x) - f64::from(old_x) == delta.0
                        && f64::from(y) - f64::from(old_y) == delta.1
                })
            })
    }
}

#[derive(Default)]
pub(crate) struct Cache {
    bands: VecDeque<Band>,
    payload_bytes: usize,
}
impl Cache {
    pub fn get(
        &mut self,
        width: f32,
        scale: f32,
        palette: &[RunPaint],
        origin: (f32, f32),
        paragraph: &Paragraph,
    ) -> Option<Arc<Pixmap>> {
        let i = self.bands.iter().position(|b| {
            b.width == width
                && b.scale == scale
                && b.colors.iter().copied().eq(palette.iter().map(|r| r.color))
                && b.matches_origin(origin, paragraph)
        })?;
        let band = self.bands.remove(i)?;
        let pixels = band.pixels.clone();
        self.bands.push_back(band);
        Some(pixels)
    }

    pub fn insert(
        &mut self,
        width: f32,
        scale: f32,
        palette: &[RunPaint],
        origin: (f32, f32),
        pixels: Arc<Pixmap>,
    ) {
        let bytes = pixels.data().len() + palette.len() * 4;
        if bytes + std::mem::size_of::<Band>() > MAX_BYTES {
            return;
        }
        // Reserve explicitly so the quota includes the deque's actual capacity.
        self.bands.reserve_exact(1);
        while self.bytes() + bytes > MAX_BYTES {
            let Some(old) = self.bands.pop_front() else {
                return;
            };
            self.payload_bytes -= old.bytes();
        }
        self.bands.push_back(Band {
            width,
            scale,
            origin,
            colors: palette.iter().map(|r| r.color).collect(),
            pixels,
        });
        self.payload_bytes += bytes;
    }

    pub fn bytes(&self) -> usize {
        self.payload_bytes + self.bands.capacity() * std::mem::size_of::<Band>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_storage_is_bounded_and_releases_evicted_images() {
        let palette = [RunPaint {
            color: [255; 4],
            source: 1,
        }];
        let mut cache = Cache::default();
        let mut text = super::super::TextEngine::new();
        let spec = crate::paint::text_spec(&exact_kernel::StyleProps::default(), "Ab");
        let paragraph = text.paragraph(&spec, Some(80.0));
        let origin = (0.0, 0.0);
        let first = Arc::new(Pixmap::new(1024, 1024).unwrap());
        let retired = Arc::downgrade(&first);
        cache.insert(1.0, 1.0, &palette, origin, first);
        cache.insert(
            2.0,
            1.0,
            &palette,
            origin,
            Arc::new(Pixmap::new(1024, 1024).unwrap()),
        );
        assert!(retired.upgrade().is_none());
        assert!(cache.get(1.0, 1.0, &palette, origin, &paragraph).is_none());
        assert!(cache.get(2.0, 1.0, &palette, origin, &paragraph).is_some());
        assert!(cache.bytes() <= MAX_BYTES);
        let large = Arc::new(Pixmap::new(2048, 1024).unwrap());
        let oversized = Arc::downgrade(&large);
        cache.insert(3.0, 1.0, &palette, origin, large);
        assert!(oversized.upgrade().is_none());
        assert!(cache.get(2.0, 1.0, &palette, origin, &paragraph).is_some());
        assert!(cache.bytes() <= MAX_BYTES);
    }
}
