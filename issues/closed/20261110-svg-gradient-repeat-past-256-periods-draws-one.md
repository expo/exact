# An SVG gradient that repeats more than 256 times across its shape is drawn as one stretched period on Apple

**Status:** Closed
**Resolution:** Past 256 periods, a repeating or reflecting linear or concentric radial gradient is evaluated at each pixel of its raster from one period drawn into a strip, so every period draws.
**Systems:** host/apple/Sources/ExactKit/SvgPaint.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`SvgPaint.draw` (`host/apple/Sources/ExactKit/SvgPaint.swift:194-239`) draws `spreadMethod="repeat"` and `"reflect"` by repeating the stops over every period the shape's bounding box covers and stretching the gradient vector to span them:

```swift
stops = repeated(stops, from: Int(lo), to: Int(hi), reflect: spread == 1)
(p1, p2) = (CGPoint(x: p1.x + d.x * lo, …), CGPoint(x: p1.x + d.x * hi, …))
```

`repeated` (`SvgPaint.swift:243-253`) gives up past 256 periods and returns the stops as they came:

```swift
guard periods <= 256 else { return stops }
```

The caller stretches the vector all the same, so the one period is drawn once across all of them. The concentric radial branch does the same with `r *= CGFloat(n)` (`SvgPaint.swift:227-234`).

The count is in periods across the bounding box, grown by a unit on each side (`gradient`, `SvgPaint.swift:151`), so it does not depend on the size the picture is shown at.

## The failure

```contract
svg width=1024 height=40
  defs
    linearGradient id="g" gradientUnits="userSpaceOnUse" x1=0 y1=0 x2=4 y2=0 spreadMethod="repeat"
      stop offset=0 stop-color="black"
      stop offset=1 stop-color="red"
  rect width=1024 height=40 fill="url(#g)"
```

Observed: one black-to-red ramp 1024 units wide. Expected: 256 ramps 4 units wide, as a period of 4.2 draws its 244 (SVG 2, `spreadMethod="repeat"`: the pattern repeats until the target region is filled). The trigger is a stripe finer than 1/256 of its shape along the gradient: 4 units across 1024, or 5 across a 1440-unit shape. Such pinstripes and hatches are uncommon, which is why this is minor.

## Verification

A temporary XCTest on macOS (`bun host/apple/build.mjs --test --only <class>`) gave `SvgPaint.parts` a rect with a black-to-red repeating linear gradient and counted the ramps in the gradient layer's raster:

```
period 8.0 over 1024 units: raster 1026x1, 128 periods expected, 130 ramps drawn
period 4.2 over 1024 units: raster 1026x1, 244 periods expected, 245 ramps drawn
period 4.0 over 1024 units: raster 1026x1, 256 periods expected, 1 ramps drawn
period 3.0 over 760 units: raster 762x1, 254 periods expected, 255 ramps drawn
period 5.0 over 1440 units: raster 1442x1, 288 periods expected, 1 ramps drawn
```

The radial branch, run from the file's lines 194-257 in a script, printed `radius 3.0 over 1024 units: 342 rings expected, 1 drawn` and 254 of 254 for radius 4 over 1016. Not run: an app on a device or in a Mac window.

## Direction

Past the bound, draw what the pixels can show instead of one period: evaluate the repeated parameter per pixel of the raster (one row or column in the usual axis-aligned case), or raise the bound to the raster's pixel count along the gradient, past which a period is under a pixel and its average colour is right.

## Related

QUEUE.md "A multicolour (non-template) SVG icon on Apple" and "Bound an SVG filter's island on Linux as Apple does" are other SVG items; neither covers spread methods. No issue names `spreadMethod`.
