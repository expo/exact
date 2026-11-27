# Child device-fact hoisting ignores `ResourceDecl.gate` and fails to rewrite `MutationDecl.refreshes`, causing silent cross-component misbinding

**Status:** Closed
**Resolution:** Require an absent gate in reads_device so gated child device facts lift per instance, exempt DEVICE_FACTS in foreign_source, and map hoisted device facts in names so MutationDecl.refreshes rewrites them.
**Systems:** contract-syntax (component inliner), contract-types, contract-lower
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Dropped plumbing)

Verification: verified by reading.

At `9c0431fd5`, child device-fact hoisting (`exactViewport()` / `exactPage()`) has two scoping/rewriting drops during component inlining:
1. `reads_device(r)` (`contract/syntax/src/inline.rs:56-61`) checks `DEVICE_FACTS.contains(&r.source.as_str()) && r.args.is_empty() && r.placeholder.is_none() && r.each.is_none()`, omitting `r.gate.is_none()`. `expand_with_sites` (`contract/syntax/src/inline.rs:438-445`) then hoists `ResourceDecl { name: hoisted(&c.name, &r.name), ..r.clone() }` verbatim into `root.resources` (with `Owner::Root`) without substituting `r.gate` in the child's scope, and collapses all instances onto a single root resource.
2. In `prepare_use` (`contract/syntax/src/inline/use_node.rs:256-287`), device-fact resources are excluded from `owned` and from `names` (added only to `child_subst` at lines 282-287 and `action_subst` at lines 480-485). At `contract/syntax/src/inline/use_node.rs:416-425`, `MutationDecl.refreshes` rewrites resource names only through `names.get(name)`—leaving references to a child's hoisted device fact as raw `r.name` instead of `hoisted(&c.name, &r.name)`.

### Trigger
Both triggers parse cleanly (`contract/syntax/src/parser/decls.rs:94-213`) and pass `check_children` (`contract/types/src/lib.rs:1425-1450`, `contract/types/src/component.rs:149-155, 325`, `contract/types/src/each.rs:84-94`, `contract/types/src/mutations.rs:22-38`):
- **Case (a) — gated child device fact:** `Child` declares `props active: bool` (or a child `state`/`derive`) and `resource vp = exactViewport() when active as shape VP`, and `App` renders `Child(active=true)`.
- **Case (b) — child mutation refreshing a device fact:** `Child` declares `resource vp = exactViewport() as shape VP` and `mutation save as shape Res refreshes vp`, and `App` renders `Child()`.

### Actual vs. Expected
- **Actual:**
  - In **case (a)**, `vp@Child` is hoisted to `root.resources` with raw `gate = Some(Ident("active"))`. In `check_root` (`contract/types/src/lib.rs:1468`), if `App` has no `active`, compilation spuriously fails with unknown identifier `active`; if `App` also declares an unrelated `bool` state/derive named `active`, compilation succeeds with zero errors and `contract/lower/src/resource_args.rs:164-167` silently binds `vp@Child`'s gate to `App`'s `active` instead of `Child`'s.
  - In **case (b)**, `Child`'s lifted mutation keeps `refreshes = [("vp", span)]`. In `check_root` (`contract/types/src/lib.rs:1468`), if `App` has no `resource vp`, `mutations::check` (`contract/types/src/mutations.rs:22-30`) spuriously fails with `` `vp` is not a resource ``; if `App` also declares an unrelated `resource vp`, compilation succeeds with zero errors and `contract/lower/src/lib.rs:692-701` silently wires `Child`'s mutation to refresh `App`'s unrelated `resource vp` instead of `vp@Child`.
- **Expected:** Per `contract/syntax/src/inline.rs:51-53` (hoist once per component only when "the same for every instance"), `contract/syntax/src/inline/use_node.rs:406` (substitutes `r.gate` on owned child resources), and `contract/syntax/src/inline/subst.rs:370-379` (rewrites `Stmt::Refresh` on a child's device fact to `name@Component`), gated device-fact resources must not be hoisted verbatim to root (they must be lifted per-instance with `r.gate` substituted in the child's scope), and `MutationDecl.refreshes` must rewrite hoisted device-fact names to `hoisted(&c.name, &r.name)`.

### Verification & Siblings Checked
Verified by reading `contract/syntax/src/inline.rs`, `contract/syntax/src/inline/use_node.rs`, `contract/syntax/src/inline/subst.rs`, `contract/types/src/lib.rs`, `contract/types/src/component.rs`, `contract/types/src/each.rs`, `contract/types/src/mutations.rs`, `contract/types/src/tasks.rs`, `contract/lower/src/lib.rs`, and `contract/lower/src/resource_args.rs` at `9c0431fd5`. Siblings checked:
- `contract/syntax/src/inline/subst.rs:370-379` (`Stmt::Refresh`): **not affected** — resolves `target` through `s.get(target)` (`action_subst`, populated at `use_node.rs:480-485`), properly rewriting `refresh vp` to `refresh vp@Child`.
- `contract/syntax/src/inline/use_node.rs:401-412` (owned non-device resources): **not affected** — substitutes `r.gate` at line 406 and inserts into `names` at line 264.
- `contract/syntax/src/inline/use_node.rs:422` (`m.then`) and `442` (`t.timer.1`): actions are always in `names` (lines 248-250), so not affected.
- `contract/types/src/tasks.rs:29, 145, 219` (`reads_own_request`, `repeated_owner`, `foreign_source`): all call `reads_device(r)`. Note for the fix: `foreign_source` (`tasks.rs:219`) should continue admitting `DEVICE_FACTS` (runner-owned sources) even when gated so a package component with a gated `exactViewport()`/`exactPage()` is not spuriously rejected as a foreign app source.

### Suggested Fix & Regression Test
1. In `contract/syntax/src/inline.rs:56-61`, require `r.gate.is_none()` in `reads_device(r)` so gated device-fact resources go through `owned` (`use_node.rs:256-265, 401-412`) with `r.gate` substituted in the child's scope; in `contract/types/src/tasks.rs:219`, exempt `DEVICE_FACTS` (or `exact_plan::runner_owned_source`) in `foreign_source` so package components can still read gated device facts.
2. In `contract/syntax/src/inline/use_node.rs:282-287` (or `416-425`), also record hoisted device-fact resource names in `names` (e.g. `names.insert(r.name.clone(), super::hoisted(&c.name, &r.name))`) or resolve `m.refreshes` against hoisted device facts as `subst.rs:373` does for `Stmt::Refresh`.

Add regression tests in `contract/syntax/src/inline/tests.rs` and `contract/cli/tests/it/viewport.rs` covering (a) a child with `resource vp = exactViewport() when active as shape VP` (both with and without a root `active`), verifying the gate reads the child's `active`, and (b) a child with `resource vp = exactViewport() as shape VP` and `mutation save as shape Res refreshes vp` (both with and without a root `resource vp`), verifying `save` refreshes `vp@Child` rather than failing or refreshing the root's `vp`.

### Siblings checked

Checked subst.rs:370-379 (Stmt::Refresh already rewrites hoisted device facts via action_subst), use_node.rs:401-412 (owned non-device resources already substitute r.gate and populate names), use_node.rs:422/442 (m.then and t.timer.1 actions are always in names), and types/src/tasks.rs:29/145/219 (all call reads_device; noted foreign_source at line 219 should keep admitting DEVICE_FACTS when reads_device adds r.gate.is_none()).
