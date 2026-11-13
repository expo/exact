# Under a page zoom the Mac draws an SVG's pattern, mask and filter pictures at the unzoomed scale

**Status:** Closed
**Resolution:** An SVG scene takes its scale from its view's `pixelScale` on the Mac, which includes the page zoom, and `SvgHost.rescale` applies each scene again when the zoom or the display's scale changes.
**Systems:** host/apple/Sources/ExactKit/SvgScene.swift (SvgHost.scene), host/apple/Sources/ExactKit/Mac/NodeViewMac.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

The parts of an SVG scene that are bitmaps (a pattern's tile, a gradient's raster, a mask island, a filter's picture) are made at `SvgScene.scale`, which `SvgHost.scene` sets from the view's layer: `scene.scale = max(1, layer.contentsScale)` (`host/apple/Sources/ExactKit/SvgScene.swift:624`; used at `:527`, `:555`, `:574`).

View ▸ Zoom In and `setPageZoom` magnify the page's scroll view (`NSScrollView.magnification`, `Mac/ZoomMac.swift`). AppKit does not raise a layer's `contentsScale` for that: under a magnification of 3 on a 1x display a view in the document reports `contentsScale` 1 and 3 pixels per unit. The rest of the host reads the second number (`NSView.pixelScale`, `Mac/ZoomMac.swift:28-32`: a box's filter picture at `Mac/NodeViewMac.swift:1268`, a gradient mask at `Mac/BoxMaskMac.swift:57`, a box's borders at `Mac/BoxLayerMac.swift:403`). The scene reads the first, so its pictures are drawn at the window's backing scale and stretched by the zoom. A scene sent again after the zoom is drawn the same way, and nothing applies a scene again when the zoom or the backing scale changes.

LLP 1021 §5.3.1: under a page zoom "pictures, icons and masks are drawn at that scale", the window's backing scale times the factor.

## The failure

```
svg width=60 height=24 viewBox="0 0 60 24"
  defs
    pattern id="dots" patternUnits="userSpaceOnUse" width=6 height=6
      circle cx=3 cy=3 r=2 fill="#0f172a"
  rect width=60 height=24 fill="url(#dots)"
svg width=60 height=24 viewBox="0 0 60 24"
  circle cx=3 cy=3 r=2 fill="#0f172a"
  circle cx=9 cy=3 r=2 fill="#0f172a"
```

After `setPageZoom(3)` the pattern's dots are blurred and the same dots drawn as shapes are sharp. Expected: both sharp, as text and boxes are.

## Verification

Run on a 1x display. A temporary Mac XCTest booted a page with a pattern, a blur filter and a diagonal gradient on a circle, set `ExactSession.pageZoom` to 3 and read each scene's bitmap layers. Before and after the zoom, and after a state change sent one picture's scene again, they were the same:

```
pattern    62x26 px over 62x26 units     view pixelScale 3.0, layer.contentsScale 1.0
filter     48x20 px over 48x19.2 units
gradient   22x22 px over 22x22 units
```

In the Mac app (the pictures above, a rectangle with a round hole masked out of it, and that picture drawn as two shapes, in `apps/hello/app.contract` with buttons for `setPageZoom(1.5)` and `setPageZoom(3)`; `bun host/apple/build.mjs hello-apple`, `bun scripts/agent.mjs macos --app hello "tap zoom" "screenshot zoomed.png"`), the pattern's dots at 3 are blurred beside the sharp circles, and the masked picture's hole and edges are blurred beside the one drawn as shapes; at 1.5 the dots are visibly softer than the circles. Not run: a 2x display.

## Direction

Give the scene the view's `pixelScale` (the layer's delegate is the `NodeView`), and apply the kept scene again when it changes (the zoom's setter calls `zoomChanged`; `viewDidChangeBackingProperties` hears a display's), as `reappear` does for the appearance.

## Related

No issue or QUEUE entry names SVG under the page zoom. `issues/20261110-svg-gradient-repeat-past-256-periods-draws-one.md` is about a gradient's raster, not its scale.
