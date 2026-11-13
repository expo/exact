# The guide and LLP 1115.000 D7.2 disagree on whether a `select` is a Tab stop with Keyboard Navigation off on the Mac; the code follows the guide

**Status:** Closed
**Resolution:** With Keyboard Navigation off a `select` is no Tab stop, as `NSPopUpButton` is none: its pop-up and the presenter's loop now ask `tabStop`, as a button does, while date and time inputs stay stops in both modes and an author's `tabindex` still makes any of them one (Charlie, 2026-11-12).
**Systems:** docs/contract-for-agents.md, host/apple/Sources/ExactKit/Mac/ValueControlsMac.swift, host/apple/Sources/ExactKit/Mac/FocusMac.swift, host/apple/Sources/ExactKit/Mac/KeyViewLoopMac.swift, host/apple/tests/ExactKitTests/KeyboardNavigationMacTests.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`. Four records answer the question two ways.

A stop whatever the setting:

- The guide, `docs/contract-for-agents.md:1699-1700`: "On macOS, date, time, datetime-local and select controls are default Tab stops, independent of the system's Keyboard navigation setting."
- The code's comment, `ValueControlsMac.swift:126-127`: "The pop-up itself receives focus and native editing keys, even with macOS Keyboard navigation off".

AppKit's own answer when the setting is off:

- LLP 1115.000 D7.2: "Tab honours Keyboard Navigation (`NSApp.isFullKeyboardAccessEnabled`): off, it moves between text fields and lists; on, it reaches every control."
- The resolution of `issues/closed/20261009-macos-controls-default-tab-order.md`: "With it off, whether each is a stop is its own AppKit control's answer, as for any `NSDatePicker` or `NSPopUpButton`, and no longer "as on the web"." `KeyboardNavigationMacTests.swift:81-84` repeats it and asserts only the setting on.

The code follows the guide. The select's `NSPopUpButton` answers `canBecomeKeyView` as `acceptsFirstResponder && owner?.tabbable == true` (`ValueControlsMac.swift:136`), `NodeView.tabbable` is true for `isDateOrSelect` (`FocusMac.swift:21-26`), and `Presenter.tabbable` returns true for it before it reads the setting (`KeyViewLoopMac.swift:227`). A native button asks `tabStop`, which reads the setting (`NativeButtonsMac.swift:212`, `KeyViewLoopMac.swift:40-43`). So the test's comment ("its own AppKit control's answer, which follows the machine's setting") is false of the code.

A date or time input is not in question: AppKit's `NSDatePicker` is a stop with the setting off, so both readings agree.

## The failure

With Keyboard Navigation off (the system default), a form of `input`, `select`, `input`: Tab from the first field stops at the select. In a hand-built AppKit window it goes to the second field, because `NSPopUpButton` is no key view there. By the test in `AGENTS.md` ("would someone who knows the platform notice?") a stop the author did not write is the platform's to decide. The decision the closed issue records (2026-10-08, "Put date, time and select controls in the default Tab order") is a day older than LLP 1115.

## Verification

A temporary XCTest on a Mac with Keyboard Navigation off, a page of input 2, date 3, select 5, input 6:

```
AppKit's NSPopUpButton canBecomeKeyView false; AppKit's NSDatePicker canBecomeKeyView true
Exact, Keyboard Navigation off: stops [2, 3, 5, 6]; the select's canBecomeKeyView true
Exact, Keyboard Navigation on: stops [2, 3, 5, 6]
Keyboard Navigation off, Tab from the date: first responder is the select true
```

## Direction

D7.2 holds: with Keyboard Navigation off a `select` is no Tab stop, as `NSPopUpButton` is none (Claude, 2026-11-11; three models asked separately each chose it; no person has ruled). The author wrote a `select`, not a stop, so the stop is the platform's to decide, and a `tabindex` the author writes still makes it one (`KeyViewLoopMac.swift:34`).

The select's control answers `acceptsFirstResponder && owner?.tabStop == true`, and `Presenter.tabbable` tests a select as it tests a button. The guide's sentence and the comment at `ValueControlsMac.swift:126` change, and `KeyboardNavigationMacTests` asserts the setting off as well as on. A date or time input stays a stop in both modes, as `NSDatePicker` is. LLP 1115.000's status row for D7.2 ("Date, time and select are skipped in both modes") is true of neither mode and is corrected.

## Related

`issues/closed/20261009-macos-controls-default-tab-order.md` (PR #327) put the three controls in the loop; this is the setting-off case its later note describes.
