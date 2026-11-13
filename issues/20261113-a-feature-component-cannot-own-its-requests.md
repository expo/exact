# A feature cannot be a component: only the root may own a resource, mutation or task

**Status:** Open
**Systems:** contract/types/src/lib.rs and contract/types/src/tasks.rs (`type-child-resource`), runner, bake
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261021-repeated-routes-share-root-state.md

## What is wrong

Any component after the root file's first that declares a `resource` (other
than a device fact), `mutation` or `task` is refused with
`type-child-resource` (`contract/types/src/lib.rs:1385-1407`; the
message is `child_owns` in `contract/types/src/tasks.rs`). The rule is
deliberate: LLP 1017.000 gives its reasons ("a row must not open N requests,
and only the root has a clock"). Its relaxation is designed and waiting:
LLP 1035.005.002 C5 stage B (a row of the navigation root's `for … in
stack(nav)` owns its requests and tasks, for LLP 1035.005.000 D5) proceeds
when a consumer extraction is scheduled with an implementer and a date, and
an instance-owned `task` also needs a take on `rules/DEFERRED.md`'s
gated-timers entry, which keeps component-scoped timers out.

Two reports show the cost:

- **A feature does not compose.** To embed a self-contained feature (a
  canvas with its own feed, a graph resource and an autosave task) in an
  existing app, its state became a `members` file of the root and its view
  was pasted into the root view. Names that clashed with the root's were
  renamed by hand: a feed and a selection against the root's, a `let`
  against a root resource of the same name, a `state` against another
  file's `let`.
- **Everything loads at launch.** In a port of an existing mobile app,
  every resource keyed on the signed-in user is the root's, so all of them
  ask at launch, including those of tabs not showing. The port added a gate
  per tab. It asked for route-scoped resources, which is stage B's first
  owner, or a `when` on a resource.

## The failure

The feature embedding is on exact2 `9e53578f2` (all hosts; a compile-time
refusal). The port's diary is on `94b8671f7`.

## Fix

Count both as consumers for LLP 1035.005.002 C5 stage B. The port's case is
stage B's first owner exactly (a screen's requests owned by its navigation
entry); the feature case also needs an owner that is not a stack row, a
child used outside every arm (stage A, skipped in r3 because a lifted
resource could be asked on a child's uninitialized state). If neither is
scheduled, the refusal could say how to keep a feature's requests in the
root with its names apart (a `members` file per feature, prefixed), and a
recipe could show a tab's resource gated on that tab showing.
