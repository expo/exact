//! What observations cost against main's local insets (LLP 1123 D9
//! "Cost"), timed; each test is ignored and run with `--release --ignored
//! --nocapture`.
use super::*;

/// What observations cost (D9 "Cost", V5's kernel part): 1,000 rows of ten
/// boxes in an owner, of which none, one, 100 or 1,000 pad their last box
/// by its left observation under a side band that reaches every row; the
/// build and first layout, a steady relayout, and a change of the band with
/// the layouts its solve adds. Ignored, since it times; run it with
/// `cargo test --release -p exact-kernel --lib observation_cost -- --ignored --nocapture`.
#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn observation_cost() {
    use std::time::{Duration, Instant};
    const ROWS: u32 = 1_000;
    const DEPTH: u32 = 10;
    let median = |mut all: Vec<Duration>| {
        all.sort();
        all[all.len() / 2]
    };
    for (model, readers) in [false, true]
        .into_iter()
        .flat_map(|local| [0, 1, 100, 1_000].map(|n| (local, n)))
    {
        let leaf = move || match model {
            // Main's model: the owner's local inset, which its subtree inherits
            true => Node::Env(crate::style::calc::EnvTerm::Local(Edge::Left)),
            false => observed(Edge::Left, None),
        };
        let set = move |k: &mut Kernel, left: f32| match model {
            true => k
                .set_local_insets(2, Some([0.0, 0.0, 0.0, left]))
                .map(|_| ()),
            false => k
                .set_owner_band(2, Some([0.0, 0.0, 0.0, left]), &[])
                .map(|_| ()),
        };
        let build = |readers: u32| {
            let mut ops = Vec::new();
            let mut tops = Vec::new();
            for row in 0..ROWS {
                let first = 10 + row * DEPTH;
                tops.push(first);
                ops.extend(create(first..first + DEPTH));
                for level in 1..DEPTH {
                    ops.push(children(first + level - 1, &[first + level]));
                }
                ops.push(style(first, vec![], &[(StyleId::Height, "20")]));
                let last = first + DEPTH - 1;
                let pad = match row < readers {
                    true => vec![(StyleId::PaddingTop, leaf())],
                    false => vec![(StyleId::PaddingTop, px(8.0))],
                };
                ops.push(style(last, pad, &[(StyleId::Height, "1")]));
            }
            ops.extend([children(1, &[2]), children(2, &tops)]);
            let mut k = owner(390.0, 844.0, ops);
            set(&mut k, 8.0).unwrap();
            k
        };
        let builds = (0..9)
            .map(|_| {
                let t = Instant::now();
                let mut k = build(readers);
                lay_out(&mut k);
                t.elapsed()
            })
            .collect();
        let mut k = build(readers);
        lay_out(&mut k);
        let steady = (0..21)
            .map(|_| {
                let t = Instant::now();
                lay_out(&mut k);
                t.elapsed()
            })
            .collect();
        let (mut changes, mut rounds) = (Vec::new(), 0);
        for run in 0..21 {
            let left = 9.0 + (run % 2) as f32;
            let t = Instant::now();
            set(&mut k, left).unwrap();
            lay_out(&mut k);
            changes.push(t.elapsed());
            rounds = k.arena.observations.rounds;
        }
        // The band set to what it is, and a change on an edge no reader reads
        let timed = |k: &mut Kernel, band: [f32; 4]| {
            let t = Instant::now();
            if model {
                k.set_local_insets(2, Some(band)).unwrap();
            } else {
                k.set_owner_band(2, Some(band), &[]).unwrap();
            }
            lay_out(k);
            t.elapsed()
        };
        let same = median(
            (0..21)
                .map(|_| timed(&mut k, [0.0, 0.0, 0.0, 10.0]))
                .collect(),
        );
        let unread = (0..21).map(|run| timed(&mut k, [0.0, (run % 2) as f32 * 5.0, 0.0, 10.0]));
        let unread = median(unread.collect());
        let name = if model {
            "local-inset readers"
        } else {
            "observers"
        };
        println!(
            "{readers:>5} {name}: build+layout {:>9.2?}  steady layout {:>9.2?}  band change+layout {:>9.2?} ({rounds} rounds added)  same band {same:>9.2?}  unread edge {unread:>9.2?}",
            median(builds),
            median(steady),
            median(changes)
        );
    }
}

/// One owner's band changing among 1, 10 or 100 owners side by side, each
/// with ten rows that pad by their left observation; the same with rows
/// that pad by their owner's controller's raw left inset, as its report
/// changes; or, as main's model, by their owner's local left inset; the
/// same change to the right edge, which no row reads; and a box that holds
/// no reader moving between two others. Ignored, since it times; run it with
/// `cargo test --release -p exact-kernel --lib observation_cost_of_many_owners -- --ignored --nocapture`.
#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn observation_cost_of_many_owners() {
    use crate::style::targets::{ChainEntry, Kind, NativeTarget, NativeTargets};
    use std::time::Instant;
    let models = ["observers", "raw readers", "local-inset readers"];
    for (name, owners) in models
        .into_iter()
        .flat_map(|m| [1u32, 10, 100].map(|o| (m, o)))
    {
        let leaf = move || match name {
            "observers" => observed(Edge::Left, None),
            "raw readers" => observe(Target::Kind(Kind::Owner), Edge::Left, None),
            _ => Node::Env(crate::style::calc::EnvTerm::Local(Edge::Left)),
        };
        let set = move |k: &mut Kernel, root: u32, left: f32, right: f32| match name {
            "observers" => k
                .set_owner_band(root, Some([0.0, right, 0.0, left]), &[])
                .map(drop),
            "raw readers" => {
                let target = NativeTarget {
                    id: u64::from(root),
                    generation: 1,
                    insets: Some([0.0, right, 0.0, left]),
                };
                let chain = vec![ChainEntry { target, kind: None }];
                let report = NativeTargets {
                    chain,
                    relations: Vec::new(),
                };
                k.set_owner_targets(root, report).map(drop)
            }
            _ => k
                .set_local_insets(root, Some([0.0, right, 0.0, left]))
                .map(drop),
        };
        let mut k = Kernel::with_monospace();
        let mut ops = create(1..=1);
        ops.push(style(1, vec![], &[(StyleId::Height, "844")]));
        let roots: Vec<u32> = (0..owners).map(|o| 10_000 + o * 100).collect();
        for &root in &roots {
            ops.extend(create(root..=root + 10));
            ops.push(takes_container(root));
            let rows: Vec<u32> = (root + 1..=root + 10).collect();
            for &row in &rows {
                let pad = vec![(StyleId::PaddingTop, leaf())];
                ops.push(style(row, pad, &[(StyleId::Height, "20")]));
            }
            ops.push(children(root, &rows));
        }
        // Two empty boxes and a third that moves between them, no reader's
        ops.extend(create(2..=4));
        let mut tops = vec![2, 3];
        tops.extend(&roots);
        ops.extend([children(1, &tops), children(2, &[4])]);
        ops.push(Op::AttachRoot { id: 1 });
        k.apply(0, 1, &ops).unwrap();
        let block = block(BlockAxis::Is(390.0), BlockAxis::Is(844.0));
        for &root in &roots {
            k.set_host_block(root, Some(block)).unwrap();
            set(&mut k, root, 8.0, 0.0).unwrap();
        }
        lay_out(&mut k);
        let mut moves: Vec<_> = (0..21u32)
            .map(|run| {
                let (from, to) = if run % 2 == 0 { (2, 3) } else { (3, 2) };
                let ops = [children(from, &[]), children(to, &[4])];
                let t = Instant::now();
                k.apply(0, 2 + u64::from(run), &ops).unwrap();
                lay_out(&mut k);
                t.elapsed()
            })
            .collect();
        moves.sort();
        let mut all: Vec<_> = (0..21)
            .map(|run| {
                let t = Instant::now();
                set(&mut k, roots[0], 9.0 + (run % 2) as f32, 0.0).unwrap();
                lay_out(&mut k);
                t.elapsed()
            })
            .collect();
        all.sort();
        let rounds = k.arena.observations.rounds;
        // The right edge, which no row reads
        let mut unread: Vec<_> = (0..21)
            .map(|run| {
                let t = Instant::now();
                set(&mut k, roots[0], 9.0, (run % 2) as f32 * 5.0).unwrap();
                lay_out(&mut k);
                t.elapsed()
            })
            .collect();
        unread.sort();
        let readers = owners * 10;
        println!(
            "one of {owners:>3} owners ({readers} {name}): band change+layout {:>9.2?} ({rounds} rounds added)  unread edge+layout {:>9.2?}  a move elsewhere+layout {:>9.2?}",
            all[10],
            unread[10],
            moves[10]
        );
    }
}

/// What taking the band costs (LLP 1123 D4): a row's change among 1,000
/// rows, then layout, under a wrapper of full or automatic height that
/// takes the band or not; and 1,000 rows that each hold a box that ignores
/// the bottom or not. Ignored, since it times; run it with
/// `cargo test --release -p exact-kernel --lib ignore_cost -- --ignored --nocapture`.
#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn ignore_cost() {
    use std::time::{Duration, Instant};
    const ROWS: u32 = 1_000;
    let median = |mut all: Vec<Duration>| {
        all.sort();
        all[all.len() / 2]
    };
    // A row's change, then layout, the median of 41
    let time = |k: &mut Kernel, row: ViewId| {
        let times = (0..41).map(|run| {
            let height = if run % 2 == 0 { "11" } else { "10" };
            let t = Instant::now();
            k.apply(
                0,
                2 + run,
                &[style(row, vec![], &[(StyleId::Height, height)])],
            )
            .unwrap();
            lay_out(k);
            t.elapsed()
        });
        median(times.collect())
    };
    let root = [(StyleId::SafeAreaIgnore, "none"), (StyleId::Height, "100%")];
    for (height, ignore) in [("100%", "container"), ("auto", "container top")] {
        for ignore in ["none", ignore] {
            let rows: Vec<ViewId> = (10..10 + ROWS).collect();
            let mut ops = create(3..=3);
            ops.extend(create(rows.iter().copied()));
            for &r in &rows {
                ops.push(style(r, vec![], &[(StyleId::Height, "10")]));
            }
            ops.push(style(2, vec![], &root));
            let wrapper = [(StyleId::SafeAreaIgnore, ignore), (StyleId::Height, height)];
            ops.push(style(3, vec![], &wrapper));
            ops.extend([children(1, &[2]), children(2, &[3]), children(3, &rows)]);
            let mut k = owner(390.0, 844.0, ops);
            k.set_owner_band(2, Some([47.0, 0.0, 34.0, 0.0]), &[])
                .unwrap();
            lay_out(&mut k);
            let t = time(&mut k, 510);
            println!("ignore_cost: wrapper of height {height}, ignore {ignore}: {t:.2?}");
        }
    }
    for ignore in ["none", "container bottom"] {
        let mut ops = Vec::new();
        let mut rows = Vec::new();
        for row in 0..ROWS {
            let (outer, inner) = (10 + row * 2, 11 + row * 2);
            rows.push(outer);
            ops.extend(create(outer..=inner));
            ops.push(style(outer, vec![], &[(StyleId::Height, "10")]));
            let held = [(StyleId::SafeAreaIgnore, ignore), (StyleId::Height, "5")];
            ops.push(style(inner, vec![], &held));
            ops.push(children(outer, &[inner]));
        }
        ops.push(style(2, vec![], &root));
        ops.extend([children(1, &[2]), children(2, &rows)]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([47.0, 0.0, 34.0, 0.0]), &[])
            .unwrap();
        lay_out(&mut k);
        let t = time(&mut k, 1010);
        println!("ignore_cost: 1,000 rows each holding a box, ignore {ignore}: {t:.2?}");
    }
}

/// What measuring a height target costs (`measure_height_targets`) where a
/// reader exists, which measures in a layout of copies with the solve, and
/// where none does: a `border-box` target of 1,000 rows in an owner, a row's
/// change pending, then the measurement and the layout after it, the
/// medians of 41, with the test build's size counting off. Run it with
/// `cargo test --release -p exact-kernel --lib height_targets_cost -- --ignored --nocapture`.
#[test]
#[ignore = "times; run with --release --ignored --nocapture"]
fn height_targets_cost() {
    use std::time::{Duration, Instant};
    let median = |mut all: Vec<Duration>| {
        all.sort();
        all[all.len() / 2]
    };
    for reader in [false, true] {
        let rows: Vec<ViewId> = (10..1_010).collect();
        let mut ops = create(3..=3);
        ops.extend(create(rows.iter().copied()));
        ops.push(style(3, vec![], &[(StyleId::BoxSizing, "border-box")]));
        for &r in &rows {
            let reads = reader && r == 10;
            let pad = match reads {
                true => vec![(StyleId::PaddingTop, observed(Edge::Top, None))],
                false => vec![],
            };
            ops.push(style(r, pad, &[(StyleId::Height, "10")]));
        }
        ops.extend([children(1, &[2]), children(2, &[3]), children(3, &rows)]);
        let mut k = owner(390.0, 844.0, ops);
        k.set_owner_band(2, Some([47.0, 0.0, 0.0, 0.0]), &[])
            .unwrap();
        lay_out(&mut k);
        k.tree_mut().track_sizes(false);
        let (target, offer) = ([k.arena.key(slot(&k, 3))], Offer::definite(390.0, 844.0));
        let (mut measures, mut layouts) = (Vec::new(), Vec::new());
        for run in 0..41 {
            let height = if run % 2 == 0 { "11" } else { "10" };
            let change = [style(510, vec![], &[(StyleId::Height, height)])];
            k.apply(0, 2 + run, &change).unwrap();
            let t = Instant::now();
            k.measure_height_targets(1, offer, &target).unwrap();
            measures.push(t.elapsed());
            let t = Instant::now();
            lay_out(&mut k);
            layouts.push(t.elapsed());
        }
        println!(
            "height target of 1,000 rows, a reader {reader}: measure {:.2?}, the layout after {:.2?}",
            median(measures),
            median(layouts)
        );
    }
}
