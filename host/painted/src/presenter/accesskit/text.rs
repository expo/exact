use crate::presenter::desktop_access::{FieldTextLayout, FieldTextRun};
use accesskit::{Node, NodeId, Rect, Role};
use unicode_segmentation::UnicodeSegmentation;

/// AccessKit indexes a grapheme within a text run. The editor indexes UTF-16 units in the whole value.
pub(super) struct FieldRuns {
    pub(super) layout: Option<FieldTextLayout>,
    pub(super) runs: Vec<FieldRun>,
}

pub(super) struct FieldRun {
    previous_on_line: bool,
    next_on_line: bool,
    bytes: usize,
    pub(super) start: u32,
    offsets: Vec<u32>,
}

impl FieldRuns {
    pub(super) fn id(field: NodeId, index: usize) -> NodeId {
        NodeId((field.0 << 32) | (index as u64 + 1))
    }

    pub(super) fn update(
        field: NodeId,
        text: &str,
        layout: Option<FieldTextLayout>,
        previous: Option<(&str, Self)>,
        changed: &mut Vec<(NodeId, Node)>,
    ) -> Self {
        let previous = match previous {
            Some((before, mut runs))
                if before == text
                    && match (&layout, &runs.layout) {
                        (Some(new), Some(old)) => new.same_shape(old),
                        (None, None) => true,
                        _ => false,
                    } =>
            {
                runs.layout = layout;
                return runs;
            }
            other => other,
        };
        let mut shapes: Vec<_> = layout.iter().flat_map(FieldTextLayout::runs).collect();
        let positioned = layout.as_ref().is_some_and(|l| l.transform.is_some());
        if !positioned {
            shapes.dedup_by_key(|run| run.line);
        }
        let starts = layout.as_ref().map(|_| shapes.iter().map(|run| run.start));
        let metrics = if positioned { shapes.as_slice() } else { &[] };
        let mut runs = Self::build(field, text, starts, metrics, previous, changed);
        runs.layout = layout;
        runs
    }

    fn build(
        field: NodeId,
        text: &str,
        starts: Option<impl Iterator<Item = u32>>,
        shapes: &[FieldTextRun<'_>],
        previous: Option<(&str, Self)>,
        changed: &mut Vec<(NodeId, Node)>,
    ) -> Self {
        fn hard_lines(text: &str) -> impl Iterator<Item = &str> {
            text.split_inclusive('\n')
                .chain((text.is_empty() || text.ends_with('\n')).then_some(""))
        }
        let (mut before, prior) = previous.unwrap_or((
            "",
            Self {
                layout: None,
                runs: Vec::new(),
            },
        ));
        let mut old_shapes = prior
            .layout
            .iter()
            .filter(|l| l.transform.is_some())
            .flat_map(FieldTextLayout::runs);
        let mut previous = prior.runs.into_iter().map(|line| {
            let (old, rest) = before.split_at(line.bytes);
            before = rest;
            (old, line)
        });
        let slices = if let Some(starts) = starts {
            let mut chars = text.chars();
            let (mut units, mut byte) = (0, 0);
            let mut slices = Vec::new();
            for next in starts.skip(1) {
                while units < next {
                    let Some(ch) = chars.next() else { break };
                    units += ch.len_utf16() as u32;
                }
                let end = text.len() - chars.as_str().len();
                slices.push(&text[byte..end]);
                byte = end;
            }
            slices.push(&text[byte..]);
            slices
        } else {
            hard_lines(text).collect()
        };
        let mut runs = Vec::new();
        let mut start = 0;
        for line in slices {
            let i = runs.len();
            let shape = shapes.get(i);
            let older = old_shapes.next();
            let same_geometry = || match (shape, older) {
                (Some(new), Some(old)) => new.same_metrics(&old),
                (None, None) => true,
                _ => false,
            };
            let previous_on_line = i > 0 && shape.is_some_and(|s| shapes[i - 1].line == s.line);
            let next_on_line = shape
                .zip(shapes.get(i + 1))
                .is_some_and(|(a, b)| a.line == b.line);
            let offsets = match previous.next() {
                Some((old, run))
                    if old == line
                        && previous_on_line == run.previous_on_line
                        && next_on_line == run.next_on_line
                        && same_geometry() =>
                {
                    run.offsets
                }
                _ => {
                    let (lengths, offsets) = text_offsets(line, 0);
                    let mut run = Node::new(Role::TextRun);
                    run.set_value(line);
                    run.set_character_lengths(lengths);
                    if let Some(s) = shape {
                        let (x, y, w, h) = s.bounds;
                        run.set_bounds(Rect::new(
                            x.into(),
                            y.into(),
                            (x + w).into(),
                            (y + h).into(),
                        ));
                        let (positions, widths) = s.character_metrics(start, &offsets);
                        run.set_character_positions(positions);
                        run.set_character_widths(widths);
                        run.set_text_direction(if s.rtl {
                            accesskit::TextDirection::RightToLeft
                        } else {
                            accesskit::TextDirection::LeftToRight
                        });
                        if previous_on_line {
                            run.set_previous_on_line(Self::id(field, i - 1));
                        }
                        if next_on_line {
                            run.set_next_on_line(Self::id(field, i + 1));
                        }
                    }
                    changed.push((Self::id(field, i), run));
                    offsets
                }
            };
            let end = start + offsets.last().unwrap();
            runs.push(FieldRun {
                previous_on_line,
                next_on_line,
                bytes: line.len(),
                start,
                offsets,
            });
            start = end;
        }
        Self { layout: None, runs }
    }

    pub(super) fn position(&self, field: NodeId, offset: u32) -> accesskit::TextPosition {
        let line = self
            .runs
            .partition_point(|line| line.start <= offset)
            .saturating_sub(1);
        let FieldRun { start, offsets, .. } = &self.runs[line];
        accesskit::TextPosition {
            node: Self::id(field, line),
            character_index: offsets
                .partition_point(|n| *n < offset - start)
                .min(offsets.len() - 1),
        }
    }

    pub(super) fn run(&self, field: NodeId, run: NodeId) -> Option<&FieldRun> {
        if run.0 >> 32 != field.0 {
            return None;
        }
        self.runs.get((run.0 as u32).checked_sub(1)? as usize)
    }

    pub(super) fn offset(&self, field: i32, at: accesskit::TextPosition) -> Option<u32> {
        let line = self.run(NodeId(field as u64), at.node)?;
        Some(line.start + line.offsets.get(at.character_index)?)
    }
}

fn text_offsets(text: &str, mut offset: u32) -> (Vec<u8>, Vec<u32>) {
    let mut lengths = Vec::new();
    let mut offsets = vec![offset];
    for grapheme in text.graphemes(true) {
        // AccessKit's byte count is u8; an unbounded combining sequence must be split.
        let mut push = |piece: &str| {
            lengths.push(piece.len() as u8);
            offset += piece.encode_utf16().count() as u32;
            offsets.push(offset);
        };
        if grapheme.len() <= u8::MAX as usize {
            push(grapheme);
        } else {
            for (at, ch) in grapheme.char_indices() {
                push(&grapheme[at..at + ch.len_utf8()]);
            }
        }
    }
    (lengths, offsets)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_field_reuses_unchanged_text_runs_as_utf16_positions_move() {
        let id = NodeId(42);
        let old = "a\r\n🦀e\u{301}\n";
        let new = "start\r\n🦀e\u{301}\n";
        let mut changed = Vec::new();
        let runs = FieldRuns::update(id, old, None, None, &mut changed);
        assert_eq!(changed.len(), 3);
        let kept = runs.runs[1].offsets.as_ptr();
        changed.clear();
        let runs = FieldRuns::update(id, old, None, Some((old, runs)), &mut changed);
        assert!(changed.is_empty());
        let runs = FieldRuns::update(id, new, None, Some((old, runs)), &mut changed);
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].0, FieldRuns::id(id, 0));
        assert_eq!(runs.runs[1].offsets.as_ptr(), kept);
        for offset in [0, 7, 9, 11, 12] {
            let position = runs.position(id, offset);
            assert_eq!(runs.offset(42, position), Some(offset));
            assert_eq!(runs.offset(43, position), None);
        }
        assert_eq!(runs.position(id, 12).node, FieldRuns::id(id, 2));
    }

    #[test]
    fn text_runs_follow_wrapping_without_splitting_graphemes_or_adding_line_breaks() {
        let id = NodeId(42);
        let text = "a🦀e\u{301} word\n";
        let update = |starts: &[u32], previous| {
            let mut changed = Vec::new();
            let runs = FieldRuns::build(
                id,
                text,
                Some(starts.iter().copied()),
                &[],
                previous,
                &mut changed,
            );
            (runs, changed)
        };
        let (narrow, changed) = update(&[0, 3, 6, 11], None);
        assert_eq!(
            changed
                .iter()
                .map(|(_, n)| n.value().unwrap())
                .collect::<Vec<_>>(),
            ["a🦀", "e\u{301} ", "word\n", ""]
        );
        assert_eq!(narrow.position(id, 6).node, FieldRuns::id(id, 2));
        let (wide, changed) = update(&[0, 11], Some((text, narrow)));
        assert_eq!(changed[0].1.value(), Some(text));
        assert_eq!(wide.offset(42, wide.position(id, 6)), Some(6));
        assert!(wide.run(id, FieldRuns::id(id, 2)).is_none());
    }

    #[test]
    fn accessibility_text_offsets_preserve_graphemes_utf16_and_crlf() {
        let (lengths, offsets) = text_offsets("a🦀e\u{301}\r\n", 5);
        assert_eq!(lengths, [1, 4, 3, 2]);
        assert_eq!(offsets, [5, 6, 8, 10, 12]);
        let long = format!("a{}", "\u{301}".repeat(200));
        let (lengths, offsets) = text_offsets(&long, 0);
        assert_eq!(
            lengths.iter().map(|n| *n as usize).sum::<usize>(),
            long.len()
        );
        assert_eq!(offsets.last(), Some(&201));
        assert_eq!(text_offsets("", 5), (vec![], vec![5]));
    }
}
