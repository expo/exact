# UIKit hits children inside an overflow box's border

**Status:** Closed
**Resolution:** A box whose overflow is not visible lets a touch reach its children only inside its padding edge, rounded by its inner border radii (`reachesChildren(at:)`); on its border the box itself is the target.
**Systems:** host/apple/Sources/ExactKit/IOS/NodeViewIOS.swift
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

The ordinary hit walk rejects clipped children only outside `bounds` (`host/apple/Sources/ExactKit/IOS/NodeViewIOS.swift:919`–`:924`). These are the border-box bounds. A non-visible overflow clips descendants at the padding edge; the parent's border must not expose a child as the target.

## The failure

Use a 100×100 border-box with `overflow="hidden"`, a 10-point solid border, and a child positioned to cover (5,5). The hit walk considers (5,5) inside the parent's clip and descends to the child. Expected: no child target through the parent's border. The parent may remain a target.

## Verification

Verified by reading the UIKit hit walk; this machine cannot run UIKit. The bounds comparisons never inset by border widths before `hitChildren`. Device check: mount the box with distinct parent/child press counters, tap (5,5), then (15,15), and verify only the second point reaches the child.

## Direction

Apply the padding-edge clip to the descendant walk, using the used border widths and inner corner geometry, while retaining the parent's own hit area.

## Related

`issues/20261110-overflow-clips-at-border-box.md` records the shared painter drawing children over a border. This is UIKit's separate hit walk: the border can look intact while the child receives its tap.
