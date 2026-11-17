//! The parsed document as the plan's own value, in the shapes both readers
//! declare. One conversion keeps their blocks and styled runs consistent.
//! Graphical readers group each table with [`into_blocks_with_tables`];
//! [`blocks`] retains the parser's individual rows. @ref LLP 1033

use crate::{Block, Document, Kind, Run};
use exact_plan::{InlineStr, Str, Value};
use std::collections::{HashMap, HashSet};

/// Sharing is scoped to one conversion: no global pool retains closed files.
/// Owned lookup keys let a consuming conversion release parsed blocks as it goes.
struct Values {
    strings: HashSet<Str>,
    indices: HashMap<usize, Value>,
    empty_list: Value,
}

impl Values {
    fn new() -> Self {
        Self {
            strings: HashSet::new(),
            indices: HashMap::new(),
            empty_list: Value::list(Vec::new()),
        }
    }

    fn string(&mut self, text: &str) -> Value {
        // Short text is held in the value itself: nothing to share.
        if text.len() <= InlineStr::CAP {
            return Value::str(text);
        }
        let text = self.strings.get(text).cloned().unwrap_or_else(|| {
            let text = Str::from(text);
            self.strings.insert(text.clone());
            text
        });
        Value::from(text)
    }

    fn index(&mut self, index: usize) -> Value {
        self.indices
            .entry(index)
            .or_insert_with(|| Value::str(&index.to_string()))
            .clone()
    }

    fn run(&mut self, index: usize, run: &Run) -> Value {
        Value::record(vec![
            self.index(index),
            self.string(&run.text),
            Value::Number(if run.bold { 700.0 } else { 400.0 }),
            self.string(if run.italic { "italic" } else { "normal" }),
            Value::Bool(run.code),
            self.string(&run.href),
        ])
    }

    fn runs(&mut self, list: &[Run]) -> Value {
        if list.is_empty() {
            return self.empty_list.clone();
        }
        Value::list(
            list.iter()
                .enumerate()
                .map(|(i, r)| self.run(i, r))
                .collect(),
        )
    }

    fn cells<'a>(&mut self, list: impl ExactSizeIterator<Item = (&'a [Run], &'a str)>) -> Value {
        if list.len() == 0 {
            return self.empty_list.clone();
        }
        Value::list(
            list.enumerate()
                .map(|(i, (c, align))| {
                    // Compact opaque keys keep the 4 MiB fixture's reply within the
                    // data module's 16 MiB bound.
                    Value::record(vec![
                        Value::str(&format!("{i:x}")),
                        self.runs(c),
                        self.string(align),
                    ])
                })
                .collect(),
        )
    }

    fn block(&mut self, index: usize, b: &Block) -> Value {
        Value::record(vec![
            self.index(index),
            self.string(b.kind.name()),
            Value::Number(b.depth as f64),
            self.string(&b.marker),
            self.string(&b.text),
            self.string(&b.href),
            Value::Bool(b.header),
            self.runs(&b.runs),
            self.cells(
                b.cells
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (c.as_slice(), b.cell_align.get(i).map_or("", String::as_str))),
            ),
            self.string(&b.dark),
            Value::Number(b.width as f64),
            Value::Number(b.height as f64),
            self.string(&b.align),
            self.empty_list.clone(),
            self.empty_list.clone(),
        ])
    }

    /// One table's rows as one block (see [`into_blocks_with_tables`]).
    fn table(&mut self, index: usize, rows: &[Block]) -> Value {
        let columns = rows[0].cells.len();
        let cells: Vec<(&[Run], &str)> = rows
            .iter()
            .flat_map(|row| {
                (0..columns).map(|c| {
                    (
                        row.cells.get(c).map_or(&[][..], Vec::as_slice),
                        rows[0].cell_align.get(c).map_or("", String::as_str),
                    )
                })
            })
            .collect();
        // Each grid stays below the native 10,000-track limit. Repeating the
        // same zero-height sizing row gives every group the whole table's
        // intrinsic column widths without measuring fonts in the parser.
        const ROWS: usize = 8192;
        let sizers = if rows.len() > ROWS {
            Value::list(
                (0..columns)
                    .map(|column| self.column_sizer(rows, column))
                    .collect(),
            )
        } else {
            self.empty_list.clone()
        };
        let groups = (0..rows.len())
            .step_by(ROWS)
            .enumerate()
            .map(|(i, start)| {
                Value::record(vec![
                    self.index(i),
                    Value::Number((start * columns) as f64),
                    Value::Number(((start + ROWS).min(rows.len()) * columns) as f64),
                ])
            })
            .collect();
        Value::record(vec![
            self.index(index),
            self.string("table"),
            Value::Number(columns as f64),
            self.string(""),
            self.string(""),
            self.string(""),
            Value::Bool(false),
            self.empty_list.clone(),
            self.cells(cells.into_iter()),
            self.string(""),
            Value::Number(0.0),
            Value::Number(0.0),
            self.string(""),
            Value::list(groups),
            sizers,
        ])
    }

    /// `shape TableSizer`: one column's sizing lines, each referring to
    /// cells of the table block's `cells` by index rather than repeating
    /// their text. The reader joins a line's single-run cells, all of one
    /// style, with preserved newlines, which give the column its widest cell
    /// and longest word as separate cells do; a cell of several runs is a
    /// line of its own, its `cell`. Identical styled cells contribute once.
    fn column_sizer(&mut self, rows: &[Block], column: usize) -> Value {
        // The reader joins a line in one evaluation: a list step per cell to
        // map and one to join, within Contract's 65,536.
        const LINE: usize = 16_384;
        struct Line {
            cells: Vec<usize>,
            cell: Option<usize>,
            style: (bool, bool, bool),
        }
        let columns = rows[0].cells.len();
        let mut lines: Vec<Line> = Vec::new();
        let mut open: HashMap<_, usize> = HashMap::new();
        let mut seen = HashSet::new();
        for (r, row) in rows.iter().enumerate() {
            let Some(cell) = row.cells.get(column).filter(|cell| !cell.is_empty()) else {
                continue;
            };
            let key: Vec<_> = cell
                .iter()
                .map(|run| {
                    (
                        run.text.as_str(),
                        run.bold || row.header,
                        run.italic,
                        run.code,
                    )
                })
                .collect();
            if !seen.insert(key) {
                continue;
            }
            let at = r * columns + column;
            let [run] = cell.as_slice() else {
                lines.push(Line {
                    cells: Vec::new(),
                    cell: Some(at),
                    style: (row.header, false, false),
                });
                continue;
            };
            let style = (run.bold || row.header, run.italic, run.code);
            let line = match open.get(&style) {
                Some(&line) if lines[line].cells.len() < LINE => line,
                _ => {
                    open.insert(style, lines.len());
                    lines.push(Line {
                        cells: Vec::new(),
                        cell: None,
                        style,
                    });
                    lines.len() - 1
                }
            };
            lines[line].cells.push(at);
        }
        let lines = lines
            .into_iter()
            .enumerate()
            .map(|(i, line)| {
                let (bold, italic, code) = line.style;
                Value::record(vec![
                    self.index(i),
                    if line.cells.is_empty() {
                        self.empty_list.clone()
                    } else {
                        Value::list(
                            line.cells
                                .into_iter()
                                .map(|at| Value::Number(at as f64))
                                .collect(),
                        )
                    },
                    Value::Number(line.cell.map_or(-1.0, |at| at as f64)),
                    Value::Number(if bold { 700.0 } else { 400.0 }),
                    self.string(if italic { "italic" } else { "normal" }),
                    Value::Bool(code),
                ])
            })
            .collect();
        Value::record(vec![self.index(column), Value::list(lines)])
    }
}

/// `shape Run` — one styled span. `weight` and `slant` are the CSS values
/// the run's `text` node takes directly; `mono` is a bool because
/// `font-family` is literal-only in Contract v1 and the reader branches on it.
pub fn run(index: usize, run: &Run) -> Value {
    Values::new().run(index, run)
}

/// `shape Block`.
pub fn block(index: usize, b: &Block) -> Value {
    Values::new().block(index, b)
}

/// The `Block[]` a document's `blocks` field takes. The top-level
/// document record is each app's own — `apps/markdown` carries the folder
/// beside the file, `apps/llp` the document's place in the index — and this
/// is the part they must not write twice.
pub fn blocks(doc: &Document) -> Value {
    let mut values = Values::new();
    Value::list(
        doc.blocks
            .iter()
            .enumerate()
            .map(|(i, b)| values.block(i, b))
            .collect(),
    )
}

fn continues_table(block: &Block) -> bool {
    block.kind == Kind::TableRow && !block.header
}

/// A graphical block's parsed rows: one ordinary block or a whole pipe table.
/// Page and viewport boundaries must fall between these groups.
pub fn block_groups(doc: &Document) -> impl Iterator<Item = &[Block]> {
    doc.blocks
        .chunk_by(|previous, next| previous.kind == Kind::TableRow && continues_table(next))
}

/// Convert one group from [`block_groups`], with its id in the grouped list.
pub fn block_group(index: usize, group: &[Block]) -> Value {
    let mut values = Values::new();
    if group[0].kind == Kind::TableRow {
        values.table(index, group)
    } else {
        values.block(index, &group[0])
    }
}

/// A graphical reader's `Block[]`, from an owned document: [`blocks`], except
/// that each pipe table is one block with shared column tracks across all
/// its rows, as a browser's table does. That block
/// is `kind` `"table"`, `depth` its column count (its header row's cells),
/// `tableGroups` bounded ranges into `cells`, and `tableSizers` each
/// column's sizing lines, which the reader repeats in each grid when a table
/// needs several groups.
/// `cells` contains the header's cells and then each row's, a row padded with
/// empty cells or cut to the header's count, as GitHub Flavored Markdown
/// says (<https://github.github.com/gfm/#tables-extension->). A block's id
/// is its place in this list.
///
/// Each parsed block is released once it is converted (a table's rows once
/// the table ends), so its allocations can serve later values instead of
/// retaining both full forms.
pub fn into_blocks_with_tables(doc: Document) -> Value {
    let mut values = Values::new();
    let mut blocks = Vec::with_capacity(doc.blocks.len());
    let mut rows: Vec<Block> = Vec::new();
    for block in doc.blocks {
        // A header row starts a table, also right after another table.
        if !rows.is_empty() && !continues_table(&block) {
            blocks.push(values.table(blocks.len(), &rows));
            rows.clear();
        }
        if block.kind == Kind::TableRow {
            rows.push(block);
        } else {
            blocks.push(values.block(blocks.len(), &block));
        }
    }
    if !rows.is_empty() {
        blocks.push(values.table(blocks.len(), &rows));
    }
    Value::list(blocks)
}
