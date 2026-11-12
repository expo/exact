# Linux grouped dragging uses different edge scroll speed and timing

**Status:** Closed
**Resolution:** The Linux grouped drag scrolls in a 32-point edge band at a flat 720 points a second with at most 32 ms of catch-up per step, as the web and Apple hosts do.
**Systems:** host/linux/src/presenter/group.rs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`host/linux/src/presenter/group.rs:17–18` sets a 24-point edge band and a maximum 300 points/second. Lines 529–532 ramp the rate with distance into the band; line 555 caps elapsed time at 50 ms. The web host uses a 32-point band, a flat 720 points/second and a 32 ms cap (`host/web/group-glue.js:11,118–123`), as does Apple (`host/apple/Sources/ExactKit/ReorderParts.swift:34–41`).

## The failure

Drag a card in a grouped board to x=292 inside a 300-point horizontal scrollport and hold it there. A 20 ms tick moves Linux's board by 4 points, rather than 14.4. A subsequent 100 ms gap moves it by 10 points, rather than the capped 23.04. The same gesture should follow the shared edge-scroll behavior.

## Verification

Ran `CARGO_BUILD_JOBS=5 cargo test -p exact-linux --test fix08_paint fix08_ -- --nocapture`. A Contract fixture with five virtualized lists used the presenter's pointer and clock operations. It printed `8pt inside board edge: 20ms delta=4, next 100ms delta=10`; the 14.4-point assertion failed. This ran the portable presenter on macOS, without a Linux window. The temporary test was removed.

## Direction

Use the same edge band, constant rate and elapsed-time cap as the other grouped-drag presenters.

## Related

Reported in the f-grouped-drag-edge-scroll review. No existing issue or QUEUE entry records this difference in rate and timing.
