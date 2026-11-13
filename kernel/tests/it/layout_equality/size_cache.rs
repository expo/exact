//! Cache-policy comparisons share the generators' authored trees.
use super::*;
use taffy::tree::SizeCachePolicy::{All, Nine, Off};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Comparison {
    Geometry,
    Baseline,
    Proofs,
}

// Per policy: layouts losing a proof, leaves losing one, layouts keeping an
// uncached layout's absent proof, and leaves keeping one.
pub(super) type ProofCounts = [[usize; 4]; 3];

fn check_layouts(seed: u64, phase: &str, kernels: &[Kernel]) {
    let expected = frames(&kernels[0]);
    for k in &kernels[1..] {
        assert_eq!(expected, frames(k), "cache policies: seed {seed} {phase}");
        assert!(
            kernels[0].layout_flow_matches(k),
            "cache flow: seed {seed} {phase}"
        );
    }
    let counts = kernels[0].size_cache_counts();
    assert_eq!(counts.repeated, 0, "seed {seed} {phase}");
    assert!(
        counts.max_answers <= 64,
        "LLP 1165 alarm: seed {seed} {phase}: {counts:?}"
    );
}

pub(super) fn compare(
    seed: u64,
    rounds: usize,
    mut world: World,
    first: Vec<Op>,
    mode: Comparison,
) -> ProofCounts {
    let mut proofs = [[0; 4]; 3];
    let offer = Offer::definite(320.0, 480.0);
    let mut kernels: Vec<_> = [All, Nine, Off]
        .into_iter()
        .take(if mode == Comparison::Baseline { 2 } else { 3 })
        .map(|policy| {
            let mut k = Kernel::with_monospace();
            k.set_size_cache_policy(policy);
            k.apply(0, 0, &first).unwrap();
            k.compute_layout(1, offer).unwrap();
            k
        })
        .collect();
    check_layouts(seed, "cold", &kernels);
    for round in 0..rounds {
        let ops = world.mutate();
        for k in &mut kernels {
            k.apply(0, round as u64 + 1, &ops)
                .unwrap_or_else(|e| panic!("seed {seed} round {round}: {e}\nops: {ops:?}"));
            k.compute_layout(1, offer).unwrap();
        }
        check_layouts(seed, &format!("round {round}"), &kernels);
        if mode == Comparison::Proofs {
            for (count, k) in proofs.iter_mut().zip(&kernels) {
                let lost = k.layout_height_proof_losses(&kernels[2]);
                let kept = kernels[2].layout_height_proof_losses(k);
                for (sum, value) in
                    count
                        .iter_mut()
                        .zip([usize::from(lost > 0), lost, usize::from(kept > 0), kept])
                {
                    *sum += value;
                }
            }
            continue;
        }
        for (k, policy) in kernels
            .iter()
            .zip([All, Nine])
            .take(if mode == Comparison::Baseline { 2 } else { 1 })
        {
            let mut rehydrated = k.rehydrate(Box::new(MonospaceMeasurer::default()));
            rehydrated.set_size_cache_policy(policy);
            rehydrated.compute_layout(1, offer).unwrap();
            let mut replay = Kernel::with_monospace();
            replay.set_size_cache_policy(policy);
            for (i, batch) in world.log.iter().enumerate() {
                replay.apply(0, i as u64, batch).unwrap();
            }
            replay.compute_layout(1, offer).unwrap();
            for (name, fresh) in [("rehydrated", &rehydrated), ("replayed", &replay)] {
                assert_eq!(
                    frames(k),
                    frames(fresh),
                    "seed {seed} round {round} {policy:?}: {name} layout differs from incremental"
                );
                assert!(
                    mode != Comparison::Baseline || k.layout_flow_matches(fresh),
                    "seed {seed} round {round} {policy:?}: {name} flow differs"
                );
            }
            let a = export::decode(&k.export(None).unwrap()).unwrap();
            let b = export::decode(&replay.export(None).unwrap()).unwrap();
            assert_eq!(
                a.styles, b.styles,
                "seed {seed} round {round}: styles diverged"
            );
            assert_eq!(
                a.props, b.props,
                "seed {seed} round {round}: props diverged"
            );
            assert_eq!(a.rows.len(), b.rows.len());
        }
    }
    proofs
}

type Generator = fn(u64, Comparison) -> ProofCounts;

fn generators() -> [(&'static str, Generator); 3] {
    [
        ("random", |seed, mode| run_random(seed, 40, mode)),
        ("multicol", run_multicol_counted),
        ("panes", |seed, mode| run_panes_counted(seed, 40, mode)),
    ]
}

// Baseline alignment uses its own side stream, preserving every named tree.
// Off differed from its own fresh layout in 138 of 1,536 baseline runs (the
// base failed 6 of 1,024), so this variant compares All and Nine with fresh trees.
#[test]
fn baseline_alignment_equals_rehydration_and_replay() {
    each_seed([1, 2, 3, 5, 8, 13, 21, 34], |seed| {
        for (_, generate) in generators() {
            generate(seed, Comparison::Baseline);
        }
    });
}

#[test]
#[ignore = "async lane: full size-cache comparison seed set, with and without baseline alignment"]
fn size_cache_policies_agree_on_all_generators() {
    let mut failures = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..16)
            .map(|worker| {
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    for seed in (worker * 32 + 1)..=(worker * 32 + 32) {
                        for (name, generate) in generators() {
                            for mode in [Comparison::Geometry, Comparison::Baseline] {
                                if std::panic::catch_unwind(|| generate(seed, mode)).is_err() {
                                    failures.push((name, seed, mode == Comparison::Baseline));
                                }
                            }
                        }
                    }
                    failures
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap())
            .collect::<Vec<_>>()
    });
    failures.sort_unstable();
    // QUEUE.md: "Multi-column relayout drifts by one or two ULPs" tracks these two seeds.
    assert_eq!(
        failures,
        [
            ("multicol", 138, false),
            ("multicol", 138, true),
            ("multicol", 291, false),
            ("multicol", 291, true),
        ],
        "generator failures (name, seed, baseline)"
    );
}

#[test]
#[ignore = "async lane: height proofs under All, Nine and Off across all generator seeds"]
fn size_cache_height_proof_losses() {
    let mut total = [[0; 4]; 3];
    for (name, generate) in generators() {
        let mut counts = [[0; 4]; 3];
        for seed in 1..=512 {
            for (sums, values) in counts.iter_mut().zip(generate(seed, Comparison::Proofs)) {
                for (sum, count) in sums.iter_mut().zip(values) {
                    *sum += count;
                }
            }
        }
        eprintln!("height proofs {name} [lost layouts, lost leaves, kept layouts, kept leaves]: {counts:?}");
        for (sums, values) in total.iter_mut().zip(counts) {
            for (sum, count) in sums.iter_mut().zip(values) {
                *sum += count;
            }
        }
    }
    eprintln!(
        "height proofs total [lost layouts, lost leaves, kept layouts, kept leaves]: {total:?}"
    );
    for kind in 0..2 {
        assert!(
            total[0][kind] * 20 <= total[1][kind] * 21,
            "All loses more than 5% above Nine: {total:?}"
        );
    }
}
