# A page-drawn macOS title row must preserve the title bar's mouse behaviour

**Status:** Closed
**Resolution:** a dblclick listener in the title band takes its double click's release, as a control does, so macOS 27.0's title-bar recognizer never sees it; proved with real input on macOS 26.6.2, 27.0 and 27.2 under every setting
**Systems:** host/apple/Sources/ExactKit/Mac/TitleBarAreaMac.swift, TitleRowTapMacTests.swift
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261009-titlebar-env-display-mode.md

## What was wrong

On `ecdaf06fb`, Caltrain was given `window-controls-overlay` and an authored
52-point `header` in the title band. A real Quartz HID double click on its empty
ground changed the window frame from `(510, 137, 900, 640)` to the native zoomed
frame `(0, 30, 1920, 974)`, on macOS 27.0.1.

The same title-band ground can have an authored `dblclick`. A real Quartz HID
drive on that listener ran the authored action once but also performed the
person's configured title action. With Zoom selected, the window frame changed.
The authored action therefore did not have precedence.

## Closed

On macOS 27.0 the title bar's double-click recognizer acts on the releases the
window routes (`NSWindow.sendEvent`), over the page's views as over its own:
in a hand-built window with a full-size content view, a double click on the
content view, a plain subview or a view that handles the double click itself
zooms, minimizes or fills the window, and one on an `NSButton`, whose tracking
takes its own release, does nothing. Fill and Minimize go through the window
manager, past `windowShouldZoom(_:toFrame:)`, so a delegate's veto would cover
Zoom alone. On macOS 26.6.2 and 27.2 AppKit takes no title action over the
content, and the ground's action is the host's (`TitleBarAreaMac.doubleClicked`).

The node a second click on title-band ground reaches now takes that click's
release from the queue when a `dblclick` listener is on it or above it
(`TitleBarAreaMac.takesRelease`), as a control does. Caltrain with
`window-controls-overlay` and a `dblclick` box in a 52-point header, driven with
real Quartz HID input in macOS VMs (26.6.2, 27.0 and 27.2), under Zoom, Fill,
Minimize and No Action: a double click on the box ran its `dblclick` once and
left the window as it was; one on the header's empty ground zoomed, filled,
minimized or did nothing; a click on either changed nothing; a drag on either
moved the window. Before the change, on 27.0, the box's double click also
zoomed or minimized the window. `TitleRowTapMacTests` asks the page's decision
for both and checks that the listener takes its release; that the window
server's double click reaches the page needs real input.
