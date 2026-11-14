//! A paragraph's glyph paint with CSS `text-shadow` (LLP 1077 D3) and
//! `-webkit-text-stroke` (D7), per inline run, in tree order: a later box's
//! shadow covers an earlier box's text and decorations (CSS Text Decoration 3
//! §4). Adjacent runs with the same shadow and stroke share one step (LLP 1001's
//! one-box painting of like runs). A step ends before the first run of a box
//! whose decoration casts a shadow, and, when it paints a shadow or stroke,
//! after the last run of a box with a painted line, so a later run's shadow
//! and stroke cover that line.
//! A decoration casts its declaring box's shadow, painted at the box's first
//! run. Each box's whole decoration blurs in one CPU island (σ is half CSS's
//! radius) and is tinted once; the step's glyphs join the last such mask
//! painted at the step when their shadow matches, so their overlap takes a
//! translucent shadow once.

use super::numerals::Roll;
use super::text_decoration::DecorationMask;
use super::{Painter, Rect4};
use crate::text::{Paragraph, RunPaint};
use exact_kernel::{Kernel, NodeRef, StyleId, TextDecorationLine, ViewId};
use std::sync::Arc;
use tiny_skia::Transform;

pub(super) type Shadow = ((f32, f32), f32, [u8; 4]);

/// One run's shadow (offset, blur, colour) and stroke (width, colour),
/// `currentcolor` resolved to the run's presented colour.
#[derive(Clone, Copy, PartialEq, Default)]
struct RunLook {
    shadow: Option<Shadow>,
    stroke: Option<(f32, [u8; 4])>,
}

pub(super) struct TextInk {
    palette: Vec<RunPaint>,
    looks: Vec<RunLook>,
    decorations: super::text_decoration::Decorations,
}

impl Painter {
    pub(super) fn text_shadow(&self, node: &NodeRef<'_>, color: [u8; 4]) -> Option<Shadow> {
        let _accent = super::accent_scope(node);
        node.computed_row(StyleId::TextShadow, |s| s.text_shadow.shadow().copied())
            .map(|s| {
                let color = s.color.map_or(color, |c| super::rgba(c.resolve(self.dark)));
                ((s.offset.x, s.offset.y), s.blur, color)
            })
    }

    fn run_looks(
        &self,
        kernel: &Kernel,
        palette: &[RunPaint],
        reveal: Option<ViewId>,
    ) -> Vec<RunLook> {
        palette
            .iter()
            .map(|run| {
                let Some(node) = kernel.node(run.source) else {
                    return RunLook::default();
                };
                // A shadow or stroke is the run's own paint. `only` would
                // redraw a hidden run in the shadow colour if the look stayed.
                if !super::paints(kernel, run.source, reveal) {
                    return RunLook::default();
                }
                // Each row where it is set: no whole style copied per run,
                // under the run's own accent.
                let _accent = super::accent_scope(&node);
                let shadow = self.text_shadow(&node, run.color);
                let width = node.computed_row(StyleId::TextStrokeWidth, |s| s.text_stroke_width);
                let stroke = (width > 0.0 && width.is_finite()).then(|| {
                    let color = node
                        .computed_row(StyleId::TextStrokeColor, |s| s.text_stroke_color)
                        .map_or(run.color, |c| super::rgba(c.resolve(self.dark)));
                    (width, color)
                });
                RunLook { shadow, stroke }
            })
            .collect()
    }

    /// Paint the background clipped to glyphs, then each stretch's shadow,
    /// underline, glyphs and stroke. A strike follows its declaring box's last run.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn text_paint(
        &mut self,
        node: &NodeRef<'_>,
        kernel: &Kernel,
        paragraph: &Paragraph,
        palette: &[RunPaint],
        origin: (f32, f32),
        rect: Rect4,
        ts: Transform,
        reveal: Option<ViewId>,
        roll: Option<&(Roll, f64)>,
        range: std::ops::Range<usize>,
    ) {
        let ink = self.capture_text_ink(kernel, paragraph, palette, reveal, range.clone());
        let selected = range;
        // The box's background, clipped to the glyphs, is under all of its
        // inline content, shadows included.
        self.glyph_pass(
            paragraph,
            roll,
            &ink.palette,
            origin,
            ts,
            |p, text, ink, at| {
                p.text_clip(
                    node,
                    kernel,
                    text,
                    ink,
                    at,
                    rect,
                    ts,
                    reveal,
                    selected.clone(),
                );
            },
        );
        self.paint_text_ink(paragraph, &ink, origin, ts, roll, None);
    }

    pub(super) fn capture_text_ink(
        &mut self,
        kernel: &Kernel,
        paragraph: &Paragraph,
        palette: &[RunPaint],
        reveal: Option<ViewId>,
        range: std::ops::Range<usize>,
    ) -> TextInk {
        let selected = range;
        let palette: Vec<_> = palette
            .iter()
            .enumerate()
            .map(|(i, r)| RunPaint {
                color: if selected.contains(&i) {
                    r.color
                } else {
                    [0; 4]
                },
                ..*r
            })
            .collect();

        let mut looks = self.run_looks(kernel, &palette, reveal);
        for (i, look) in looks.iter_mut().enumerate() {
            if !selected.contains(&i) {
                *look = RunLook::default();
            }
        }
        let decorations =
            self.text_decorations(kernel, paragraph, &palette, reveal, selected.clone(), None);
        TextInk {
            palette,
            looks,
            decorations,
        }
    }

    pub(super) fn capture_field_ink(
        &mut self,
        kernel: &Kernel,
        paragraph: &Paragraph,
        palette: &[RunPaint],
        shadows: &[exact_kernel::style::TextShadow],
        reveal: Option<ViewId>,
    ) -> TextInk {
        let shadows: Vec<_> = shadows
            .iter()
            .zip(palette)
            .map(|(shadow, ink)| {
                shadow.shadow().map(|s| {
                    (
                        (s.offset.x, s.offset.y),
                        s.blur,
                        s.color
                            .map_or(ink.color, |c| super::rgba(c.resolve(self.dark))),
                    )
                })
            })
            .collect();
        let decorations = self.text_decorations(
            kernel,
            paragraph,
            palette,
            reveal,
            0..palette.len(),
            Some(&shadows),
        );
        TextInk {
            palette: palette.to_vec(),
            looks: shadows
                .into_iter()
                .map(|shadow| RunLook {
                    shadow,
                    stroke: None,
                })
                .collect(),
            decorations,
        }
    }

    pub(super) fn paint_text_ink(
        &mut self,
        paragraph: &Paragraph,
        ink: &TextInk,
        origin: (f32, f32),
        ts: Transform,
        roll: Option<&(Roll, f64)>,
        field_clips: Option<(Rect4, Rect4)>,
    ) {
        let (foreground_clip, shadow_clip) = field_clips.unzip();
        let palette = ink.palette.as_slice();
        let looks = &ink.looks;
        let decorations = &ink.decorations;
        let shadows = decorations.shadows();
        let mut steps: Vec<(usize, usize)> = Vec::new();
        for i in 0..looks.len() {
            match steps.last_mut() {
                Some((_, end))
                    if looks[*end - 1] == looks[i]
                        && !(looks[i] != RunLook::default() && decorations.ends_at(i))
                        && !shadows.iter().any(|s| s.first_run == i) =>
                {
                    *end = i + 1;
                }
                _ => steps.push((i, i + 1)),
            }
        }
        let single = steps.len() == 1;
        for (start, end) in steps {
            let look = looks[start];
            let shadowed = look
                .shadow
                .map(|_| only(palette, |i| (start..end).contains(&i), [0, 0, 0, 255]));
            let masks: Vec<_> = shadows.iter().filter(|s| s.first_run == start).collect();
            // The step's own shadow paints next, so only the last mask can take
            // its glyphs without moving them under another box's shadow.
            let joins = masks.last().is_some_and(|s| look.shadow == Some(s.shadow));
            for (index, s) in masks.iter().enumerate() {
                let glyphs = shadowed
                    .as_deref()
                    .filter(|_| joins && index + 1 == masks.len());
                self.shadow_pass(
                    paragraph,
                    glyphs,
                    &s.mask,
                    s.shadow,
                    origin,
                    ts,
                    roll,
                    shadow_clip,
                );
            }
            if let Some(shadow) = look.shadow.filter(|_| !joins) {
                self.shadow_pass(
                    paragraph,
                    shadowed.as_deref(),
                    &DecorationMask::default(),
                    shadow,
                    origin,
                    ts,
                    roll,
                    shadow_clip,
                );
            }
            // A field's shadow reaches the padding; its foreground stays in the content box.
            if let Some(clip) = foreground_clip {
                self.backend.push_clip(&super::Shape::rect(clip), ts);
            }
            // CSS Text Decoration 3 §5.1: shadows, underlines, glyphs, strikes.
            decorations.paint(
                self.backend.as_mut(),
                start..end,
                origin,
                ts,
                TextDecorationLine::Underline,
            );
            let fill;
            let visible = if single {
                palette
            } else {
                fill = palette
                    .iter()
                    .enumerate()
                    .map(|(i, r)| RunPaint {
                        color: if (start..end).contains(&i) {
                            r.color
                        } else {
                            [0; 4]
                        },
                        ..*r
                    })
                    .collect::<Vec<_>>();
                &fill
            };
            self.glyph_pass(paragraph, roll, visible, origin, ts, |p, text, ink, at| {
                p.fill_pass(text, ink, at, ts);
            });
            if let Some((width, color)) = look.stroke {
                self.glyph_pass(
                    paragraph,
                    roll,
                    &only(palette, |i| (start..end).contains(&i), color),
                    origin,
                    ts,
                    |p, text, ink, at| p.stroke_pass(text, ink, width, at, ts),
                );
            }
            decorations.paint(
                self.backend.as_mut(),
                start..end,
                origin,
                ts,
                TextDecorationLine::LineThrough,
            );
            if foreground_clip.is_some() {
                self.backend.pop_clip();
            }
        }
    }

    fn fill_pass(
        &mut self,
        paragraph: &Paragraph,
        palette: &[RunPaint],
        origin: (f32, f32),
        ts: Transform,
    ) {
        let mut engine = self.text.borrow_mut();
        self.backend
            .text(&mut engine, paragraph, palette, origin, ts);
    }

    /// Glyphs and decorations form one mask, so their overlap takes the shadow
    /// colour only once, including when that colour is translucent.
    #[allow(clippy::too_many_arguments)]
    fn shadow_pass(
        &mut self,
        paragraph: &Paragraph,
        shadowed: Option<&[RunPaint]>,
        decorations: &DecorationMask<'_>,
        ((x, y), blur, color): Shadow,
        origin: (f32, f32),
        ts: Transform,
        roll: Option<&(Roll, f64)>,
        clip: Option<Rect4>,
    ) {
        let sigma = blur / 2.0;
        let reach = 3.0 * sigma + 1.0;
        let glyph_bounds = roll.map_or_else(|| line_bounds(paragraph), |(r, _)| r.bounds());
        let (left, top, width, height) = decorations.bounds(glyph_bounds);
        let at = (origin.0 + x, origin.1 + y);
        let mut bounds = (
            at.0 + left - reach,
            at.1 + top - reach,
            width + 2.0 * reach,
            height + 2.0 * reach,
        );
        if let Some(clip) = clip {
            let Some(inverse) = ts.invert() else {
                return;
            };
            let viewport = super::bbox(inverse, (0., 0., self.viewport.0, self.viewport.1));
            let (x, y, w, h) = super::intersect(clip, viewport);
            if w <= 0. || h <= 0. {
                return;
            }
            // Clip the offset mask before allocating it. Keep offscreen ink
            // within blur reach so it can still cast into the visible padding.
            bounds = super::intersect(
                bounds,
                (x - reach, y - reach, w + 2. * reach, h + 2. * reach),
            );
        }
        if bounds.2 <= 0. || bounds.3 <= 0. {
            return;
        }
        // Whole device pixels preserve the mask's coverage when it is composited.
        let s = self.scale;
        let (x0, y0, x1, y1) = (
            (bounds.0 * s).floor() / s,
            (bounds.1 * s).floor() / s,
            ((bounds.0 + bounds.2) * s).ceil() / s,
            ((bounds.1 + bounds.3) * s).ceil() / s,
        );
        let rect: Rect4 = (x0, y0, x1 - x0, y1 - y0);
        let Some(mut pixels) = self.island(rect, |p, t| {
            if let Some(shadowed) = shadowed {
                p.glyph_pass(paragraph, roll, shadowed, at, t, |p, text, ink, at| {
                    p.fill_pass(text, ink, at, t);
                });
            }
            decorations.paint(p.backend.as_mut(), at, t);
        }) else {
            return;
        };
        let (w, h) = (pixels.width() as usize, pixels.height() as usize);
        exact_svg_raster::backdrop_blur(pixels.data_mut(), w, h, sigma * self.scale);
        for pixel in pixels.data_mut().chunks_exact_mut(4) {
            let alpha = (pixel[3] as u32 * color[3] as u32 + 127) / 255;
            for i in 0..3 {
                pixel[i] = ((color[i] as u32 * alpha + 127) / 255) as u8;
            }
            pixel[3] = alpha as u8;
        }
        self.backend.island_image(Arc::new(pixels), rect, ts, 0);
    }
}

/// The paragraph's box widened to where its lines sit. `width` is the widest
/// line's advance, but a centred or right-aligned line (or one in a wider
/// field) starts past zero, so its glyphs can lie beyond `width`.
fn line_bounds(paragraph: &Paragraph) -> Rect4 {
    let (mut left, mut right) = (0.0f32, paragraph.width);
    for line in paragraph.layout_runs() {
        // Glyphs are in visual order: the first is leftmost, the last rightmost.
        if let (Some(first), Some(last)) = (line.glyphs.first(), line.glyphs.last()) {
            left = left.min(first.x);
            right = right.max(last.x + last.w);
        }
    }
    (left, 0.0, right - left, paragraph.height)
}

/// `palette` with the selected runs in `color` and the rest transparent.
fn only(palette: &[RunPaint], selected: impl Fn(usize) -> bool, color: [u8; 4]) -> Vec<RunPaint> {
    palette
        .iter()
        .enumerate()
        .map(|(i, r)| RunPaint {
            color: if selected(i) { color } else { [0; 4] },
            ..*r
        })
        .collect()
}
