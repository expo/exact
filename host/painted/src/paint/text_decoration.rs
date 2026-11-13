//! Solid currentcolor decorations stay paint-only, following shaped glyph advances.
use super::text_shadow::Shadow;
use super::{Backend, Painter, Rect4, Shape};
use crate::text::markup::{STRIKE, UNDERLINE};
use crate::text::{Paragraph, RunPaint};
use exact_kernel::{Kernel, PropId, TextDecorationLine, ViewId};
use std::collections::HashMap;
use tiny_skia::Transform;

struct Decoration {
    run: usize,
    first_run: usize,
    last_run: usize,
    line: TextDecorationLine,
    foreground: bool,
    rect: Rect4,
    paint: DecorationPaint,
}

#[derive(Clone, Copy)]
struct DecorationPaint {
    source: ViewId,
    depth: usize,
    color: [u8; 4],
    shadow: Option<Shadow>,
}

impl Decoration {
    fn at(&self, origin: (f32, f32)) -> Shape {
        let (x, y, w, h) = self.rect;
        Shape::rect((origin.0 + x, origin.1 + y, w, h))
    }
}

pub(super) struct Decorations(Vec<Decoration>);

impl Decorations {
    /// Whether a box with a painted line ends just before `run`.
    pub(super) fn ends_at(&self, run: usize) -> bool {
        self.0.iter().any(|d| d.foreground && d.last_run + 1 == run)
    }

    /// Underlines precede their run; strikes follow their declaring box's last run.
    pub(super) fn paint(
        &self,
        backend: &mut dyn Backend,
        range: std::ops::Range<usize>,
        origin: (f32, f32),
        ts: Transform,
        line: TextDecorationLine,
    ) {
        for d in self.0.iter().filter(|d| {
            let run = if line == TextDecorationLine::LineThrough {
                d.last_run
            } else {
                d.run
            };
            d.foreground && d.line == line && range.contains(&run)
        }) {
            backend.fill(&d.at(origin), d.paint.color, ts);
        }
    }

    /// Each declaring box's whole decoration in one mask, so a blur has no
    /// seam where a descendant overrides the shadow. Ordered by the box's first
    /// run, ancestors before descendants. A Markdown span is a box of its own,
    /// beside the other spans of its text.
    pub(super) fn shadows(&self) -> Vec<DecorationShadow<'_>> {
        let mut shadows: Vec<DecorationShadow<'_>> = Vec::new();
        for d in &self.0 {
            let Some(shadow) = d.paint.shadow else {
                continue;
            };
            match shadows.iter_mut().find(|s| {
                s.mask.0[0].paint.source == d.paint.source
                    && s.first_run == d.first_run
                    && s.shadow == shadow
            }) {
                Some(s) => s.mask.0.push(d),
                None => shadows.push(DecorationShadow {
                    first_run: d.first_run,
                    shadow,
                    mask: DecorationMask(vec![d]),
                }),
            }
        }
        shadows.sort_by_key(|s| (s.first_run, s.mask.0[0].paint.depth));
        shadows
    }
}

pub(super) struct DecorationShadow<'a> {
    pub first_run: usize,
    pub shadow: Shadow,
    pub mask: DecorationMask<'a>,
}

#[derive(Default)]
pub(super) struct DecorationMask<'a>(Vec<&'a Decoration>);

impl DecorationMask<'_> {
    pub(super) fn paint(&self, backend: &mut dyn Backend, origin: (f32, f32), ts: Transform) {
        for d in &self.0 {
            backend.fill(&d.at(origin), [0, 0, 0, 255], ts);
        }
    }

    pub(super) fn bounds(&self, rect: Rect4) -> Rect4 {
        self.0.iter().fold(rect, |(x, y, w, h), d| {
            let (left, top, width, height) = d.rect;
            let (x0, y0) = (x.min(left), y.min(top));
            (
                x0,
                y0,
                (x + w).max(left + width) - x0,
                (y + h).max(top + height) - y0,
            )
        })
    }
}

impl Painter {
    pub(super) fn text_decorations(
        &mut self,
        kernel: &Kernel,
        paragraph: &Paragraph,
        palette: &[RunPaint],
        reveal: Option<ViewId>,
    ) -> Decorations {
        // CSS Text Decoration 3 §2.1: a box's lines are drawn through its
        // descendants' visible text in the box's own colour, whatever the
        // box's own visibility. Keep covered lines for their shadows; only
        // the nearest box's foreground line shows.
        let mut run_ranges = HashMap::new();
        let flags: Vec<_> = palette
            .iter()
            .enumerate()
            .map(|(index, run)| {
                if !super::paints(kernel, run.source, reveal) {
                    return Vec::new();
                }
                let mut current = Some(run.source);
                let mut declarations = Vec::new();
                while let Some(id) = current {
                    let Some(node) = kernel.node(id) else {
                        break;
                    };
                    // A leaf's own colour is its run's, as presented; a
                    // Markdown paragraph's pieces take the paragraph's.
                    let color =
                        if id == run.source && node.props.str(PropId::Markup) != Some("markdown") {
                            run.color
                        } else {
                            super::rgba(node.text_color().resolve(self.dark))
                        };
                    let line = node.style.text_decoration_line;
                    if line != TextDecorationLine::None {
                        run_ranges.entry(id).or_insert((index, index)).1 = index;
                        declarations.push((
                            line,
                            DecorationPaint {
                                source: id,
                                depth: 0,
                                color,
                                shadow: self.text_shadow(&node, color),
                            },
                        ));
                    }
                    current = node.parent;
                }
                let count = declarations.len();
                for (index, (_, paint)) in declarations.iter_mut().enumerate() {
                    paint.depth = count - index - 1;
                }
                declarations
            })
            .collect();
        let underlines = flags.iter().flatten().any(|(line, _)| {
            matches!(
                line,
                TextDecorationLine::Underline | TextDecorationLine::UnderlineLineThrough
            )
        });
        if underlines && !self.decoration_warning {
            self.decoration_warning = true;
            self.notes.push("CSS text-decoration: Linux uses solid font-size-based line metrics; underline text-decoration-skip-ink:auto is not implemented (LLP 1001)".into());
        }
        // A Markdown piece's lines are its span's, as an element's are: the
        // runs on either side of it with the same lines and link.
        let runs = paragraph.runs();
        let lines_of = |i: usize| (runs[i].mark & (UNDERLINE | STRIKE), runs[i].href.as_str());
        let mut spans: Vec<(usize, usize)> = (0..runs.len()).map(|i| (i, i)).collect();
        for i in 1..runs.len() {
            if lines_of(i) == lines_of(i - 1) {
                spans[i].0 = spans[i - 1].0;
            }
        }
        for i in (1..runs.len()).rev() {
            if lines_of(i - 1) == lines_of(i) {
                spans[i - 1].1 = spans[i].1;
            }
        }
        let mut lines = Vec::new();
        for (glyph, baseline, paint) in paragraph.paint_glyphs(palette) {
            if !super::paints(kernel, paint.source, reveal) {
                continue;
            }
            // A Markdown piece's own: a followed link, `~~strike~~`.
            let mark = paragraph.runs().get(glyph.run()).map_or(0, |r| r.mark);
            let declarations = &flags[glyph.run()];
            let marked_line = match (mark & UNDERLINE != 0, mark & STRIKE != 0) {
                (true, true) => TextDecorationLine::UnderlineLineThrough,
                (true, false) => TextDecorationLine::Underline,
                (false, true) => TextDecorationLine::LineThrough,
                (false, false) => TextDecorationLine::None,
            };
            let marked = (marked_line != TextDecorationLine::None).then(|| {
                (
                    marked_line,
                    DecorationPaint {
                        source: paint.source,
                        depth: declarations.len(),
                        color: paint.color,
                        shadow: kernel
                            .node(paint.source)
                            .and_then(|node| self.text_shadow(&node, paint.color)),
                    },
                    spans[glyph.run()],
                )
            });
            let thickness = (glyph.font_size / 16.0).max(1.0);
            let (mut underlined, mut struck) = (false, false);
            // Markdown marks are nearest, followed by boxes from child to ancestor.
            for (line, paint, (first_run, last_run)) in marked.into_iter().chain(
                declarations
                    .iter()
                    .map(|(line, paint)| (*line, *paint, run_ranges[&paint.source])),
            ) {
                if matches!(
                    line,
                    TextDecorationLine::Underline | TextDecorationLine::UnderlineLineThrough
                ) {
                    lines.push(Decoration {
                        run: glyph.run(),
                        first_run,
                        last_run,
                        line: TextDecorationLine::Underline,
                        foreground: !underlined,
                        rect: (glyph.x, baseline + thickness, glyph.w, thickness),
                        paint,
                    });
                    underlined = true;
                }
                if matches!(
                    line,
                    TextDecorationLine::LineThrough | TextDecorationLine::UnderlineLineThrough
                ) {
                    lines.push(Decoration {
                        run: glyph.run(),
                        first_run,
                        last_run,
                        line: TextDecorationLine::LineThrough,
                        foreground: !struck,
                        rect: (
                            glyph.x,
                            baseline - glyph.font_size * 0.3,
                            glyph.w,
                            thickness,
                        ),
                        paint,
                    });
                    struck = true;
                }
            }
        }
        Decorations(lines)
    }
}
