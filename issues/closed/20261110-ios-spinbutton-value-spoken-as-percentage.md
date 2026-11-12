# iOS speaks a `spinbutton`'s value as a percentage of its range

**Status:** Closed
**Resolution:** A `spinbutton` speaks its number (or `aria-valuetext`) on iOS and has no bounds it does not write, on the Mac too, and the guide says so.
**Systems:** host/apple/Sources/ExactKit/Accessibility.swift (rangeNumbers, rangeValueText), host/apple/Sources/ExactKit/IOS/NodeViewIOS.swift, docs/contract-for-humans.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

A drawn range role's value on iOS is `rangeValueText` (`host/apple/Sources/ExactKit/IOS/NodeViewIOS.swift:1177-1181`): `aria-valuetext`, else `aria-valuenow` as a percentage of the span from `aria-valuemin` to `aria-valuemax`, with 0 and 100 where a bound is unsaid (`host/apple/Sources/ExactKit/Accessibility.swift:254-267`). That is how UIKit's slider and progress view speak, and it suits `progressbar`, `slider`, `meter` and `scrollbar`.

`spinbutton` is in the same set (`Accessibility.swift:250`). A spin button's value is the number itself: a quantity of 3 is "3", wherever 3 sits between its bounds. ARIA gives the role no default bounds either: for `spinbutton`, "Default for aria-valuemin is that there is no minimum value" and "Default for aria-valuemax is that there is no maximum value" (WAI-ARIA 1.2), where a `slider`'s defaults are 0 and 100. WebKit on iOS answers the role's value as `aria-valuetext`, else the number (`WebAccessibilityObjectWrapperIOS.mm`, `accessibilityValue`: a range control's `valueForRange`, and `isRangeControl` includes `SpinButton`).

`docs/contract-for-humans.md:1074-1078` states the percentage and "ARIA's 0 and 100 where a bound is unsaid" for all five roles; for `spinbutton` both are wrong.

## The failure

```
row gap=8 align-items="center"
  button press=less aria-label="Fewer"
    text "−"
  text `${qty}` role="spinbutton" aria-label="Quantity" aria-valuenow=qty aria-valuemin=1 aria-valuemax=99
  button press=more aria-label="More"
    text "+"
```

With `qty` 3, the element's `accessibilityValue` on iOS is "2%", which VoiceOver reads after the label. With no bounds written it is "3%". Expected "3" in both. The Mac gives AppKit the value and its bounds, and reads the number.

## Verification

Run on the Mac build of ExactKit, which compiles `rangeValueText`: a temporary XCTest (`bun host/apple/build.mjs --test --only <class>`) booted the Contract above, with a second `spinbutton` that writes no bounds and a `slider` at 30, and read each node:

```
qty 3   bounded: now 3 min 1 max 99, rangeValueText 2%; the Mac's value 3, min 1, max 99
qty 3   no bounds: now 3, rangeValueText 3%; the Mac's value 3, min 0, max 100
        slider: now 30, rangeValueText 30%
qty 50  bounded: rangeValueText 50%; no bounds: rangeValueText 50%
```

The iOS line that writes it to `accessibilityValue` was read, not run: no simulator runs on this machine, so VoiceOver's speech was not heard.

## Direction

For `spinbutton`, answer the number (`aria-valuetext` first, as now) and give it no default bounds, on the Mac too; correct the guide's sentence.

## Related

`issues/closed/20261009-macos-role-modal-accessibility.md` landed the range roles (LLP 1116 D8) and tests the Mac's value, minimum and maximum. Nothing open names the role.
