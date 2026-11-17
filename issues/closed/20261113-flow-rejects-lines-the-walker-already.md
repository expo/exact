# Flow rejects lines the walker already accepted within fit epsilon

**Status:** Closed
**Resolution:** flow compares a fragment with its slot under the walker's own fit tolerance (one shared FIT_EPSILON), so a line the walker accepted for that offer is placed there, not retried below the float.
**Systems:** textflow
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`Prepared::fit` adds 0.005px before it decides a line fits. `next_line` can therefore return a line up to 0.005px wider than the slot it was offered. On an obstructed band, `flow` discards that fragment when `line.width > slot.width` and does not advance the cursor. Later bands with the same slot retry the same line.

A float that leaves a slot 0.001px to 0.005px under the measured line pushes the line below the float. If the exclusion is taller than the probe guard, `complete` is false. Callers that refuse a partial flow paint the paragraph as if the float were not there.

## Where

`textflow/src/walker.rs`, `FIT_EPSILON` and `fit`. `textflow/src/flow.rs`, the slot loop that `continue`s without assigning `cursor`. `textflow/tests/it/walker.rs`, `sub_pixel_fit_tolerance_matches_engine_rounding`.

## How to reproduce

Flow a line whose width is 12.5px beside a float that leaves a slot of 12.499px. The walker accepts the line for that offer. `flow` drops it and retries the same break on the next band.

## Direction

Accept a fragment when `line.width` is within the walker's 0.005px epsilon of the slot. Keep rejecting a line that does not fit even with that epsilon. Share one constant.
