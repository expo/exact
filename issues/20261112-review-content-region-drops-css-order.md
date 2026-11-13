# A content region ignores CSS order in flex and grid layout

**Status:** Open
**Systems:** kernel/src/region/tree.rs, kernel/src/layout/order.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, registering a content region changes the layout of
its flex and grid children when they have nonzero `order`. The ordinary tree
uses `order::laid_out`, which stably sorts flex and grid items by their authored
order (`kernel/src/layout/order.rs:15-42`). The region's derived tree instead
passes the arena's original child sequence directly to the engine
(`kernel/src/region/tree.rs:49-65`). Both region profiles accept and publish this
different geometry as current.

For a 100 by 100 clipped owner, make its content a flex row with two 20 by 20
views, `a` followed by `b`, with `a.order = 1` and `b.order = 0`. Ordinary layout
puts `b` first; a content region puts `a` first. The same omission changes grid
auto-placement. Apple and Linux content-region registration both reach this
kernel path (`host/apple/src/content_region.rs:69-71`,
`host/linux/src/content_region.rs:100-102`). This is an incorrect accepted
publication, rather than a pending measurement.

## Verification

A temporary Rust probe built the same operation stream in ordinary and region
kernels and used the existing compiled kernel library. It measured these
coordinates for `[a, b]`:

| Container | Ordinary layout | PinnedOffers region | SplitFacts region |
| --- | --- | --- | --- |
| Flex, x | `[20, 0]` | `[0, 20]` | `[0, 20]` |
| Grid, y | `[20, 0]` | `[0, 20]` | `[0, 20]` |
| Block control, y | `[0, 20]` | `[0, 20]` | `[0, 20]` |

Every region receipt had `current = true`. The fixture has no text, so no
asynchronous shaping or native UI was involved. Source and output are
`/tmp/exact-review-20261112/region-order.rs` and `region-order.log`.

## Direction and validation

Build derived child lists through the same ordering rule as ordinary layout,
using the derived tree's handle map and preserving the explicit branch cut.
Compare ordinary and accepted region frames for flex and grid containers with
positive, negative and equal orders, including changes after publication and a
pending branch. Keep the block control in document order.

## Duplicate check

Searched open and closed filesystem issues and all 275 cached GitHub issues.
The closed `20261110-order-sorts-children-of-a-new-block-parent.md`, read in full,
fixes stale display information in the ordinary tree's ordering helper. This
derived tree never calls that helper. The closed
`20260924-content-region-quadratic-line-break.md` concerns native text shaping,
not child layout. No matching report was found.
