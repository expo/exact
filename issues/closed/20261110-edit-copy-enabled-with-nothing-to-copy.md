# macOS: Edit ▸ Copy is enabled with a node focused, no text selected and no `copy` handler

**Status:** Closed
**Resolution:** `NodeView` and `ExactView` validate Edit ▸ Copy: enabled while text is selected or a `copy` handler hears it (from the focused node up, or at the first root with nothing focused), as iOS's `canPerformAction` and `RootEdits` already answered.
**Systems:** host/apple/Sources/ExactKit/Clipboard.swift, host/apple/Sources/ExactKit/Mac/ExactViewMac.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

Edit ▸ Copy is `copy:` with no target (`DevMenuMac.swift:220`), so AppKit enables it when the first object in the responder chain that responds to `copy:` has no `validateMenuItem(_:)` or answers true ("Enabling Menu Items", Application Menu and Pop-up List Programming Topics).

Every node declares `NodeView.copy(_:)` (`Clipboard.swift:98`). `responds(to:)` (`Clipboard.swift:104-108`) withholds cut and paste while no node hears them, but not copy, and `NodeView` has no `validateMenuItem`. So any node that is first responder enables Copy. Unheard, the action is `TextSelection.copy()`, which does nothing when no text is selected (`TextSelectionMac.swift:773-778`).

`ExactView`, above every node in the chain, would answer the same way: `copy(_:)` at `ExactViewMac.swift:22`, and `default: true` in its `validateMenuItem` (`ExactViewMac.swift:463-470`).

## The failure

With no field being edited, click once on a paragraph (`TextSelection.press` makes it first responder, `TextSelectionMac.swift:327-332`), or Tab to a button with Keyboard Navigation on. Edit ▸ Copy is enabled and does nothing. Expected: Copy dimmed until text is selected or a node from the focus up hears `copy`, as AppKit's `NSTextView` dims it for an empty selection. The other paths already hold this rule: iOS's `canPerformAction` answers for a selection or a `copy` hearer (`Clipboard.swift:139-143`), and on the Mac `RootEdits.validateMenuItem` refuses Copy with no handler while nothing is focused (`Clipboard.swift:200`).

## Verification

A temporary XCTest booted a page in a session and, for each first responder, ran `NSMenu.update()` on a Copy item aimed at the first responder in the window's chain that answers `copy:`:

```
nothing focused (the window): no responder answers
one click on a paragraph: NodeView, selected text "": enabled true
a button focused: NodeView, selected text "": enabled true
AppKit's NSTextView (selectable, not editable), nothing selected: enabled false
AppKit's NSTextView, five characters selected: enabled true
```

Not run: the menu bar's own Copy. AppKit finds its target from the key window, which the test process could not have.

## Direction

Answer `copy:` as iOS does. Give `NodeView` a `validateMenuItem` (or withhold `copy:` in `responds(to:)`, as for cut and paste) that is true for `clipboardTarget(copy) != nil` or a non-empty `selection.selectedText()`, and have `ExactView.validateMenuItem` answer `rootEdits.hearer(copy) != nil || !selection.selectedText().isEmpty` for `copy:`.

## Related

`issues/closed/20261011-macos-list-copy-unselectable-text.md` and `issues/closed/20261012-list-copy-control-labels.md` concern what a copy holds, not whether the item is enabled.
