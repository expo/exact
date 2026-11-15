//! What custom properties cost (LLP 1123 D5), against the same tree with
//! literal lengths and with local-inset readers: 2,000 readers, each eight
//! boxes below the root that declares what they read; then changes no reader
//! sees, and a long chain of declarations. Ignored, since it times; run it
//! with
//! `cargo test --release -p exact-kernel --test it property_cost -- --ignored --nocapture`.

use exact_kernel::{Kernel, NodeType, Offer, Op, StyleId, StyleProps, StyleValue};
use std::time::{Duration, Instant};

const READERS: u32 = 2_000;
const DEPTH: u32 = 8;

fn style(id: u32, rows: &[(StyleId, &str)]) -> Op {
    let mut patch = StyleProps::default();
    for (row, text) in rows {
        patch
            .set_dynamic(*row, &StyleValue::Text((*text).into()))
            .unwrap();
    }
    Op::SetStyle {
        id,
        patch: Box::new(patch),
    }
}

/// The root (1) declares `declared`; each reader pads its top by `pad`
/// below a chain of `DEPTH` boxes of its own.
fn build(declared: Option<&str>, pad: &str) -> (Kernel, Vec<u32>) {
    let mut k = Kernel::with_monospace();
    let mut ops = vec![Op::CreateView {
        id: 1,
        node_type: NodeType::View,
    }];
    let mut root: Vec<(StyleId, &str)> = vec![(StyleId::Height, "844px")];
    if let Some(d) = declared {
        root.push((StyleId::SafeAreaProperties, d));
    }
    ops.push(style(1, &root));
    let (mut next, mut tops, mut readers) = (2, Vec::new(), Vec::new());
    for _ in 0..READERS {
        let mut parent = 0;
        for level in 0..=DEPTH {
            ops.push(Op::CreateView {
                id: next,
                node_type: NodeType::View,
            });
            if level == 0 {
                tops.push(next);
            } else {
                ops.push(Op::SetChildren {
                    id: parent,
                    children: vec![next],
                });
            }
            parent = next;
            next += 1;
        }
        ops.push(style(
            parent,
            &[(StyleId::PaddingTop, pad), (StyleId::Height, "1px")],
        ));
        readers.push(parent);
    }
    ops.push(Op::SetChildren {
        id: 1,
        children: tops,
    });
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    (k, readers)
}

fn layout(k: &mut Kernel) {
    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
}

/// The median of `runs` timings of `f`.
fn median(runs: usize, mut f: impl FnMut() -> Duration) -> Duration {
    let mut all: Vec<Duration> = (0..runs).map(|_| f()).collect();
    all.sort();
    all[runs / 2]
}

#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn property_cost() {
    let cases: [(&str, Option<&str>, &str); 4] = [
        ("literal 8px", None, "8px"),
        ("var(--pad)", Some("--pad: 8px"), "var(--pad)"),
        (
            "capture var(--top)",
            Some("--top: var(--exact-safe-area-inset-top)"),
            "var(--top)",
        ),
        ("local inset", None, "var(--exact-safe-area-inset-top)"),
    ];
    for (name, declared, pad) in cases {
        let build_time = median(9, || {
            let t = Instant::now();
            let (mut k, _) = build(declared, pad);
            layout(&mut k);
            t.elapsed()
        });
        let (mut k, readers) = build(declared, pad);
        layout(&mut k);
        let steady = median(21, || {
            let t = Instant::now();
            layout(&mut k);
            t.elapsed()
        });
        // A change every reader sees: the declaration, the root's inset, or each literal
        let mut flip = false;
        let change = median(21, || {
            flip = !flip;
            let v = if flip { "9px" } else { "8px" };
            let t = Instant::now();
            match (declared, pad) {
                (Some(d), _) if d.starts_with("--pad") => {
                    k.apply(
                        0,
                        2,
                        &[style(
                            1,
                            &[(StyleId::SafeAreaProperties, &format!("--pad: {v}"))],
                        )],
                    )
                    .unwrap();
                }
                (Some(_), _) | (None, "var(--exact-safe-area-inset-top)") => {
                    let top = if flip { 9.0 } else { 8.0 };
                    k.set_local_insets(1, Some([top, 0.0, 0.0, 0.0])).unwrap();
                }
                _ => {
                    let ops: Vec<Op> = readers
                        .iter()
                        .map(|&r| style(r, &[(StyleId::PaddingTop, v)]))
                        .collect();
                    k.apply(0, 2, &ops).unwrap();
                }
            }
            layout(&mut k);
            t.elapsed()
        });
        let y = k.node(*readers.last().unwrap()).unwrap();
        println!(
            "{name:>20}: build+layout {build_time:>9.2?}  steady layout {steady:>9.2?}  change+layout {change:>9.2?}  (last reader's content y {})",
            y.frame.y
        );
    }
}

/// Changes no reader's value depends on: a declaration nobody reads, an
/// owner's inset under readers of a literal property, and a change hidden
/// below a nearer declaration of the same name. Each reports the commit and
/// the layout apart, with the nodes the commit touched and the frames the
/// layout changed.
#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn property_cost_of_unseen_changes() {
    let report =
        |name: &str, k: &mut Kernel, change: &mut dyn FnMut(&mut Kernel, bool) -> Option<usize>| {
            let (mut flip, mut commits, mut layouts, mut touched, mut moved) =
                (false, Vec::new(), Vec::new(), None, 0);
            for _ in 0..21 {
                flip = !flip;
                let t = Instant::now();
                touched = change(k, flip);
                commits.push(t.elapsed());
                let t = Instant::now();
                moved = k
                    .compute_layout(1, Offer::definite(390.0, 844.0))
                    .unwrap()
                    .changed
                    .len();
                layouts.push(t.elapsed());
            }
            commits.sort();
            layouts.sort();
            let touched = touched.map_or("touched nodes unreported".into(), |n: usize| {
                format!("{n} nodes touched")
            });
            println!(
                "{name:>34}: commit {:>9.2?} ({touched})  layout {:>9.2?} ({moved} frames changed)",
                commits[10], layouts[10]
            );
        };
    let (mut k, _) = build(Some("--pad: 8px; --unread: 1px"), "var(--pad)");
    layout(&mut k);
    report("an unread declaration", &mut k, &mut |k, flip| {
        let v = if flip {
            "--pad: 8px; --unread: 2px"
        } else {
            "--pad: 8px; --unread: 1px"
        };
        k.apply(0, 2, &[style(1, &[(StyleId::SafeAreaProperties, v)])])
            .unwrap()
            .touched
            .len()
            .into()
    });
    report("an inset under literal readers", &mut k, &mut |k, flip| {
        let top = if flip { 9.0 } else { 8.0 };
        k.set_local_insets(1, Some([top, 0.0, 0.0, 0.0])).unwrap();
        // `set_local_insets` reports only whether any was
        None
    });
    // Each reader's chain declares --pad again at its top box
    let (mut k, _) = build(Some("--pad: 8px"), "var(--pad)");
    let tops: Vec<Op> = (0..READERS)
        .map(|i| {
            style(
                2 + i * (DEPTH + 1),
                &[(StyleId::SafeAreaProperties, "--pad: 6px")],
            )
        })
        .collect();
    k.apply(0, 2, &tops).unwrap();
    layout(&mut k);
    report(
        "a change below nearer declarations",
        &mut k,
        &mut |k, flip| {
            let v = if flip { "--pad: 9px" } else { "--pad: 8px" };
            k.apply(0, 3, &[style(1, &[(StyleId::SafeAreaProperties, v)])])
                .unwrap()
                .touched
                .len()
                .into()
        },
    );
    // For scale: a change every reader sees
    let (mut k, _) = build(Some("--pad: 8px"), "var(--pad)");
    layout(&mut k);
    report("a change every reader sees", &mut k, &mut |k, flip| {
        let v = if flip { "--pad: 9px" } else { "--pad: 8px" };
        k.apply(0, 3, &[style(1, &[(StyleId::SafeAreaProperties, v)])])
            .unwrap()
            .touched
            .len()
            .into()
    });
}

/// A chain of declarations, each reading the one before twice: what one
/// reader's resolution costs as it grows, to the expansion bound.
#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn property_cost_of_a_doubling_chain() {
    for levels in [8, 12, 16] {
        let mut declared = vec!["--p0: 1%".to_string()];
        for i in 1..=levels {
            declared.push(format!(
                "--p{i}: calc(var(--p{}) + env(safe-area-inset-top) + var(--p{}))",
                i - 1,
                i - 1
            ));
        }
        let declared = declared.join("; ");
        let pad = format!("var(--p{levels}, 7px)");
        let build_time = median(9, || {
            let t = Instant::now();
            let (mut k, _) = build(Some(&declared), &pad);
            layout(&mut k);
            t.elapsed()
        });
        println!("{levels:>2} levels: build+layout of {READERS} readers {build_time:>9.2?}");
    }
}
