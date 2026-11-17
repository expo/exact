# On macOS an authored material always blends within the window, so an app's own sidebar never shows the desktop

**Status:** Closed
**Resolution:** The material implementation stood; 3d3458a2c adds the window-server pixel proof that an authored sidebar's backdrop equals AppKit's own.
**Systems:** host/apple/Sources/ExactKit/Mac/NodeViewMac.swift, host/apple/tests/ExactKitTests/MaterialTable.swift
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What was wrong

An authored `backgroundMaterial="sidebar"` blended within the window, unlike
AppKit's sidebar material, so it sampled the page rather than the backdrop
behind the window.

Main already carries the implementation: an authored sidebar uses
`.behindWindow`, while authored fill and content remain above the material. On
`ecdaf06fb`, both existing `MaterialTableTests` passed, as did all three
`SheetSurfaceMacTests`.

## What this commit adds

The material implementation stands. The carried WindowServer test supplies
the missing pixel proof: it places AppKit's own sidebar and the authored
sidebar at the same frame in one window, waits for a distinct paint marker to
appear, then compares their backdrop pixels. It also proves that the authored
blue ancestor remains beside the material and its red child remains above it.

On macOS 27.0.1 with the display unlocked, all 3 `MaterialTableTests` and all
3 `SheetSurfaceMacTests` pass.

Nothing is owed on another unlocked screen; a macOS 26.6 run was outside this
work.
