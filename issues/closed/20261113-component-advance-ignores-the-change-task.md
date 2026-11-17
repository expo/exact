# Component advance ignores the change-task apply boundary

**Status:** Closed
**Resolution:** `cadvance` mirrors `advance`'s two change-task returns and its overdue-`now` rule, so a component step stops after a change commit as the flat semantics and the runner do.
**Systems:** semantics
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`cadvance` fires a due timer and recurses, including a change timer. Flat `advance` returns after one change and, when that change is overdue, keeps `now`. The runner sets `needs_apply` and returns the same way. One component-semantics step past an ungated `change(action)` also runs every later timer due in that window and leaves `now` at the requested time. A later timer's writes and a following `performanceNow` disagree with the runner.

`cboot` accepts the ungated form. The README's refusal covers queued sends and gated or keyed tasks, not change tasks.

## Where

`semantics/Contract/CompSem.lean`, `cadvance`, the due arm that sets `now` to `tm.next` and recurses on success. `semantics/Contract/Runtime.lean`, `advance`, the `tm.change` returns. `runner/src/runner/advance.rs`, the `needs_apply` return when a change timer is still due.

## How to reproduce

Advance a component program whose ungated `change(action)` and a later timer are both due inside one clock step. The component semantics runs both and publishes the requested time. The flat semantics and the runner stop after the change.

## Direction

Mirror the two change returns, and the overdue-`now` formula, in `cadvance`. Until that matches, refuse `TaskKind.change` from `cboot`.
