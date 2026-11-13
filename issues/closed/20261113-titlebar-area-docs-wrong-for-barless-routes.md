# The title-bar area's docs say an app with routes always has a toolbar, which is false when every route says `navigationBar="none"`

**Status:** Closed
**Resolution:** The title-bar guides describe barless routes, retained session toolbars and the overlay row independently of whether the app has routes.
**Systems:** docs/contract-grammar.md, contract/cli/src/vocab/concepts.md (`titlebar`), docs/contract-for-agents.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261009-titlebar-env-display-mode.md, issues/20261113-mac-page-title-row-double-click-does-not-zoom.md

## What is wrong

Three passages describe the Mac's `env(titlebar-area-*)`:

- `docs/contract-grammar.md` (lines 203-206): all zero "while a toolbar fills the
  title bar (an app with routes has one, LLP 1115.002 D2)", and the tall row of
  `window-controls-overlay` is for "an app without routes".
- `contract vocab titlebar` (`concepts.md:1379-1380`): "while the window has a
  toolbar (an app with routes has one)".
- `docs/contract-for-agents.md` (line 5255): "the tall title row an app without
  routes gets from `display_override`".

The code says otherwise. `ToolbarMac.swift`'s header: "An app with no stack,
or one whose every mounted route asks for no bar, gets no toolbar; one comes
with the first route that shows a bar and then stays." And
`window-controls-overlay`'s empty toolbar is installed whenever the session
shows no toolbar of its own (`coverChrome`, `ExactMac/main.swift:319`,
`!view.hasWindowToolbar`), routes or not. So an app with routes whose every
route says `navigationBar="none"` has a free title row, with the area defined
and `displayMode` `window-controls-overlay`. That is what an app with its own
side panel and its own Back needs, and the docs steer it away.

## The failure

An app with routes, every mounted route `navigationBar="none"`, measured on
macOS 26: under `viewport-fit="cover"` the area was `78 0 w×32`; with
`display_override` `window-controls-overlay` it was `98 0 w×52`, `displayMode`
`window-controls-overlay`, and it stayed so after a push. Seen on exact2
`5434f4325`. Confirmed here against `ToolbarMac.swift` and `main.swift` at
`49ae6357`.

## Fix

In all three passages, replace "an app with routes" / "an app without routes"
with the rule the code keeps: the area is zero while the window shows a
toolbar, which a route that shows a bar brings (and which then stays); an app
with no stack, or whose every route says `navigationBar="none"`, has none, so
the title row is free and `window-controls-overlay` applies. The reporter also
asked for a line that `open()` leaves the stack at `[root, top]`, so the host's
Back always lands on the root; that belongs in the router's section, if it is
still true.

## Verification

A Mac Contract fixture with routes and `navigationBar="none"` retained
`titlebar-area 78,0 342×32` before and after pushing its detail route; the
window reported no installed toolbar. `TitleBarAreaMacTests` also covers the
empty overlay toolbar and the zero area under a session toolbar.

`open` is not always `[root, top]`: `route/src/router.rs` builds the
destination's declared ancestor chain. The router guide now explains the
direct-child case and deeper chains. The CLI vocabulary regression fails on
the old answer; the vocabulary suite compiles the title-bar example too.


The guides distinguish a toolbar retained by navigation history from one supplied
by a single-location app's root header. Hiding that header or setting its route to
`navigationBar="none"` removes its toolbar before history retains one.
`HeaderMacTests.testASingleLocationRootHeaderDoesNotRetainItsToolbarWithoutHistory`
checks both changes on a booted single-location app. The title-bar vocabulary test
requires that exception, and the retention citation is LLP 1115.000 (as built).
