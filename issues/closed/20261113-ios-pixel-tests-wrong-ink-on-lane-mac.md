# iOS pixel tests read the wrong ink on the async lane's Mac

**Status:** Closed
**Resolution:** Main fixed the dimmed-pixel fixture and the text fade; 7c88152c2 renders the two static ink tests from the layer tree, so their capture no longer follows simulator display updates.
**Systems:** host/apple/tests/ExactKitTests (pixel captures)
**Severity:** P3
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

Fixed on `fs2/apple-ui`; awaiting landing.

Five XCTests that read the simulator's pixels failed on the original lane:

- `OneColorInkIOSTests.testEachColourShowsAsItIs` and `testALayerShadowFallsFromTheDrawnInk`: no pixel of the ink is found ("0 is not greater than 100").
- `DimmedPixelsIOSTests.testBehindAnAlertAColourOfItsOwnKeepsIt`: the mark is `[192, 179, 245]` before the alert and `[189, 183, 248]` behind it, not `[97, 85, 245]`. `testBehindAnAlertTheTintGreysAsUIKitGreysIt`: `[255, 59, 64]`, not `[255, 56, 60]` (4 over a tolerance of 2).
- `TextPlaceIOSTests.testHalfWayThroughTheFadeTheScreenShowsBothLinesBlended`: the middle frame's distance from the blend is 883,103, over the bound of 219,525.

On current main (`ecdaf06fb`), a dedicated iPhone 18 Pro simulator on iOS 27.0 passes the two
dimmed-pixel tests and the halfway-fade test. Main's `56cc3e8af` repaired the paragraph fade,
and its dimmed-pixel fixture now keeps the document below the scroll-edge effect. Those three
changes from the original repair are not carried here.

The two static ink tests still fail on current main: both find no ink. Their helper asks
`drawHierarchy(afterScreenUpdates: true)` for a static layer result, which can return before
the simulator display supplies it. The carried change renders the window's layer tree. The
presentation-pixel helper used by the numeral-roll test still uses `drawHierarchy`.

## How to reproduce

On a dedicated simulator, run:

```
bun host/apple/build.mjs --test --ios --only OneColorInkIOSTests,DimmedPixelsIOSTests,TextPlaceIOSTests
```

The two repaired tests pass after the layer-tree capture. The original fade test still does
not distinguish a transition that keeps only part of the old paragraph; this issue does not
claim that narrower case is covered.
