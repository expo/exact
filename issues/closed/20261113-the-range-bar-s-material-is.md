# The range bar's material is missing on iOS 17 through 25

**Status:** Closed
**Resolution:** The range bar's material is inserted on every iOS version that clears the bar; the iOS 26 check guards only the HDR reference, its headroom colours and the diagnostic sampler.
**Systems:** apps/ui-gallery
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

On iOS, a route that contains `range-bar` installs a transparent navigation-bar appearance on every iOS version. The material that should show through that bar is inserted only when the OS is iOS 26 or newer. `element` returns before creating `RangeBarBackground` on iOS 17 through 25.

The deployment target is iOS 17. On those versions the header is transparent and the blur is missing, so the bar and the controls under a collapsed header have no material. `RangeBarBackground` is a `UIVisualEffectView` and a gradient mask.

## Where

`apps/ui-gallery/modules/apple/ColorReference.swift`, `route` and `element`. `apps/ui-gallery/modules/apple/GalleryRangeBar.swift`, `RangeBarBackground`.

## How to reproduce

Open the range-bar screen in the UI Gallery on an iOS 17, 18, or 25 simulator. The navigation bar is clear. The range controls have no material behind them. The same screen on iOS 26 shows the blur.

## Direction

Insert `RangeBarBackground` on every iOS version that clears the bar. Keep the iOS 26 check around the HDR reference, the headroom colors, and the diagnostic sampler.
