# iOS opens a confirmation popover below its invoker when `position-area` names the left column

**Status:** Closed
**Resolution:** `MenuHost.place` permits only a right-pointing arrow for a `left…` area, so UIKit opens the sheet to its invoker's left.
**Systems:** host/apple/Sources/ExactKit/IOS/MenusIOS.swift (MenuHost.place), LLP 1021 §5 "Placement"
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, `MenuHost.place` (`host/apple/Sources/ExactKit/IOS/MenusIOS.swift:1377-1386`) turns a confirmation popover's `position-area` into the arrow direction UIKit may use:

```swift
presentation.permittedArrowDirections = area.hasPrefix("top") ? .down : area.hasPrefix("right") ? .left : .up
```

`top…` is above the invoker and `right…` is beside it on its right. Every other value falls to `.up`, which UIKit places below the source, and that includes the four in the left column: `left`, `left span-top`, `left span-bottom` and `left span-all`, which LLP 1021's 2026-10-13 slice admitted (`kernel/tables/schema.json`, `PositionArea`). CSS puts each in the anchor's left column. `UIPopoverArrowDirection.right` is the popover on the source's left with its arrow pointing right at it, as `.left` already serves `right…`.

The other hosts place the column on the left. The web writes the CSS declaration and the browser places the popover. On the Mac a chooser's `NSMenu` pops up at `PositionArea.origin`, which reads `left` as the left column (`Mac/MenusMac.swift:447-455`, `PositionArea.swift:376`), and an `NSPopover` takes `.minX` (`Mac/PopoversMac.swift:292`).

## The failure

```
button popovertarget="where"
  text "Move"
column id="where" popover="auto" role="alertdialog" position-area="left"
  button press=toInbox popovertarget="where" popovertargetaction="hide"
    text "Inbox"
  button press=toArchive popovertarget="where" popovertargetaction="hide"
    text "Archive"
```

A chooser with no cancel is a popover on every size class (`MenuHost.style`, `MenuConfirmationIOS.swift:55`). On iOS it is permitted only an up arrow, so UIKit opens it under the button. Expected, as CSS and the Mac have it: to the button's left.

## Verification

`place` was copied into a bare Mac Catalyst process (the macOS 27.2 SDK's UIKit) and applied to a real `UIPopoverPresentationController` for all 22 `PositionArea` values. The four in the left column came out with `permittedArrowDirections == .up`:

```
left               iOS: below            macOS: left
left span-top      iOS: below            macOS: left
left span-bottom   iOS: below            macOS: left
left span-all      iOS: below            macOS: left
```

Of the other 18, 16 agree with the Mac's edge rule; `none` and `center` differ as LLP 1021 declares. `cargo test -p contract --test it position_area_places` passes, so the compiler admits the four values, and the Mac's `PositionAreaTests` pass, among them `left` placing a box at its anchor's left. Not run: the presented popover on a simulator (none runs on this machine).

## Direction

Add the case: `area.hasPrefix("left") ? .right`. `NativeContextsIOSTests` asserts the `none` and no-invoker cases beside it.

## Related

Closed `issues/closed/20261009-popover-css-flip-fallbacks.md` added these values to the vocabulary and the painted placement (`PositionArea.swift`); it did not touch UIKit's sheet. QUEUE's "The terminal, Linux and Windows place a popover without `position-area`'s region" is other hosts.
