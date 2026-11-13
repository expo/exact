//! `-webkit-text-stroke` (LLP 1077 D7): a stroke centred on the glyphs'
//! outlines, painted over their fill. The painters draw glyphs as coverage,
//! not outlines, so the stroke is a band: the paragraph in the stroke's
//! colour dilated by half the width, less the same eroded by half. It is a
//! CPU island kept with the paragraph and placed over its normal glyphs.

use super::{Painter, Rect4};
use crate::text::{Paragraph, RunPaint};
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};

#[cfg(test)]
thread_local! {
    pub(super) static BAND_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Painter {
    /// The stroke of the runs `stroked` paints (the rest transparent), as
    /// one band island of `width` over their glyphs (`text_paint` orders it).
    pub(super) fn stroke_pass(
        &mut self,
        paragraph: &Paragraph,
        stroked: &[RunPaint],
        width: f32,
        origin: (f32, f32),
        ts: Transform,
    ) {
        if !self.backend.supports_island_image() {
            return;
        }
        let scale = self.scale;
        let reach = ((width + 1.0) * scale).ceil();
        let device = (origin.0 * scale, origin.1 * scale);
        let (x, y) = (device.0.floor(), device.1.floor());
        let rect: Rect4 = (
            (x - reach) / scale,
            (y - reach) / scale,
            ((paragraph.width * scale + (device.0 - x)).ceil() + 2.0 * reach) / scale,
            ((paragraph.height * scale + (device.1 - y)).ceil() + 2.0 * reach) / scale,
        );
        if let Some(band) = paragraph
            .strokes
            .borrow_mut()
            .get(width, scale, stroked, origin, paragraph)
        {
            self.backend.island_image(band, rect, ts, 0);
            return;
        }
        #[cfg(test)]
        BAND_BUILDS.set(BAND_BUILDS.get() + 1);
        let radius = width / 2.0 * scale;
        // Keep the fill's glyph phases and pixel rows. Whole-pixel island bounds
        // avoid resampling coverage on the way into and out of the island.
        let Some(mut band) = self.island(rect, |p, t| {
            let text = p.text.clone();
            let mut engine = text.borrow_mut();
            p.backend.text(&mut engine, paragraph, stroked, origin, t);
        }) else {
            return;
        };
        let mut inside = band.clone();
        morphology(&mut band, radius, true);
        morphology(&mut inside, radius, false);
        // The band: what the dilation covers and the erosion does not.
        for (px, e) in band
            .data_mut()
            .chunks_exact_mut(4)
            .zip(inside.data().chunks_exact(4))
        {
            let keep = 255 - e[3] as u32;
            for v in px.iter_mut() {
                *v = ((*v as u32 * keep + 127) / 255) as u8;
            }
        }
        let band = Arc::new(band);
        paragraph
            .strokes
            .borrow_mut()
            .insert(width, scale, stroked, origin, band.clone());
        self.backend.island_image(band, rect, ts, 0);
    }
}

/// Dilate (max) or erode (min) every channel over a disc of `radius` device
/// pixels. The disc's edge is soft — a neighbour counts by how much of its
/// pixel the radius reaches past the source pixel's own half-pixel edge — so
/// a fractional width (1.5px at 1×) draws as wide as it is, not rounded to
/// whole pixels.
fn morphology(pixels: &mut Pixmap, radius: f32, dilate: bool) {
    if !radius.is_finite() || radius <= 0.0 {
        return;
    }
    let reach = radius.ceil() as i32;
    let (w, h) = (pixels.width() as usize, pixels.height() as usize);
    let src = pixels.data().to_vec();
    let out = pixels.data_mut();
    out.fill(if dilate { 0 } else { 255 });
    let mut queue = std::collections::VecDeque::with_capacity(w);
    let mut row = vec![0; w * 4];
    for dy in -reach..=reach {
        let mut inner = None;
        let mut edge = Vec::new();
        for dx in -reach..=reach {
            let weight = (radius + 1.0
                - ((i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy)) as f32).sqrt())
            .clamp(0.0, 1.0);
            // Tiny discs cost less as direct samples than as sliding windows.
            if weight == 1.0 && reach > 2 {
                inner = Some(dx.unsigned_abs() as usize);
            } else if weight > 0.0 {
                let values: [u8; 256] = std::array::from_fn(|s| {
                    if dilate {
                        (s as f32 * weight).round() as u8
                    } else {
                        (255.0 - (255.0 - s as f32) * weight).round() as u8
                    }
                });
                edge.push((dx, values));
            }
        }
        for (y, dst) in out.chunks_exact_mut(w * 4).enumerate() {
            let sy = y as i64 + i64::from(dy);
            let source = (0..h as i64)
                .contains(&sy)
                .then(|| &src[sy as usize * w * 4..(sy as usize + 1) * w * 4]);
            if let Some(radius) = inner {
                if let Some(source) = source {
                    horizontal(source, &mut row, radius, dilate, &mut queue);
                    for (v, s) in dst.iter_mut().zip(&row) {
                        *v = extreme(*v, *s, dilate);
                    }
                } else if !dilate {
                    dst.fill(0);
                }
            }
            for (dx, values) in &edge {
                if let Some(source) = source {
                    let lo = (-i64::from(*dx)).clamp(0, w as i64) as usize;
                    let hi = (w as i64 - i64::from(*dx)).clamp(lo as i64, w as i64) as usize;
                    if !dilate {
                        for v in &mut dst[..lo * 4] {
                            *v = (*v).min(values[0]);
                        }
                        for v in &mut dst[hi * 4..] {
                            *v = (*v).min(values[0]);
                        }
                    }
                    if lo < hi {
                        let start = (lo as i64 + i64::from(*dx)) as usize * 4;
                        for (v, s) in dst[lo * 4..hi * 4].iter_mut().zip(&source[start..]) {
                            *v = extreme(*v, values[*s as usize], dilate);
                        }
                    }
                } else if !dilate {
                    for v in dst.iter_mut() {
                        *v = (*v).min(values[0]);
                    }
                }
            }
        }
    }
}

fn extreme(a: u8, b: u8, dilate: bool) -> u8 {
    if dilate {
        a.max(b)
    } else {
        a.min(b)
    }
}

// A disc's opaque interior is a horizontal window on each row. Sliding its
// extrema visits each sample twice at most, instead of once per output pixel.
// Only the soft, one-pixel rim needs individual weighted neighbours. Total
// pixel work is O(area * radius); disc weights cost O(radius²) once per pass.
fn horizontal(
    src: &[u8],
    out: &mut [u8],
    radius: usize,
    dilate: bool,
    queue: &mut std::collections::VecDeque<usize>,
) {
    let w = src.len() / 4;
    for c in 0..4 {
        queue.clear();
        let mut next = 0;
        for x in 0..w {
            while queue.front().is_some_and(|&i| i < x.saturating_sub(radius)) {
                queue.pop_front();
            }
            while next <= x.saturating_add(radius).min(w - 1) {
                let v = src[next * 4 + c];
                while queue
                    .back()
                    .is_some_and(|&i| extreme(src[i * 4 + c], v, dilate) == v)
                {
                    queue.pop_back();
                }
                queue.push_back(next);
                next += 1;
            }
            out[x * 4 + c] = if !dilate && (x < radius || x.saturating_add(radius) >= w) {
                0
            } else {
                src[queue[0] * 4 + c]
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_pixels_follow_the_fills_device_phase() {
        use crate::text::TextEngine;
        use std::cell::RefCell;
        use std::rc::Rc;

        let text = Rc::new(RefCell::new(TextEngine::new()));
        let style = exact_kernel::StyleProps {
            font_size: 24.0,
            ..Default::default()
        };
        let spec = crate::paint::text_spec(&style, "Ab\nCd");
        let paragraph = text.borrow_mut().paragraph(&spec, Some(80.0));
        let palette = [RunPaint {
            color: [40, 80, 120, 255],
            source: 1,
        }];
        for scale in [1.0, 1.25, 2.0, 3.0] {
            let mut painter = Painter::new(
                text.clone(),
                scale,
                Box::new(crate::raster::Raster::transparent()),
            );
            for origin in [
                (20.0, 20.0),
                (20.25, 20.0),
                (20.0, 10.4),
                (20.25, 10.4),
                (24.25, 14.4),
                (-0.25, -10.5),
                (-4.25, -30.5),
                (20.25, 10.4),
            ] {
                let ts = Transform::from_translate(40.0, 60.0);
                painter.backend.begin(200.0, 200.0, scale);
                painter.stroke_pass(&paragraph, &palette, 2.0, origin, ts);
                let actual = painter.backend.finish().unwrap();

                // Build the band from the fill's pixels on the full frame, without
                // an island or a cached placement.
                let mut expected = Pixmap::new(actual.width(), actual.height()).unwrap();
                text.borrow_mut().paint(
                    &mut expected,
                    &paragraph,
                    &palette,
                    origin,
                    scale,
                    ts.post_scale(scale, scale),
                    None,
                );
                assert!(expected.data().chunks_exact(4).any(|p| p[3] > 0));
                let mut inside = expected.clone();
                morphology(&mut expected, scale, true);
                morphology(&mut inside, scale, false);
                for (px, e) in expected
                    .data_mut()
                    .chunks_exact_mut(4)
                    .zip(inside.data().chunks_exact(4))
                {
                    for v in px {
                        *v = ((*v as u32 * (255 - e[3] as u32) + 127) / 255) as u8;
                    }
                }
                let different = actual
                    .data()
                    .chunks_exact(4)
                    .zip(expected.data().chunks_exact(4))
                    .filter(|(a, b)| a != b)
                    .count();
                assert_eq!(different, 0, "origin {origin:?}, scale {scale}");
            }
        }
    }

    #[test]
    fn stroke_pixels_survive_moves_with_rounded_integer_deltas() {
        use crate::text::TextEngine;
        use std::cell::RefCell;
        use std::rc::Rc;

        let text = Rc::new(RefCell::new(TextEngine::new()));
        let style = exact_kernel::StyleProps {
            font_size: 24.0,
            ..Default::default()
        };
        let spec = crate::paint::text_spec(&style, "Ab\nCd");
        let paragraph = text.borrow_mut().paragraph(&spec, Some(80.0));
        let palette = [RunPaint {
            color: [40, 80, 120, 255],
            source: 1,
        }];
        let mut painter = Painter::new(text, 1.0, Box::new(crate::raster::Raster::transparent()));
        let near_sixteen = f32::from_bits(0x417ffff8);
        for (old, origin) in [
            ((-984.0, 0.0), (near_sixteen, 0.0)),
            ((984.0, 0.0), (-near_sixteen, 0.0)),
            ((0.0, -984.0), (0.0, near_sixteen)),
            ((0.0, 984.0), (0.0, -near_sixteen)),
            ((near_sixteen, 0.0), (-984.0, 0.0)),
            ((-near_sixteen, 0.0), (984.0, 0.0)),
            ((0.0, near_sixteen), (0.0, -984.0)),
            ((0.0, -near_sixteen), (0.0, 984.0)),
        ] {
            *paragraph.strokes.borrow_mut() = Default::default();
            painter.backend.begin(200.0, 200.0, 1.0);
            painter.stroke_pass(&paragraph, &palette, 2.0, old, Transform::identity());
            let ts = Transform::from_translate(40.0 - origin.0.floor(), 60.0 - origin.1.floor());
            painter.backend.begin(200.0, 200.0, 1.0);
            painter.stroke_pass(&paragraph, &palette, 2.0, origin, ts);
            let actual = painter.backend.finish().unwrap();

            *paragraph.strokes.borrow_mut() = Default::default();
            painter.backend.begin(200.0, 200.0, 1.0);
            painter.stroke_pass(&paragraph, &palette, 2.0, origin, ts);
            let expected = painter.backend.finish().unwrap();
            assert!(expected.data().chunks_exact(4).any(|p| p[3] > 0));
            let different = actual
                .data()
                .chunks_exact(4)
                .zip(expected.data().chunks_exact(4))
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(different, 0, "move from {old:?} to {origin:?}");
        }
    }

    #[test]
    fn unchanged_stroke_reuses_the_band() {
        use crate::text::TextEngine;
        use std::cell::RefCell;
        use std::rc::Rc;

        let text = Rc::new(RefCell::new(TextEngine::new()));
        let style = exact_kernel::StyleProps {
            font_size: 32.0,
            font_weight: 800,
            ..Default::default()
        };
        let spec = crate::paint::text_spec(&style, "Exact paints a headline");
        let paragraph = text.borrow_mut().paragraph(&spec, Some(390.0));
        let palette = [RunPaint {
            color: [0, 0, 0, 255],
            source: 1,
        }];
        let mut painter = Painter::new(text, 3.0, Box::new(crate::raster::Raster::new()));
        painter.backend.begin(390.0, 300.0, 3.0);
        let before = BAND_BUILDS.get();
        for _ in 0..7 {
            painter.stroke_pass(&paragraph, &palette, 2.0, (0.0, 0.0), Transform::identity());
        }
        assert_eq!(
            BAND_BUILDS.get() - before,
            1,
            "seven identical paints build one band"
        );

        let first = paragraph
            .strokes
            .borrow_mut()
            .get(2.0, 3.0, &palette, (0.0, 0.0), &paragraph)
            .unwrap();
        assert!(first.data().chunks_exact(4).any(|p| p[3] > 0));
        let mut moved = palette;
        moved[0].source = 20;
        painter.stroke_pass(
            &paragraph,
            &moved,
            2.0,
            (17.0, 30.0),
            Transform::from_scale(0.8, 0.8),
        );
        assert_eq!(
            BAND_BUILDS.get() - before,
            1,
            "whole-pixel placement and source IDs do not rebuild ink"
        );
        painter.stroke_pass(&paragraph, &palette, 4.0, (0.0, 0.0), Transform::identity());
        let wider = paragraph
            .strokes
            .borrow_mut()
            .get(4.0, 3.0, &palette, (0.0, 0.0), &paragraph)
            .unwrap();
        assert!(wider.width() > first.width());
        painter.scale = 1.0;
        painter.stroke_pass(&paragraph, &palette, 2.0, (0.0, 0.0), Transform::identity());
        let smaller = paragraph
            .strokes
            .borrow_mut()
            .get(2.0, 1.0, &palette, (0.0, 0.0), &paragraph)
            .unwrap();
        assert!(smaller.width() < first.width());
        moved[0].color = [255, 0, 0, 128];
        painter.stroke_pass(&paragraph, &moved, 2.0, (0.0, 0.0), Transform::identity());
        let red = paragraph
            .strokes
            .borrow_mut()
            .get(2.0, 1.0, &moved, (0.0, 0.0), &paragraph)
            .unwrap();
        assert!(red.data().chunks_exact(4).any(|p| p[0] > 0));
        assert_eq!(
            BAND_BUILDS.get() - before,
            4,
            "width, scale and colour each rebuild"
        );
        painter.scale = 3.0;
        painter.stroke_pass(&paragraph, &palette, 2.0, (0.0, 0.0), Transform::identity());
        assert_eq!(
            BAND_BUILDS.get() - before,
            4,
            "an earlier live band remains reusable"
        );

        // A walk that draws nothing makes no band; the recorder, which
        // places one as a picture, does.
        let mut unpainted = Painter::new(
            painter.text.clone(),
            3.0,
            Box::new(crate::paint::layer::Unpainted),
        );
        unpainted.stroke_pass(&paragraph, &palette, 5.0, (0.0, 0.0), Transform::identity());
        assert_eq!(
            BAND_BUILDS.get() - before,
            4,
            "a backend that places no island builds no band"
        );
        let mut recorder = Painter::new(
            painter.text.clone(),
            3.0,
            Box::new(crate::canvas::Recorder::new()),
        );
        recorder.stroke_pass(&paragraph, &palette, 6.0, (0.0, 0.0), Transform::identity());
        assert_eq!(
            BAND_BUILDS.get() - before,
            5,
            "the Android recorder strokes"
        );

        let before_phase = BAND_BUILDS.get();
        for (origin, builds) in [
            ((17.25, 30.5), 1),
            ((18.25, 31.5), 1),
            ((18.5, 31.5), 2),
            ((18.5, 31.75), 3),
            ((17.25, 30.5), 3),
        ] {
            painter.stroke_pass(&paragraph, &palette, 2.0, origin, Transform::identity());
            assert_eq!(BAND_BUILDS.get() - before_phase, builds, "{origin:?}");
        }
    }

    #[test]
    fn adjacent_stroke_passes_keep_their_own_bands() {
        use crate::text::TextEngine;
        use std::cell::RefCell;
        use std::rc::Rc;

        let text = Rc::new(RefCell::new(TextEngine::new()));
        let mut spec = crate::paint::text_spec(&exact_kernel::StyleProps::default(), "ABC");
        spec.runs.push(spec.runs[0].clone());
        let paragraph = text.borrow_mut().paragraph(&spec, Some(300.0));
        let mut painter = Painter::new(text.clone(), 1.0, Box::new(crate::raster::Raster::new()));
        painter.backend.begin(300.0, 100.0, 1.0);
        let palettes = [
            [
                RunPaint {
                    color: [255, 0, 0, 255],
                    source: 1,
                },
                RunPaint {
                    color: [0; 4],
                    source: 2,
                },
            ],
            [
                RunPaint {
                    color: [0; 4],
                    source: 1,
                },
                RunPaint {
                    color: [0, 0, 255, 255],
                    source: 2,
                },
            ],
        ];
        let before = BAND_BUILDS.get();
        paragraph.lines();
        let bytes_before = text.borrow().residency().owned_capacity_bytes;
        for _ in 0..3 {
            for palette in &palettes {
                painter.stroke_pass(&paragraph, palette, 2.0, (0.0, 0.0), Transform::identity());
            }
        }
        assert_eq!(BAND_BUILDS.get() - before, 2);
        let mut other = Painter::new(text.clone(), 1.0, Box::new(crate::raster::Raster::new()));
        other.backend.begin(300.0, 100.0, 1.0);
        other.stroke_pass(
            &paragraph,
            &palettes[0],
            2.0,
            (0.0, 0.0),
            Transform::identity(),
        );
        assert_eq!(
            BAND_BUILDS.get() - before,
            2,
            "the paragraph owns the bands across painters"
        );
        for (palette, channel) in palettes.iter().zip([0, 2]) {
            let band = paragraph
                .strokes
                .borrow_mut()
                .get(2.0, 1.0, palette, (0.0, 0.0), &paragraph)
                .unwrap();
            assert!(band.data().chunks_exact(4).any(|p| p[channel] > 0));
            assert!(band.data().chunks_exact(4).all(|p| p[2 - channel] == 0));
        }
        assert_eq!(
            text.borrow().residency().owned_capacity_bytes - bytes_before,
            paragraph.ink_capacity_bytes() + paragraph.strokes.borrow().bytes(),
            "lazy stroke storage is included in text residency"
        );
    }

    #[test]
    fn a_disc_grows_a_dot_and_shrinks_it_back() {
        let mut p = Pixmap::new(9, 9).unwrap();
        let i = (4 * 9 + 4) * 4;
        p.data_mut()[i..i + 4].copy_from_slice(&[255; 4]);
        morphology(&mut p, 2.0, true);
        let lit = p.data().chunks(4).filter(|c| c[3] == 255).count();
        assert_eq!(lit, 13, "a radius-2 disc");
        morphology(&mut p, 2.0, false);
        assert_eq!(p.data().chunks(4).filter(|c| c[3] > 128).count(), 1);
        // Half a pixel more reach is half-covered pixels, not nothing.
        let mut q = Pixmap::new(9, 9).unwrap();
        q.data_mut()[i..i + 4].copy_from_slice(&[255; 4]);
        morphology(&mut q, 1.5, true);
        let half = q
            .data()
            .chunks(4)
            .filter(|c| c[3] > 0 && c[3] < 255)
            .count();
        assert!(half > 0, "a soft edge");
    }
    #[test]
    fn morphology_matches_the_disc_at_fractional_and_large_radii() {
        for (w, h) in [(1, 1), (9, 7), (32, 16)] {
            let mut source = Pixmap::new(w, h).unwrap();
            let mut seed = 1234u32;
            for pixel in source.data_mut().chunks_exact_mut(4) {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let a = (seed >> 24) as u8;
                pixel.copy_from_slice(&[a / 2, a / 3, a, a]);
            }
            for radius in [0.0, 0.1, 0.5, 1.0, 1.5, 2.0, 4.0, 8.0, 12.0] {
                for dilate in [false, true] {
                    let mut expected = source.clone();
                    disc_reference(&mut expected, radius, dilate);
                    let mut actual = source.clone();
                    morphology(&mut actual, radius, dilate);
                    assert_eq!(
                        actual.data(),
                        expected.data(),
                        "{w}x{h}, radius {radius}, dilate {dilate}"
                    );
                }
            }
        }
    }

    // The direct disc scan is the pixel oracle for the faster row passes.
    fn disc_reference(pixels: &mut Pixmap, radius: f32, dilate: bool) {
        if radius.is_nan() || radius <= 0.0 {
            return;
        }
        let reach = radius.ceil() as i32;
        let (w, h) = (pixels.width() as i32, pixels.height() as i32);
        let src = pixels.data().to_vec();
        let out = pixels.data_mut();
        for y in 0..h {
            for x in 0..w {
                let mut v = if dilate { [0.0f32; 4] } else { [255.0f32; 4] };
                for dy in -reach..=reach {
                    for dx in -reach..=reach {
                        let weight =
                            (radius + 1.0 - ((dx * dx + dy * dy) as f32).sqrt()).clamp(0.0, 1.0);
                        if weight == 0.0 {
                            continue;
                        }
                        let (sx, sy) = (x + dx, y + dy);
                        let s = if (0..w).contains(&sx) && (0..h).contains(&sy) {
                            let i = ((sy * w + sx) * 4) as usize;
                            [src[i], src[i + 1], src[i + 2], src[i + 3]]
                        } else {
                            [0; 4]
                        };
                        for c in 0..4 {
                            let s = s[c] as f32;
                            v[c] = if dilate {
                                v[c].max(s * weight)
                            } else {
                                v[c].min(255.0 - (255.0 - s) * weight)
                            };
                        }
                    }
                }
                let i = ((y * w + x) * 4) as usize;
                for c in 0..4 {
                    out[i + c] = v[c].round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}
