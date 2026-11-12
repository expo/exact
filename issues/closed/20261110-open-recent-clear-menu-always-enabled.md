# macOS: Open Recent ▸ Clear Menu is enabled when the list is empty

**Status:** Closed
**Resolution:** `RecentMenu` validates Clear Menu, enabled only while `KeptDocuments.recent` holds a document, as `NSDocumentController` refuses `clearRecentDocuments:` with an empty list.
**Systems:** host/apple/Sources/ExactKit/Mac/DevMenuMac.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`RecentMenu.menuNeedsUpdate` (`host/apple/Sources/ExactKit/Mac/DevMenuMac.swift:69-87`) draws File ▸ Open Recent from `NSDocumentController.recentDocumentURLs` and always ends it with

```swift
menu.addItem(withTitle: "Clear Menu", action: #selector(clear(_:)), keyEquivalent: "").target = self
```

`RecentMenu` implements `clear(_:)` and no `validateMenuItem(_:)`, so AppKit enables the item whenever its target answers the action, which is always ("Enabling Menu Items", Application Menu and Pop-up List Programming Topics). The type's comment says the menu is AppKit's own list "drawn here with public API"; AppKit's menu dims Clear Menu when there is nothing to clear.

## The failure

Launch an app that declares `file_handlers` and has opened nothing, or choose Clear Menu once, and open File ▸ Open Recent. Observed: one item, Clear Menu, enabled. Choosing it runs `KeptDocuments.clear()`, which tells every keeping session its list changed though nothing did. Expected: Clear Menu dimmed, as in an `NSDocument` app, where `NSDocumentController` refuses `clearRecentDocuments:` while the list is empty. A person who knows the platform notices an enabled command that does nothing (Human Interface Guidelines, Menus).

## Verification

A temporary XCTest built the menu bar with `DevMenu.makeMenu(shortcuts:documents:)`, had Open Recent's delegate fill it and called `NSMenu.update()`:

```
NSDocumentController has 0 recent documents; Open Recent's items ["Clear Menu"]; Clear Menu enabled after NSMenu.update() true
NSDocumentController validates its own Clear Menu false
```

A standalone AppKit tool asked `NSDocumentController.validateUserInterfaceItem` about an item whose action is `clearRecentDocuments:` as its list changed: false with no recent document, true with one, false after a clear.

## Direction

Give `RecentMenu` a `validateMenuItem` that answers `!KeptDocuments.recent.isEmpty` for `clear(_:)`; that list also covers a clear on its way (`KeptDocumentsMac.swift:75`).

## Related

`issues/` and `QUEUE.md` have nothing on Open Recent's Clear Menu. `KeptDocuments.clear` itself (LLP 1069.010.000 D6) is not in question.
