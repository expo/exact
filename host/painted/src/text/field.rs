//! Editing geometry from the immutable glyphs and CSS line boxes used to paint a field.
use super::{shaping::ShapeData, LayoutRun, Paragraph, Spec, TextEngine};
use exact_kernel::{Direction, TextAlign};
use std::{collections::HashMap, ops::Range, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

type Rect = (f32, f32, f32, f32);

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Cursor {
    pub offset: u32,
    /// The preceding visual line at a shared wrap boundary. Commands default downstream.
    pub upstream: bool,
}

#[derive(Debug)]
struct Span {
    start: u32,
    end: u32,
    // Logical start and end positions; RTL spans run from right to left.
    left: f32,
    right: f32,
    // Font metrics relative to the visual line's baseline.
    ascent: f32,
    height: f32,
}

#[derive(Debug)]
struct Line {
    start: u32,
    end: u32,
    top: f32,
    height: f32,
    baseline: f32,
    empty_x: f32,
    /// The strut font's content area, for a line with no text.
    text: (f32, f32),
    /// Spans use UTF-16 offsets from the beginning of their hard line.
    base: u32,
    spans: Rc<[Span]>,
}

#[derive(Debug)]
pub(crate) struct Geometry {
    lines: Vec<Line>,
    len: u32,
}

/// A piece of a painted line with one baseline, font height and writing direction.
#[derive(Clone, Copy)]
pub struct FieldTextRun<'a> {
    /// Source offset in UTF-16.
    pub start: u32,
    /// Visual line index, including soft wraps.
    pub line: usize,
    /// Rectangle in paragraph coordinates.
    pub bounds: (f32, f32, f32, f32),
    /// Characters advance from the right edge of this run.
    pub rtl: bool,
    base: u32,
    spans: &'a [Span],
}

impl FieldTextRun<'_> {
    /// Whether two paints retained the same glyph placements in this run.
    pub fn same_metrics(&self, other: &Self) -> bool {
        self.bounds == other.bounds
            && self.rtl == other.rtl
            && self.start - self.base == other.start - other.base
            && std::ptr::eq(self.spans, other.spans)
    }

    /// Positions and widths of the editor's graphemes in logical order. Offsets
    /// are UTF-16 boundaries relative to the run. A hard break has zero advance.
    pub fn character_metrics(&self, source_start: u32, offsets: &[u32]) -> (Vec<f32>, Vec<f32>) {
        let (left, _, width, _) = self.bounds;
        let mut spans = self.spans;
        let mut trailing = spans.first().map_or(left, |s| s.left);
        let mut positions = Vec::with_capacity(offsets.len().saturating_sub(1));
        let mut widths = Vec::with_capacity(positions.capacity());
        for pair in offsets.windows(2) {
            let start = source_start + pair[0];
            let end = source_start + pair[1];
            spans = &spans[spans.partition_point(|s| self.base + s.end <= start)..];
            let mut extent: Option<(f32, f32)> = None;
            for s in spans.iter().take_while(|s| self.base + s.start < end) {
                let (x0, x1) = (s.left.min(s.right), s.left.max(s.right));
                extent = Some(extent.map_or((x0, x1), |(l, r)| (l.min(x0), r.max(x1))));
                trailing = s.right;
            }
            let (x0, x1) = extent.unwrap_or((trailing, trailing));
            positions.push(if self.rtl {
                left + width - x1
            } else {
                x0 - left
            });
            widths.push(x1 - x0);
        }
        (positions, widths)
    }
}

pub(crate) fn line_left(align: TextAlign, direction: Direction, width: f32, w: f32) -> f32 {
    if w > width {
        return if direction == Direction::Rtl {
            width - w
        } else {
            0.
        };
    }
    let align = match align {
        TextAlign::Justify => TextAlign::Start,
        other => other,
    };
    let share = match align.physical(direction) {
        TextAlign::Center => 0.5,
        TextAlign::Right => 1.,
        _ => 0.,
    };
    share * (width - w)
}

#[derive(Clone, Copy)]
pub(super) enum WidthUse<'a> {
    Measure,
    Paint,
    Field(Option<&'a Paragraph>),
}

pub(super) fn text_runs(spec: &Spec) -> bool {
    spec.runs
        .iter()
        .all(|r| r.inline.is_none() && r.edges.is_empty())
}

impl<'a> WidthUse<'a> {
    fn field(engine: &TextEngine, spec: &Spec, previous: Option<&'a Paragraph>) -> Self {
        Self::Field(previous.filter(|p| {
            Rc::ptr_eq(&engine.catalog, &p.source.catalog)
                && text_runs(spec)
                && text_runs(&p.source.spec)
                && super::cache::same_paragraph_style(spec, &p.source.spec)
        }))
    }

    pub(super) fn previous(self) -> Option<&'a Paragraph> {
        match self {
            Self::Field(p) => p,
            _ => None,
        }
    }
}

impl TextEngine {
    /// [`TextEngine::paragraph`] for a text that replaces the one `owner`
    /// (a field's node and which of its texts) showed before: the replaced
    /// one is not kept ([`super::cache::Cache::superseding`]).
    pub(crate) fn paragraph_replacing(
        &mut self,
        owner: (u32, u8),
        spec: &Spec,
        width: Option<f32>,
        previous: Option<&Paragraph>,
    ) -> Rc<Paragraph> {
        let key = self.paragraphs.identity(spec);
        let p = self.layout_paragraph(width, key, WidthUse::field(self, spec, previous));
        self.paragraphs.superseding(owner, key);
        p
    }

    pub(crate) fn edited_field(
        &mut self,
        spec: &Spec,
        width: Option<f32>,
        previous: Option<&Paragraph>,
    ) -> Rc<Paragraph> {
        let key = self.paragraphs.identity(spec);
        self.layout_paragraph(width, key, WidthUse::field(self, spec, previous))
    }
}

impl Paragraph {
    /// The editor keeps a final empty line for the caret after a hard break.
    pub(crate) fn textarea_scroll_max(
        &self,
        value: &str,
        (width, height): (f32, f32),
    ) -> (f32, f32) {
        let mut bottom = self.line_bottoms().last().copied().unwrap_or_default();
        if value.ends_with(super::PARAGRAPH_BREAKS) {
            bottom += self.source.flow_box(std::iter::empty(), &[]).1;
        }
        (
            (self.width + 1. - width).max(0.),
            (self.height.max(bottom) - height).max(0.),
        )
    }
}

/// Painted byte boundaries and DOM offsets within one hard line. ASCII uses the byte offset directly.
struct Offsets {
    boundaries: Vec<(usize, u32)>,
    ascii_len: usize,
}

impl Offsets {
    fn new(value: &str, masked: bool) -> Self {
        let mut boundaries = Vec::new();
        if masked || !value.is_ascii() {
            boundaries.push((0, 0));
            let (mut byte, mut units) = (0, 0);
            for c in value.chars() {
                byte += if masked {
                    '•'.len_utf8()
                } else {
                    c.len_utf8()
                };
                units += c.len_utf16() as u32;
                boundaries.push((byte, units));
            }
        }
        Self {
            boundaries,
            ascii_len: value.len(),
        }
    }

    fn at(&self, byte: usize) -> u32 {
        if self.boundaries.is_empty() {
            return byte.min(self.ascii_len) as u32;
        }
        let end = self.boundaries.partition_point(|(b, _)| *b <= byte);
        self.boundaries[end - 1].1
    }
}

fn line_spans(run: LayoutRun<'_>, data: &ShapeData, offsets: &Offsets) -> Rc<[Span]> {
    let mut spans = Vec::new();
    let mut glyphs = run.glyphs.iter().peekable();
    while let Some(g) = glyphs.next() {
        let (mut left, mut right) = (g.x, g.x + g.w);
        while let Some(next) = glyphs.peek().filter(|n| n.range() == g.range()) {
            left = left.min(next.x);
            right = right.max(next.x + next.w);
            glyphs.next();
        }
        let (ascent, descent) = data
            .run_metrics
            .get(g.run(run.run_base))
            .map_or(data.strut_font, |m| (m.0, m.1));
        let cluster = &run.text[g.range()];
        let mut boundaries: Vec<_> = cluster.grapheme_indices(true).map(|(i, _)| i).collect();
        boundaries.push(cluster.len());
        let count = boundaries.len() - 1;
        for (i, pair) in boundaries.windows(2).enumerate() {
            let x = |part: usize| {
                let t = part as f32 / count as f32;
                if g.level & 1 == 0 {
                    left + (right - left) * t
                } else {
                    right - (right - left) * t
                }
            };
            spans.push(Span {
                start: offsets.at(g.start as usize + pair[0]),
                end: offsets.at(g.start as usize + pair[1]),
                left: x(i),
                right: x(i + 1),
                ascent,
                height: ascent + descent,
            });
        }
    }
    spans.sort_by_key(|s| s.start);
    spans.into()
}

impl Geometry {
    pub(crate) fn new(
        paragraph: &Paragraph,
        value: &str,
        masked: bool,
        empty_x: f32,
        first_x: f32,
        previous: Option<(&Paragraph, &Geometry)>,
    ) -> Self {
        let data = &paragraph.source.data;
        let mut retained = HashMap::new();
        let key = |run: &LayoutRun<'_>| (run.text.as_ptr(), run.glyphs[0].start);
        if let Some((old, geometry)) =
            previous.filter(|(p, _)| !masked && p.source.data.strut_font == data.strut_font)
        {
            // Only a shared shape owns the same hard-line text and font metrics.
            // Its first visual cluster identifies the row when wrapping changes.
            for (run, line) in old.layout_runs().zip(&geometry.lines) {
                if !run.glyphs.is_empty() {
                    retained.insert(key(&run), (run.glyphs, &line.spans));
                }
            }
        }
        let mut hard_starts = vec![(0, 0)];
        let mut units = 0;
        let mut after_cr = false;
        for (byte, c) in value.char_indices() {
            units += c.len_utf16() as u32;
            if !masked && super::PARAGRAPH_BREAKS.contains(&c) {
                let next = (byte + c.len_utf8(), units);
                if c == '\n' && after_cr {
                    *hard_starts.last_mut().unwrap() = next
                } else {
                    hard_starts.push(next)
                }
            }
            after_cr = c == '\r';
        }
        let (ascent, descent) = data.strut_font;
        let area = |y: f32| (y - ascent, ascent + descent);
        let mut lines = Vec::new();
        let empty_spans: Rc<[Span]> = Rc::default();
        let mut mapped_line = None;
        for run in paragraph.layout_runs() {
            let (byte, base) = hard_starts[run.line_i];
            let spans = if value.is_empty() || run.glyphs.is_empty() {
                empty_spans.clone()
            } else if let Some((_, spans)) = retained
                .get(&key(&run))
                .filter(|(glyphs, _)| std::ptr::eq(*glyphs, run.glyphs) || *glyphs == run.glyphs)
            {
                Rc::clone(spans)
            } else {
                // Wrapped rows index the same hard line. A flowed row indexes the whole paragraph.
                if mapped_line.as_ref().is_none_or(|(i, _)| *i != run.line_i) {
                    let end = hard_starts.get(run.line_i + 1).map_or(value.len(), |s| s.0);
                    let end = if run.text.len() > end - byte {
                        value.len()
                    } else {
                        end
                    };
                    mapped_line = Some((run.line_i, Offsets::new(&value[byte..end], masked)));
                }
                line_spans(run, data, &mapped_line.as_ref().unwrap().1)
            };
            let start = base + spans.first().map_or(0, |s| s.start);
            let end = base + spans.last().map_or(0, |s| s.end);
            lines.push(Line {
                start,
                end,
                base,
                top: run.line_top,
                height: run.line_height,
                baseline: run.line_y,
                empty_x: if lines.is_empty() { first_x } else { empty_x },
                text: area(run.line_y),
                spans,
            });
            if value.is_empty() {
                break;
            }
        }
        // Ordinary text omits its empty final hard line. An editor can put
        // a caret there; its box is the paragraph's own empty-line strut.
        if !value.is_empty() && hard_starts.last() == Some(&(value.len(), units)) {
            let top = lines.last().map_or(0., |l| l.top + l.height);
            let height = paragraph.source.flow_box(std::iter::empty(), &[]).1;
            lines.push(Line {
                base: units,
                start: units,
                end: units,
                top,
                height,
                baseline: top + data.strut.0,
                empty_x,
                text: area(top + data.strut.0),
                spans: empty_spans,
            });
        }
        Self { lines, len: units }
    }

    #[cfg(test)]
    pub(crate) fn shares_spans(&self, row: usize, old: &Self, old_row: usize) -> bool {
        Rc::ptr_eq(&self.lines[row].spans, &old.lines[old_row].spans)
    }

    pub(crate) fn access_runs(&self) -> impl Iterator<Item = FieldTextRun<'_>> {
        self.lines.iter().enumerate().flat_map(|(index, line)| {
            // A mixed-direction line can put logically adjacent characters at opposite ends.
            // Each AccessKit run advances in one direction from its own leading edge.
            let chunks = line.spans.chunk_by(|a, b| {
                (a.right < a.left) == (b.right < b.left)
                    && (a.right - b.left).abs() < 0.01
                    && a.ascent == b.ascent
                    && a.height == b.height
            });
            chunks
                .chain(line.spans.is_empty().then_some(&[][..]))
                .map(move |spans| {
                    let first = spans.first();
                    let leading = first.map_or(line.empty_x, |s| s.left);
                    let trailing = spans.last().map_or(leading, |s| s.right);
                    let (top, height) =
                        first.map_or(line.text, |s| (line.baseline - s.ascent, s.height));
                    FieldTextRun {
                        start: first.map_or(line.start, |s| line.base + s.start),
                        line: index,
                        bounds: (
                            leading.min(trailing),
                            top,
                            (trailing - leading).abs(),
                            height,
                        ),
                        rtl: trailing < leading,
                        base: line.base,
                        spans,
                    }
                })
        })
    }

    pub(crate) fn wrap_boundary(&self, offset: u32) -> bool {
        self.lines
            .windows(2)
            .any(|pair| pair[0].end == offset && pair[1].start == offset)
    }

    fn line_index(&self, cursor: Cursor) -> usize {
        if cursor.upstream {
            if let Some(i) = self.lines.iter().position(|l| l.end == cursor.offset) {
                return i;
            }
        }
        self.lines
            .iter()
            .rposition(|l| l.start <= cursor.offset)
            .unwrap_or(0)
    }

    pub(crate) fn caret(&self, cursor: Cursor) -> Rect {
        let Some(line) = self.lines.get(self.line_index(cursor)) else {
            return (0., 0., 1., 0.);
        };
        let x = line
            .spans
            .iter()
            .find(|s| cursor.offset < line.base + s.end)
            .map(|s| s.left)
            .or_else(|| line.spans.last().map(|s| s.right))
            .unwrap_or(line.empty_x);
        (x, line.top, 1., line.height)
    }

    #[cfg(any(target_os = "android", test))]
    pub(crate) fn selection_edge(&self, offset: u32, trailing: bool) -> (usize, Rect, bool) {
        let cursor = Cursor {
            offset,
            upstream: trailing,
        };
        let index = self.line_index(cursor);
        let Some(line) = self.lines.get(index) else {
            return (index, self.caret(cursor), false);
        };
        let span = line.spans.iter().find(|s| {
            if trailing {
                line.base + s.end == offset
            } else {
                line.base + s.start == offset
            }
        });
        if let Some(s) = span {
            let x = if trailing { s.right } else { s.left };
            return (index, (x, line.top, 0., line.height), s.right < s.left);
        }
        (index, self.caret(cursor), false)
    }

    pub(crate) fn horizontal_bounds(&self) -> (f32, f32) {
        self.lines
            .iter()
            .flat_map(|line| {
                std::iter::once(line.empty_x)
                    .chain(line.spans.iter().flat_map(|s| [s.left, s.right]))
            })
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(left, right), x| {
                (left.min(x), right.max(x))
            })
    }

    fn on_line(&self, index: usize, x: f32) -> Cursor {
        let line = &self.lines[index];
        let offset = line
            .spans
            .iter()
            .flat_map(|s| [(line.base + s.start, s.left), (line.base + s.end, s.right)])
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map_or(line.start, |s| s.0);
        Cursor {
            offset,
            upstream: offset == line.end
                && self.lines.get(index + 1).is_some_and(|l| l.start == offset),
        }
    }

    pub(crate) fn hit(&self, x: f32, y: f32) -> Cursor {
        if self.lines.is_empty() {
            return Cursor::default();
        }
        let index = self
            .lines
            .iter()
            .position(|l| y < l.top + l.height)
            .unwrap_or(self.lines.len() - 1);
        self.on_line(index, x)
    }

    /// The character under the pointer, rather than the nearest insertion edge.
    pub(crate) fn hit_character(&self, x: f32, y: f32) -> u32 {
        let Some(line) = self
            .lines
            .iter()
            .find(|l| y < l.top + l.height)
            .or_else(|| self.lines.last())
        else {
            return 0;
        };
        line.spans
            .iter()
            .min_by(|a, b| {
                let distance = |s: &Span| {
                    let left = s.left.min(s.right);
                    let right = s.left.max(s.right);
                    (left - x).max(0.).max(x - right)
                };
                distance(a).total_cmp(&distance(b))
            })
            .map_or(line.start, |span| line.base + span.start)
    }

    pub(crate) fn vertical(&self, cursor: Cursor, x: f32, down: bool) -> Cursor {
        let index = self.line_index(cursor);
        if down && index + 1 < self.lines.len() {
            self.on_line(index + 1, x)
        } else if !down && index > 0 {
            self.on_line(index - 1, x)
        } else {
            Cursor {
                offset: if down { self.len } else { 0 },
                upstream: false,
            }
        }
    }

    pub(crate) fn edge(&self, cursor: Cursor, end: bool) -> Cursor {
        let Some(line) = self.lines.get(self.line_index(cursor)) else {
            return Cursor::default();
        };
        Cursor {
            offset: if end { line.end } else { line.start },
            upstream: end,
        }
    }

    /// `rangeFrame`'s boxes (LLP 1114.007), field-local, as Chrome's
    /// `Range.getClientRects()` gives them for a textarea: each a text box,
    /// its glyphs across and its font's content area down. Collapsed, the
    /// caret's at width 0, upstream at a wrap (the line it ends); wider,
    /// each span's the range overlaps, then a zero-width one at the end of a
    /// line whose break it holds, which a bound skips unless it is alone.
    /// `None` past the text.
    pub(crate) fn range(&self, start: u32, end: u32) -> Option<Vec<Rect>> {
        if start > end || end > self.len {
            return None;
        }
        let at = |line: &Line, span: Option<&Span>| {
            span.map_or(line.text, |s| (line.baseline - s.ascent, s.height))
        };
        if start == end {
            let cursor = Cursor {
                offset: start,
                upstream: self.wrap_boundary(start),
            };
            let line = self.lines.get(self.line_index(cursor))?;
            // The run the offset closes and the one it opens meet there,
            // or (where two directions meet) stand apart: Chrome takes the
            // first in visual order, left to right, whatever the line's
            // direction (measured: `abc אבג` at 4 is the space's end, and
            // under `direction: rtl` `אבג abc` at 4 is `abc`'s left edge).
            let before = line.spans.iter().find(|s| line.base + s.end == start);
            let after = line.spans.iter().find(|s| start < line.base + s.end);
            let leftmost = |s: &Span| s.left.min(s.right);
            let (x, span) = match (before, after) {
                (Some(b), Some(a)) if leftmost(a) < leftmost(b) => (a.left, Some(a)),
                (Some(s), _) => (s.right, Some(s)),
                (None, Some(s)) => (s.left, Some(s)),
                (None, None) => line
                    .spans
                    .last()
                    .map_or((line.empty_x, None), |s| (s.right, Some(s))),
            };
            let (top, height) = at(line, span);
            return Some(vec![(x, top, 0., height)]);
        }
        let mut rects = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            for s in line
                .spans
                .iter()
                .filter(|s| line.base + s.start < end && line.base + s.end > start)
            {
                let (left, right) = (s.left.min(s.right), s.left.max(s.right));
                rects.push((left, line.baseline - s.ascent, right - left, s.height));
            }
            // The break's characters (a CR LF is two) lie between the lines.
            let broken = self
                .lines
                .get(index + 1)
                .is_some_and(|next| next.start > line.end && start < next.start);
            if broken && end > line.end {
                let last = line.spans.last();
                let x = last.map_or(line.empty_x, |s| s.right);
                let (top, height) = at(line, last);
                rects.push((x, top, 0., height));
            }
        }
        Some(rects)
    }

    pub(crate) fn highlight(&self, start: u32, end: u32, visible: Option<Range<f32>>) -> Vec<Rect> {
        let lines = visible.map_or(0..self.lines.len(), |clip| {
            let first = self
                .lines
                .partition_point(|l| l.top + l.height < clip.start);
            let count = self.lines[first..].partition_point(|l| l.top <= clip.end);
            first..first + count
        });
        self.marks(start, end, lines, |line| (line.top, line.height))
    }

    pub(crate) fn underlines(&self, start: u32, end: u32, thickness: f32) -> Vec<Rect> {
        self.marks(start, end, 0..self.lines.len(), |line| {
            (line.baseline + thickness, thickness)
        })
    }

    fn marks(
        &self,
        start: u32,
        end: u32,
        lines: Range<usize>,
        vertical: impl Fn(&Line) -> (f32, f32),
    ) -> Vec<Rect> {
        let mut rects: Vec<Rect> = Vec::new();
        for index in lines {
            let line = &self.lines[index];
            let (top, height) = vertical(line);
            let mut pieces: Vec<_> = line
                .spans
                .iter()
                .filter(|s| line.base + s.start < end && line.base + s.end > start)
                .map(|s| (s.left.min(s.right), s.left.max(s.right)))
                .collect();
            // A selected hard break also marks an empty line.
            if start <= line.end
                && end > line.end
                && self
                    .lines
                    .get(index + 1)
                    .is_some_and(|l| l.start > line.end)
            {
                let x = line.spans.last().map_or(line.empty_x, |s| s.right);
                pieces.push((x, x + line.height / 4.));
            }
            pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (left, right) in pieces {
                if let Some(last) = rects
                    .last_mut()
                    .filter(|r| r.1 == top && left <= r.0 + r.2 + 0.01)
                {
                    last.2 = right.max(last.0 + last.2) - last.0;
                } else {
                    rects.push((left, top, right - left, height));
                }
            }
        }
        rects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_kernel::{StyleProps, WhiteSpace};

    #[test]
    fn edited_rows_keep_their_hit_spans_when_their_offsets_and_baselines_move() {
        let style = StyleProps::default().with_rows(|rows| rows.white_space = WhiteSpace::PreWrap);
        let spec = |text| crate::paint::text_spec(&style, text);
        let mut engine = super::super::fixtures::engine();
        let tail = "office e\u{301} אבג long wrapped row\nend\n";
        let before = format!("first 😀\n{tail}");
        let after = format!("first 😀\r\nnew\n{tail}");
        let old = engine.edited_field(&spec(&before), Some(70.), None);
        let original = Geometry::new(&old, &before, false, 0., 0., None);
        let new = engine.edited_field(&spec(&after), Some(70.), Some(&old));
        let edited = Geometry::new(&new, &after, false, 0., 0., Some((&old, &original)));
        let fresh = Geometry::new(&new, &after, false, 0., 0., None);

        let old_tail = old.layout_runs().position(|run| run.line_i == 1).unwrap();
        let new_tail = new.layout_runs().position(|run| run.line_i == 2).unwrap();
        assert!(original.lines.len() - old_tail > 3, "the suffix wraps");
        for (a, b) in original.lines[old_tail..]
            .iter()
            .zip(&edited.lines[new_tail..])
        {
            if !a.spans.is_empty() {
                assert!(Rc::ptr_eq(&a.spans, &b.spans));
                assert!(b.base > a.base && b.baseline > a.baseline);
            }
        }
        let end = after.encode_utf16().count() as u32;
        assert_eq!(edited.range(0, end), fresh.range(0, end));
        assert_eq!(
            edited.highlight(0, end, None),
            fresh.highlight(0, end, None)
        );
        for offset in 0..=end {
            assert_eq!(edited.range(offset, offset), fresh.range(offset, offset));
            for trailing in [false, true] {
                assert_eq!(
                    edited.selection_edge(offset, trailing),
                    fresh.selection_edge(offset, trailing)
                );
            }
        }
        for row in &fresh.lines {
            for x in [0., 15.5, 43., 100.] {
                let y = row.top + row.height / 2.;
                assert_eq!(edited.hit(x, y).offset, fresh.hit(x, y).offset);
                assert_eq!(edited.hit_character(x, y), fresh.hit_character(x, y));
            }
        }
    }
}
