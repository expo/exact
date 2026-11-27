# Portal mover whose parent loses holds while its destination unmounts is destroyed before re-attachment, poisoning Runner

**Status:** Closed
**Resolution:** Included previously moved surviving entries in Portals::apply's step 1 detachment pass so movers whose parent loses holds are detached and recorded in extracted before destination destruction.
**Systems:** runner (instance tree portal placement, kernel batch commit)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by reading.

At `9c0431fd5`, `Portals::apply` in `runner/src/instance/portal.rs:508-509, 579-660` omits surviving moved portal elements from step 1's detachment pass (`leaving` / `detach` / `self.extracted`) whenever their declaration parent's `holds` flag dynamically becomes `false` (`runner/src/instance/portal.rs:172-176`).

### Trigger
A valid, compiler-accepted Contract program declares a mover inside a parent whose `role` can transition to `"tablist"` dynamically (e.g. `column role=(as_tabs ? "tablist" : "")`), together with a conditional destination:
```contract
component App
  state as_tabs = false
  state show_dest = true
  action flip
    as_tabs = true
    show_dest = false
  view
    column
      if show_dest
        box id="dest" testId="dest"
      column role=(as_tabs ? "tablist" : "") testId="p"
        box portal="dest" testId="e"
```
(Note: the other scenario mentioned in the original report—`<header>` under a parent with dynamic `navigationKey`—is already rejected at compile time by `contract/lower/src/tags/portal.rs:99, 164-166` because `said` returns `None` for computed `navigationKey`, setting `within.route = true` and `within.bar = true`. Dynamic `role` on a parent is **not** checked by `contract/lower/src/tags/portal.rs:15-31, 81-109, 161-174` and compiles cleanly.)

1. At boot (`as_tabs = false`, `show_dest = true`), `entries[p].holds` is `true`, `entries[e].movable` is `true`, and `e` moves into `dest`. `self.moved` records `e -> Moved { at: Some(dest), parent: p, .. }` and `self.lists` records `p -> []`, `dest -> [e]`.
2. Calling `flip` sets `as_tabs = true` and `show_dest = false`, producing `ops`: `SetChildren { id: col, children: [p] }`, `DestroyView { id: dest }`, `SetProp { id: p, prop: AccessibilityRole, value: "tablist" }`.
3. In `Portals::publish`, `entries[p].holds` becomes `false` (`portal.rs:175`), so `entries[e].movable` becomes `false` (`portal.rs:137, 171`). Line 365 skips `e` from `edge`, leaving `moved` empty.
4. In `Portals::apply`, `placeable(e)` (`portal.rs:508-509`: `e.bears && e.step == 1 && e.parent.is_some_and(|p| entries[p].holds)`) is now `false`.
5. `p` is still in `self.lists` and `index`, so `p` enters `touched` (`portal.rs:521`), and `finals[p]` is computed as `[e]` (`portal.rs:531-548`).
6. Step 1 (`portal.rs:580-592`) iterates only `entries.iter().filter(|e| placeable(e))`, skipping `e`. `dying` (`portal.rs:594-597`) filters `self.moved` by `!index.contains_key(m)`, also skipping `e` because `e` is still declared under `p`. Thus `leaving`, `detach`, and `self.extracted` are all empty.
7. Step 3 (`portal.rs:639-653`) appends `Op::SetChildren { id: p, children: [e] }` **after** `ops`, which contains `Op::DestroyView { id: dest }`.

### Actual vs. Expected
- **Actual:** `kernel/src/txn.rs:290-309` executes `DestroyView { id: dest }` while `e` is still `dest`'s child, recursively marking `e` destroyed. When validation reaches the appended `SetChildren { id: p, children: [e] }`, `staged.require` (`kernel/src/txn.rs:444 -> 160`) fails with `ApplyError::DestroyedInBatch { op_index, id: e }`, and `Runner::publish_render` (`runner/src/runner/render.rs:89-103`) permanently poisons the `Runner`. In addition, `kept_by_portals()` (`runner/src/runner/apply.rs:38-46`, consumed by `live_time.rs:109` and `text_axes.rs:200`) misses `e`'s subtree because `self.extracted` omitted `e`.
- **Expected:** Per `runner/src/instance/portal.rs:574-578` ("1. Detach each surviving mover that moves, and each moved mover whose declaration died, from where it is shown. The surviving movers detached, in preorder: a destroy in this batch takes none of their subtrees (`extracted`)") and LLP 1013.002 D2, `e` must be detached from `dest` and recorded in `self.extracted` before `DestroyView { id: dest }` runs, then reattached cleanly under `p`.

### Verification & Siblings Checked
Verified by reading `runner/src/instance/portal.rs`, `kernel/src/txn.rs`, `contract/lower/src/tags/portal.rs`, and `runner/src/runner/render.rs` at `9c0431fd5`. Siblings checked:
- `host/web-js/portal.js:145-147, 202-205, 249, 265-266`: **not affected** — `rescue(n)` iterates all registered `Movers` before DOM removal regardless of `movable(m)`, and `placeAll` returns movers with `edge.get(m) === undefined` back to `m.anchor.parentNode`.
- Other dynamic losses of `e.movable` (e.g. `e` itself gaining dynamic `role="dialog"` or `popover`): `entries[p].holds` remains `true`, so `placeable(e)` remains `true` and step 1 detaches `e` normally.
- `contract/lower/src/tags/portal.rs:15-31, 81-109, 161-174`: rejects static/dynamic `navigationKey` on a `<header>`'s parent (`within.bar`), `list` (`within.cells`), and `table`/`ul`/`ol`/`dl` (`structural_parent`), but does not reject a parent with `role="tablist"` (static or dynamic).

### Suggested Fix & Regression Test
In `runner/src/instance/portal.rs:580`, iterate surviving entries that either satisfy `placeable(e)` **or** are currently present in `self.moved` (`entries.iter().filter(|e| placeable(e) || self.moved.contains_key(&e.view))`). Because `shown_before(e.view)` reads `self.moved.get(&e.view)` and returns `Some(Some(dest))` while `now` is `e.parent.map(view)` (`Some(p)`), `Some(dest) != Some(p)` will add `e` to `leaving` (emitting `SetChildren { id: dest, children: ... }` before `DestroyView { id: dest }`) and push `e` into `surviving` (`self.extracted`).

Add a regression test in `contract/cli/tests/it/portal.rs` with the dynamic `role=(as_tabs ? "tablist" : "")` parent and conditional `box id="dest"` above, asserting that `flip` succeeds without poisoning, destroys `dest`, keeps `e` alive, and reattaches `e` as a child of `p`.

### Siblings checked

Checked host/web-js/portal.js (not affected: rescue iterates all Movers regardless of movable(m) and placeAll returns unedged movers to anchor.parentNode), other dynamic movable=false transitions on the mover itself (placeable stays true when parent holds stays true), and contract/lower/src/tags/portal.rs (already rejects dynamic navigationKey on header's parent, but does not check parent role="tablist").
