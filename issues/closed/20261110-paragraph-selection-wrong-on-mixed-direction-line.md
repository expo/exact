# A paragraph selection on a line of both directions highlights the wrong glyphs, or none

**Status:** Closed
**Resolution:** Apple paragraph and reader selections cover the selected glyphs of each run they reach, a run's glyphless directional marks included, and the Mac's keyboard menu and UIKit's `firstRect` enclose the selection's whole first line.
**Systems:** host/apple/Sources/ExactCore/ParagraphGeometry.swift, paragraph text selection on macOS and iOS
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, `Paragraph.selectedLines` (`host/apple/Sources/ExactCore/ParagraphGeometry.swift:217-233`) makes one rectangle a line, between the caret offsets of the selection's two ends:

```swift
let x0 = CTLineGetOffsetForStringIndex(line, lo, nil), x1 = CTLineGetOffsetForStringIndex(line, hi, nil)
rects.append((i, CGRect(x: x + min(x0, x1), y: y - above, width: max(1, abs(x1 - x0)), height: above + below)))
```

That is the selected text only when everything between the ends reads in one direction. Where the line holds a run of the other direction, a logical range is several visual pieces, and its two ends can have the same offset. Three callers use the result: the Mac's selection paint (`ExactKit/Mac/TextSelectionMac.swift:934`), the rectangles a `selectionchange` action reads as `Selection.rects` (`TextSelectionMac.swift:258`), and the iOS selection rects (`ExactKit/IOS/TextSelectionIOS.swift:381`).

## The failure

```swift
let r = Run(text: "abc אבג def", size: 16, weight: 400, family: 0, italic: false, lineHeight: nil, letterSpacing: 0)
let s = Spec(runs: [r], align: 0, lineClamp: 0, color: [0, 0, 0, 255], strut: r)
let p = TextEngine(resolve: { _ in nil }).paragraph(s, width: 300)
p.selectionRects(NSRange(location: 4, length: 3), align: 0, in: CGRect(x: 0, y: 0, width: 300, height: 40), dirty: .infinite)
```

The rects as x extents from the line's start. The expected column is the extent of the selected glyphs in the same `CTLine`; `NSLayoutManager` and Chromium 156 (`Range.getClientRects()`) give the same numbers.

| paragraph | selected (UTF-16) | Exact | expected |
| --- | --- | --- | --- |
| `abc אבג def`, ltr | 4..<7, the Hebrew word | [30.9, 31.9] | [30.9, 56.6] |
| | 1..<6, `bc אב` | [8.5, 37.2] | [8.5, 30.9] [37.2, 56.6] |
| | 5..<9, `בג d` | [46.5, 70.3] | [30.9, 46.5] [56.6, 70.3] |
| `אבג abc דהו`, rtl | 4..<7, `abc` | [26.9, 27.9] | [26.9, 53.5] |
| `שלום 2026 עולם`, rtl | 5..<9, `2026` | [35.8, 36.8] | [35.8, 74.1] |

A word of the other direction selected alone (a double click) gets a one-point sliver: both of its ends have the same caret offset. The third row covers the one Hebrew letter that is not selected and neither of the two that are. The last two rows are the common case: a Latin word or a number in a right-to-left paragraph.

A line of one direction is right: `שלום עולם` 0..<4 and `plain words` 2..<8 match their glyphs.

## Verification

A temporary Mac XCTest (`bun host/apple/build.mjs --test --only <Class>`) printed the table: `Paragraph.selectionRects`, the selected glyphs' extents from `CTRunGetStringIndices`, `CTRunGetPositions` and `CTRunGetAdvances`, and `NSLayoutManager.enumerateEnclosingRects` for the same text, font and base direction. Chromium's rectangles are from a page holding the same strings at `16px system-ui`. Not driven in a running app, and not run on iOS.

By reading only: the reader's paragraphs apply the same two-offset rule (`ExactKit/RegionTextGeometry.swift:203-219`, `ExactKit/RegionWorkerLayout.swift:69-95`).

## Direction

Make a rectangle for each glyph run of the line that the selection reaches: intersect the run's string range with the selection, take that piece's extent within the run, and merge neighbours that touch. The reader's two copies need the same.

## Related

`QUEUE.md`, "Markdown viewer follow-ups", says bidi selection geometry "still need[s] dedicated fixtures", and LLP 1033 lists bidi highlight geometry as a follow-up it does not claim. Neither says the rectangles are wrong or names an input. No filed issue names `selectedLines` or a mixed-direction selection.
