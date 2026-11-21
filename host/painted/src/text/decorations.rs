//! A textarea's paint-only decorations on the shared Linux/Windows field
//! presenter (LLP 1114.007): each span's colour, background, shadow and
//! text-decoration line as its own run, with the field's font, so every
//! advance, line break and caret position is the undecorated field's.
use super::Spec;
use exact_kernel::style::TextShadow;
use exact_kernel::text::decorations::Decorations;
use exact_kernel::{ColorValue, PropValue, StyleId};
use exact_runner::decorations::StyleTable;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Debug)]
pub(crate) struct FieldDecorations {
    inputs: (PropValue, PropValue),
    record: Decorations,
    styles: StyleTable,
}

pub(crate) struct Projection {
    pub spec: Spec,
    pub inks: Vec<Option<ColorValue>>,
    pub backgrounds: Vec<Option<Option<ColorValue>>>,
    pub shadows: Vec<TextShadow>,
}

impl FieldDecorations {
    pub(crate) fn state(&self, value: &str) -> serde_json::Value {
        let resolved = self.record.resolve(value);
        let spans: Vec<_> = resolved.spans.iter().filter(|span|self.styles.names().any(|name|name==span.name)).map(|span| {
            serde_json::json!({"name":span.name,"start":span.range.start,"end":span.range.end})
        }).collect();
        serde_json::json!({"source":value,"recordValue":self.record.value,"provisional":resolved.provisional,"spans":spans})
    }

    pub(crate) fn cached(
        record: &PropValue,
        styles: &PropValue,
        previous: Option<&Rc<Self>>,
    ) -> Option<Rc<Self>> {
        if let Some(previous) = previous.filter(|p| &p.inputs.0 == record && &p.inputs.1 == styles)
        {
            return Some(previous.clone());
        }
        Some(Rc::new(Self {
            record: exact_runner::decorations::decode(record.as_str()?).ok()?,
            styles: StyleTable::decode(styles.as_str()?).ok()?,
            inputs: (record.clone(), styles.clone()),
        }))
    }

    #[cfg(test)]
    fn read(record: &str, styles: &str) -> Option<Rc<Self>> {
        Self::cached(
            &PropValue::Str(record.into()),
            &PropValue::Str(styles.into()),
            None,
        )
    }

    pub(crate) fn project(&self, mut spec: Spec, value: &str) -> Projection {
        let plain = |spec| Projection {
            spec,
            inks: vec![None],
            backgrounds: vec![None],
            shadows: vec![TextShadow::default()],
        };
        if self.record.spans.is_empty() {
            return plain(spec);
        }
        let resolved;
        let spans = if self.record.value == value {
            &self.record.spans
        } else {
            resolved = self.record.resolve(value);
            &resolved.spans
        };
        let mut events: BTreeMap<u32, Vec<(usize, bool)>> = BTreeMap::new();
        events.entry(0).or_default();
        for (index, span) in spans.iter().enumerate() {
            if span.range.start < span.range.end && self.styles.get(&span.name).is_some() {
                events
                    .entry(span.range.start)
                    .or_default()
                    .push((index, true));
                events
                    .entry(span.range.end)
                    .or_default()
                    .push((index, false));
            }
        }
        if events.len() == 1 {
            return plain(spec);
        }
        let mut events = events.into_iter().peekable();
        let mut chars = value.chars();
        let mut units = 0_u64;
        let mut start = 0;
        let mut runs = Vec::new();
        let mut inks = Vec::new();
        let mut backgrounds = Vec::new();
        let mut shadows = Vec::new();
        let mut active = BTreeMap::new();
        while let Some((_, changes)) = events.next() {
            for (index, starts) in changes {
                if starts {
                    active.insert(index, self.styles.get(&spans[index].name));
                } else {
                    active.remove(&index);
                }
            }
            // Boundaries arrive in UTF-16 order. Consuming the whole scalar also rounds a split surrogate up.
            let end = if let Some((offset, _)) = events.peek() {
                while units < u64::from(*offset) {
                    let Some(ch) = chars.next() else { break };
                    units += ch.len_utf16() as u64;
                }
                value.len() - chars.as_str().len()
            } else {
                value.len()
            };
            let segment = &value[start..end];
            start = end;
            if segment.is_empty() {
                continue;
            }
            let mut run = spec.strut.clone();
            run.text = segment.into();
            let (mut ink, mut background) = (None, None);
            let mut shadow = TextShadow::default();
            for style in active.values().flatten() {
                let has = |id| style.mask.has(id);
                if has(StyleId::TextColor) {
                    ink = Some(style.text_color)
                }
                if has(StyleId::BackgroundColor) {
                    background = Some(style.background_color)
                }
                if has(StyleId::TextShadow) {
                    shadow = style.text_shadow;
                }
                if has(StyleId::TextDecorationLine) {
                    use exact_kernel::TextDecorationLine as Line;
                    run.mark = match style.text_decoration_line {
                        Line::None => 0,
                        Line::Underline => super::markup::UNDERLINE,
                        Line::LineThrough => super::markup::STRIKE,
                        Line::UnderlineLineThrough => {
                            super::markup::UNDERLINE | super::markup::STRIKE
                        }
                    };
                }
            }
            runs.push(run);
            inks.push(ink);
            backgrounds.push(background);
            shadows.push(shadow);
        }
        if runs.is_empty() {
            return plain(spec);
        }
        spec.runs = runs;
        Projection {
            spec,
            inks,
            backgrounds,
            shadows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_kernel::{StyleProps, WhiteSpace};
    use serde_json::json;

    fn record(value: &str, spans: serde_json::Value) -> String {
        json!({"value":value,"spans":spans}).to_string()
    }
    fn spec(value: &str) -> Spec {
        crate::paint::text_spec(
            &StyleProps::default().with_rows(|rows| {
                rows.font_size = 16.;
                rows.white_space = WhiteSpace::PreWrap;
            }),
            value,
        )
    }
    fn text(spec: &Spec) -> String {
        spec.runs.iter().map(|run| run.text.as_str()).collect()
    }

    #[test]
    fn spans_split_runs_at_utf16_boundaries_and_later_spans_win() {
        let source = "a😀bold z";
        let record = record(
            source,
            json!([
                {"name":"red","start":1,"end":7},
                {"name":"blue","start":3,"end":7},
            ]),
        );
        let styles = json!({
            "red":[[StyleId::TextColor as u16,"#ff0000"],[StyleId::TextDecorationLine as u16,"underline"]],
            "blue":[[StyleId::TextColor as u16,"#0000ff"],[StyleId::BackgroundColor as u16,"#ffff00"]]
        });
        let decorated = FieldDecorations::read(&record, &styles.to_string())
            .unwrap()
            .project(spec(source), source);
        let runs = &decorated.spec.runs;
        assert_eq!(text(&decorated.spec), source);
        assert_eq!(
            runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>(),
            ["a", "😀", "bold", " z"]
        );
        let plain = spec(source).strut;
        assert!(
            runs.iter().all(|run| run.size == plain.size
                && run.weight == plain.weight
                && run.family == plain.family
                && run.italic == plain.italic),
            "a decoration never changes the font"
        );
        assert_eq!(runs[1].mark, super::super::markup::UNDERLINE);
        assert_eq!(
            crate::paint::rgba(decorated.inks[1].unwrap().resolve(false)),
            [255, 0, 0, 255]
        );
        assert_eq!(
            crate::paint::rgba(decorated.inks[2].unwrap().resolve(false)),
            [0, 0, 255, 255]
        );
        assert_eq!(
            crate::paint::rgba(decorated.backgrounds[2].unwrap().unwrap().resolve(false)),
            [255, 255, 0, 255]
        );
        assert!(decorated.inks[3].is_none());
    }

    #[test]
    fn split_surrogates_and_out_of_bounds_ends_share_scalar_boundaries() {
        let source = "😀end";
        let record = record(
            source,
            json!([
                {"name":"line","start":0,"end":1},
                {"name":"line","start":1,"end":2},
                {"name":"line","start":3,"end":99},
                {"name":"line","start":4,"end":1},
            ]),
        );
        let styles = json!({"line":[[StyleId::TextDecorationLine as u16,"underline"]]}).to_string();
        let projected = FieldDecorations::read(&record, &styles)
            .unwrap()
            .project(spec(source), source);
        let runs: Vec<_> = projected
            .spec
            .runs
            .iter()
            .map(|run| (run.text.as_str(), run.mark))
            .collect();
        let line = super::super::markup::UNDERLINE;
        assert_eq!(runs, [("😀", line), ("e", 0), ("nd", line)]);
    }

    #[test]
    fn decoded_inputs_are_reused_until_the_record_or_styles_change() {
        let empty = PropValue::Str(record("a", json!([])).into());
        let marked = PropValue::Str(record("a", json!([{"name":"ink","start":0,"end":1}])).into());
        let bare = PropValue::Str("{}".into());
        let styled = PropValue::Str(
            json!({"ink":[[StyleId::TextColor as u16,"#ff0000"]]})
                .to_string()
                .into(),
        );
        let first = FieldDecorations::cached(&empty, &bare, None).unwrap();
        let same = FieldDecorations::cached(&empty, &bare, Some(&first)).unwrap();
        assert!(Rc::ptr_eq(&first, &same));
        let changed = FieldDecorations::cached(&marked, &bare, Some(&same)).unwrap();
        assert!(!Rc::ptr_eq(&same, &changed));
        let visible = FieldDecorations::cached(&marked, &styled, Some(&changed)).unwrap();
        assert!(changed.project(spec("a"), "a").inks[0].is_none());
        assert!(visible.project(spec("a"), "a").inks[0].is_some());
    }

    #[test]
    fn stale_spans_reconcile_and_unknown_names_paint_nothing() {
        let record = record(
            "@a end",
            json!([{"name":"line","start":0,"end":2},{"name":"missing","start":3,"end":6}]),
        );
        let styles = json!({"line":[[StyleId::TextDecorationLine as u16,"underline"]]}).to_string();
        let source = "X @a end";
        let decorations = FieldDecorations::read(&record, &styles).unwrap();
        let projected = decorations.project(spec(source), source);
        assert_eq!(text(&projected.spec), source);
        let span = projected
            .spec
            .runs
            .iter()
            .find(|run| run.mark == super::super::markup::UNDERLINE)
            .unwrap();
        // Text inserted at a span's start joins it, as at a live DOM Range's start.
        assert_eq!(span.text, "X @a");
        let state = decorations.state(source);
        assert_eq!(state["provisional"], true);
        assert_eq!(state["spans"], json!([{"name":"line","start":0,"end":4}]));
    }

    #[test]
    fn overlapping_shadows_resolve_per_attribute_and_none_clears_the_previous_shadow() {
        let source = "abcd";
        let record = record(
            source,
            json!([
                {"name":"shadow","start":0,"end":4},
                {"name":"ink","start":1,"end":3},
                {"name":"clear","start":2,"end":4},
            ]),
        );
        let styles = json!({
            "shadow":[[StyleId::TextShadow as u16,"-4px 8px 6px"]],
            "ink":[[StyleId::TextColor as u16,"#0000ff"]],
            "clear":[[StyleId::TextShadow as u16,"none"]],
        });
        let projected = FieldDecorations::read(&record, &styles.to_string())
            .unwrap()
            .project(spec(source), source);
        let shadow = TextShadow::parse("-4px 8px 6px").unwrap();
        assert_eq!(
            projected.shadows,
            [shadow, shadow, TextShadow::default(), TextShadow::default()]
        );
        assert!(projected.shadows[1].shadow().unwrap().color.is_none());
        assert!(projected.inks[0].is_none());
        assert_eq!(projected.inks[1], projected.inks[2]);
        assert!(projected.inks[3].is_none());
    }

    #[test]
    fn many_disjoint_spans_keep_source_and_precedence_without_rescanning_the_document() {
        let source = "ab".repeat(10_000);
        let spans: Vec<_> = (0..10_000)
            .map(|index| json!({"name":"red","start":index*2,"end":index*2+1}))
            .collect();
        let styles = json!({"red":[[StyleId::TextColor as u16,"#ff0000"]]}).to_string();
        let projected = FieldDecorations::read(&record(&source, json!(spans)), &styles)
            .unwrap()
            .project(spec(&source), &source);
        assert_eq!(projected.spec.runs.len(), 20_000);
        assert_eq!(text(&projected.spec), source);
        assert!(projected
            .inks
            .iter()
            .enumerate()
            .all(|(index, ink)| ink.is_some() == (index % 2 == 0)));
    }
    #[test]
    fn editing_a_coloured_multiline_field_keeps_unchanged_rows() {
        use crate::text::{field::Geometry, fixtures::engine};
        let mut engine = engine();
        let styles = json!({"accent":[[StyleId::TextColor as u16,"#db1141"]]}).to_string();
        let projected = |value: &str, end: usize| {
            let marks = record(value, json!([{"name":"accent","start":0,"end":end}]));
            FieldDecorations::read(&marks, &styles)
                .unwrap()
                .project(spec(value), value)
                .spec
        };
        let mut value = (0..1024)
            .map(|i| format!("{i:04} ordinary source"))
            .collect::<Vec<_>>()
            .join("\n");
        let width = Some(280.);
        let mut old = engine.edited_field(&projected(&value, 5), width, None);
        let mut geometry = Geometry::new(&old, &value, false, 0., 0., None);
        for key in ['a', 'b', 'c', 'd'] {
            value.insert(0, key);
            let next = engine.edited_field(&projected(&value, 5), width, Some(&old));
            let edited = Geometry::new(&next, &value, false, 0., 0., Some((&old, &geometry)));
            let (before, after) = (old.lines(), next.lines());
            assert_eq!(after.lines.len(), 1024);
            let shared = (1..1024)
                .filter(|&row| {
                    before.glyphs_of(&before.lines[row]).as_ptr()
                        == after.glyphs_of(&after.lines[row]).as_ptr()
                })
                .count();
            let hits = (1..1024)
                .filter(|&row| edited.shares_spans(row, &geometry, row))
                .count();
            assert_eq!((shared, hits), (1023, 1023), "after typing {key}");
            old = next;
            geometry = edited;
        }
        // Moving only the colour boundary leaves the text intact but changes glyph paint indices.
        let recoloured = projected(&value, 30);
        let next = engine.edited_field(&recoloured, width, Some(&old));
        let fresh = crate::text::fixtures::engine().layout(&recoloured, width);
        for (line, expected) in next.lines().lines.iter().zip(&fresh.lines().lines) {
            assert_eq!(
                next.lines().glyphs_of(line),
                fresh.lines().glyphs_of(expected)
            );
        }
    }

    #[test]
    fn moving_or_clearing_a_field_highlight_keeps_the_untouched_glyph_rows() {
        use crate::text::{field::Geometry, fixtures::engine, shaping::shape_line_calls};
        let mut engine = engine();
        let styles = json!({"accent":[[StyleId::TextColor as u16,"#db1141"]]}).to_string();
        let projected = |value: &str, start: Option<usize>| {
            let spans: Vec<_> = start
                .into_iter()
                .map(|byte| value[..byte].encode_utf16().count())
                .map(|start| json!({"name":"accent","start":start,"end":start + 6}))
                .collect();
            FieldDecorations::read(&record(value, json!(spans)), &styles)
                .unwrap()
                .project(spec(value), value)
        };
        let original = (0..128)
            .map(|i| format!("export const line_{i} = '{i}😀';"))
            .collect::<Vec<_>>()
            .join("\n");
        let last = original.rfind('\n').unwrap() + 1;
        let penultimate = original[..last - 1].rfind('\n').unwrap() + 1;
        let mut edited = original.clone();
        edited.insert(last, '\n');
        let width = Some(600.);
        let old = engine.edited_field(&projected(&original, Some(0)).spec, width, None);
        let geometry = Geometry::new(&old, &original, false, 0., 0., None);
        for start in [None, Some(penultimate)] {
            let current = projected(&edited, start);
            let before = shape_line_calls();
            let next = engine.edited_field(&current.spec, width, Some(&old));
            let shaped = shape_line_calls() - before;
            let hits = Geometry::new(&next, &edited, false, 0., 0., Some((&old, &geometry)));
            let (old_lines, new_lines) = (old.lines(), next.lines());
            for row in 1..128 {
                if start.is_some() && row == 126 {
                    continue;
                }
                let to = if row == 127 { row + 1 } else { row };
                assert_eq!(
                    old_lines.glyphs_of(&old_lines.lines[row]).as_ptr(),
                    new_lines.glyphs_of(&new_lines.lines[to]).as_ptr(),
                    "unchanged row {row}"
                );
                assert!(hits.shares_spans(to, &geometry, row));
            }
            // The original highlight boundary, the inserted LF and any new highlight boundary.
            assert_eq!(shaped, 2 + usize::from(start.is_some()));

            let fresh = crate::text::fixtures::engine().layout(&current.spec, width);
            assert_eq!(new_lines.lines, fresh.lines().lines);
            for (line, expected) in new_lines.lines.iter().zip(&fresh.lines().lines) {
                assert_eq!(new_lines.glyphs_of(line), fresh.lines().glyphs_of(expected));
            }
            let mut byte = 0;
            for line in next.layout_runs() {
                for glyph in line.glyphs {
                    let at = byte + glyph.start as usize;
                    assert_eq!(
                        current.inks[glyph.run(line.run_base)].is_some(),
                        start.is_some_and(|begin| (begin..begin + 6).contains(&at))
                    );
                }
                byte += line.text.len() + 1;
            }
            let expected = Geometry::new(&fresh, &edited, false, 0., 0., None);
            let caret = edited[..last + 1].encode_utf16().count() as u32;
            assert_eq!(hits.range(caret, caret), expected.range(caret, caret));
        }
    }
}
