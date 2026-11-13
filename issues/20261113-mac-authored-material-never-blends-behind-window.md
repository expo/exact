# On macOS an authored material always blends within the window, so an app's own sidebar never shows the desktop

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/Mac/NodeViewMac.swift (material, line 605), kernel/tables/schema.json (`materials`), contract/cli/src/vocab/concepts.md (`glass`)
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

Every `backgroundMaterial` that is an `NSVisualEffectView` gets
`blendingMode = .withinWindow`; only the sheet's own ground gets
`.behindWindow` (`NodeViewMac.swift:603` to `:606`). So
`backgroundMaterial="sidebar"` on an app-drawn side panel blurs the page under
it and never the desktop behind the window. AppKit's source list and
`NSSplitViewItem(sidebarWithViewController:)` blend behind the window, and the
latter is what Exact uses when the root `tablist` is the window's sidebar
(`SidebarViewsMac.swift:62`, LLP 1115.002 D3).
An app whose side panel is its own, not a `tablist`, cannot get that look:
a Mac user would see the difference beside any Finder or Mail window. LLP
1053's materials table records no decision on blending mode.

## The failure

A panel with `backgroundMaterial="sidebar"` over coloured content in a key
window took the page's colours, not the wallpaper's. Seen on exact2
`5434f4325`, macOS host. Confirmed by reading the code at `49ae6357`; not run.

## Fix

Blend behind the window where the material is a window-level one and nothing
of the page lies under the node: `sidebar` (and probably `hud-window`, which
AppKit uses for panels) on a node with no painted ancestor between it and the
window could take `.behindWindow` by default, as a hand-built sidebar does,
leaving `.withinWindow` for a material over page content. If the host cannot
tell, the reporter's suggestion is a way to ask (a second word on the value,
`backgroundMaterial="sidebar behind-window"`), but the platform default is
better than a new knob, per LLP 1115's "say less". A behind-window material
needs the window's background under it to be clear, which the page's
`background-color` would cover; the fix has to state what the root paints
there.
