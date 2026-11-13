# iOS pixel tests read the wrong ink on the async lane's Mac

**Status:** Open
**Systems:** host/apple/tests/ExactKitTests (pixel captures)
**Severity:** P3
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

Five XCTests that read the simulator's pixels fail:

- `OneColorInkIOSTests.testEachColourShowsAsItIs` and `testALayerShadowFallsFromTheDrawnInk`: no pixel of the ink is found ("0 is not greater than 100").
- `DimmedPixelsIOSTests.testBehindAnAlertAColourOfItsOwnKeepsIt`: the mark is `[192, 179, 245]` before the alert and `[189, 183, 248]` behind it, not `[97, 85, 245]`. `testBehindAnAlertTheTintGreysAsUIKitGreysIt`: `[255, 59, 64]`, not `[255, 56, 60]` (4 over a tolerance of 2).
- `TextPlaceIOSTests.testHalfWayThroughTheFadeTheScreenShowsBothLinesBlended`: the middle frame's distance from the blend is 883,103, over the bound of 219,525.

The machine's display is a 1440 × 900 virtual one that sleeps after 10 minutes, and nobody is at it. Which of these follow the display and which are the host's is not established: run them on a Mac with a display awake before taking them as defects.

## How to reproduce

At 69668888a on the async lane's Mac (Xcode 27.2, an iPhone 18 Pro simulator on iOS 27.0), run at a load average near 40:

```
bun host/apple/build.mjs --test --ios --only OneColorInkIOSTests,DimmedPixelsIOSTests,TextPlaceIOSTests
```

The same five failed in the async lane's run at afde7b25e.
