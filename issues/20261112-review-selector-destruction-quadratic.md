# Destroying a subtree with descending selector names takes quadratic time

**Status:** Open
**Systems:** kernel/src/txn.rs, kernel/src/sorted.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At reviewed commit `f6be99af`, removing a large subtree stalls in selector cleanup
when its `testId` or `id` names arrive out of lexical order. The main selector
index already uses a `BTreeMap` for this reason (`kernel/src/selector.rs:13-18`),
but the destruction batch groups departing names in a `SortedMap` by repeated
insertion (`kernel/src/txn.rs:1223-1227`). Each insertion moves the vector's tail
(`kernel/src/sorted.rs:106-110`). Descending distinct names therefore move
N(N−1)/2 entries before removing them from the index.

This path runs for one `DestroyView` of a parent as well as a run of child
destroys. A screen replacement or closing a populated panel can hit it;
virtualization is not involved in the reproduction.

## Measurement

An external release-mode Rust probe used this checkout's `exact-kernel` and
vendored Taffy, with Rust 1.97.0 on this Mac. It created one root with N ordinary
children through `Kernel::with_monospace_on_demand()`, then timed only
`apply(..., &[DestroyView { id: 1 }])`. No layout tree, layout pass, host, or
network was involved. Creation and destruction of the probe's surrounding
Rust objects were outside the timed interval. Every receipt contained N+1
destroyed nodes.

Each cell is the median of three runs, in milliseconds:

| Children | No selector | Ascending names | Descending names |
| ---: | ---: | ---: | ---: |
| 5,000 | 0.648 | 1.332 | 11.267 |
| 10,000 | 0.452 | 1.545 | 43.036 |
| 20,000 | 0.924 | 3.382 | 170.220 |
| 40,000 | 1.919 | 6.871 | 681.914 |

Names were `row-00000000` upward, or the same range downward; tree structure and
operation counts were identical. The near-fourfold destruction time each time
the descending case doubles agrees with the vector-insertion cost. These are
isolated kernel timings, not end-to-end or phone latency measurements.

Command: `cargo run --release --offline --manifest-path
/tmp/exact-review-20261112/kernel-destroy/Cargo.toml`. The temporary probe is
outside the repository.

## Direction and validation

Group the collected names by sorting once and scanning equal-name runs, or use
a map that does not shift all preceding entries on insertion. Preserve the
one-pass removal of many nodes sharing a selector. Check unique names in both
orders, repeated names, and mixed `id`/`testId` removal, including stale selector
absence and unchanged receipt order. Measure scaling in an optimized build.

## Duplicate check

Searched open and closed filesystem issues, `QUEUE.md`, and all 275 GitHub
issues retrieved on 2026-11-12. The closed
`20260904-measure-runner-update-scaling.md` addressed other child-list and keyed
lookup costs. The closed `20261102-collection-duplicate-keys-rekey-quadratically.md`
addressed row identity allocation. GitHub #211 addresses compiler scope
lookups. None describes this remaining destruction-time grouping loop.
