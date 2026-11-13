# Point reads hit children beyond a scroll element's viewport

**Status:** Closed
**Resolution:** The point walk reads a `scroll` or `list` with no `overflow-y` row as scrolling on y, as the engine lays it out, so it clips the walk to the scrollport.
**Systems:** runner/src/geometry.rs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

The point-read walk enables a descendant clip only from `overflow_x` and `overflow_y` (`runner/src/geometry.rs:1314`). A `scroll` element without explicit overflow rows still has a scrollport, but these style values alone leave its child walk unclipped.

## The failure

A page contains `scroll id="scroller" width=100 height=100` and its child `box id="child" width=100 height=300`. An action reads `elementFromPoint(50,150)`. Observed: `child`. Expected: the containing page, since the point is outside the scroller's viewport. No scrolling is needed.

## Verification

A temporary runner integration test compiled that Contract, computed a 400×400 layout and ran the read action:

```
cargo test -p exact-runner --test fix08_scroll fix08_ -- --nocapture
elementFromPoint(50,150) = Some(Str("child"))
left: Some(Str("child"))
right: Some(Str("page"))
```

One test failed as expected; the test was removed.

## Direction

Have the point walk use the node's effective scroll clipping, including the `scroll` element's implicit scrollport, as the painter does.

## Related

`issues/20261110-overflow-clips-at-border-box.md` concerns which edge an existing clip uses. This case fails to establish a scroll clip at all.
