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

## Verification

`MaterialTableTests` checks an authored sidebar under a painted ancestor,
including changes to other materials and back. It uses behind-window blending
and preserves the ancestor's paint. `VisibilityMacTests` checks that authored
fills and content remain above materials. Caltrain's Mac host also ran a
Contract fixture with a sidebar beside an in-window material over a red page.

Desktop pixels could not be qualified: the window server supplied no picture
of the fixture window, reporting an asleep display or locked screen. The
existing `SheetSurfaceMacTests.testAuthoredGroundMatchesAnAppKitSheetInDarkMode`
likewise failed reading its standalone AppKit reference picture before testing
Exact's material. The known-failure tool reports that failure as unknown on
origin/main. No window background or ancestor fill was cleared.


## Capture still owed

A WindowServer capture was retried with `/usr/sbin/screencapture -x -o -l`
on an AppKit `.sidebar` / `.behindWindow` reference over a solid blue window,
with red content beside the material. The reference was visible but could not
become key. `screencapture` exited 1 with “could not create image from window”.
No desktop-pixel comparison was possible.

The eleven `MaterialTableTests` and `VisibilityMacTests` pass: the blending
mode is built and authored fill and content remain above the material. The
issue remains open until an unlocked-display capture compares the authored
sidebar over a painted ancestor with the AppKit reference and confirms both
the backdrop and the authored paint above it.
