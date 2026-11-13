# Component advance breaks equal-deadline timer order

**Status:** Open
**Systems:** semantics
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

`cadvance` keeps a due timer when its `next` is strictly earlier than the current best, so an equal deadline keeps the earlier index. Flat `advance` orders with `Timer.precedes`: an idle timer loses to a non-idle timer at the same time, and otherwise the earlier nominal wins. Two ungated timers with the same execution time, the idle one declared first, fire idle-first in the component semantics and non-idle-first in the flat semantics. Command order and a last-write-wins slot then diverge on one advance.

`wait=idle` sets no gate and no key, so `cboot` accepts it.

## Where

`semantics/Contract/CompSem.lean`, `cadvance`'s due fold. `semantics/Contract/Runtime.lean`, `Timer.precedes` and `advance`.

## How to reproduce

Declare an ungated `wait=idle` timer and an ordinary timer at the same deadline, idle first. Advance once in both semantics. The idle action runs first only in `cadvance`.

## Direction

Use `Timer.precedes` in `cadvance`'s due fold.
