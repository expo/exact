# Linux paints nowrap text outside its overflow clip

**Status:** Closed
**Resolution:** A text box whose overflow is not visible now paints its own lines under its overflow clip, as it clips its children, so its glyphs stop at its padding edge.
**Systems:** host/linux/src/paint/fragments.rs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`host/linux/src/paint/fragments.rs:147–149` paints an unfragmented paragraph without applying its own overflow clip. The clip used for descendants does not contain this earlier text paint.

## The failure

Render this text on a white background:

```contract
text "MMMMMMMMMMMMMMMM" width=50 height=30 white-space="nowrap" overflow="hidden" font-size=20 color="#000000"
```

The glyphs continue past x=50. The text's ink should stop at its overflow edge.

## Verification

Ran `CARGO_BUILD_JOBS=5 cargo test -p exact-linux --test fix08_paint fix08_ -- --nocapture`. The test compiled the Contract, booted `Presenter` with the CPU painter, rendered a frame and counted pixels outside the text box. It printed `nowrap text width=50: 1850 dark pixels at x=60..299, y=0..29`; the assertion expected zero and failed. The portable painter ran on macOS. The temporary test was removed.

## Direction

Apply the node's overflow clip while painting its paragraph ink, including the unfragmented path.

## Related

[Overflow clips at the border box](20261110-overflow-clips-at-border-box.md) concerns the position of a descendant clip. This paragraph is painted without its own clip. Reported in the f-overflow-clips review.
