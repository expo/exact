# Apple Canvas 2D drops `wordSpacing` when `letterSpacing` is also set

**Status:** Closed
**Resolution:** `CanvasText.line` gives each space a tracking of `letterSpacing + wordSpacing` instead of a kern, which Core Text ignored beside the run's tracking.
**Systems:** host/apple/Sources/ExactSurfaces/Canvas2DText.swift (`CanvasText.line`)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, `CanvasText.line` (`host/apple/Sources/ExactSurfaces/Canvas2DText.swift:76-101`) sets `letterSpacing` as Core Text tracking on the whole run (`:91`) and `wordSpacing` as a kern of `wordSpacing + letterSpacing` on each U+0020 (`:93-98`). Core Text does not apply that kern where the run also has tracking: the space advances by the tracking alone. The same line is built for `measureText` and for drawing, so both are short by `wordSpacing` for every space.

## The failure

```ts
ctx.font = '40px system-ui';
ctx.letterSpacing = '5px';
ctx.wordSpacing = '10px';
ctx.measureText('AVAVA To').width;
```

Widths on macOS, from `CanvasText.measure`, beside headless Chromium 156 running the lines above:

| `letterSpacing` | `wordSpacing` | Exact | Chromium |
| --- | --- | --- | --- |
| 0 | 0 | 164.570 | 164.570 |
| 5 | 0 | 204.570 | 204.570 |
| 0 | 10 | 174.570 | 174.570 |
| 5 | 10 | 204.570 | 214.570 |

With `fontKerning = "none"` the last row is 222.344 against 232.344. The drawn line has the same loss: with `letterSpacing` 5 the `T` starts at 152.129 whether `wordSpacing` is 0 or 10. `a b c d` at 20 px with `letterSpacing` 2 and `wordSpacing` 8 measures 72.363, the width with `letterSpacing` alone, where the three spaces should add 24.

Expected: both apply. The 2D context puts its letter spacing and word spacing on the run's inline box as `letter-spacing` and `word-spacing` (HTML, "text preparation algorithm"), and CSS applies `letter-spacing` in addition to `word-spacing` (CSS Text 3 §7.2). The recorder's own estimate adds both (`canvas/src/font.rs:499`).

## Verification

A temporary Mac XCTest (`bun host/apple/build.mjs --test --only <Class>`) called `CanvasText.measure` and `CanvasText.line` with a `Canvas2DFont` holding each pair of spacings and printed the Exact figures. A Core Text script with the same attributes gives 204.570 for tracking 5 on the run with kern 15 on the space, and 214.570 for either change under Direction. Not run on iOS, where the same file compiles.

## Direction

Give each space a tracking of `letterSpacing + wordSpacing` instead of a kern, or leave tracking off the spaces that carry the kern.

## Related

`QUEUE.md`, "LLP 1056 (Canvas 2D) stage 2 owes" (4), lists what Apple's canvas text ignores (`fontStretch`, `textRendering`, oblique); `wordSpacing` is not among them.
