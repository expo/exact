# HTML table `form_grid` allocates O(rows × total_colspan) memory without enforcing the `LINES` cap

**Status:** Closed
**Resolution:** Track active downward-growing table spans sparsely in form_grid and cap formed.columns and grid_template_columns at LINES with bounds-checked width indexing.
**Systems:** kernel HTML table grid formation & Taffy layout style generation
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Quadratic work, stack depth and allocation)

Verification: verified by reading.

At `9c0431fd5`, HTML table grid formation allocates memory proportional to `rows × Σcolspan` without bounding occupied-slot tracking or `formed.columns` to `LINES` (`i16::MAX = 32,767`, `kernel/src/layout/table.rs:85`):

- `form_grid` (`kernel/src/layout/table.rs:584-625`) tracks occupied cells in a dense `taken: Vec<Vec<bool>> = vec![Vec::new(); rows.len()]`. For each cell, `spans()` (`table.rs:406-407`) permits `colspan` up to `1000` and `rowspan == 0` (meaning all remaining rows in the row group, `covers = left`). Line 607 computes `end = (x + colspan) as usize`, lines 608-613 resize every covered row's `Vec<bool>` to `end`, and line 622 sets `columns = columns.max(x)` with no upper bound.
- `anonymous()` (`kernel/src/layout/table.rs:112`) and `Tables::style()` (`kernel/src/layout/table.rs:264`) allocate `s.rare.grid_template_columns = vec![track; columns as usize]` directly from the uncapped `columns` count, even though `place()` (`table.rs:412`) saturates every column line past `32,767` to `i16::MAX` and Taffy clamps explicit tracks to `MAX_GRID_TRACKS = 10_000` after iterating the whole template vector (`vendor/taffy/src/compute/grid/explicit_grid.rs:60-72`, `vendor/taffy/EXACT-PATCHES.md:2258`).

### Reachable trigger
Render a `<table>` (via Contract—`contract/lower/src/tables.rs:141-142` admits `colspan` 1..=1000 and `rowspan` 0..=65534—or via EXWF wire ops) whose first `<tr>` contains 1,000 `<td>` cells with `colspan=1000 rowspan=0`, followed by 1,000 empty `<tr>` rows (~2,002 nodes total), and call `Kernel::compute_layout` -> `LayoutTree::refresh_tables` -> `make_table` -> `form` -> `form_grid`.

### Actual vs. expected
- **Actual:** `x` advances to `1,000,000`. `taken` resizes all 1,001 row vectors to 1,000,000 `bool`s each (~1 GB of live heap, peaking near ~1.5–2 GB during `Vec` reallocation), and `s.rare.grid_template_columns` allocates 1,000,000 `GridTemplateComponent` entries (tens of MB per restyle, iterated four times in Taffy's `compute_explicit_grid_size_in_axis`). Even staying within `LINES`, a 30,000-row table (the scale benchmarked in `EXACT-PATCHES.md:2265`) with 33 `colspan=1000 rowspan=0` cells in row 0 still allocates ~983 MB in `taken`.
- **Expected:** `form_grid` and style lowering (`kernel/src/layout/table.rs:80-85, 411-413`) bound memory and `grid_template_columns` to representable grid tracks and avoid O(rows × total_colspan) dense allocations (e.g., tracking active downward-growing rowspans as a sparse list of column intervals as in HTML §4.9.12.1 and capping `formed.columns` / `grid_template_columns` at `LINES`).

### Verification & siblings checked
Verified by reading `kernel/src/layout/table.rs:80-115, 258-273, 389-433, 584-649`, `contract/lower/src/tables.rs:135-145`, `vendor/taffy/src/compute/grid/explicit_grid.rs:60-75`, and `vendor/taffy/EXACT-PATCHES.md:2243-2258`.
Siblings checked:
- `anonymous()` (`kernel/src/layout/table.rs:90-115`, used for blockified/misparented table runs) and `Tables::style()` (`kernel/src/layout/table.rs:264`): both share the same uncapped `vec![track; columns as usize]` allocation.
- `formed.widths` (`kernel/src/layout/table.rs:265-268, 628-640, 865-890`): indexes directly into `s.rare.grid_template_columns[column as usize]`, so capping `grid_template_columns` must also guard `column < s.rare.grid_template_columns.len()`.
- `Tables::order()` (`kernel/src/layout/table.rs:185`): `vec![false; children.len()]` is linear in actual child nodes (safe).
- Authored CSS grids (`kernel/src/style/grid.rs` and `vendor/taffy/src/compute/grid/explicit_grid.rs:60-72`): bounded by parser limits and `MAX_GRID_TRACKS`.

### Suggested fix & regression test
- Replace `taken: Vec<Vec<bool>>` in `form_grid` (`kernel/src/layout/table.rs:584-625`) with a sparse list of active spanning intervals `(start_col, end_col, end_row)` (sorted/merged per row as `r` advances, dropping expired spans where `end_row <= r`), making memory O(active spanning cells) instead of O(rows × columns).
- Cap `formed.columns` (and `Anonymous::columns` / `grid_template_columns` length in `anonymous()` and `Tables::style()`) at `u32::from(LINES)` (or Taffy's explicit track limit), and bounds-check `column` when writing `formed.widths` at `table.rs:265-268` and `resize_table_column` at `table.rs:865-890`.
- Add a regression test in `kernel/tests/it/table.rs` laying out a table with 100+ `colspan=1000 rowspan=0` cells in row 0 and 1,000+ trailing `<tr>` rows, verifying layout completes promptly with bounded `grid_template_columns` length (≤ `LINES`) and without huge allocation.

### Siblings checked

Checked anonymous() (kernel/src/layout/table.rs:112, same uncapped vec![track; columns as usize]), Tables::style() (table.rs:264-268, same uncapped allocation plus direct indexing from formed.widths), formed.widths / resize_table_column (table.rs:628-640, 865-890), Tables::order() (table.rs:185, linear in children.len()), and authored CSS grids (vendor/taffy/src/compute/grid/explicit_grid.rs:60-72, capped at MAX_GRID_TRACKS).
