# Hiding the keyboard toolbar clears scroller insets while enumerating them

**Status:** Closed
**Resolution:** Not a defect: a Swift `Dictionary` is a value, so `for (id, inset) in insets` iterates its own copy and `removeValue` copies on write; nothing traps, and the released inset is undone (`KeyboardToolbarIOSTests.testChangingPolicyReleasesTheToolbarLiftAndItsOwnedScrollInset` drives this path and passes).
**Systems:** host/apple
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`ride` walks `insets` and calls `removeValue` for each id the toolbar no longer owns. That mutates the dictionary under the iterator, and Swift traps. Hidden toolbars are left out of the owned set. Hiding the bar, or leaving `overlays-content`, while a scroller still holds a positive inset takes this path on the next ride.

## Where

`host/apple/Sources/ExactKit/IOS/KeyboardToolbarIOS.swift`, `ride`.

## How to reproduce

Show the keyboard with an overlays-content toolbar that has padded a scroller, then hide the toolbar. The process traps in `ride`.

## Direction

Collect the ids to drop, then adjust and remove them.
