//! `page(path, find)`: the open document as the terminal reader draws it
//! (`apps/llp/terminal.contract`, LLP 1101).
//!
//! The same parsed blocks as `document`, with three things a reader in a
//! terminal needs and the windowed reader does not ask for:
//!
//! - **Find in the page.** Every run is split where `find` matches, case
//!   insensitively, and each match carries its ordinal, so the screen can
//!   paint the current one apart from the rest; `marks` is the block each
//!   match is in, which `scrollIntoView` brings to the screen. A match that
//!   crosses two runs (half of it bold) is not found.
//! - **Links as a list.** A terminal cannot follow a click on a run of text,
//!   so the page lists its links, and says which name a document in this
//!   corpus (the reader opens those itself).
//! - **Neighbours.** The documents before and after this one in number
//!   order, for paging through a corpus.
//!
//! Block ids are `b0`, `b1`, …: an HTML `id` the screen puts on each block,
//! and what `marks` and the outline name.

use crate::Entry;
use exact_plan::Value;
use markdown_parse::{Document, Kind, Run};
use std::ops::Range;
use std::path::{Path, PathBuf};

type Link = (String, String, String);

/// The current document's query-independent page projection. `Value` objects
/// are immutable, so an unchanged block can be handed back as the same object.
pub(crate) struct Cache {
    path: PathBuf,
    scope: String,
    blocks: Vec<Value>,
    ranges: Vec<Range<usize>>,
    columns: Vec<Vec<usize>>,
    outline: Value,
    links: Value,
}

impl Cache {
    fn is_for(&self, path: &Path, scope: &str, doc: &Document) -> bool {
        self.path == path && self.scope == scope && self.ranges.len() == doc.blocks.len()
    }
}

/// Where the page sits in its corpus, and the text to find in it.
pub(crate) struct Place<'a> {
    pub entry: Option<&'a Entry>,
    pub prev: String,
    pub next: String,
    pub find: &'a str,
    /// A path's document in this corpus, as its number and title, or
    /// `None` for anything else.
    pub local: &'a dyn Fn(&str) -> Option<String>,
}

struct Parts {
    ranges: Vec<Range<usize>>,
    columns: Vec<Vec<usize>>,
    page: PageParts,
}

struct PageParts {
    blocks: Vec<Value>,
    outline: Value,
    hits: usize,
    marks: Vec<String>,
    links: Value,
}

struct Build<'a> {
    columns: &'a [Vec<usize>],
    find: &'a str,
    collect_links: bool,
    hits: usize,
    marks: Vec<String>,
    links: Vec<Link>,
    blocks: Vec<Value>,
}

impl<'a> Build<'a> {
    fn new(columns: &'a [Vec<usize>], find: &'a str, collect_links: bool, capacity: usize) -> Self {
        Self {
            columns,
            find,
            collect_links,
            hits: 0,
            marks: Vec::new(),
            links: Vec::new(),
            blocks: Vec::with_capacity(capacity),
        }
    }

    fn append(&mut self, i: usize, b: &markdown_parse::Block) {
        // An LLP's metadata header, one row per field.
        if let Some(fields) = metadata(i, b) {
            for (k, (label, runs)) in fields.into_iter().enumerate() {
                let id = format!("b{i}m{k}");
                let before = self.hits;
                let mut out = Vec::new();
                for r in &runs {
                    if self.collect_links && !r.href.is_empty() && !r.text.trim().is_empty() {
                        self.links
                            .push((r.text.clone(), r.href.clone(), id.clone()));
                    }
                }
                split(&id, &runs, false, self.find, &mut self.hits, &mut out);
                self.marks
                    .extend(std::iter::repeat_n(id.clone(), self.hits - before));
                self.blocks.push(Value::record(vec![
                    Value::str(&id),
                    Value::str("meta"),
                    Value::Number(0.0),
                    Value::str(&label),
                    Value::str(""),
                    Value::Bool(false),
                    Value::list(out),
                    Value::list(vec![]),
                ]));
            }
            return;
        }
        let id = format!("b{i}");
        let before = self.hits;
        let columns = self.columns;
        let find = self.find;
        let collect_links = self.collect_links;
        let (hits, links) = (&mut self.hits, &mut self.links);
        let (spans, cells) = {
            // Spans are named under `prefix` (the block's id, or a cell's),
            // so a span's id is the page's only one of that name.
            let mut spans_of = |prefix: &str, runs: &[Run], mono: bool| {
                let mut out = Vec::new();
                for r in runs {
                    if collect_links && !r.href.is_empty() && !r.text.trim().is_empty() {
                        links.push((r.text.clone(), r.href.clone(), id.clone()));
                    }
                }
                split(prefix, runs, mono, find, hits, &mut out);
                Value::list(out)
            };
            match b.kind {
                Kind::Code | Kind::Html => {
                    let run = Run {
                        text: b.text.clone(),
                        code: true,
                        ..Run::default()
                    };
                    (
                        spans_of(&id, std::slice::from_ref(&run), true),
                        Value::list(vec![]),
                    )
                }
                Kind::TableRow => {
                    let cells = b
                        .cells
                        .iter()
                        .enumerate()
                        .map(|(c, runs)| {
                            let grow = columns[i].get(c).copied().unwrap_or(1);
                            let cell = format!("{id}c{c}");
                            Value::record(vec![
                                Value::str(&cell),
                                spans_of(&cell, runs, false),
                                Value::Number(grow as f64),
                            ])
                        })
                        .collect();
                    (Value::list(vec![]), Value::list(cells))
                }
                Kind::Image => {
                    let run = Run {
                        text: b.text.clone(),
                        ..Run::default()
                    };
                    (
                        spans_of(&id, std::slice::from_ref(&run), false),
                        Value::list(vec![]),
                    )
                }
                _ => (spans_of(&id, &b.runs, false), Value::list(vec![])),
            }
        };
        let after = *hits;
        self.marks
            .extend(std::iter::repeat_n(id.clone(), after - before));
        self.blocks.push(Value::record(vec![
            Value::str(&id),
            Value::str(b.kind.name()),
            Value::Number(b.depth as f64),
            Value::str(&b.marker),
            Value::str(if b.kind == Kind::Code { &b.href } else { "" }),
            Value::Bool(b.header),
            spans,
            cells,
        ]));
    }
}

fn outline(doc: &Document) -> Value {
    Value::list(
        doc.outline()
            .iter()
            .map(|(level, text, at)| {
                Value::record(vec![
                    Value::str(&format!("b{at}")),
                    Value::str(text),
                    Value::Number(*level as f64),
                ])
            })
            .collect(),
    )
}

fn link_values(path: &Path, place: &Place<'_>, links: Vec<Link>) -> Value {
    let mut seen = std::collections::HashSet::new();
    Value::list(
        links
            .into_iter()
            // A document's own number (its title's `LLP 0001`) goes nowhere new.
            .filter(|(_, href, _)| Path::new(href) != path)
            .filter(|(text, href, _)| seen.insert((text.clone(), href.clone())))
            .enumerate()
            .map(|(i, (text, href, block))| {
                let local = (place.local)(&href);
                Value::record(vec![
                    Value::str(&i.to_string()),
                    Value::str(&text),
                    Value::Bool(local.is_some()),
                    Value::str(&href),
                    Value::str(&block),
                    Value::str(local.as_deref().unwrap_or(&href)),
                ])
            })
            .collect(),
    )
}

fn page_parts(
    path: &Path,
    doc: &Document,
    place: &Place<'_>,
    find: &str,
    columns: &[Vec<usize>],
    mut ranges: Option<&mut Vec<Range<usize>>>,
) -> PageParts {
    let mut build = Build::new(columns, find, true, doc.blocks.len());
    for (i, b) in doc.blocks.iter().enumerate() {
        let start = build.blocks.len();
        build.append(i, b);
        if let Some(ranges) = ranges.as_deref_mut() {
            ranges.push(start..build.blocks.len());
        }
    }
    let Build {
        blocks,
        hits,
        marks,
        links,
        ..
    } = build;
    PageParts {
        blocks,
        outline: outline(doc),
        hits,
        marks,
        links: link_values(path, place, links),
    }
}

fn parts(path: &Path, doc: &Document, place: &Place<'_>, find: &str) -> Parts {
    let columns = columns(doc);
    let mut ranges = Vec::with_capacity(doc.blocks.len());
    let page = page_parts(path, doc, place, find, &columns, Some(&mut ranges));
    Parts {
        ranges,
        columns,
        page,
    }
}

fn finish(path: &Path, doc: &Document, place: &Place<'_>, message: &str, page: PageParts) -> Value {
    let entry = place.entry;
    Value::record(vec![
        Value::str(&path.to_string_lossy()),
        Value::str(
            &path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
        Value::str(entry.map(|e| e.number.as_str()).unwrap_or("")),
        // The index's title, without its own `LLP NNNN:` prefix.
        Value::str(entry.map(|e| e.title.as_str()).unwrap_or(&doc.title)),
        Value::str(entry.map(|e| e.kind.as_str()).unwrap_or("")),
        Value::str(entry.map(|e| e.status.as_str()).unwrap_or("")),
        Value::str(message),
        Value::Bool(!message.is_empty()),
        Value::list(page.blocks),
        page.outline,
        Value::Number(page.hits as f64),
        Value::list(page.marks.iter().map(|m| Value::str(m)).collect()),
        page.links,
        Value::str(&place.prev),
        Value::str(&place.next),
    ])
}

/// The original `shape Page` assembler, retained as an independent test oracle.
#[cfg(test)]
fn reference_value(path: &Path, doc: &Document, place: &Place<'_>, message: &str) -> Value {
    let find = fold(place.find.trim());
    let mut hits = 0usize;
    let mut marks = Vec::new();
    let mut links = Vec::new();
    let mut blocks = Vec::with_capacity(doc.blocks.len());
    let columns = columns(doc);
    for (i, b) in doc.blocks.iter().enumerate() {
        // An LLP's metadata header, one row per field.
        if let Some(fields) = metadata(i, b) {
            for (k, (label, runs)) in fields.into_iter().enumerate() {
                let id = format!("b{i}m{k}");
                let before = hits;
                let mut out = Vec::new();
                for r in &runs {
                    if !r.href.is_empty() && !r.text.trim().is_empty() {
                        links.push((r.text.clone(), r.href.clone(), id.clone()));
                    }
                }
                split(&id, &runs, false, &find, &mut hits, &mut out);
                marks.extend(std::iter::repeat_n(id.clone(), hits - before));
                blocks.push(Value::record(vec![
                    Value::str(&id),
                    Value::str("meta"),
                    Value::Number(0.0),
                    Value::str(&label),
                    Value::str(""),
                    Value::Bool(false),
                    Value::list(out),
                    Value::list(vec![]),
                ]));
            }
            continue;
        }
        let id = format!("b{i}");
        let before = hits;
        // Spans are named under `prefix` (the block's id, or a cell's), so a
        // span's id is the page's only one of that name.
        let mut spans_of = |prefix: &str, runs: &[Run], mono: bool| {
            let mut out = Vec::new();
            for r in runs {
                if !r.href.is_empty() && !r.text.trim().is_empty() {
                    links.push((r.text.clone(), r.href.clone(), id.clone()));
                }
            }
            split(prefix, runs, mono, &find, &mut hits, &mut out);
            Value::list(out)
        };
        let (spans, cells) = match b.kind {
            Kind::Code | Kind::Html => {
                let run = Run {
                    text: b.text.clone(),
                    code: true,
                    ..Run::default()
                };
                (
                    spans_of(&id, std::slice::from_ref(&run), true),
                    Value::list(vec![]),
                )
            }
            Kind::TableRow => {
                let cells = b
                    .cells
                    .iter()
                    .enumerate()
                    .map(|(c, runs)| {
                        let grow = columns[i].get(c).copied().unwrap_or(1);
                        let cell = format!("{id}c{c}");
                        Value::record(vec![
                            Value::str(&cell),
                            spans_of(&cell, runs, false),
                            Value::Number(grow as f64),
                        ])
                    })
                    .collect();
                (Value::list(vec![]), Value::list(cells))
            }
            Kind::Image => {
                let run = Run {
                    text: b.text.clone(),
                    ..Run::default()
                };
                (
                    spans_of(&id, std::slice::from_ref(&run), false),
                    Value::list(vec![]),
                )
            }
            _ => (spans_of(&id, &b.runs, false), Value::list(vec![])),
        };
        marks.extend(std::iter::repeat_n(id.clone(), hits - before));
        blocks.push(Value::record(vec![
            Value::str(&id),
            Value::str(b.kind.name()),
            Value::Number(b.depth as f64),
            Value::str(&b.marker),
            Value::str(if b.kind == Kind::Code { &b.href } else { "" }),
            Value::Bool(b.header),
            spans,
            cells,
        ]));
    }
    let outline = doc
        .outline()
        .iter()
        .map(|(level, text, at)| {
            Value::record(vec![
                Value::str(&format!("b{at}")),
                Value::str(text),
                Value::Number(*level as f64),
            ])
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    let links = links
        .into_iter()
        // A document's own number (its title's `LLP 0001`) goes nowhere new.
        .filter(|(_, href, _)| Path::new(href) != path)
        .filter(|(text, href, _)| seen.insert((text.clone(), href.clone())))
        .enumerate()
        .map(|(i, (text, href, block))| {
            let local = (place.local)(&href);
            Value::record(vec![
                Value::str(&i.to_string()),
                Value::str(&text),
                Value::Bool(local.is_some()),
                Value::str(&href),
                Value::str(&block),
                Value::str(local.as_deref().unwrap_or(&href)),
            ])
        })
        .collect();
    let entry = place.entry;
    Value::record(vec![
        Value::str(&path.to_string_lossy()),
        Value::str(
            &path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
        Value::str(entry.map(|e| e.number.as_str()).unwrap_or("")),
        // The index's title, without its own `LLP NNNN:` prefix.
        Value::str(entry.map(|e| e.title.as_str()).unwrap_or(&doc.title)),
        Value::str(entry.map(|e| e.kind.as_str()).unwrap_or("")),
        Value::str(entry.map(|e| e.status.as_str()).unwrap_or("")),
        Value::str(message),
        Value::Bool(!message.is_empty()),
        Value::list(blocks),
        Value::list(outline),
        Value::Number(hits as f64),
        Value::list(marks.iter().map(|m| Value::str(m)).collect()),
        Value::list(links),
        Value::str(&place.prev),
        Value::str(&place.next),
    ])
}

fn uncached_value(
    path: &Path,
    doc: &Document,
    place: &Place<'_>,
    message: &str,
    find: &str,
) -> Value {
    let columns = columns(doc);
    finish(
        path,
        doc,
        place,
        message,
        page_parts(path, doc, place, find, &columns, None),
    )
}

/// `shape Page`.
pub(crate) fn value(path: &Path, doc: &Document, place: &Place<'_>, message: &str) -> Value {
    let find = fold(place.find.trim());
    uncached_value(path, doc, place, message, &find)
}

/// `shape Page`, reusing the current document's blocks that this find leaves
/// unchanged. A page first opened with a nonempty find builds normally; its
/// base is retained only after the empty page has actually been requested.
pub(crate) fn value_reusing(
    path: &Path,
    scope: &str,
    doc: &Document,
    place: &Place<'_>,
    message: &str,
    cache: &mut Option<Cache>,
) -> Value {
    let find = fold(place.find.trim());
    if find.is_empty() {
        if let Some(base) = cache.as_ref().filter(|c| c.is_for(path, scope, doc)) {
            return finish(
                path,
                doc,
                place,
                message,
                PageParts {
                    blocks: base.blocks.clone(),
                    outline: base.outline.clone(),
                    hits: 0,
                    marks: Vec::new(),
                    links: base.links.clone(),
                },
            );
        }
        let parts = parts(path, doc, place, "");
        let answer = finish(
            path,
            doc,
            place,
            message,
            PageParts {
                blocks: parts.page.blocks.clone(),
                outline: parts.page.outline.clone(),
                hits: 0,
                marks: Vec::new(),
                links: parts.page.links.clone(),
            },
        );
        *cache = Some(Cache {
            path: path.to_path_buf(),
            scope: scope.to_string(),
            blocks: parts.page.blocks,
            ranges: parts.ranges,
            columns: parts.columns,
            outline: parts.page.outline,
            links: parts.page.links,
        });
        return answer;
    }
    let Some(base) = cache.as_ref().filter(|c| c.is_for(path, scope, doc)) else {
        return uncached_value(path, doc, place, message, &find);
    };
    let mut build = Build::new(&base.columns, &find, false, base.blocks.len());
    for (i, block) in doc.blocks.iter().enumerate() {
        if block_matches(i, block, &find) {
            build.append(i, block);
        } else {
            build
                .blocks
                .extend(base.blocks[base.ranges[i].clone()].iter().cloned());
        }
    }
    finish(
        path,
        doc,
        place,
        message,
        PageParts {
            blocks: build.blocks,
            outline: base.outline.clone(),
            hits: build.hits,
            marks: build.marks,
            links: base.links.clone(),
        },
    )
}

/// Each table row's column weights, by block: a column's widest cell in its
/// table, in characters, from 2 to 48. The screen grows each cell by its
/// column's weight from nothing, so the columns line up across rows and a
/// `#` column is not as wide as a description.
fn columns(doc: &Document) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new(); doc.blocks.len()];
    let mut i = 0;
    while i < doc.blocks.len() {
        if doc.blocks[i].kind != Kind::TableRow {
            i += 1;
            continue;
        }
        let end = (i..doc.blocks.len())
            .find(|&j| doc.blocks[j].kind != Kind::TableRow)
            .unwrap_or(doc.blocks.len());
        let mut widths: Vec<usize> = Vec::new();
        for b in &doc.blocks[i..end] {
            for (c, runs) in b.cells.iter().enumerate() {
                let w: usize = runs.iter().map(|r| r.text.chars().count()).sum();
                if widths.len() <= c {
                    widths.push(2);
                }
                widths[c] = widths[c].max(w.clamp(2, 48));
            }
        }
        for row in &mut out[i..end] {
            *row = widths.clone();
        }
        i = end;
    }
    out
}

/// The fields of an LLP's metadata header — a paragraph near the top that
/// starts `**Type:**` and runs on as `**Status:** … **Author:** …` (its
/// line breaks are soft, so Markdown joins them) — as (label, value runs).
/// Block ids for these are `b{i}m{k}`. Not a header: `None`.
fn metadata(i: usize, b: &markdown_parse::Block) -> Option<Vec<(String, Vec<Run>)>> {
    let label = |r: &Run| r.bold && r.text.trim_end().ends_with(':');
    if i > 4 || b.kind != Kind::Paragraph || !b.runs.first().is_some_and(label) {
        return None;
    }
    let mut fields: Vec<(String, Vec<Run>)> = Vec::new();
    for r in &b.runs {
        if label(r) {
            fields.push((r.text.trim().trim_end_matches(':').to_string(), Vec::new()));
        } else if let Some((_, runs)) = fields.last_mut() {
            let mut r = r.clone();
            if runs.is_empty() {
                r.text = r.text.trim_start().to_string();
            }
            runs.push(r);
        }
    }
    (fields.len() >= 2).then_some(fields)
}

/// `text` as find compares it: each character lowercased on its own, and
/// final sigma as sigma, so a query matches whatever ends the word.
fn fold(text: &str) -> String {
    text.chars().flat_map(fold_char).collect()
}

fn fold_char(c: char) -> impl Iterator<Item = char> {
    c.to_lowercase().map(|l| if l == 'ς' { 'σ' } else { l })
}

/// The byte ranges of `text` where `find` (already folded) matches its
/// folded form. Folding can lengthen a character (`İ` to `i̇`) and shorten
/// another (`ẞ` to `ß`), so a match is mapped back character by character
/// and widened to the whole characters it touches.
fn matches(text: &str, find: &str) -> Vec<Range<usize>> {
    if find.is_empty() {
        return Vec::new();
    }
    // Each character's start in `text` and in the folded text.
    let mut starts = Vec::new();
    let mut folded = String::with_capacity(text.len());
    for (at, c) in text.char_indices() {
        starts.push((at, folded.len()));
        folded.extend(fold_char(c));
    }
    // The character of `text` that folded byte `at` came from.
    let char_at = |at: usize| starts.partition_point(|&(_, f)| f <= at) - 1;
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = folded[from..].find(find) {
        let first = char_at(from + found);
        let last = char_at(from + found + find.len() - 1);
        let (end, next) = starts
            .get(last + 1)
            .copied()
            .unwrap_or((text.len(), folded.len()));
        out.push(starts[first].0..end);
        from = next;
    }
    out
}

/// Whether `runs`, read as one text as `split` reads them, hold `find`.
fn runs_match(runs: &[Run], find: &str) -> bool {
    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
    !find.is_empty() && fold(&text).contains(find)
}

fn text_matches(text: &str, find: &str) -> bool {
    !find.is_empty() && fold(text).contains(find)
}

/// Whether rendering this block for `find` would change any of its base
/// spans. This follows the same run boundaries and folding as `split`; it
/// deliberately does no `Value` construction.
fn block_matches(i: usize, b: &markdown_parse::Block, find: &str) -> bool {
    if let Some(fields) = metadata(i, b) {
        return fields.iter().any(|(_, runs)| runs_match(runs, find));
    }
    match b.kind {
        Kind::Code | Kind::Html | Kind::Image => text_matches(&b.text, find),
        Kind::TableRow => b.cells.iter().any(|runs| runs_match(runs, find)),
        _ => runs_match(&b.runs, find),
    }
}

/// `runs` as spans, each match of `find` its own span with its ordinal. A
/// match is found in the runs' text as one, so a phrase that crosses a
/// change of style is found; each run's part of it is a span with that
/// run's style and the match's ordinal.
fn split(
    prefix: &str,
    runs: &[Run],
    mono: bool,
    find: &str,
    hits: &mut usize,
    out: &mut Vec<Value>,
) {
    let push = |run: &Run, text: &str, hit: usize, out: &mut Vec<Value>| {
        if text.is_empty() {
            return;
        }
        out.push(Value::record(vec![
            Value::str(&format!("{prefix}s{}", out.len())),
            Value::str(text),
            Value::Number(if run.bold { 700.0 } else { 400.0 }),
            Value::str(if run.italic { "italic" } else { "normal" }),
            Value::Bool(mono || run.code),
            Value::str(&run.href),
            Value::Number(hit as f64),
        ]));
    };
    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
    let found = matches(&text, find);
    let first = *hits;
    *hits += found.len();
    let mut start = 0;
    for run in runs {
        let end = start + run.text.len();
        let mut at = start;
        for (k, m) in found.iter().enumerate() {
            if m.end <= start || m.start >= end {
                continue;
            }
            let (from, to) = (m.start.max(start), m.end.min(end));
            push(run, &text[at..from], 0, out);
            push(run, &text[from..to], first + k + 1, out);
            at = to;
        }
        push(run, &text[at..end], 0, out);
        start = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_plan::Items;
    use markdown_parse::parse;

    const MARKDOWN: &str = r#"# LLP 0001: Kernel reader

**Type:** RFC **Status:** Draft **Author:** Exact

KERNEL before **kernel** and [local document](other.md).

This paragraph stays untouched by the query.

İstanbul and STRAẞE fold to other lengths.

| Name | Value |
| --- | --- |
| kernel | table |

```rust
kernel();
```

![kernel image](image.png)

<span>kernel html</span>
"#;

    fn document(source: &str) -> Document {
        parse(source, &|target| format!("/corpus/{target}"))
    }

    fn place<'a>(find: &'a str, local: &'a dyn Fn(&str) -> Option<String>) -> Place<'a> {
        Place {
            entry: None,
            prev: "/corpus/0000-prev.md".into(),
            next: "/corpus/0002-next.md".into(),
            find,
            local,
        }
    }

    fn blocks(value: &Value) -> &exact_plan::Items {
        let Value::Record(page) = value else {
            panic!("page is a record")
        };
        let Value::List(blocks) = &page[8] else {
            panic!("page blocks are a list")
        };
        blocks
    }

    fn same_object(a: &Value, b: &Value) -> bool {
        matches!((a, b), (Value::Record(a), Value::Record(b)) if Items::ptr_eq(a, b))
    }

    fn hit_ordinals(value: &Value) -> Vec<usize> {
        fn from_spans(spans: &Value, out: &mut Vec<usize>) {
            let Value::List(spans) = spans else {
                panic!("spans are a list")
            };
            for span in spans.iter() {
                let Value::Record(span) = span else {
                    panic!("span is a record")
                };
                let Value::Number(hit) = &span[6] else {
                    panic!("span hit is a number")
                };
                if *hit > 0.0 {
                    out.push(*hit as usize);
                }
            }
        }

        let mut out = Vec::new();
        for block in blocks(value).iter() {
            let Value::Record(block) = block else {
                panic!("block is a record")
            };
            from_spans(&block[6], &mut out);
            let Value::List(cells) = &block[7] else {
                panic!("cells are a list")
            };
            for cell in cells.iter() {
                let Value::Record(cell) = cell else {
                    panic!("cell is a record")
                };
                from_spans(&cell[1], &mut out);
            }
        }
        out
    }

    #[test]
    fn differential_fixture_reaches_every_special_block_path() {
        let doc = document(MARKDOWN);
        for kind in [Kind::TableRow, Kind::Code, Kind::Image, Kind::Html] {
            assert!(
                doc.blocks.iter().any(|block| block.kind == kind),
                "{kind:?}"
            );
        }
        assert!(doc
            .blocks
            .iter()
            .enumerate()
            .any(|(i, block)| metadata(i, block).is_some()));
    }

    #[test]
    fn find_maps_folded_matches_to_whole_characters() {
        // `İ` folds one byte longer and `ẞ` one byte shorter: the folded
        // text is as long as the original, but its offsets differ.
        let text = "İstanbul and STRAẞE.";
        assert_eq!(fold(text).len(), text.len());
        let found = |find: &str| -> Vec<&str> {
            matches(text, &fold(find))
                .into_iter()
                .map(|r| &text[r])
                .collect()
        };
        assert_eq!(found("i"), ["İ"]);
        assert_eq!(found("i\u{307}"), ["İ"]);
        assert_eq!(found("\u{307}s"), ["İs"]);
        assert_eq!(found("stanbul"), ["stanbul"]);
        assert_eq!(found("ß"), ["ẞ"]);
        assert_eq!(found("e."), ["E."]);
        assert_eq!(found("STRASSE"), Vec::<&str>::new());
        assert_eq!(found("straße"), ["STRAẞE"]);
        assert_eq!(found("a"), ["a", "a", "A"]);
        assert_eq!(matches("ΟΔΟΣ", &fold("οδος")), vec![0..8]);
        // U+2126 OHM SIGN folds one byte shorter and `İ` one longer.
        assert_eq!(matches("\u{2126}İ", "i"), vec![3..5]);
        assert!(matches(text, "").is_empty());
    }

    #[test]
    fn find_reads_a_paragraph_across_its_styles() {
        // Each hit span as (text, weight, mono, href, ordinal).
        let hits = |source: &str, find: &str| {
            let local = |_: &str| None;
            let page = value(
                Path::new("/corpus/0001-a.md"),
                &document(source),
                &place(find, &local),
                "",
            );
            let mut out = Vec::new();
            for block in blocks(&page).iter() {
                let Value::Record(block) = block else {
                    panic!("block is a record")
                };
                let Value::List(spans) = &block[6] else {
                    panic!("spans are a list")
                };
                for span in spans.iter() {
                    let Value::Record(s) = span else {
                        panic!("span is a record")
                    };
                    let hit = s[6].as_number().expect("a span's hit");
                    if hit > 0.0 {
                        out.push((
                            s[1].as_str().expect("a span's text").to_string(),
                            s[2].as_number().expect("a span's weight") as u32,
                            s[4].as_bool().expect("a span's face"),
                            s[5].as_str().expect("a span's link").to_string(),
                            hit as usize,
                        ));
                    }
                }
            }
            out
        };
        let span = |text: &str, weight, mono, href: &str, hit| {
            (text.to_string(), weight, mono, href.to_string(), hit)
        };
        assert_eq!(
            hits("Hello **world**.", "hello world"),
            [
                span("Hello ", 400, false, "", 1),
                span("world", 700, false, "", 1)
            ]
        );
        assert_eq!(
            hits("some**thing** and some**thing**", "something"),
            [
                span("some", 400, false, "", 1),
                span("thing", 700, false, "", 1),
                span("some", 400, false, "", 2),
                span("thing", 700, false, "", 2)
            ]
        );
        // Starting inside inline code, ending inside a link.
        assert_eq!(
            hits(
                "Call `run()` then [the guide](guide.md) now.",
                "n() then the g"
            ),
            [
                span("n()", 400, true, "", 1),
                span(" then ", 400, false, "", 1),
                span("the g", 400, false, "/corpus/guide.md", 1)
            ]
        );
        // A phrase across a table cell's styles, and none across two cells.
        let table = "| A | B |\n| --- | --- |\n| plain *slanted* | other |\n";
        let local = |_: &str| None;
        let doc = document(table);
        let page = value(
            Path::new("/x.md"),
            &doc,
            &place("plain slanted", &local),
            "",
        );
        assert_eq!(hit_ordinals(&page), [1, 1]);
        let page = value(
            Path::new("/x.md"),
            &doc,
            &place("slanted other", &local),
            "",
        );
        assert!(hit_ordinals(&page).is_empty());
    }

    #[test]
    fn production_pages_are_canonical_for_every_find_shape() {
        let path = Path::new("/corpus/0001-kernel.rfc.md");
        let doc = document(MARKDOWN);
        let local = |href: &str| href.ends_with("other.md").then_some("Other LLP".into());
        let mut cache = None;
        for query in [
            "", "k", "ke", "ker", "kernel", "absent", "i\u{307}", "table", "html", "i", "stanbul",
            "straße", "e f",
        ] {
            let place = place(query, &local);
            let expected = reference_value(path, &doc, &place, "");
            let uncached = value(path, &doc, &place, "");
            let reused = value_reusing(path, "/corpus", &doc, &place, "", &mut cache);
            assert_eq!(
                uncached.to_bytes(),
                expected.to_bytes(),
                "uncached {query:?}"
            );
            assert_eq!(reused.to_bytes(), expected.to_bytes(), "reused {query:?}");
            if query == "kernel" {
                let ordinals = hit_ordinals(&expected);
                assert!(ordinals.len() > 3);
                assert_eq!(ordinals, (1..=ordinals.len()).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn misses_and_unmatched_blocks_keep_their_value_objects() {
        let path = Path::new("/corpus/0001-kernel.rfc.md");
        let doc = document(MARKDOWN);
        let local = |_: &str| None;
        let mut cache = None;
        let base = value_reusing(path, "/corpus", &doc, &place("", &local), "", &mut cache);
        let miss = value_reusing(
            path,
            "/corpus",
            &doc,
            &place("absent", &local),
            "",
            &mut cache,
        );
        assert!(blocks(&base)
            .iter()
            .zip(blocks(&miss).iter())
            .all(|(a, b)| same_object(a, b)));

        let hit = value_reusing(
            path,
            "/corpus",
            &doc,
            &place("kernel", &local),
            "",
            &mut cache,
        );
        let same = blocks(&base)
            .iter()
            .zip(blocks(&hit).iter())
            .filter(|(a, b)| same_object(a, b))
            .count();
        assert!((1..blocks(&base).len()).contains(&same));
    }

    #[test]
    fn first_nonempty_find_does_not_build_or_retain_a_base() {
        let path = Path::new("/corpus/0001-kernel.rfc.md");
        let doc = document(MARKDOWN);
        let local = |_: &str| None;
        let place = place("kernel", &local);
        let mut cache = None;
        let expected = reference_value(path, &doc, &place, "");
        let actual = value_reusing(path, "/corpus", &doc, &place, "", &mut cache);
        assert_eq!(actual.to_bytes(), expected.to_bytes());
        assert!(cache.is_none());
    }

    #[test]
    fn path_scope_and_failure_message_bound_reuse() {
        let first_path = Path::new("/corpus/0001-kernel.rfc.md");
        let first = document(MARKDOWN);
        let local = |href: &str| Some(format!("local:{href}"));
        let mut cache = None;
        value_reusing(
            first_path,
            "/corpus",
            &first,
            &place("", &local),
            "",
            &mut cache,
        );

        // A failed navigation can draw the still-open document against the
        // requested corpus and a refusal. Its link resolution and message
        // must come from that request, not the retained page.
        let nowhere = |_: &str| None;
        let failed_place = place("kernel", &nowhere);
        let expected = reference_value(first_path, &first, &failed_place, "cannot read new root");
        let actual = value_reusing(
            first_path,
            "/missing-root",
            &first,
            &failed_place,
            "cannot read new root",
            &mut cache,
        );
        assert_eq!(actual.to_bytes(), expected.to_bytes());

        let second_path = Path::new("/corpus/0002-next.md");
        let second = document("# Second\n\nA second kernel document.");
        let second_place = place("kernel", &local);
        let expected = reference_value(second_path, &second, &second_place, "");
        let actual = value_reusing(
            second_path,
            "/corpus",
            &second,
            &second_place,
            "",
            &mut cache,
        );
        assert_eq!(actual.to_bytes(), expected.to_bytes());
    }
}
