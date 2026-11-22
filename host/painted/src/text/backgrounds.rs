//! Inline background fragments use character ownership, independent of shaping.
use super::{inline::Ownership, Paragraph, RoomKind};
use crate::paint::{border::resolve_radii, Shape};
use exact_kernel::{BorderStyle, Dimension, Direction, Env, ViewId};
use std::collections::BTreeMap;
use std::ops::Range;

/// An inline box's `background-color` and border, which paint on each of its
/// fragments (CSS 2 §8.6, §14.2).
#[derive(Clone, Debug)]
pub(crate) struct InlineBackground {
    /// Clear for a box with a border and no background.
    pub color: [u8; 4],
    pub id: ViewId,
    pub radii: [Dimension; 4],
    /// How far its box's padding box stands out from its text at its first
    /// character and its last (`NodeRef::inline_background_reach`): its
    /// padding and border, which the background covers (CSS 2 §14.2).
    pub reach: exact_kernel::InlineEdges,
    /// How far its padding box stands above and below its text's content
    /// area (`NodeRef::inline_background_block_reach`), which the background
    /// covers and the line does not make room for (CSS 2 §10.6.1).
    pub above: f32,
    pub below: f32,
    pub border: Option<InlineBorder>,
    /// The boxes around it with backgrounds of their own, nearest first,
    /// which paint under it.
    pub outer: Vec<InlineBackground>,
}

/// An inline box's border, top, right, bottom and left: the widths its line
/// makes room for (`NodeRef::inline_border_widths`), its colours and styles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineBorder {
    pub widths: [f32; 4],
    pub colors: [[u8; 4]; 4],
    pub styles: [BorderStyle; 4],
}

/// One fragment's paint, in paint order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum InlinePaint {
    /// Its background.
    Fill(Shape, [u8; 4]),
    /// Its border on its border box; a side the box is sliced at has no width.
    Border(Shape, InlineBorder),
}

impl InlinePaint {
    pub(crate) fn shape(&self) -> &Shape {
        match self {
            InlinePaint::Fill(shape, _) | InlinePaint::Border(shape, _) => shape,
        }
    }

    pub(crate) fn translated(mut self, dx: f32, dy: f32) -> Self {
        let (InlinePaint::Fill(shape, _) | InlinePaint::Border(shape, _)) = &mut self;
        shape.rect.0 += dx;
        shape.rect.1 += dy;
        self
    }
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
    /// characters select lines; the paragraph direction, which is each
    /// inline box's, selects each line's outermost piece for the start and
    /// end corners and borders. Breaks stay open. Each box paints its
    /// background, then its border, outer boxes first (CSS 2 Appendix E).
    pub(crate) fn inline_backgrounds(
        &self,
        backgrounds: &[Option<InlineBackground>],
        env: &Env,
    ) -> Vec<InlinePaint> {
        // The room beside a box (`Run::edges`) is no glyph's ink: only its
        // own boxes' reach covers it, read by character.
        let edged = self.source.spec.runs.iter().any(|r| !r.edges.is_empty());
        if !edged
            && !backgrounds.iter().flatten().any(|b| {
                !b.reach.is_empty()
                    || b.above != 0.0
                    || b.below != 0.0
                    || b.border.is_some()
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
                .filter(|(_, color)| color[3] != 0)
                .map(|(rect, color)| InlinePaint::Fill(Shape::rect(rect), color))
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
        // The runs with no text, where they stand (`empty_run_place`).
        let empties: Vec<_> = runs
            .iter()
            .filter(|(range, run)| {
                range.is_empty()
                    && self.source.spec.runs[*run].inline.is_none()
                    && backgrounds.get(*run).is_some_and(Option::is_some)
            })
            .filter_map(|&(_, run)| Some((run, self.empty_run_place(&ownership, run)?)))
            .collect();
        let mut out = Vec::new();
        for (index, line) in self.layout_runs().enumerate() {
            let mut edges = BTreeMap::<ViewId, Edges>::new();
            let mut spans = Vec::new();
            // A box with no characters is its reach either side of its place,
            // starting and ending there.
            for &(run, (_, x, _)) in empties.iter().filter(|(_, p)| p.0 == index) {
                let background = backgrounds[run].as_ref().expect("filtered");
                for (depth, layer) in background.layers().enumerate() {
                    if boxes.contains_key(&layer.id) {
                        continue;
                    }
                    let (before, after) = if rtl {
                        (layer.reach.end, layer.reach.start)
                    } else {
                        (layer.reach.start, layer.reach.end)
                    };
                    let edge = edges.entry(layer.id).or_default();
                    (edge.start, edge.end) = (true, true);
                    spans.push(Span {
                        lo: x - before,
                        hi: x + after,
                        run,
                        layer,
                        depth,
                    });
                }
            }
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
                    if let Some(background) = backgrounds
                        .get(g.run(line.run_base))
                        .and_then(Option::as_ref)
                    {
                        if g.w > 0.0 {
                            for (depth, layer) in background.layers().enumerate() {
                                spans.push(Span {
                                    lo: g.x,
                                    hi: g.x + g.w,
                                    run: g.run(line.run_base),
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
                    for (depth, layer) in own.layers().enumerate() {
                        let (lo, hi) = match (flow, boxes.get(&layer.id)) {
                            (Some(_), Some(extent)) => self.flowed_reach(
                                g,
                                range.clone(),
                                (piece.lo, piece.hi),
                                piece.run,
                                layer,
                                extent,
                                runs,
                            ),
                            _ => (piece.lo, piece.hi),
                        };
                        spans.push(Span {
                            lo,
                            hi,
                            run: piece.run,
                            layer,
                            depth,
                        });
                    }
                }
            }
            // The room beside a run (`Run::edges`), where the line draws it:
            // a box covers the room of each run inside it, and at its own
            // start and end only its reach, beside its text.
            for room in line.rooms {
                let run = room.run as usize;
                if matches!(room.kind, RoomKind::Before | RoomKind::After) {
                    continue;
                }
                let (Some(own), Some((chars, _))) =
                    (backgrounds.get(run).and_then(Option::as_ref), runs.get(run))
                else {
                    continue;
                };
                let start = room.kind == RoomKind::Start;
                // A start is right of its text under `rtl`, an end left of it.
                let text_left = start == rtl;
                for (depth, layer) in own.layers().enumerate() {
                    let Some(extent) = boxes.get(&layer.id) else {
                        continue;
                    };
                    let w = if start && extent.start == chars.start {
                        layer.reach.start
                    } else if !start && extent.end == chars.end {
                        layer.reach.end
                    } else {
                        room.w
                    }
                    .min(room.w);
                    if w <= 0.0 {
                        continue;
                    }
                    let lo = if text_left {
                        room.x
                    } else {
                        room.x + room.w - w
                    };
                    spans.push(Span {
                        lo,
                        hi: lo + w,
                        run,
                        layer,
                        depth,
                    });
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
                let (above, below) = (ascent + background.above, descent + background.below);
                let rect = (
                    piece.lo,
                    line.line_y - above,
                    piece.hi - piece.lo,
                    above + below,
                );
                let radii = std::array::from_fn(|i| {
                    if kept[i] {
                        background.radii[i].clone()
                    } else {
                        Dimension::Points(0.0)
                    }
                });
                let shape = Shape::elliptical(rect, resolve_radii(radii, env, rect.2, rect.3));
                if background.color[3] != 0 {
                    out.push(InlinePaint::Fill(shape, background.color));
                }
                if let Some(border) = background.border {
                    let [top, r, bottom, l] = border.widths;
                    let widths = [
                        top,
                        if right { r } else { 0.0 },
                        bottom,
                        if left { l } else { 0.0 },
                    ];
                    out.push(InlinePaint::Border(
                        shape,
                        InlineBorder { widths, ..border },
                    ));
                }
            }
        }
        out
    }

    /// A box's piece of glyph `g` in text flowed around shapes, which draws
    /// a run's room only before its first glyph: its padding past its first
    /// and last characters, from its first glyph back and from its last
    /// glyph's own end, less the room after it, on the glyph's start and
    /// end sides.
    #[allow(clippy::too_many_arguments)]
    fn flowed_reach(
        &self,
        g: &super::LayoutGlyph,
        range: Range<usize>,
        (mut lo, mut hi): (f32, f32),
        run: usize,
        layer: &InlineBackground,
        extent: &Range<usize>,
        runs: &[(Range<usize>, usize)],
    ) -> (f32, f32) {
        let rtl = g.level % 2 == 1;
        let first = runs.get(run).map(|r| r.0.start);
        let before = if range.contains(&extent.start) {
            layer.reach.start
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
            let past = layer.reach.end - self.source.spec.runs[run].edges.end;
            if rtl {
                lo -= past;
            } else {
                hi += past;
            }
        }
        (lo, hi)
    }
}
