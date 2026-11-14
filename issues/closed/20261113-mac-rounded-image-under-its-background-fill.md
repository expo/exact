# On macOS a rounded image with a `background-color` can draw only the background: its fill sublayer covers the picture

**Status:** Closed
**Resolution:** A rounded image that does not clip and whose picture `draw(_:)` paints now draws its fill there too, under the picture, instead of on a sublayer over it.
**Systems:** host/apple/Sources/ExactKit/Mac/BoxLayerMac.swift, host/apple/Sources/ExactKit/Mac/NodeViewMac.swift
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

An `image` puts its pixels in one of two places (`BoxLayerMac.swift`):

- a sublayer, `imageLayer`, when `imagePlan` can say the picture
  (`applyImageLayer`, line 342). `insertBoxSublayer` keeps it above the box's
  fill sublayer.
- the view's own layer contents, painted by `draw(_:)`
  (`NodeViewMac.swift:1379`), when `imagePlan` is `nil`. It is `nil` for a
  rounded image unless the picture fills the whole border box: `fits` (line
  325) needs `content == bounds && shown == content`, so padding, a border,
  `object-fit="contain"` or `"scale-down"` that letterboxes, a radius
  per corner, or a `corner-shape` the layer cannot say all send the picture to
  `draw(_:)`.

The box's background goes on the layer's own `backgroundColor`, under its
contents, except for a rounded box that does not clip: `applyBoxLayer`
(line 175) then fills a sublayer, `boxFill`. A sublayer composites over its
layer's contents, so a rounded image that does not clip, has a
`background-color`, and whose picture does not fill its border box shows only
the background. The capture path already knows this case
(`captureShowsLayerPaint`, the "podcast F7" note); the window does not.

## The failure

An image with a remote `https` source and a grey `background-color` as a
placeholder drew only the grey on macOS; its `load` event fired. Moving the
`background-color` to a wrapping view made the picture show. Seen on exact2
`94b8671f7`, macOS (Apple silicon), in an app's diary under "macOS rendering
differences". The diary does not say whether the image had a radius, padding
or an `object-fit`.

Checked here by reading the code at `49ae6357`; not run. The path above is the one
that ends with a fill over the picture; if the report's image had no radius,
the cause is elsewhere and this issue should be retitled.

## Fix

When the picture goes to `draw(_:)`, keep the fill there too: have `boxPlan`
mark the box `drawn` for an image whose `imagePlan` is `nil` and whose fill
would be a sublayer, so `draw(_:)` paints fill then picture in CSS's order. Or
put the picture on `imageLayer` in every case, masking it to the rounded
border box. An XCTest: a 100×100 image, `border-radius=12`, `padding=8`,
`background-color` grey, a red bitmap; render the window and read a centre
pixel.


## Verification

`BoxPaintMacTests.testARoundedDrawnImageKeepsItsBackgroundBehindItsPixels`
checks a padded image with a blue fill, both with every corner rounded and
with only the top-left corner rounded plus `overflow: hidden`. The live
layer tree and capture show the red picture above the fill and blue in the
padding. The partial-corner case failed both picture assertions before the
fill-sublayer guard was widened. All eight tests in the class pass.
