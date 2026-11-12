# On iOS an `inert` row of a native menu is enabled and its press runs

**Status:** Closed
**Resolution:** A row that is inert at or above it is a disabled `UIAction` in the native menu (an inert submenu opener a dimmed action), and choosing it presses nothing.
**Systems:** host/apple/Sources/ExactKit/IOS/MenuItemsIOS.swift (MenuHost.items)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, `MenuHost.items(of:)` (`host/apple/Sources/ExactKit/IOS/MenuItemsIOS.swift:19-86`) turns the rows of a `role="menu"` popover into UIKit's menu, for a pull-down and for a context menu. A row is dimmed only for its own `disabled` (line 75, and line 35 for a row that opens a submenu), and its handler asks again only for `disabled` before it dispatches the press (lines 48-52). `inert`, on the row or above it, is not read, and neither `Presenter.press` nor the runner refuses a press by node id for it.

Everywhere else an inert row cannot be chosen:

- The Mac's menu disables it (`Mac/MenusMac.swift:860`, "a hidden or inert row is shown, never chosen"; line 849 for a submenu's row).
- iOS's own alert and action sheet dim an inert action (`MenusIOS.swift:1259`, through `eligible`, lines 418-434), and its painted popover cancels a press on one (`PointerIOS.swift:354` and `:414`).
- On the web the row carries HTML's `inert` attribute and takes no click.

## The failure

```
button popovertarget="more"
  text "More"
column id="more" popover="auto" role="menu"
  button press=rename popovertarget="more" popovertargetaction="hide"
    text "Rename"
  button press=remove inert=busy popovertarget="more" popovertargetaction="hide"
    text "Remove"
```

With `busy` true, open the menu on iOS and choose Remove. Expected: nothing, as on the web and the Mac. By reading: Remove is an enabled item and `remove` runs.

## Verification

- The Mac, run: a temporary XCTest that applied this popover with a third, `disabled` row and asked `MenuHost.menu(of:from:)` for its `NSMenu` printed `["Rename: enabled", "Remove: disabled", "Archive: disabled"]`.
- iOS, read, and run as copied code: the statements of lines 41-78 that set the action's attributes and dispatch the press, in a bare Mac Catalyst process (the macOS 27.2 SDK's UIKit) with stand-ins for the node and the presenter, each action then performed:

```
Rename: authoredInert false, disabled false -> UIAction enabled, choosing it presses 1 time(s)
Remove: authoredInert true, disabled false -> UIAction enabled, choosing it presses 1 time(s)
Archive: authoredInert false, disabled true -> UIAction disabled, choosing it presses 0 time(s)
```

- The pinned Chromium: a click on an `inert` button in an open popover ran no handler, where the same click on its neighbour did.
- `contract build` accepts the snippet (with its state and actions).

Not run: `items(of:)` itself, which needs a simulator (none runs on this machine).

## Direction

In `items(of:)`, treat a row that is `authoredInert` as a `disabled` one: the `.disabled` attribute at lines 35 and 75, and the guard in `current()` at line 48.

## Related

No issue or QUEUE entry names it. QUEUE's "iOS: what a popover over a dialog's sheet still lacks" mentions inertness around a painted popover lifted over a sheet, which is another path.
