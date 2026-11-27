# Wide-color GC omits `NodeArena::looks` and `Op::SetLooks`, reclaiming live `:hover`/`:focus-visible` colors

**Status:** Closed
**Resolution:** Mark NodeArena::looks in wide_colors/wide_colors_shared, visit Op::SetLooks in Colors for Op, and retain a wide Keep on StyleTable.
**Systems:** kernel wide-color GC & state looks (`Op::SetLooks`)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Two sources of truth)

Verification: verified by reading.

At `9c0431fd5`, wide-color mark-and-sweep reclamation misses state look patches in three places:

- `kernel/src/arena/styles.rs:103-110` (`NodeArena::wide_colors`) marks `self.shared`, `self.control_styles`, and `self.document_style`, but never visits `self.looks` (`kernel/src/arena.rs:168`, `IdMap<u32, Rc<[(u8, StyleProps)]>>`).
- `kernel/src/arena/styles.rs:113-120` (`NodeArena::wide_colors_shared`, called by `NodeArena::clone` at `kernel/src/arena.rs:257` to build its `Keep`) likewise omits `self.looks`.
- `kernel/src/style/wide/marks.rs:155-161` (`impl Colors for crate::wire::Op`) visits only `Op::SetStyle { patch, .. }` and omits `Op::SetLooks { looks, .. }` (`kernel/src/wire/ops.rs:49-52`).

### Reachable trigger
1. Author a wide/modern CSS color (e.g., `oklch(...)` or `color(display-p3 ...)`) in a `:hover` or `:focus-visible` block on any node. `contract/lower/src/nodes.rs:1085-1107` lowers look attributes identically to regular style rows; `runner/src/instance/looks.rs:46-64, 109-115` resolves them through `bridge::set_plan_style` into `Op::SetLooks` (allowed by `look_rows()` at `kernel/src/wire/ops.rs:61-77`, which admits `BackgroundColor`, `TextColor`, `BorderColorTop/Right/Bottom/Left`, and `BoxShadow`).
2. `Kernel::apply` (`kernel/src/txn.rs:833-838`) stores the patches in `NodeArena::looks` and `publish_document_commit` (`kernel/src/kernel/document.rs:209-211`) calls `self.wide_changed()` -> `self.wide_mark()` (`kernel/src/kernel/wide.rs:10-20`) -> `self.arena.wide_colors(&mut m)`. Because `Holder::publish` (`kernel/src/style/wide/reclaim.rs:401-409`) closes the thread's open batch, the look's wide color IDs are left unmarked and protected only by `GRACE` (1,024 interns).
3. Churn enough distinct wide colors elsewhere in the app (≥3,072 live slots and >1,024 interns past the look's minting) to trigger `Table::sweep()` (`kernel/src/style/wide/reclaim.rs:212-214, 245-272`).
4. Hover or focus the element (or serialize its looks via `host/painted/src/paint/presented.rs:428`, `host/apple/src/looks.rs:85`, or `host/web/src/host/looks.rs:226`), or clone the arena / retain `Op::SetLooks` across a sweep.

### Actual vs. expected
- **Actual:** `Table::sweep()` reclaims the unmarked slot and increments its 4-bit generation (`reclaim.rs:266-270`). When the host later resolves the look's `ColorValue::Wide(id)` via `ColorValue::fallback()` (`kernel/src/style/roles.rs:807`) -> `wide::wide(id)` (`kernel/src/style/wide.rs:63-69`), `stale(id)` (`kernel/src/style/wide/reclaim.rs:504-522`) fires `debug_assert!(false, ...)` (panicking in debug builds) and returns `None`, rendering `Color::TRANSPARENT` in release builds (or silently aliasing an unrelated wide color once the 4-bit generation wraps after 16 slot reuses).
- **Expected:** Per LLP 1121 §5 stage 4 (`kernel/src/style/wide/marks.rs:1-4`, `kernel/src/style/wide/reclaim.rs:3-11, 44-54`), every live wide color ID held by a kernel, cloned arena, or retained `Op` must be marked in `WideMark` so `Table::sweep()` never reclaims live slots.

### Verification & siblings checked
Verified by reading the exact commit path from `contract/lower/src/nodes.rs:1085` -> `runner/src/instance/looks.rs:46-116` -> `kernel/src/txn.rs:833-838` -> `kernel/src/kernel/document.rs:209-211` -> `kernel/src/kernel/wide.rs:10-20` -> `kernel/src/arena/styles.rs:103-120` -> `kernel/src/style/wide/reclaim.rs:245-272`.
Siblings checked:
- `SharedStyles` (`kernel/src/shared_style.rs:465-496`), `RegionState::wide_colors` (`kernel/src/region/state.rs:1053-1055`), `LayoutTree::wide_colors` (`kernel/src/layout/buttons.rs:188-206`), `Export::Snapshot` (`kernel/src/export.rs:499-503`), `DocTree` (`runner/src/instance/document.rs:254-258`, where `DocNode` only holds `has_looks: bool`), `Roles` (`kernel/src/style/roles/session.rs:33-37`), `BarText` (`kernel/src/arena/bar_text.rs:60`, only `ColorValue::Role`), and `MountedSection` (`runner/src/runner/mounted_reorder.rs:241-265`, only non-color rows) are properly handled.
- `runner/src/runner/reorder.rs:148-150` uses `Colors::colors(&queued[..], m)` on `Vec<Op>`, relying directly on `impl Colors for Op`.
- **Related sibling worth checking during fix:** `StyleTable` (`runner/src/decorations.rs:26`, `BTreeMap<String, StyleProps>`) carries no `Keep` and is cached across commits in `FieldDecorations` (`host/painted/src/text/decorations.rs:14-18, 45-49`); `ResolvedStyles._wide` (`runner/src/decorations.rs:157-171`) only keeps colors when `resolved.tokens != 0`, so literal wide colors in textarea decoration styles on the painted host can also go unmarked across sweeps.

### Suggested fix & regression test
- In `NodeArena::wide_colors` and `NodeArena::wide_colors_shared` (`kernel/src/arena/styles.rs:103-120`), iterate `self.looks.values()` and mark each `(_, patch)` via `patch.colors(m)`.
- In `impl Colors for crate::wire::Op` (`kernel/src/style/wide/marks.rs:155-161`), match `Op::SetLooks { looks, .. }` and call `patch.colors(m)` for each `(_, patch)` in `looks`.
- Add a test in `kernel/tests/wide.rs` (or a dedicated single-process integration test alongside `wide_grace.rs`) that applies `Op::SetLooks` with a `color(display-p3 ...)` background/shadow, clones the arena and also retains a `Keep::of(|m| op.colors(m))` on an `Op::SetLooks`, churns >4,096 distinct wide colors on another kernel to force sweeps, and asserts that `wide::wide(id)` succeeds for the kernel's look, the cloned arena's look, and the kept `Op::SetLooks`.

### Siblings checked

SharedStyles (kernel/src/shared_style.rs:465-496), RegionState::wide_colors (kernel/src/region/state.rs:1053-1055), LayoutTree::wide_colors (kernel/src/layout/buttons.rs:188-206), Export::Snapshot (kernel/src/export.rs:499-503), DocTree (runner/src/instance/document.rs:254-258), Roles (kernel/src/style/roles/session.rs:33-37), BarText (kernel/src/arena/bar_text.rs:60), MountedSection (runner/src/runner/mounted_reorder.rs:241-265), reorder_kept (runner/src/runner/reorder.rs:148-150). Also noted a related sibling: StyleTable (runner/src/decorations.rs:26) cached in FieldDecorations (host/painted/src/text/decorations.rs:14-18, 45-49) holds StyleProps without a Keep for non-token literal wide colors.
