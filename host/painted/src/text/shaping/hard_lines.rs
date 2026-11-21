//! Split the changed part of a field between hard-line boundaries in its previous layout.
use super::{cache, HardLine, Run, ShapedSource, Spec, PARAGRAPH_BREAKS};
use std::ops::Range;

/// Whole lines before and after the replacement keep their shapes. Only
/// the bytes between these slices need splitting in the new paragraph.
pub(super) struct LineEdit<'a> {
    pub(super) prefix: &'a [HardLine],
    pub(super) bytes: Range<usize>,
    pub(super) suffix: &'a [HardLine],
}

impl<'a> LineEdit<'a> {
    pub(super) fn between(spec: &Spec, old: Option<&'a ShapedSource>) -> Self {
        let len = |spec: &Spec| {
            spec.runs
                .iter()
                .filter(|r| r.inline.is_none())
                .map(|r| r.text.len())
                .sum::<usize>()
        };
        let new_len = len(spec);
        let Some(old) = old.filter(|s| s.data.lines.len() > 1) else {
            return Self {
                prefix: &[],
                bytes: 0..new_len,
                suffix: &[],
            };
        };
        let old_len = len(&old.spec);
        let pairs = old.spec.runs.iter().zip(&spec.runs);
        let prefix = matching_bytes(pairs.clone(), false);
        // These slices keep their run bases. The per-line matcher can also
        // reuse a line whose runs moved within the paragraph.
        let suffix = if prefix < old_len.min(new_len) && old.spec.runs.len() == spec.runs.len() {
            matching_bytes(pairs.rev(), true)
                .min(old_len - prefix)
                .min(new_len - prefix)
        } else {
            0
        };
        let lines = &old.data.lines;
        let mut first = lines
            .partition_point(|line| line.start <= prefix)
            .saturating_sub(1);
        if first > 0 && lines[first].start == prefix {
            // A new LF can complete the preceding CR. Truncating here can
            // also remove the empty final line. Split that separator again.
            let mut offset = prefix;
            let next = spec.runs.iter().find_map(|run| {
                let byte = run.text.as_bytes().get(offset);
                offset = offset.saturating_sub(run.text.len());
                byte
            });
            if matches!(next, None | Some(b'\n')) {
                first -= 1;
            }
        }
        let last = lines.partition_point(|line| line.start <= old_len - suffix);
        let end = lines
            .get(last)
            .map_or(new_len, |line| new_len - (old_len - line.start));
        Self {
            prefix: &lines[..first],
            bytes: lines[first].start..end,
            suffix: &lines[last..],
        }
    }
}

fn matching_bytes<'a>(pairs: impl Iterator<Item = (&'a Run, &'a Run)>, reverse: bool) -> usize {
    let mut matched = 0;
    for (before, after) in pairs {
        if !cache::same_run_style(before, after) {
            break;
        }
        if before.text != after.text {
            let a = before.text.bytes();
            let b = after.text.bytes();
            matched += if reverse {
                a.rev().zip(b.rev()).take_while(|(a, b)| a == b).count()
            } else {
                a.zip(b).take_while(|(a, b)| a == b).count()
            };
            break;
        }
        matched += before.text.len();
    }
    matched
}

#[cfg(test)]
thread_local! {
    pub(in crate::text) static SPLIT_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A hard line's runs: (range in the line, run index).
pub(super) type Pieces = Vec<(Range<usize>, usize)>;
/// A hard line's inline boxes: (byte offset in the line, run index).
pub(super) type Boxes = Vec<(usize, usize)>;
/// A hard line's runs with no text: (byte offset in the line, run index).
pub(super) type Empties = Vec<(usize, usize)>;

/// The paragraph's text split into hard lines as cosmic-text split it: at
/// each paragraph separator (`\r\n` one), the last empty piece dropped,
/// and one empty line for no text. Each line keeps its byte offset and
/// pieces: (range in the line, run index). An inline box's run has no text,
/// only its place in the line (LLP 1011.002), and so has a run with no text.
pub(super) fn hard_lines(
    spec: &Spec,
    bytes: Range<usize>,
) -> Vec<(usize, String, Pieces, Boxes, Empties)> {
    #[cfg(test)]
    SPLIT_BYTES.set(SPLIT_BYTES.get() + bytes.len());
    let mut out: Vec<(usize, String, Pieces, Boxes, Empties)> = vec![(
        bytes.start,
        String::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )];
    let mut offset = bytes.start;
    let mut run_start = 0;
    let mut after_cr = false;
    for (index, run) in spec.runs.iter().enumerate() {
        let start = run_start;
        if run.inline.is_none() {
            run_start += run.text.len();
        }
        if start > bytes.end {
            break;
        }
        if run_start < bytes.start {
            continue;
        }
        if run.inline.is_some() {
            after_cr = false;
            let (_, text, _, boxes, _) = out.last_mut().expect("one line");
            boxes.push((text.len(), index));
            continue;
        }
        if run.text.is_empty() {
            let (_, text, _, _, empties) = out.last_mut().expect("one line");
            empties.push((text.len(), index));
            continue;
        }
        let lo = bytes.start.saturating_sub(start);
        let hi = (bytes.end - start).min(run.text.len());
        let mut rest = &run.text[lo..hi];
        while !rest.is_empty() {
            let split = rest.find(PARAGRAPH_BREAKS);
            let (piece, sep) = match split {
                Some(at) => {
                    let ch = rest[at..].chars().next().expect("found");
                    (&rest[..at], Some((at, ch)))
                }
                None => (rest, None),
            };
            let consumed = sep.map_or(rest.len(), |(at, ch)| at + ch.len_utf8());
            if !piece.is_empty() {
                after_cr = false;
                let (_, text, pieces, _, _) = out.last_mut().expect("one line");
                let start = text.len();
                text.push_str(piece);
                pieces.push((start..text.len(), index));
            }
            match sep {
                Some((at, ch)) => {
                    // `\r\n` is one break.
                    let next = offset + consumed;
                    if ch == '\n' && after_cr && piece.is_empty() {
                        out.last_mut().expect("one line").0 = next;
                    } else {
                        out.push((next, String::new(), Vec::new(), Vec::new(), Vec::new()));
                    }
                    after_cr = ch == '\r';
                    rest = &rest[at + ch.len_utf8()..];
                }
                None => rest = "",
            }
            offset += consumed;
        }
    }
    if out.len() > 1
        && out
            .last()
            .is_some_and(|l| l.1.is_empty() && l.3.is_empty() && l.4.is_empty())
    {
        out.pop();
    }
    out
}
