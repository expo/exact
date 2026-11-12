# A Mac bar item's badge draws an icon token's path in black, not in the author's colour

**Status:** Closed
**Resolution:** A Mac badge draws an icon path's template image in its ink with `FaceSymbol.tinted`, and keeps the palette colour for a named SF Symbol.
**Systems:** host/apple/Sources/ExactKit/BadgeFace.swift, host/apple/Sources/ExactKit/FaceSymbol.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

A header button whose one child is a filled box holding a symbol is a bar item's badge (`BadgeFace`; the Mac's toolbar uses it at `Mac/ToolbarMac.swift:637-638`). On the Mac its image draws the glyph with `FaceSymbol.image(symbol, …)?.withSymbolConfiguration(NSImage.SymbolConfiguration(paletteColors: [ink]))` (`host/apple/Sources/ExactKit/BadgeFace.swift:78-80`). A palette colours an SF Symbol. For an icon token with no `sf` the glyph is `FaceSymbol.pathImage`'s template image (`FaceSymbol.swift:52-61`, `IconPath.image`), which is not a symbol: the configuration changes nothing and `draw(in:)` draws a template's own black. The ink, the `color` the author wrote on the box or the white a dark scheme gives it, is not used.

`FaceSymbol.tinted` (`FaceSymbol.swift:84-94`) is the function that draws a path's template in a colour, and `MacBarFace` and `ToolbarInkMac` use it. iOS's badge uses `withTintColor`, which tints a template (`BadgeFace.swift:99`).

## The failure

```
icons
  icon.logo = icon(svg "icons/logo.svg")        // app.tamber: no `sf`
```
```
header
  text "Notes" role="heading" aria-level=1 flex=1
  button press=account aria-label="Account"
    column background-color="#0000ff" color="#ffffff" border-radius=18
      image "symbol:icon.logo"
```

Expected: a white logo on the blue badge, as the page and iOS draw it. Observed on the Mac: a black logo. With the colour unsaid, a dark scheme draws black on the author's fill.

## Verification

Run as a temporary Mac XCTest (`bun host/apple/build.mjs --test --only <class>`): a presenter given the button, the blue box and an image node with the props the Apple host sends for a symbol image (`host/apple/src/paragraph.rs:711-727`): `symbolName: "square.fill"` for an SF Symbol, and `symbolName: ""` with `symbolPath: "M 2 2 L 22 2 L 22 22 L 2 22 Z"` and `symbolViewBox: "0 0 24 24"` for an svg-only icon. `BadgeFace(button).image` drawn into a bitmap under the light and the dark appearance gave, at its centre:

```
SF Symbol, text_color white      [255, 255, 255, 255]
icon path, text_color white      [0, 0, 0, 255]
icon path, text_color yellow     [0, 0, 0, 255]
the box                          [0, 0, 255, 255]
FaceSymbol.tinted(template, .white), drawn alone    [255, 255, 255, 255]
```

The Contract above was not built: no app here declares an svg-only icon in a badge.

## Direction

Draw a path's glyph with `FaceSymbol.tinted(glyph, ink)` where `FaceSymbol.isPath(symbol)`, and keep the palette for a named symbol.

## Related

QUEUE's "Apple: two icon tokens with one SVG name a face's path by the first declared" is about which path a face gets, not its colour.
