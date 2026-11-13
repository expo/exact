//! Inline background fragments use character ownership, independent of shaping.
use super::{
    inline::{Ownership, Piece},
    Paragraph,
};
use crate::paint::{border::resolve_radii, Shape};
use exact_kernel::{Dimension, Direction, Env, ViewId};
use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Clone, Debug)]
pub(crate) struct InlineBackground {
    pub color: [u8; 4],
    pub id: ViewId,
    pub radii: [Dimension; 4],
    /// How far its box's padding box stands out from its text at its first
    /// character and its last (`NodeRef::inline_background_reach`): its
    /// padding and border, which the background covers (CSS 2 §14.2).
    pub reach: exact_kernel::InlineEdges,
    /// The boxes around it with backgrounds of their own, nearest first,
    /// which paint under it.
    pub outer: Vec<InlineBackground>,
}

impl InlineBackground {
    /// Its boxes, outermost first: each one's paint order (CSS 2 Appendix E).
    fn layers(&self) -> impl Iterator<Item = &InlineBackground> {
        self.outer.iter().rev().chain(std::iter::once(self))
    }
}

/// One box's background over part of a glyph, with its run's metrics.
struct Span<'a> {
    lo: f32,
    hi: f32,
    run: usize,
    layer: &'a InlineBackground,
    depth: usize,
}

#[derive(Default)]
struct Edges {
    start: bool,
    end: bool,
    left: Option<usize>,
    right: usize,
}

fn endpoints_in(
    endpoints: &[(usize, ViewId, bool)],
    range: Range<usize>,
    edges: &mut BTreeMap<ViewId, Edges>,
) {
    let from = endpoints.partition_point(|(at, _, _)| *at < range.start);
    for &(at, id, end) in &endpoints[from..] {
        if at >= range.end {
            break;
        }
        let edge = edges.entry(id).or_default();
        if end {
            edge.end = true;
        } else {
            edge.start = true;
        }
    }
}

impl Paragraph {
    /// CSS 2 §8.6 and `box-decoration-break: slice`: a box's first and last
    /// characters select lines; the paragraph direction selects each line's
    /// outermost piece for the start and end corners. Breaks stay open.
    pub(crate) fn inline_backgrounds(
        &self,
        backgrounds: &[Option<InlineBackground>],
        env: &Env,
    ) -> Vec<(Shape, [u8; 4])> {
        // The room beside a box (`Run::edges`) is no glyph's ink: only its
        // own boxes' reach covers it, read by character.
        let edged = self.source.spec.runs.iter().any(|r| !r.edges.is_empty());
        if !edged
            && !backgrounds.iter().flatten().any(|b| {
                !b.reach.is_empty()
                    || !b.outer.is_empty()
                    || b.radii.iter().any(|d| match d {
                        Dimension::Auto => false,
                        Dimension::Points(n) | Dimension::Percent(n) => *n > 0.0,
                        Dimension::Calc(p, n) => *p > 0.0 || *n > 0.0,
                        _ => true,
                    })
            })
        {
            return self
                .run_backgrounds(
                    &backgrounds
                        .iter()
                        .map(|b| b.as_ref().map(|b| b.color))
                        .collect::<Vec<_>>(),
                )
                .into_iter()
                .map(|(rect, color)| (Shape::rect(rect), color))
                .collect();
        }
        // Built only for rounded paint, never part of the paragraph's shape key.
        let flow = self.flow.as_ref().filter(|f| !f.incomplete);
        let ownership = Ownership::new(self);
        let runs = &ownership.runs;
        let hard_ranges = &ownership.hard;
        let mut boxes: BTreeMap<ViewId, Range<usize>> = BTreeMap::new();
        let mut add = |range: Range<usize>, run: usize| {
            let Some(background) = backgrounds.get(run).and_then(Option::as_ref) else {
                return;
            };
            for layer in background.layers() {
                boxes
                    .entry(layer.id)
                    .and_modify(|b| {
                        b.start = b.start.min(range.start);
                        b.end = b.end.max(range.end);
                    })
                    .or_insert(range.clone());
            }
        };
        if flow.is_some() {
            for (range, run) in runs.iter().filter(|(r, _)| !r.is_empty()) {
                add(range.clone(), *run);
            }
        } else {
            // Ordinary layout omits hard separators from its glyph ranges.
            for hard in hard_ranges.iter().filter(|r| !r.is_empty()) {
                let from = runs.partition_point(|(r, _)| r.end <= hard.start);
                for (range, run) in &runs[from..] {
                    if range.start >= hard.end {
                        break;
                    }
                    if !range.is_empty() {
                        add(range.start.max(hard.start)..range.end.min(hard.end), *run);
                    }
                }
            }
        }
        let mut endpoints: Vec<_> = boxes
            .iter()
            .flat_map(|(&id, range)| [(range.start, id, false), (range.end - 1, id, true)])
            .collect();
        endpoints.sort_unstable();
        let metrics = &self.source.data.run_metrics;
        let rtl = self.source.spec.direction == Direction::Rtl;
        let mut out = Vec::new();
        for (index, line) in self.layout_runs().enumerate() {
            let mut edges = BTreeMap::<ViewId, Edges>::new();
            let mut spans = Vec::new();
            let base = if flow.is_some() {
                0
            } else {
                hard_ranges[line.line_i].start
            };
            if let Some(flow) = flow {
                endpoints_in(
                    &endpoints,
                    flow.fragments[index].start..flow.paint_ends[index],
                    &mut edges,
                );
            }
            // Flow has already removed collapsible trailing whitespace. Ordinary
            // layout keeps its endpoints, but the spaces carry no background ink.
            let logical_end = line
                .glyphs
                .iter()
                .map(|g| g.end as usize)
                .max()
                .unwrap_or(0);
            let trim = if flow.is_some() || self.source.spec.white_space.model().preserves() {
                logical_end
            } else {
                line.text[..logical_end].trim_end_matches([' ', '\t']).len()
            };
            for g in line.glyphs {
                let local = g.range();
                if local.is_empty() {
                    // An ellipsis carries its run's backgrounds, but no endpoint.
                    if let Some(background) = backgrounds.get(g.run()).and_then(Option::as_ref) {
                        if g.w > 0.0 {
                            for (depth, layer) in background.layers().enumerate() {
                                spans.push(Span {
                                    lo: g.x,
                                    hi: g.x + g.w,
                                    run: g.run(),
                                    layer,
                                    depth,
                                });
                            }
                        }
                    }
                    continue;
                }
                let range = base + local.start..base + local.end;
                if flow.is_none() {
                    endpoints_in(&endpoints, range.clone(), &mut edges);
                }
                if g.w <= 0.0 || local.start >= trim {
                    continue;
                }
                for piece in ownership.pieces(g, range.clone()) {
                    let Some(own) = backgrounds.get(piece.run).and_then(Option::as_ref) else {
                        continue;
                    };
                    for (depth, background) in own.layers().enumerate() {
                        let Piece {
                            mut lo,
                            mut hi,
                            run,
                        } = piece;
                        // Its padding past its first and last characters: from its
                        // first glyph back, and from its last glyph's own end, less
                        // the room after it (`Run::edges`), on (the line's) start
                        // and end sides. Inside it, the room before a later run's
                        // first glyph is its own too.
                        if let Some(extent) = boxes.get(&background.id) {
                            let rtl = g.level % 2 == 1;
                            let first = runs.get(run).map(|r| r.0.start);
                            let before = if range.contains(&extent.start) {
                                background.reach.start
                            } else if first.is_some_and(|f| range.contains(&f)) {
                                self.source.spec.runs[run].edges.start
                            } else {
                                0.0
                            };
                            if rtl {
                                hi += before;
                            } else {
                                lo -= before;
                            }
                            if range.contains(&(extent.end - 1)) {
                                let room = self.source.spec.runs[run].edges.end;
                                let past = background.reach.end - room;
                                if rtl {
                                    lo -= past;
                                } else {
                                    hi += past;
                                }
                            }
                        }
                        spans.push(Span {
                            lo,
                            hi,
                            run,
                            layer: background,
                            depth,
                        });
                    }
                }
            }
            // Outer boxes first, so that a box inside another paints over it.
            spans.sort_by(|a, b| a.depth.cmp(&b.depth).then(a.lo.total_cmp(&b.lo)));
            let mut merged: Vec<Span> = Vec::new();
            for span in spans {
                match merged.last_mut() {
                    Some(last)
                        if last.layer.id == span.layer.id
                            && metrics[last.run] == metrics[span.run]
                            && span.lo <= last.hi + 0.01 =>
                    {
                        last.hi = last.hi.max(span.hi);
                    }
                    _ => merged.push(span),
                }
            }
            for (i, piece) in merged.iter().enumerate() {
                let edge = edges.entry(piece.layer.id).or_default();
                edge.left.get_or_insert(i);
                edge.right = i;
            }
            for (i, piece) in merged.into_iter().enumerate() {
                let background = piece.layer;
                let edge = &edges[&background.id];
                let left = edge.left == Some(i) && if rtl { edge.end } else { edge.start };
                let right = edge.right == i && if rtl { edge.start } else { edge.end };
                let kept = [left, right, right, left];
                let (ascent, descent, _) = metrics[piece.run];
                let rect = (
                    piece.lo,
                    line.line_y - ascent,
                    piece.hi - piece.lo,
                    ascent + descent,
                );
                let radii = std::array::from_fn(|i| {
                    if kept[i] {
                        background.radii[i].clone()
                    } else {
                        Dimension::Points(0.0)
                    }
                });
                out.push((
                    Shape::elliptical(rect, resolve_radii(radii, env, rect.2, rect.3)),
                    background.color,
                ));
            }
        }
        out
    }
}
