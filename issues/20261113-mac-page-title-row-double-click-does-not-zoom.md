# On macOS a double-click on the empty part of a page-drawn title row does not zoom the window

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/Mac/MouseEventsMac.swift, host/apple/Sources/ExactKit/Mac/MouseChainMac.swift, host/apple/Sources/ExactKit/Mac/TitleBarAreaMac.swift, host/apple/Sources/ExactMac/main.swift
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261009-titlebar-env-display-mode.md

## What is wrong

Under `viewport-fit="cover"` or `window-controls-overlay`, the page underlaps
the title bar (`coverChrome`, `ExactMac/main.swift:311`) and an app draws its
own title row in `env(titlebar-area-*)`. Dragging the empty part of that row
moves the window, as AppKit lets a non-opaque view do. A double-click there
does nothing: the page's views receive the mouse-down, as the window's hit test
finds them under the empty toolbar (`TitleRowTapMacTests`), and nothing in the
host performs the title bar's double-click action. A double-click on the empty
part of a native toolbar in the same app does zoom, because AppKit's title bar
view handles it. A hand-built Mac window that draws its own title row zooms or
minimizes there by the system setting (System Settings ▸ Desktop & Dock,
"Double-click a window's title bar to"; the `AppleActionOnDoubleClick`
default).

The host already knows where the row is (`TitleBarAreaMac.rect`) and which
nodes in it are controls; only the action is missing. Contract also has no
command to zoom the window, so an app cannot do it from its own `dblclick`.

## The failure

A window with `display_override` `window-controls-overlay`, `viewport-fit="cover"`
and `navigationBar="none"` on every route, with a row at `env(titlebar-area-x)`
and `env(titlebar-area-height)`. A double-click on the row's empty part did not
zoom the window; a drag there moved it. Seen on exact2 `5434f4325`, macOS 26.

Checked here by reading the code at `49ae6357`: there is no `performZoom` or
`AppleActionOnDoubleClick` outside the Window menu (`DevMenuMac.swift:290`).
Not run.

## Fix

On a second click in the title-bar area that lands on no control and that no
`dblclick` handler takes (`dblclickTarget`), do what the system setting says:
`window.performZoom(nil)`, `performMiniaturize(nil)`, or nothing, reading
`AppleActionOnDoubleClick` from the global domain as AppKit's title bar does.
This is the drag's existing rule applied to the double-click.

## Fix built, not yet run

`PageScrollView.mouseDown` hands a second click on the page's ground in the
title bar's band to `TitleBarAreaMac.doubleClicked`, which zooms, minimizes or
does nothing by `AppleActionOnDoubleClick` (else `AppleMiniaturizeOnDoubleClick`)
unless a `dblclick` listener takes it. "Fill" zooms: AppKit has no public call
for it. `TitleRowTapMacTests` covers the ground, a button in the row and the
setting; it compiles, and has not run: the screen was locked. Close this once
`bun host/apple/build.mjs --test --only TitleRowTapMacTests` passes on an
unlocked Mac and a double click on a real title row zooms.
