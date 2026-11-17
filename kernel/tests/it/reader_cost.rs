//! What a change costs by how many boxes read it (LLP 1123 D9 "Cost", V5):
//! 1,000 rows, each a chain of ten boxes below the owner's root, of which
//! none, one, 100 or 1,000 read what changes. Each case times the commit
//! and the layout after it apart: an owner's inset, a declaration, and a row
//! inserted where a declaration is in scope; then a declaration over many
//! nodes that declare it again, one of many owners, and a chain of
//! captures. Ignored, since it times; run it with
//! `cargo test --release -p exact-kernel --test it reader_cost -- --ignored --nocapture --test-threads=1`.

use exact_kernel::{Kernel, NodeType, Offer, Op, StyleId, StyleProps, StyleValue};
use std::time::{Duration, Instant};

const ROWS: u32 = 1_000;
const DEPTH: u32 = 10;
const COUNTS: [u32; 4] = [0, 1, 100, 1_000];

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

/// A row of `DEPTH` boxes from id `first`, its last padded by `pad`, in a
/// fixed height, so a reader's padding moves no other row.
fn row(first: u32, pad: &str, ops: &mut Vec<Op>) {
    for level in 0..DEPTH {
        ops.push(Op::CreateView {
            id: first + level,
            node_type: NodeType::View,
        });
        if level > 0 {
            ops.push(Op::SetChildren {
                id: first + level - 1,
                children: vec![first + level],
            });
        }
    }
    ops.push(style(first, &[(StyleId::Height, "20px")]));
    ops.push(style(
        first + DEPTH - 1,
        &[(StyleId::PaddingTop, pad), (StyleId::Height, "1px")],
    ));
}

/// The root (1) declares `--pad: 8px`; the first `readers` rows pad by
/// `read`, the rest by a literal.
fn build(readers: u32, read: &str) -> Kernel {
    let mut k = Kernel::with_monospace();
    let mut ops = vec![
        Op::CreateView {
            id: 1,
            node_type: NodeType::View,
        },
        style(
            1,
            &[
                (StyleId::Height, "844px"),
                (StyleId::SafeAreaProperties, "--pad: 8px"),
            ],
        ),
    ];
    let tops: Vec<u32> = (0..ROWS).map(|i| 2 + i * DEPTH).collect();
    for (i, &top) in tops.iter().enumerate() {
        row(
            top,
            if (i as u32) < readers { read } else { "8px" },
            &mut ops,
        );
    }
    ops.push(Op::SetChildren {
        id: 1,
        children: tops,
    });
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.set_local_insets(1, Some([8.0, 0.0, 0.0, 0.0])).unwrap();
    layout(&mut k);
    k
}

fn layout(k: &mut Kernel) -> usize {
    k.compute_layout(1, Offer::definite(390.0, 844.0))
        .unwrap()
        .changed
        .len()
}

/// The median commit and layout over 21 runs of `change`, and the frames
/// the last layout changed.
fn time(k: &mut Kernel, mut change: impl FnMut(&mut Kernel, u32)) -> (Duration, Duration, usize) {
    let (mut commits, mut layouts, mut frames) = (Vec::new(), Vec::new(), 0);
    for run in 0..21 {
        let t = Instant::now();
        change(k, run);
        commits.push(t.elapsed());
        let t = Instant::now();
        frames = layout(k);
        layouts.push(t.elapsed());
    }
    commits.sort();
    layouts.sort();
    (commits[10], layouts[10], frames)
}

fn report(name: &str, readers: u32, (commit, layout, frames): (Duration, Duration, usize)) {
    println!(
        "{name:>22}, {readers:>5} readers: commit {commit:>9.2?}  layout {layout:>9.2?} ({frames} frames changed)"
    );
}

#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn reader_cost_of_an_inset() {
    for readers in COUNTS {
        let mut k = build(readers, "var(--exact-safe-area-inset-top)");
        let cost = time(&mut k, |k, run| {
            let top = 9.0 + (run % 2) as f32;
            k.set_local_insets(1, Some([top, 0.0, 0.0, 0.0])).unwrap();
        });
        report("an owner's inset", readers, cost);
    }
}

#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn reader_cost_of_a_declaration() {
    for readers in COUNTS {
        let mut k = build(readers, "var(--pad)");
        let cost = time(&mut k, |k, run| {
            let v = format!("--pad: {}px", 9 + run % 2);
            k.apply(
                0,
                2 + run as u64,
                &[style(1, &[(StyleId::SafeAreaProperties, &v)])],
            )
            .unwrap();
        });
        report("a declaration", readers, cost);
        // The same readers, while one row's own declaration changes
        let mut k = build(readers, "var(--pad)");
        let last = 2 + (ROWS - 1) * DEPTH;
        let cost = time(&mut k, |k, run| {
            let v = format!("--pad: {}px", 9 + run % 2);
            k.apply(
                0,
                2 + run as u64,
                &[style(last, &[(StyleId::SafeAreaProperties, &v)])],
            )
            .unwrap();
        });
        report("one row's declaration", readers, cost);
    }
}

#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn reader_cost_of_an_inserted_row() {
    for readers in COUNTS {
        let mut k = build(readers, "var(--pad)");
        let mut tops: Vec<u32> = (0..ROWS).map(|i| 2 + i * DEPTH).collect();
        let cost = time(&mut k, |k, run| {
            let first = 2 + (ROWS + run) * DEPTH;
            let mut ops = Vec::new();
            row(first, "var(--pad)", &mut ops);
            tops.push(first);
            ops.push(Op::SetChildren {
                id: 1,
                children: tops.clone(),
            });
            k.apply(0, 2 + run as u64, &ops).unwrap();
        });
        report("an inserted row", readers, cost);
    }
}

/// A declaration over `children` nodes that each declare its name again,
/// with the only reader in another subtree: nothing reads the change.
#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn reader_cost_of_a_wide_redeclaration() {
    for children in [10, 1_000, 10_000] {
        let mut k = Kernel::with_monospace();
        let mut ops = Vec::new();
        let kids: Vec<u32> = (0..children).map(|i| 10 + i).collect();
        for id in [1, 2, 3, 4].into_iter().chain(kids.iter().copied()) {
            ops.push(Op::CreateView {
                id,
                node_type: NodeType::View,
            });
        }
        ops.push(style(1, &[(StyleId::Height, "844px")]));
        ops.push(style(2, &[(StyleId::SafeAreaProperties, "--x: 8px")]));
        ops.push(style(3, &[(StyleId::SafeAreaProperties, "--y: 4px")]));
        ops.push(style(4, &[(StyleId::PaddingTop, "var(--y)")]));
        for &id in &kids {
            ops.push(style(id, &[(StyleId::SafeAreaProperties, "--x: 6px")]));
        }
        ops.push(Op::SetChildren {
            id: 2,
            children: kids,
        });
        ops.push(Op::SetChildren {
            id: 3,
            children: vec![4],
        });
        ops.push(Op::SetChildren {
            id: 1,
            children: vec![2, 3],
        });
        ops.push(Op::AttachRoot { id: 1 });
        k.apply(0, 1, &ops).unwrap();
        layout(&mut k);
        let cost = time(&mut k, |k, run| {
            let v = format!("--x: {}px", 9 + run % 2);
            k.apply(
                0,
                2 + run as u64,
                &[style(2, &[(StyleId::SafeAreaProperties, &v)])],
            )
            .unwrap();
        });
        report(&format!("{children} redeclaring"), 1, cost);
    }
}

/// `owners` owners side by side, each with one box that reads its top
/// inset; one owner's inset changes.
#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn reader_cost_of_many_owners() {
    for owners in [1, 10, 100, 1_000] {
        let mut k = Kernel::with_monospace();
        let mut ops = vec![Op::CreateView {
            id: 1,
            node_type: NodeType::View,
        }];
        ops.push(style(1, &[(StyleId::Height, "844px")]));
        let tops: Vec<u32> = (0..owners).map(|i| 2 + 2 * i).collect();
        for &owner in &tops {
            for id in [owner, owner + 1] {
                ops.push(Op::CreateView {
                    id,
                    node_type: NodeType::View,
                });
            }
            // A fixed height, so one owner's reader moves no other owner
            ops.push(style(owner, &[(StyleId::Height, "20px")]));
            ops.push(style(
                owner + 1,
                &[(StyleId::PaddingTop, "var(--exact-safe-area-inset-top)")],
            ));
            ops.push(Op::SetChildren {
                id: owner,
                children: vec![owner + 1],
            });
        }
        ops.push(Op::SetChildren {
            id: 1,
            children: tops.clone(),
        });
        ops.push(Op::AttachRoot { id: 1 });
        k.apply(0, 1, &ops).unwrap();
        for &owner in &tops {
            k.set_local_insets(owner, Some([8.0, 0.0, 0.0, 0.0]))
                .unwrap();
        }
        layout(&mut k);
        let cost = time(&mut k, |k, run| {
            let top = 9.0 + (run % 2) as f32;
            k.set_local_insets(tops[0], Some([top, 0.0, 0.0, 0.0]))
                .unwrap();
        });
        report(&format!("one of {owners} owners"), owners, cost);
    }
}

/// A chain of `depth` captures below the owner's root, each declaring its
/// own name from the top inset, and one box below the last that reads it.
#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn reader_cost_of_nested_captures() {
    for depth in [1, 10, 100] {
        let mut k = Kernel::with_monospace();
        let mut ops = Vec::new();
        for id in 1..=depth + 2 {
            ops.push(Op::CreateView {
                id,
                node_type: NodeType::View,
            });
        }
        ops.push(style(1, &[(StyleId::Height, "844px")]));
        for level in 1..=depth {
            let declared = format!("--c{level}: var(--exact-safe-area-inset-top)");
            ops.push(style(
                1 + level,
                &[(StyleId::SafeAreaProperties, &declared)],
            ));
        }
        let read = format!("var(--c{depth})");
        ops.push(style(depth + 2, &[(StyleId::PaddingTop, &read)]));
        for id in 1..=depth + 1 {
            ops.push(Op::SetChildren {
                id,
                children: vec![id + 1],
            });
        }
        ops.push(Op::AttachRoot { id: 1 });
        k.apply(0, 1, &ops).unwrap();
        k.set_local_insets(1, Some([8.0, 0.0, 0.0, 0.0])).unwrap();
        layout(&mut k);
        let cost = time(&mut k, |k, run| {
            let top = 9.0 + (run % 2) as f32;
            k.set_local_insets(1, Some([top, 0.0, 0.0, 0.0])).unwrap();
        });
        report(&format!("{depth} nested captures"), 1, cost);
    }
}
