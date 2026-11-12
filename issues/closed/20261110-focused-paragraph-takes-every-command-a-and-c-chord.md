# A focused paragraph takes every ⌘ chord on A or C as Select All or Copy

**Status:** Closed
**Resolution:** A focused paragraph's Select All and Copy, and a text region's Copy, take ⌘A and ⌘C only with Command the one modifier held (`NodeView.isCommandAlone`, the test `isCommandA` already made), so ⇧⌘C, ⌥⌘A and the rest reach the menu item that owns them.
**Systems:** host/apple/Sources/ExactKit/Mac/NodeViewMac.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

A paragraph that is first responder (the person clicked selectable text) answers Select All and Copy itself, in `keyDown` and in `performKeyEquivalent` (`host/apple/Sources/ExactKit/Mac/NodeViewMac.swift:286-292` and `:333-339`, at `dd6f5dcd`):

```swift
if isParagraph, window?.firstResponder === self, event.modifierFlags.contains(.command) {
    switch event.charactersIgnoringModifiers?.lowercased() {
    case "a": presenter?.selection.selectAll(); return true
    case "c": presenter?.selection.copy(); return true
```

The test is that ⌘ is among the modifiers, not that it is the only one, and `charactersIgnoringModifiers` keeps the letter under Shift, Option and Control. So ⇧⌘A, ⌥⌘A, ⌃⌘A, ⇧⌘C, ⌥⌘C and ⌃⌘C all match. The same file has the exact rule for a field: `isCommandA` (`:352-356`) masks the device-independent flags and requires `mods == .command`. The two disagree on any chord with a second modifier.

## The failure

With page text focused and some selected:

- ⇧⌘C, ⌥⌘C or ⌃⌘C replaces the pasteboard with the selection, and ⇧⌘A or ⌥⌘A selects all. An `NSTextView` hears Copy and Select All from the Edit menu's items, which answer ⌘C and ⌘A alone.
- AppKit offers a key equivalent to the key window's views before the menu bar, so a menu item that owns the chord never runs. Exact's own menu has none on these chords, and a chord an app declares with `aria-keyshortcuts` is taken earlier by the session's key monitor (`Presenter.routeKey`, `KeyEvents.swift:226`). An embedder's menu (LLP 1031), or an item the person bound in System Settings, is what loses: "Copy Link ⇧⌘C" or "Deselect All ⇧⌘A" stops working while page text has the focus.

Expected: only ⌘A and ⌘C, as `isCommandA` tests it.

## Verification

A temporary XCTest booted a page of paragraphs in a session, clicked one (it became first responder), selected text and sent each chord with `NSWindow.performKeyEquivalent`, the pasteboard holding `sentinel` beforehand:

```
Shift-Command-C     taken: true    pasteboard: the selection
Option-Command-C    taken: true    pasteboard: the selection
Control-Command-C   taken: true    pasteboard: the selection
Shift-Command-A     taken: true
Option-Command-A    taken: true
Command-B           taken: false   pasteboard: sentinel
```

With a menu item "Copy Link" on ⇧⌘C, asked after the window as AppKit asks: the views took the chord and the item did not fire; with no paragraph focused the views declined and it fired. An `NSMenu` holding Copy on ⌘C and Select All on ⌘A declined all five chords. Not run: the chord through `NSApp.sendEvent` in a key window, which the test process could not have.

## Direction

Use one test for both: the device-independent modifiers, less Caps Lock, the numeric pad and Fn, equal to `.command`, as `isCommandA` has it, with a sibling for C.

## Related

Nothing in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md` names these chords. `issues/closed/20261009-field-selection-beforeinput-range-edit.md` is about fields.
