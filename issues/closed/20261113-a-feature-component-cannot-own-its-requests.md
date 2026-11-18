# A feature cannot be a component: only the root may own a resource, mutation or task

**Status:** Closed
**Resolution:** fixed on main by LLP 1035.005.004 (026e422ce, 73799b4d9, 7f0750f22): a child owns its resources, mutations and tasks per instance; both reports run in runner/tests/it/feature.rs
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

## Fixed by LLP 1035.005.004

A component other than the root now owns its resources, mutations and tasks,
each instance its own, at every use whose repetition the data does not size:
outside every region, under `if` and `match`, and in a row of `stack(nav)`,
`t.stack` or `nav.tabs` (026e422ce, 73799b4d9, 7f0750f22; `rules/DEFERRED.md`
records Charlie's waiver for instance-owned tasks, 2026-11-16). Both reports
run in `runner/tests/it/feature.rs`:

- A canvas feature with a feed, a graph, a save that refreshes the graph and
  an autosave, used outside every region and in a stack entry, beside a root
  that declares its own `selection`, `feed`, `saved` and `save`, compiles
  unrenamed. Each instance asks for its own, its autosave sends only its own
  document, the save refreshes only its graph, and popping the entry cancels
  its armed autosave. Two instances of one use asking and refreshing apart is
  `runner/tests/it/owned.rs`'s `two_instances_ask_and_refresh_apart`.
- Tab screens in `for e in t.stack`, owning resources keyed on the signed-in
  user and gated on their tab showing (`when shown`, the prop the recipe
  passes): the unselected tab asks nothing, built or not, and asks when
  selected; a sign-in while it is hidden asks nothing for it. A resource gated
  on `top(nav).id == e.id` idles while a pushed screen covers its mounted
  screen and asks again on Back. `when` is the permission to ask (D3), so a
  hidden or covered screen that stays mounted asks only as its gate allows.

What `type-child-resource` still refuses is kept on purpose, and each message
names the repair: a use under a data-sized `for` or a virtualized list,
through wrappers and slot fills (D1: the root's resource over a list); a
child's `sequence` (D6); and a child's own resource over a list (LLP 1017.000
P4c as amended by D1). The last is the one case a self-contained feature
still cannot hold: a screen that needs one request per element takes the
root's list resource as a prop (`QUEUE.md`).
