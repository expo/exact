# A macOS agent viewport screenshot under glass must show the composited page

**Status:** Closed
**Resolution:** Fixed by 3d3458a2c: a viewport screenshot under glass is the window server's picture cropped to the viewport, with the shared compositor as the fallback.
**Systems:** host/apple/Sources/ExactKit/Mac/AgentMac.swift, CaptureMac.swift, NodeViewMac.swift
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20260904-macos-offscreen-capture-can-be-transparent.md

Fixed by “Mac glass viewport screenshots crop the window server picture and fall back to the compositor” when that commit lands.

## What was wrong

The plain agent `screenshot` draws the page viewport. AppKit composites an
`NSGlassEffectView` at the WindowServer, so a CPU picture can differ from the
page a person sees.

On `ecdaf06fb`, main's shared compositor passed four reviewed cases unchanged:
nested glass, a focused field editor, a clean layer-painted image, and a dirty
translucent image. The root/live mirror still failed. Its live glass backdrop
was blue, while the captured backdrop was white; the largest sampled channel
delta was `0.443`, against a `0.04` bound. The existing root test only checked
the red child and therefore passed.

## What is fixed

Main's shared compositor remains the ordinary screenshot and canvas-upload
path. A viewport containing non-hidden native glass, with no hidden ancestor,
prefers the WindowServer composite cropped to the viewport. Selection does not
interpret rectangles, clips, masks or transforms: a false positive still gives
the right viewport pixels. If the server has no picture, the capture falls back
to the shared compositor and its reply notes that glass was not composited.
Canvas readbacks are installed before the WindowServer picture and restored
before the shared-compositor fallback, so a translucent canvas paints once.
Live views are neither reparented nor CPU-drawn.

Both macOS WindowServer screenshot forms flush and wait two display refreshes,
as main's whole-window capture did on `ecdaf06fb`. Capture inserts no marker or
other view into the live window. The whole-window path is main's capture again,
then composes any attached window.

## Verification and limits

`GlassGroupMacTests` covers the root/live mirror, nested glass, the focused
editor, clean and dirty layer images, conservative hidden-view selection,
translated and nested shape clips, unavailable-server fallback, unchanged view
trees, a translucent canvas fallback, and unequal corners. The product
drive compared viewport and whole-window screenshots of the same native-glass
page on macOS 27.0.1. The viewport was 1280 by 800; the window picture was
1280 by 832, including its 32 title-bar rows. Comparing the viewport with the
remaining window rows found zero changed pixels and a maximum channel delta of
zero. `GlassGroupMacTests`, `AgentCanvasCaptureMacTests`,
`MaterialTableTests` and `SheetSurfaceMacTests` cover this path.

The unchecked parent-and-sheet composite and the macOS 26.6 run remain owed.
