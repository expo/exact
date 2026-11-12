# On iOS a custom invoker's overlay that has served a confirmation never opens the menu its target becomes

**Status:** Closed
**Resolution:** An invoker whose target changes between a confirmation and a menu gets a new overlay of the right kind, so a confirmation's overlay never serves a menu.
**Systems:** host/apple/Sources/ExactKit/IOS/MenusIOS.swift (MenuHost.sync)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`MenuHost.sync` lays one transparent button over each custom invoker and keeps it by the invoker's id (`overlays`, `host/apple/Sources/ExactKit/IOS/MenusIOS.swift:18`, at `dd6f5dcd`). It configures the button in one of two branches, and each starts from the button already there:

- A confirmation (lines 193-213): `overlays[v.id] ?? InvokerButton(...)`, then `menu = nil`, `showsMenuAsPrimaryAction = false`, and a `.touchUpInside` action (`exact.confirmation`) that holds the popover weakly and calls `openConfirmation`.
- A menu (lines 214-255): `overlays[v.id] ?? { MenuButton … showsMenuAsPrimaryAction = true … }()`, then `button.menu = pullDown(...)`.

The menu branch sets `showsMenuAsPrimaryAction` only on a button it makes (line 222) and never removes the confirmation's action. An overlay stays while its invoker opens either kind (`live`, lines 192 and 257). So once an invoker's target has been a confirmation and becomes a menu, the reused button has the menu but not as its primary action, and a tap still runs `exact.confirmation` against the popover it captured. UIKit shows a button's menu on a tap only when `showsMenuAsPrimaryAction` is true (Apple, `UIControl.showsMenuAsPrimaryAction`).

The opposite change is handled: the confirmation branch resets both properties and replaces its own action each sync, and `InvokerButtonIOSTests.testAPanCancelsAConfirmationInvokerButKeepsPrimaryMenuTracking` asserts it.

## The failure

```
button popovertarget=(one ? "confirm" : "more") padding=8 border-radius=8
  row
    image "symbol:ellipsis"
    text "More"
column id="confirm" popover="auto" role="alertdialog" aria-label="Remove?"
  text "This cannot be undone."
  button press=remove popovertarget="confirm" popovertargetaction="hide" destructive=true
    text "Remove"
  button popovertarget="confirm" popovertargetaction="hide"
    text "Cancel"
column id="more" popover="auto" role="menu"
  button press=rename popovertarget="more" popovertargetaction="hide"
    text "Rename"
```

With `one` true, let a batch land; set `one` false; tap the button. Expected: the pull-down menu of `more`. By reading: the tap runs the stale action with `confirm`, which is still live. The button has no `press`, so `openConfirmation` checks only that the source is eligible and that popover live (lines 1235-1239) and presents the alert the button no longer names. With a `press` on the button it is refused at line 1221 ("its press retargeted the button to a menu"). Neither opens the menu, on this tap or a later one.

## Verification

Read, not run in `MenuHost`: it needs a presenter and node views, and no simulator runs on this machine. `contract build` accepts the snippet (with its state and actions). The statements of the two branches that touch the button were copied into a bare Mac Catalyst process (the macOS 27.2 SDK's UIKit) and run in order:

```
menu from the first sync          : MenuButton, menu set, showsMenuAsPrimaryAction true, a tap runs the confirmation action 0 time(s)
confirmation, then a menu         : InvokerButton, menu set, showsMenuAsPrimaryAction false, a tap runs the confirmation action 1 time(s)
menu, confirmation, then menu     : MenuButton, menu set, showsMenuAsPrimaryAction false, a tap runs the confirmation action 1 time(s)
```

## Direction

Make the overlay's kind part of what is kept: when the branch and the button's class disagree, remove the old overlay and make the right one, as `ControlHost.control(for:)` remakes a control whose kind changes (`ControlsIOS.swift:166-180`).

## Related

No issue or QUEUE entry names the overlay's reuse. `SubmenuIOSTests` and `NativeContextsIOSTests` cover each branch from a fresh button.
