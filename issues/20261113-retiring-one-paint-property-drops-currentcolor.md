# Retiring one paint property drops currentcolor sides

**Status:** Open
**Systems:** kernel
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

A paint sync records the node's remaining `currentcolor` sides, then the retired pass deletes that whole per-node record. `pending` is filtered down to the one property that left. `current` is not.

The same commit still decides `follows` from the record it took at the start. The next `color` change sees an empty record, so `follows` is false. A border that is still `currentcolor` seeds from `pending` and runs its own curve. `shown` paints that sample, which leaves the presented color when the durations differ.

## Where

`kernel/src/motion/paint.rs`, the insert of `current` and the retired loop that removes the node. `kernel/src/motion.rs`, `adopt_paint`, which can push a change and a retirement in one sync.

## How to reproduce

Transition `color` and `border-color`, with `border-color` as `currentcolor`, and drop another paint target such as `background-color` in that commit. Change `color` again. The border starts its own curve instead of following the presented color.

## Direction

Delete only the retired property from the `current` record, or write the remaining `currentcolor` sides back after the retired pass.
