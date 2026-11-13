# Enum-typed list layout props are invisible to cell admission

**Status:** Open
**Systems:** contract
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`cells::rows` reads a layout prop only when its value is `Expr::Str`. A component argument ascribed to an enum is `Expr::Typed` around that string. `literal` then returns none. A `list` whose `flex-direction` is the enum `column` is not marked `listRows`, so `delete=` is dropped. The same value passed as a string prop is a cell list and grows the delete button. A horizontal list whose `overflow-x` is the enum `scroll` is not recognized as sideways, so it is marked as a vertical cell list and grows a delete button the string form refuses.

`semantics/corpus/views/delete-row-components.contract` expects `enum-cell-delete` missing and `enum-scroll-delete` present.

## Where

`contract/lower/src/cells.rs`, `literal` and `rows`. The list is classified in `contract/lower/src/nodes.rs` after attributes are expanded. `contract/lower/src/swipe_rows.rs` already unwraps `Expr::Typed` when it reads a row.

## How to reproduce

Instantiate a list component with `direction: enum<"column", "row">` as `flex-direction` and pass `"column"`, and one with `overflow-x: enum<"scroll", "hidden">` passed `"scroll"` beside `overflow-y="hidden"`. The column list has no delete button. The sideways list has one.

## Direction

Unwrap `Expr::Typed` in `cells::rows`'s `literal`, as `swipe_rows` does, so an enum string is that literal. Expect `enum-cell-delete` present and `enum-scroll-delete` absent, and update the corpus test that pins the current tree.
