# iOS: a long press on a segmented control whose segments are sized by content can send `contextmenu` to the neighbouring tab

**Status:** Closed
**Resolution:** A long press and inspect mode find the segment under a point from the segment views' own boxes (`UISegmentedControl.segment(at:)`), with equal shares only when the views are not found.
**Systems:** host/apple/Sources/ExactKit/IOS/SegmentsIOS.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

References are at `dd6f5dcd`.

A tablist or radio group drawn as a `UISegmentedControl` hears a long press and sends `contextmenu` to the tab under it. `SegmentHost.contextTab` finds that tab by dividing the control's width into equal shares (`SegmentsIOS.swift:224-227`):

```swift
var index = Int(point.x / control.bounds.width * CGFloat(ids.count))
```

The segments are not always equal. In a box narrower than the control's natural width, `segment` turns on `apportionsSegmentWidthsByContent` (`SegmentsIOS.swift:611-614`), and UIKit then sizes each segment by its content ([apportionsSegmentWidthsByContent](https://developer.apple.com/documentation/uikit/uisegmentedcontrol/apportionssegmentwidthsbycontent)). The two other places that map a point to a segment read each segment view's own box first and fall back to equal shares only when the views are not found: `drawn(_:)` (`SegmentsIOS.swift:656-665`) and inspect mode's `tab(at:in:)` (`SegmentsIOS.swift:208-214`).

## The failure

```
row role="tablist" width=160
  button role="tab" aria-selected=(unit == "t") press=pick("t") contextmenu=menu("t")
    text "Temperature"
  button role="tab" aria-selected=(unit == "l") press=pick("l") contextmenu=menu("l")
    text "Length"
```

With equal segments the control is 200 points wide, so in 160 points UIKit apportions it: "Temperature" takes 98 points and "Length" 62. A long press at x = 90 is on "Temperature". Observed: `contextTab` computes `Int(90 / 160 * 2) = 1` and `menu("l")` runs. Expected: `menu("t")`, the tab whose segment is under the finger, as a tap there selects it.

## Verification

Run in a Mac Catalyst process with no window (UIKit 27.0, the default text size): a `UISegmentedControl` with these two titles, framed as `segment` frames it, then both rules at every point across it. It printed:

```
["Temperature", "Length"] in 160.0 points; natural width 200.0; apportions by content: true
  segment boxes: x 0.0 to 98.0, x 98.0 to 160.0
  contextTab names another tab at 18 of 160 points, x 80.5 to 97.5: at 80.5 it says "Length", the segment there is "Temperature"
```

At 300 points the segments are equal and the rules agree. With "All", "Unread" and "Mentions and replies" in 260 points they differ at 124 points of 260.

Not run: the long press itself, Exact's presenter and an iPhone simulator, which this machine cannot run. A simulator run should hold a finger at x = 90 on the control above and read which `contextmenu` runs.

## Direction

Have `contextTab` find the segment as `drawn(_:)` does, from `segmentViews`, with equal shares as the fallback, so the three callers share one rule.

## Related

`issues/closed/20261031-macos-context-popover-not-menu-shaped.md` is the Mac's right click. Nothing in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md` names a segmented control's long press.
