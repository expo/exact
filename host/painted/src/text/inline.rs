//! Character ownership shared by inline backgrounds and client rectangles.
use super::{LayoutGlyph, LayoutRun, Paragraph, RoomKind};
use crate::paint::Rect4;
use std::ops::Range;

pub(super) struct Ownership {
    pub text: String,
    pub runs: Vec<(Range<usize>, usize)>,
    pub hard: Vec<Range<usize>>,
}

pub(super) struct Piece {
    pub lo: f32,
    pub hi: f32,
    pub run: usize,
}

impl Ownership {
    pub fn new(p: &Paragraph) -> Self {
        let text: String = p.runs().iter().map(|r| r.text.as_str()).collect();
        let mut offset = 0;
        let runs = p
            .runs()
            .iter()
            .enumerate()
            .map(|(i, run)| {
                let start = offset;
                offset += run.text.len();
                (start..offset, i)
            })
            .collect();
        let hard = p
            .source
            .data
            .lines
            .iter()
            .map(|line| line.start..line.start + line.text.len())
            .collect();
        Self { text, runs, hard }
    }

    fn overlaps(&self, range: Range<usize>) -> impl Iterator<Item = (Range<usize>, usize)> + '_ {
        let from = self.runs.partition_point(|(r, _)| r.end <= range.start);
        self.runs[from..]
            .iter()
            .take_while(move |(r, _)| r.start < range.end)
            .filter_map(move |(r, run)| {
                let part = r.start.max(range.start)..r.end.min(range.end);
                (!part.is_empty()).then_some((part, *run))
            })
    }

    /// Background paint divides a cross-run ligature by characters in its
    /// bidi direction, without reshaping the paragraph.
    pub fn pieces(&self, g: &LayoutGlyph, range: Range<usize>) -> Vec<Piece> {
        let chars = self.text[range.clone()].chars().count() as f32;
        let mut near = 0.;
        self.overlaps(range)
            .map(|(part, run)| {
                let far = near + self.text[part].chars().count() as f32 / chars * g.w;
                let (lo, hi) = if g.level % 2 == 1 {
                    (g.x + g.w - far, g.x + g.w - near)
                } else {
                    (g.x + near, g.x + far)
                };
                near = far;
                Piece { lo, hi, run }
            })
            .collect()
    }

    /// Chrome's inline client rectangles give a cluster's advance to its
    /// first character's run. Later runs have insertion boxes at its end.
    pub fn cluster_pieces(&self, g: &LayoutGlyph, range: Range<usize>) -> Vec<Piece> {
        let start = range.start;
        let end_x = if g.level % 2 == 1 { g.x } else { g.x + g.w };
        self.overlaps(range)
            .map(|(part, run)| {
                let (lo, hi) = if part.start == start {
                    (g.x, g.x + g.w)
                } else {
                    (end_x, end_x)
                };
                Piece { lo, hi, run }
            })
            .collect()
    }

    pub fn base(&self, p: &Paragraph, line: &LayoutRun<'_>) -> usize {
        if p.flow.as_ref().is_some_and(|f| !f.incomplete) {
            0
        } else {
            self.hard[line.line_i].start
        }
    }
}

/// How far an inline box's border box stands out from its text: past its
/// first and last characters (`NodeRef::inline_background_reach`), and above
/// and below its content area (`NodeRef::inline_background_block_reach`).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BoxReach {
    pub sides: exact_kernel::InlineEdges,
    pub above: f32,
    pub below: f32,
}

impl Paragraph {
    /// Where the run `run`, which has no text, stands: its line and the x of
    /// its content edge, inside its room (`Run::edges`) as the line draws
    /// it, its start edge on its paragraph's start side; and whether that
    /// paragraph is right to left. With no room, before the character after
    /// it, else after the one before it.
    pub(super) fn empty_run_place(
        &self,
        ownership: &Ownership,
        run: usize,
    ) -> Option<(usize, f32, bool)> {
        let rtl = self.source.spec.direction == exact_kernel::Direction::Rtl;
        let edges = self.source.spec.runs.get(run)?.edges;
        for (i, line) in self.layout_runs().enumerate() {
            if let Some(room) = line.rooms.iter().find(|r| r.run as usize == run) {
                let x = if rtl {
                    room.x + room.w - edges.start
                } else {
                    room.x + edges.start
                };
                return Some((i, x, rtl));
            }
        }
        let at = ownership.runs.get(run)?.0.start;
        let mut after = None;
        for (i, line) in self.layout_runs().enumerate() {
            let base = ownership.base(self, &line);
            for g in line.glyphs {
                let range = g.range();
                let glyph_rtl = g.level % 2 == 1;
                if base + range.start == at && !range.is_empty() {
                    return Some((i, if glyph_rtl { g.x + g.w } else { g.x }, rtl));
                }
                if base + range.end == at && !range.is_empty() {
                    after = Some((i, if glyph_rtl { g.x } else { g.x + g.w }, rtl));
                }
            }
        }
        after
    }

    pub(crate) fn inline_metrics(&self, style: exact_kernel::TextStyle) -> (f32, f32) {
        let (ascent, descent, _) = self
            .source
            .catalog
            .borrow_mut()
            .font_metrics(&super::Run::from_style("", style));
        (ascent, descent)
    }

    /// Each contiguous fragment of the addressed inline's border box: its
    /// own ascent and descent and `reach.above` and `reach.below` down, its
    /// glyphs across, and `reach.sides` past its first and last characters
    /// (CSS 2 §10.6.1, §8.6), as Chrome's `getClientRects` answers. Line
    /// height places its baseline; it does not size the box.
    pub(crate) fn inline_rects(
        &self,
        runs: Range<usize>,
        metrics: (f32, f32),
        reach: BoxReach,
    ) -> Vec<(usize, Rect4)> {
        let ownership = Ownership::new(self);
        let start = ownership
            .runs
            .get(runs.start)
            .map_or(ownership.text.len(), |r| r.0.start);
        let end = runs
            .clone()
            .last()
            .and_then(|i| ownership.runs.get(i))
            .map_or(start, |r| r.0.end);
        let flowed = self.flow.as_ref().is_some_and(|f| !f.incomplete);
        let paragraph_rtl = self.source.spec.direction == exact_kernel::Direction::Rtl;
        // Flowed text: the room after its last glyph, which no glyph there has.
        let end_room = runs
            .clone()
            .rev()
            .find(|i| ownership.runs.get(*i).is_some_and(|r| !r.0.is_empty()))
            .and_then(|i| self.source.spec.runs.get(i))
            .map_or(0., |r| r.edges.end);
        let (ascent, descent) = (metrics.0 + reach.above, metrics.1 + reach.below);
        let rect = |x, y, width| (x, y - ascent, width, ascent + descent);
        let mut out = Vec::new();
        let mut insertion = None;
        let mut previous = None;
        for (i, line) in self.layout_runs().enumerate() {
            let base = ownership.base(self, &line);
            let logical_end = line
                .glyphs
                .iter()
                .map(|g| g.end as usize)
                .max()
                .unwrap_or(0);
            let trim = if self.source.spec.white_space.model().preserves() {
                logical_end
            } else {
                line.text[..logical_end].trim_end_matches([' ', '\t']).len()
            };
            let mut spans = Vec::new();
            for g in line.glyphs {
                let local = g.range();
                if local.is_empty() {
                    continue;
                }
                let range = base + local.start..base + local.end;
                if start == end {
                    let caret = |at: usize| {
                        let at_start = at == range.start;
                        let x = if at_start == (g.level % 2 == 0) {
                            g.x
                        } else {
                            g.x + g.w
                        };
                        (i, rect(x, line.line_y, 0.))
                    };
                    if range.start <= start && start <= range.end && insertion.is_none() {
                        insertion = Some(caret(start));
                    }
                    if range.end <= start {
                        previous = Some(caret(range.end));
                    }
                }
                if g.w <= 0. || local.start >= trim {
                    continue;
                }
                let rtl = g.level % 2 == 1;
                let first = flowed && range.contains(&start);
                let last = flowed && end > start && range.contains(&(end - 1));
                for piece in ownership.cluster_pieces(g, range.clone()) {
                    if !runs.contains(&piece.run) {
                        continue;
                    }
                    let (mut lo, mut hi) = (piece.lo, piece.hi);
                    // Text flowed around shapes draws only a start room, before
                    // its run's first glyph.
                    if first {
                        if rtl {
                            hi += reach.sides.start;
                        } else {
                            lo -= reach.sides.start;
                        }
                    }
                    if last {
                        let past = reach.sides.end - end_room;
                        if rtl {
                            lo -= past;
                        } else {
                            hi += past;
                        }
                    }
                    spans.push((lo, hi));
                }
            }
            // The room beside its runs (`Run::edges`), where the line draws it:
            // its own reach at its start and end, beside its text (a start is
            // right of its text under `rtl`, an end left of it), and all of the
            // room of the runs inside it.
            for room in line.rooms {
                let run = room.run as usize;
                let Some((chars, _)) = ownership.runs.get(run).filter(|_| runs.contains(&run))
                else {
                    continue;
                };
                let w = match room.kind {
                    RoomKind::Start if chars.start == start => reach.sides.start,
                    RoomKind::End if chars.end == end => reach.sides.end,
                    _ => room.w,
                }
                .min(room.w);
                if start == end || w <= 0. {
                    continue;
                }
                let text_left = (room.kind == RoomKind::Start) == paragraph_rtl;
                let lo = match room.kind {
                    RoomKind::Start | RoomKind::End if !text_left => room.x + room.w - w,
                    _ => room.x,
                };
                spans.push((lo, lo + w));
            }
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged: Vec<(f32, f32)> = Vec::new();
            for (lo, hi) in spans {
                match merged.last_mut() {
                    Some(last) if lo <= last.1 + 0.01 => last.1 = last.1.max(hi),
                    _ => merged.push((lo, hi)),
                }
            }
            out.extend(
                merged
                    .into_iter()
                    .map(|(lo, hi)| (i, rect(lo, line.line_y, hi - lo))),
            );
            if start == end && insertion.is_none() && line.glyphs.is_empty() && base == start {
                insertion = Some((i, rect(0., line.line_y, 0.)));
            }
        }
        if start == end {
            // A run with no text: its border box around its place.
            let place = runs
                .clone()
                .last()
                .and_then(|run| self.empty_run_place(&ownership, run));
            if let Some(((i, x, rtl), line)) =
                place.and_then(|p| Some((p, self.layout_runs().nth(p.0)?)))
            {
                let (before, after) = if rtl {
                    (reach.sides.end, reach.sides.start)
                } else {
                    (reach.sides.start, reach.sides.end)
                };
                return vec![(i, rect(x - before, line.line_y, before + after))];
            }
            return insertion.or(previous).into_iter().collect();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{catalog, Run, TextEngine};
    use exact_kernel::{StyleProps, TextStyle, WhiteSpace};

    #[test]
    fn inline_frame_ownership_keeps_separator_offsets_and_empty_lines() {
        let mut engine = TextEngine::with_catalog(catalog::Catalog::from_bytes(
            &[
                include_bytes!("../../../../scripts/fixtures/fonts/assets/DejaVuSans.ttf")
                    .as_slice(),
            ],
            "DejaVu Sans",
        ));
        let style = StyleProps::default().with_rows(|rows| {
            rows.white_space = WhiteSpace::Pre;
        });
        for sep in [
            "\n", "\r\n", "\r", "\u{1c}", "\u{1d}", "\u{1e}", "\u{85}", "\u{2029}",
        ] {
            let text = format!("a{sep}{sep}b");
            let mut spec = crate::paint::text_spec(&style, "");
            // Separate runs also exercise CRLF split between runs.
            spec.runs = text
                .chars()
                .map(|c| Run::from_style(&c.to_string(), TextStyle::from_style(&style)))
                .collect();
            let p = engine.paragraph(&spec, Some(300.));
            let ownership = Ownership::new(&p);
            let blank = 1 + sep.len();
            let last = blank + sep.len();
            assert_eq!(
                ownership.hard,
                [0..1, blank..blank, last..last + 1],
                "{sep:?}"
            );
        }
    }
}
