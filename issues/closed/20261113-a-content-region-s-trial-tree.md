# A content region's trial tree skips order and tables

**Status:** Closed
**Resolution:** The trial tree marks its tables to be made, so a region's table cells share the table's grid, and its frames place a row's cells from the table's origin as publication does; CSS order was already fixed (2d76891a2), and the alert title, which the region's text protocol cannot measure yet, is issues/20261113-a-content-region-lays-an-alert-out.md.
**Systems:** kernel
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`Derived::build` is the tree a content region lays out and publishes. It copies children in document order, makes no alert prefix, and never calls `tables.find`. The ordinary rebuild and `of_subtrees` sort flex and grid children with `order::laid_out`, add alert prefixes, and form tables. `refresh_tables` then sees an empty stale set, so a table's grid is never made. While a region is registered, ordinary layout is refused, and these frames are the ones published.

A flex or grid item with a non-zero `order` stays in document order. A table's rows stay nested blocks, so cells do not share one grid and `colspan` is ignored. The shell outside the region still sorts and forms tables.

## Where

`kernel/src/region/tree.rs`, `Derived::build`. `kernel/src/layout/order.rs`, `laid_out`. `kernel/src/layout/rebuild.rs`, `rebuild` and `of_subtrees`. `kernel/src/layout.rs`, `refresh_tables` inside `compute_mapped`. `kernel/src/kernel.rs`, `layout_root_slot`. `kernel/src/region/state.rs`, the builds that publish.

## How to reproduce

Register a content region whose content is a flex row with `order`, or an HTML table with `colspan`. The published frames follow document order, and the table's cells do not share the table's grid. The same tree with no region lays out as the browser does.

## Direction

Build the trial the way `rebuild` and `of_subtrees` do: `laid_out`, alert prefixes, and `tables.find` before compute.
