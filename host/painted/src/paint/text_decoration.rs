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
        selected: std::ops::Range<usize>,
        run_shadows: Option<&[Option<Shadow>]>,
    ) -> Decorations {
        // CSS Text Decoration 3 §2.1: a box's lines are drawn through its
        // descendants' visible text in the box's own colour, whatever the
        // box's own visibility. Keep covered lines for their shadows; only
        // the nearest box's foreground line shows.
        let mut run_ranges = HashMap::new();
        let mut flags = vec![None; palette.len()];
        let mut end = selected.start;
        // Adjacent runs of a field or Markdown paragraph share one source box.
        for group in palette[selected].chunk_by(|a, b| a.source == b.source) {
            let start = end;
            end += group.len();
            let source = group[0].source;
            if !super::paints(kernel, source, reveal) {
                continue;
            }
            let mut boxes = Vec::new();
            let mut current = kernel.node(source);
            while let Some(node) = current {
                current = node.parent.and_then(|parent| kernel.node(parent));
                if node.style.text_decoration_line != TextDecorationLine::None {
                    // A leaf's own line follows the presented run colour.
                    // Markdown and ancestor boxes supply their own colour.
                    let color = (node.id != source
                        || node.props.str(PropId::Markup) == Some("markdown"))
                    .then(|| super::rgba(node.text_color().resolve(self.dark)));
                    run_ranges.entry(node.id).or_insert((start, end - 1)).1 = end - 1;
                    boxes.push((node, color));
                }
            }
            for (flag, run) in flags[start..end].iter_mut().zip(group) {
                let declarations = boxes
                    .iter()
                    .enumerate()
                    .map(|(i, (node, color))| {
                        let color = color.unwrap_or(run.color);
                        (
                            node.style.text_decoration_line,
                            DecorationPaint {
                                source: node.id,
                                depth: boxes.len() - i - 1,
                                color,
                                shadow: run_shadows
                                    .is_none()
                                    .then(|| self.text_shadow(node, color))
                                    .flatten(),
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                *flag = Some(declarations);
            }
        }
        let runs = paragraph.runs();
        if run_ranges.is_empty() && runs.iter().all(|run| run.mark & (UNDERLINE | STRIKE) == 0) {
            return Decorations(Vec::new());
        }
        let underlines = flags.iter().flatten().flatten().any(|(line, _)| {
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
        let lines_of = |i: usize| {
            (
                runs[i].mark & (UNDERLINE | STRIKE),
                runs[i].href.as_str(),
                run_shadows.map(|shadows| shadows[i]),
            )
        };
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
        for (glyph, baseline, run) in paragraph.glyphs_with_runs() {
            let paint = palette[run];
            let Some(declarations) = &flags[run] else {
                continue;
            };
            // A Markdown piece's own: a followed link, `~~strike~~`.
            let mark = paragraph.runs().get(run).map_or(0, |r| r.mark);
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
                        shadow: run_shadows.map_or_else(
                            || {
                                kernel
                                    .node(paint.source)
                                    .and_then(|node| self.text_shadow(&node, paint.color))
                            },
                            |shadows| shadows[run],
                        ),
                    },
                    spans[run],
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
                        run,
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
                        run,
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
