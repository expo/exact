# `Presenter.interacting` is written at every press and read by nothing

**Status:** Closed
**Resolution:** `Presenter.interacting` and its four writes are gone, and the dialog tests' press helper asserts what ends a contact instead: no bubbled press and no held pointer.
**Systems:** host/apple/Sources/ExactKit/Mac/MouseEventsMac.swift, host/apple/Sources/ExactKit/Mac/MouseChainMac.swift, host/apple/Sources/ExactKit/Mac/PresenterMac.swift, host/apple/Sources/ExactKit/IOS/PresenterIOS.swift, host/apple/tests/ExactKitTests/DialogMacTests.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

Both presenters declare `var interacting: UInt32 = 0` (`Mac/PresenterMac.swift:757`, `IOS/PresenterIOS.swift:608`). On the Mac a left press sets it to the pressed node's id and the release clears it:

- `MouseEventsMac.swift:232` (`presenter?.interacting = id`)
- `MouseChainMac.swift:126` (a bubbled press)
- `MouseEventsMac.swift:302` and `:310` (reset on the up)

No source file reads it, and on iOS nothing writes it. What a list needs of a press goes through `CollectionHost.pointerDown` and `releaseInteractionLater`, which keep their own `interaction`.

Its only readers are three assertions that it is 0 after an up, in the test helper `assertPresentationPresses` (`host/apple/tests/ExactKitTests/DialogMacTests.swift:1356`, `1385`, `1395`), which `DialogMacTests`, `DialogSheetMacTests`, `PopoverMacTests` and `PopoverNativeMacTests` call. They hold a field nothing uses, and read as if they held a contact's end.

## Verification

With the two declarations and the four writes deleted, `bun host/apple/build.mjs --test --only DialogMacTests` compiled every source target and failed on those three lines alone:

```
DialogMacTests.swift:1356:26: error: value of type 'Presenter' has no member 'interacting'
DialogMacTests.swift:1385:26: error: value of type 'Presenter' has no member 'interacting'
DialogMacTests.swift:1395:26: error: value of type 'Presenter' has no member 'interacting'
```

With each replaced by `XCTAssertTrue(p.mouseChain.bubbledPress == nil && p.pointerHeld == nil)`, `DialogMacTests` (38 tests), `DialogSheetMacTests` (38) and `PopoverMacTests` (41) passed, as did the `PopoverNativeMacTests` case that calls the helper. The Mac build does not compile the iOS declaration; `git grep -n interacting -- '*.swift'` finds no other use of the property.

## Direction

Delete the two declarations and the four writes, and have the helper assert what ends the contact: `p.mouseChain.bubbledPress == nil` and `p.pointerHeld == nil`.

## Related

No issue or QUEUE entry names it.
