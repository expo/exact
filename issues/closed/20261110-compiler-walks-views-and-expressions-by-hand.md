# The compiler has no shared walk over a view or an expression: 93 functions spell out `Node`'s six kinds and 32 spell out `Expr`'s eighteen

**Status:** Closed
**Resolution:** contract-syntax's `walk.rs` gives every pass `Node::child_lists`, `Node::walk`, `Expr::operands` and their scoped and mutable forms, the descending walks in syntax, types, analyze, lower and cli use them, and the walks that left out `match` now read it.
**Systems:** contract/syntax/src/ast.rs, contract/lower, contract/cli, contract/types, contract/analyze
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

A view is a tree of `Node` (`contract/syntax/src/ast.rs:804`: `Element`, `Use`, `Children`, `When`,
`Each`, `Match`) and an expression a tree of `Expr` (`ast.rs:1087`, eighteen variants).
`contract-syntax` offers no traversal of either (its `VisitSpans`, `spans.rs:12`, hands out spans
only), so every pass that needs "each element under this one" or "each name this expression reads"
writes the recursion again. At `dd6f5dcd`, in the 303
non-test files under `contract/`:

| | Recursive functions | Files | By crate |
| --- | ---: | ---: | --- |
| name all six `Node` kinds | 93 | 63 | lower 54, cli 16, syntax 10, types 7, analyze 6 |
| name two to five `Node` kinds | 24 | | |
| name all eighteen `Expr` variants | 32 | 25 | lower 12, syntax 7, types 7, analyze 4, cli 2 |

The counts are a floor: a function that recurses through `.map(strip)` is not counted. Two general
forms exist and stay local: `miswiring::arms` (`contract/lower/src/warnings/miswiring.rs:21`, used
by `parts.rs`) and `snapshot::visit_nodes` (`contract/cli/src/snapshot.rs:77`, used in
`snapshot/`). Seventeen of the 93 are in the warning modules (`contract/lower/src/warnings*`).

## What it has cost

Each copy decides again which kinds it reads through, where it starts and how it treats a binder,
and the copies differ:

- **A kind left out.** `grouped::unsheet` reads `if` and not `match`
  (`issues/20261110-class-native-grouped-button-match-leaks-ua-row.md`). A paragraph's `run` check
  does the same (`contract/lower/src/nodes.rs:837`; drafted beside this one as
  `match-in-a-text-paragraph-is-refused`, proved by running). Four more read `if` and not `match`
  and were not tested: `grouped/cell.rs:69`, `grouped/lines.rs:98`, `grouped/messages.rs:114` and
  `warnings.rs:411`, all under `contract/lower/src`.
- **Where a walk starts.** `scroll_bound.rs:92` gives the view's roots the window as parent and
  `width.rs:200` gives them nothing (`issues/20261110-width-padding-warning-skips-the-view-root.md`).
- **Three walkers for one question.** The terminal, watch and snapshot profiles each walk the view
  against one schema column (`contract/cli/src/terminal.rs:150`, `watch.rs:77`, `snapshot.rs:77`).
  Watch and snapshot check named styles and shorthands; the terminal does not (`QUEUE.md`, "The
  terminal surface's next work": "the profile's admission holes").
- **Binders.** "Does this expression reach X through derives and `fn`s" is `gates.rs:79`,
  `properties.rs:74` and `analyze/src/lib.rs:841`, with three scoping rules (review draft
  `task-gate-clock-check-resolves-names-without-scope`, not yet filed when this was written).

## Verification

Counted with a script that blanks comments and strings, splits functions by braces, and counts
those that call themselves and name the variants; read `cells.rs:45`, `posts.rs:27` and
`snapshot.rs:77` to confirm what it counts.

## Direction

Put two functions in `contract-syntax` beside `Node` and `Expr`: a node's child lists (what
`miswiring::arms` returns) and an expression's operands, each with the names it binds. Rewrite the
walks that only descend (the warnings, the three profiles, the `cli` collectors) over them, so a new
kind is one edit. Walks that rebuild a tree (`inline`, `subst`) keep their own match.

## Related

No issue or QUEUE entry names the missing traversal. The filed issues above each fix one walk.
