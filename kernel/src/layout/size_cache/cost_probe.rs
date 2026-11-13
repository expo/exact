//! Layout cost by tree shape and size (LLP 1165 §2).
//! Run with `cargo test -p exact-kernel --release --lib cost_probe -- --ignored --nocapture`.
use crate::{Kernel, NodeType, Offer, Op, PropId, StyleId, StyleProps, StyleValue, ViewId};
use std::time::{Duration, Instant};

pub(super) fn style(id: ViewId, rows: &[(StyleId, StyleValue)]) -> Op {
    let mut patch = StyleProps::default();
    for (row, value) in rows {
        patch.set_dynamic(*row, value).unwrap();
    }
    Op::SetStyle {
        id,
        patch: Box::new(patch),
    }
}
pub(super) fn t(s: &str) -> StyleValue<'_> {
    StyleValue::Text(s.into())
}
pub(super) fn n(x: f64) -> StyleValue<'static> {
    StyleValue::Number(x)
}
pub(super) fn view(id: ViewId) -> Op {
    Op::CreateView {
        id,
        node_type: NodeType::View,
    }
}
pub(super) fn text(id: ViewId, value: &str) -> [Op; 2] {
    [
        Op::CreateView {
            id,
            node_type: NodeType::Text,
        },
        Op::SetProp {
            id,
            prop: PropId::Text,
            value: value.into(),
        },
    ]
}
pub(super) fn children(id: ViewId, children: Vec<ViewId>) -> Op {
    Op::SetChildren { id, children }
}

/// A built tree: its operations, a text leaf near the middle, and its node count.
struct Shape {
    name: &'static str,
    ops: Vec<Op>,
    leaf: ViewId,
    nodes: usize,
}

/// A feed of cards, ten nodes each: a block card holding a flex row (an
/// avatar and a column of two texts) and a flex footer of three texts.
fn feed(cards: u32) -> Shape {
    let mut ops = vec![view(1), view(2)];
    ops.push(style(
        2,
        &[
            (StyleId::Display, t("flex")),
            (StyleId::FlexDirection, t("column")),
        ],
    ));
    let mut list = Vec::new();
    let mut leaf = 0;
    for c in 0..cards {
        let b = 10 + c * 10;
        let [card, row, avatar, col, title, body, footer, like, reply, time] =
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9].map(|i| b + i);
        for id in [card, row, avatar, col, footer] {
            ops.push(view(id));
        }
        ops.extend(text(title, &format!("Person {c}")));
        ops.extend(text(
            body,
            &"Yes! I found a place near the park. ".repeat(1 + (c % 3) as usize),
        ));
        ops.extend(text(like, "Like"));
        ops.extend(text(reply, "Reply"));
        ops.extend(text(time, "2h"));
        ops.extend([
            style(
                card,
                &[
                    (StyleId::PaddingTop, n(12.0)),
                    (StyleId::PaddingLeft, n(12.0)),
                    (StyleId::PaddingRight, n(12.0)),
                    (StyleId::MarginBottom, n(8.0)),
                ],
            ),
            style(
                row,
                &[
                    (StyleId::Display, t("flex")),
                    (StyleId::ColumnGap, n(8.0)),
                    (StyleId::AlignItems, t("center")),
                ],
            ),
            style(
                avatar,
                &[
                    (StyleId::Width, n(40.0)),
                    (StyleId::Height, n(40.0)),
                    (StyleId::FlexShrink, n(0.0)),
                ],
            ),
            style(
                col,
                &[
                    (StyleId::Display, t("flex")),
                    (StyleId::FlexDirection, t("column")),
                    (StyleId::FlexGrow, n(1.0)),
                    (StyleId::MinWidth, n(0.0)),
                ],
            ),
            style(
                footer,
                &[
                    (StyleId::Display, t("flex")),
                    (StyleId::JustifyContent, t("space-between")),
                ],
            ),
            children(col, vec![title, body]),
            children(row, vec![avatar, col]),
            children(footer, vec![like, reply, time]),
            children(card, vec![row, footer]),
        ]);
        list.push(card);
        if c == cards / 2 {
            leaf = body;
        }
    }
    ops.extend([
        children(2, list),
        children(1, vec![2]),
        Op::AttachRoot { id: 1 },
    ]);
    Shape {
        name: "feed",
        ops,
        leaf,
        nodes: 2 + 10 * cards as usize,
    }
}

/// Paragraphs as block siblings of one block root.
fn flat_block(count: u32) -> Shape {
    let mut ops = vec![view(1)];
    let ids: Vec<ViewId> = (0..count).map(|i| 10 + i).collect();
    for &id in &ids {
        ops.extend(text(id, "Yes! I found a place near the park."));
    }
    let leaf = ids[ids.len() / 2];
    ops.extend([children(1, ids), Op::AttachRoot { id: 1 }]);
    Shape {
        name: "flat_block",
        ops,
        leaf,
        nodes: 1 + count as usize,
    }
}

/// The same paragraphs as items of one flex column.
fn flat_flex(count: u32) -> Shape {
    let mut shape = flat_block(count);
    shape.ops.insert(
        1,
        style(
            1,
            &[
                (StyleId::Display, t("flex")),
                (StyleId::FlexDirection, t("column")),
            ],
        ),
    );
    shape.name = "flat_flex";
    shape
}

/// Rows of five `flex: 1 1 0` boxes in a flex column, with a text in the first.
fn flex_grow(rows: u32) -> Shape {
    let mut ops = vec![view(1)];
    ops.push(style(
        1,
        &[
            (StyleId::Display, t("flex")),
            (StyleId::FlexDirection, t("column")),
        ],
    ));
    let mut list = Vec::new();
    let mut leaf = 0;
    for r in 0..rows {
        let b = 10 + r * 7;
        ops.push(view(b));
        ops.push(style(b, &[(StyleId::Display, t("flex"))]));
        let mut cells = Vec::new();
        for c in 0..5 {
            let cell = b + 1 + c;
            ops.push(view(cell));
            ops.push(style(
                cell,
                &[
                    (StyleId::FlexGrow, n(1.0)),
                    (StyleId::FlexShrink, n(1.0)),
                    (StyleId::FlexBasis, n(0.0)),
                    (StyleId::Height, n(20.0)),
                ],
            ));
            cells.push(cell);
        }
        ops.extend(text(b + 6, "cell"));
        ops.push(children(b + 1, vec![b + 6]));
        ops.push(children(b, cells));
        list.push(b);
        if r == rows / 2 {
            leaf = b + 6;
        }
    }
    ops.extend([children(1, list), Op::AttachRoot { id: 1 }]);
    Shape {
        name: "flex_grow",
        ops,
        leaf,
        nodes: 1 + 7 * rows as usize,
    }
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

fn measure(shape: Shape) {
    let narrow = Offer::definite(390.0, 844.0);
    let wide = Offer::definite(430.0, 844.0);
    let build = || {
        let mut k = Kernel::with_monospace();
        k.apply(0, 1, &shape.ops).unwrap();
        k.tree_mut().taffy.track_size_computations(false);
        k
    };
    let runs = if shape.nodes > 20_000 { 5 } else { 15 };
    let cold = median(
        (0..runs)
            .map(|_| {
                let mut k = build();
                let start = Instant::now();
                k.compute_layout(1, narrow).unwrap();
                start.elapsed()
            })
            .collect(),
    );
    let mut k = build();
    k.compute_layout(1, narrow).unwrap();
    let unchanged = median(
        (0..runs)
            .map(|_| {
                let start = Instant::now();
                k.compute_layout(1, narrow).unwrap();
                start.elapsed()
            })
            .collect(),
    );
    // One text leaf changes to a text of another length, then back.
    let (mut edits, mut edit_layouts) = (Vec::new(), Vec::new());
    for (batch, i) in (2..).zip(0..runs * 2) {
        let value = if i % 2 == 0 {
            "A much longer reply than the one that was here before, wrapping onto more lines."
        } else {
            "Short."
        };
        let op = Op::SetProp {
            id: shape.leaf,
            prop: PropId::Text,
            value: value.into(),
        };
        let start = Instant::now();
        k.apply(0, batch, &[op]).unwrap();
        let applied = start.elapsed();
        k.compute_layout(1, narrow).unwrap();
        edits.push(start.elapsed());
        edit_layouts.push(start.elapsed() - applied);
    }
    let resize = median(
        (0..runs * 2)
            .map(|i| {
                let start = Instant::now();
                k.compute_layout(1, if i % 2 == 0 { wide } else { narrow })
                    .unwrap();
                start.elapsed()
            })
            .collect(),
    );
    eprintln!(
        "COST {:<10} {:>6} nodes: cold {:>10.3?} ({:>5.0} ns/node); unchanged {:>9.3?}; edit {:>9.3?} (layout {:>9.3?}); resize {:>10.3?}",
        shape.name,
        shape.nodes,
        cold,
        cold.as_nanos() as f64 / shape.nodes as f64,
        unchanged,
        median(edits),
        median(edit_layouts),
        resize,
    );
}

#[test]
#[ignore = "a measurement"]
fn layout_cost_by_shape_and_size() {
    for cards in [10, 100, 1_000, 5_000] {
        measure(feed(cards));
    }
    for count in [1_000, 10_000, 50_000] {
        measure(flat_block(count));
    }
    measure(flat_flex(10_000));
    for rows in [150, 1_500] {
        measure(flex_grow(rows));
    }
}

struct Count(std::rc::Rc<std::cell::Cell<usize>>);
impl crate::TextMeasurer for Count {
    fn measure(&mut self, request: &crate::TextMeasureRequest<'_>) -> crate::TextMetrics {
        self.0.set(self.0.get() + 1);
        crate::MonospaceMeasurer::default().measure(request)
    }
}

/// A chain of containers by pattern (`r` flex row, `c` flex column, `b`
/// block, repeated to `levels`) around one text.
pub(super) fn chain(pattern: &str, levels: u32, value: &str) -> Vec<Op> {
    let kinds: Vec<char> = pattern.chars().collect();
    let mut ops = Vec::new();
    for l in 0..levels {
        let id = 1 + l;
        ops.push(view(id));
        match kinds[l as usize % kinds.len()] {
            'r' => ops.push(style(id, &[(StyleId::Display, t("flex"))])),
            'R' => ops.push(style(
                id,
                &[
                    (StyleId::Display, t("flex")),
                    (StyleId::AlignItems, t("flex-start")),
                ],
            )),
            'C' => ops.push(style(
                id,
                &[
                    (StyleId::Display, t("flex")),
                    (StyleId::FlexDirection, t("column")),
                    (StyleId::AlignItems, t("flex-start")),
                ],
            )),
            'c' => ops.push(style(
                id,
                &[
                    (StyleId::Display, t("flex")),
                    (StyleId::FlexDirection, t("column")),
                ],
            )),
            'B' => {
                ops.push(style(
                    id,
                    &[
                        (StyleId::Display, t("flex")),
                        (StyleId::AlignItems, t("baseline")),
                    ],
                ));
                ops.extend(text(10_000 + l, "x"));
            }
            'm' => ops.push(style(
                id,
                &[
                    (StyleId::MarginTop, n(4.0)),
                    (StyleId::MarginBottom, n(4.0)),
                ],
            )),
            'w' => ops.push(style(
                id,
                &[
                    (StyleId::Display, t("flex")),
                    (StyleId::FlexWrap, t("wrap")),
                ],
            )),
            'g' => ops.push(style(id, &[(StyleId::Display, t("grid"))])),
            _ => {}
        }
    }
    ops.extend(text(1 + levels, value));
    for l in 0..levels {
        let mut kids = vec![2 + l];
        if kinds[l as usize % kinds.len()] == 'B' {
            kids.push(10_000 + l);
        }
        ops.push(children(1 + l, kids));
    }
    ops.push(Op::AttachRoot { id: 1 });
    ops
}

#[test]
#[ignore = "a measurement"]
fn measure_calls_by_depth() {
    let long = "A much longer reply than the one that was here before, wrapping onto more lines.";
    for (label, value) in [("short", "Short."), ("long", long)] {
        let patterns: Vec<String> = std::env::var("PATTERNS")
            .unwrap_or("rc,RC,wc,Bc,r,gc,b".into())
            .split(',')
            .map(String::from)
            .collect();
        for pattern in patterns.iter().map(String::as_str) {
            let mut line = format!("DEPTH {label:<5} {pattern:<3}");
            for levels in [4, 8, 12, 16, 20, 24, 32, 64, 100] {
                let calls = std::rc::Rc::new(std::cell::Cell::new(0));
                let mut samples = Vec::new();
                for _ in 0..5 {
                    calls.set(0);
                    let mut k = Kernel::new(Box::new(Count(calls.clone())));
                    k.apply(0, 1, &chain(pattern, levels, value)).unwrap();
                    k.tree_mut().taffy.track_size_computations(false);
                    let start = Instant::now();
                    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
                    samples.push(start.elapsed());
                    if samples[0] > Duration::from_millis(60) {
                        break;
                    }
                }
                let cold = median(samples);
                line += &format!(" | {levels}: {} calls {cold:.1?}", calls.get());
                if cold > Duration::from_millis(60) {
                    break;
                }
            }
            eprintln!("{line}");
        }
    }
}

/// The storage freed between markers belongs to the layout tree. An allocator
/// profiler can count it without counting the arena, receipts or test operations.
#[test]
#[ignore = "a measurement"]
fn feed_layout_heap() {
    use std::io::Write;
    for stage in 0..3 {
        let shape = feed(1_000);
        let mut k = Kernel::with_monospace();
        k.tree_mut().taffy.track_size_computations(false);
        k.apply(0, 1, &shape.ops).unwrap();
        k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
        if stage > 0 {
            for width in 0..20 {
                k.compute_layout(1, Offer::definite(350.0 + width as f32 * 10.0, 844.0))
                    .unwrap();
            }
        }
        if stage > 1 {
            let mut ops = vec![children(2, (0..500).map(|c| 10 + c * 10).collect())];
            ops.extend((500..1_000).map(|c| Op::DestroyView { id: 10 + c * 10 }));
            k.apply(0, 2, &ops).unwrap();
            k.compute_layout(1, Offer::definite(540.0, 844.0)).unwrap();
        }
        let tree = std::mem::take(k.tree_mut());
        eprintln!(
            "FEED stage {stage}: {} cache bytes",
            tree.taffy.size_cache_bytes()
        );
        let before = format!("EXACT_HEAP feed stage {stage} before\n");
        let after = format!("EXACT_HEAP feed stage {stage} after\n");
        std::io::stderr().write_all(before.as_bytes()).unwrap();
        drop(tree);
        std::io::stderr().write_all(after.as_bytes()).unwrap();
    }
}
