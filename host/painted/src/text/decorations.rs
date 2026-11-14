//! A textarea's paint-only decorations on the shared Linux/Windows field
//! presenter (LLP 1114.007): each span's colour, background, shadow and
//! text-decoration line as its own run, with the field's font, so every
//! advance, line break and caret position is the undecorated field's.
use super::Spec;
use exact_kernel::style::TextShadow;
use exact_kernel::text::decorations::Decorations;
use exact_kernel::{ColorValue, StyleId};
use exact_runner::decorations::StyleTable;
use std::collections::BTreeMap;

#[derive(Debug)]
pub(crate) struct FieldDecorations {
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

    pub(crate) fn read(record: &str, styles: &str) -> Result<Self, String> {
        Ok(Self {
            record: exact_runner::decorations::decode(record)?,
            styles: StyleTable::decode(styles)?,
        })
    }

    pub(crate) fn project(&self, mut spec: Spec, value: &str) -> Projection {
        let resolved = self.record.resolve(value);
        let length = value.encode_utf16().count() as u32;
        let mut events: BTreeMap<u32, Vec<(usize, bool)>> = BTreeMap::new();
        events.entry(0).or_default();
        events.entry(length).or_default();
        for (index, span) in resolved.spans.iter().enumerate() {
            if self.styles.get(&span.name).is_some() {
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
        let events: Vec<_> = events.into_iter().collect();
        let mut units = 0;
        let mut offsets = vec![(0, 0)];
        for (byte, character) in value.char_indices() {
            units += character.len_utf16() as u32;
            offsets.push((units, byte + character.len_utf8()));
        }
        let byte = |offset: u32| {
            offsets[offsets
                .partition_point(|(units, _)| *units < offset)
                .min(offsets.len() - 1)]
            .1
        };
        let mut runs = Vec::new();
        let mut inks = Vec::new();
        let mut backgrounds = Vec::new();
        let mut shadows = Vec::new();
        let mut active = BTreeMap::new();
        for pair in events.windows(2) {
            for &(index, starts) in &pair[0].1 {
                if starts {
                    active.insert(index, self.styles.get(&resolved.spans[index].name));
                } else {
                    active.remove(&index);
                }
            }
            let start = byte(pair[0].0);
            let end = byte(pair[1].0);
            if start == end {
                continue;
            }
            let mut run = spec.strut.clone();
            run.text = value[start..end].into();
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
            let mut run = spec.strut.clone();
            run.text = value.into();
            runs.push(run);
            inks.push(None);
            backgrounds.push(None);
            shadows.push(TextShadow::default());
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
}
