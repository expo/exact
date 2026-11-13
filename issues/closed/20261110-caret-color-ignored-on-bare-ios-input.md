# `caret-color` on an `input` with `appearance="none"` is ignored on iOS

**Status:** Closed
**Resolution:** A bare `input` takes its `caret-color` as its field's tint, set by its style and kept by every props write, as a bare `textarea` does.
**Systems:** host/apple/Sources/ExactKit/IOS/NativeFieldsIOS.swift (applyField, styleNativeField)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

On iOS `caret-color` is the editor's tint (LLP 1104, the fields table; LLP 1001 §1: "UIKit's editor tint colours both insertion carets and native selection handles"). For a `textarea`, `styleTextArea` sets it for a native and a bare one alike (`host/apple/Sources/ExactKit/IOS/TextAreaIOS.swift:567`):

```swift
f.tintColor = isNativeTextControl ? caretColor ?? ownTintColor("accent_color") : caretColor
```

For an `input`, the only line that reads `caretColor` is inside `styleNativeField`'s native branch (`host/apple/Sources/ExactKit/IOS/NativeFieldsIOS.swift:236-238`):

```swift
if isNativeTextControl {
    field.defaultTextAttributes[.kern] = number("letter_spacing")
    field.tintColor = caretColor ?? ownTintColor("accent_color")
```

`isNativeTextControl` is false under `appearance="none"` (lines 168-170). The one other write to the field's tint sets it to nil at every props write (line 58: `f.tintColor = props["emojiPicker"] == "true" ? .clear : nil`). A bare input's caret is therefore always the tint it inherits, "the first nondefault tint color value in the view's hierarchy" (Apple, `UIView.tintColor`).

The Mac applies the colour to every field's editor (`Mac/NativeFieldsMac.swift:167`, `Mac/NodeViewMac.swift:1226`), and `CaretColorMacTests` asserts it for this node: `"style": ["appearance": "none", "caret_color": green]`, "the author's `caret-color`". The web writes the declaration (CSS UI 4, `caret-color`).

## The failure

```
input appearance="none" caret-color="#ff3b30" value=name input=setName
textarea appearance="none" caret-color="#ff3b30" value=note input=setNote
```

On iOS the textarea's caret is red and the input's is the app's accent. Expected: both red, as on the web and the Mac.

## Verification

Read: the presenter, which does not build for the Mac test target. In `host/apple/Sources/ExactKit` outside `Mac/`, lines 58 and 238 are the only writes to an authored input's `UITextField` tint (the bar's search field has its own, `NavigationBarIOS.swift:99`), and `caret_color` is read only through `caretColor` (`TextAreaIOS.swift:455`). No iOS test names `caret_color`. `contract build` accepts the two lines above.

Run: UIKit's part, as a bare Mac Catalyst process (the macOS 27.2 SDK's UIKit; no iOS simulator runs on this machine). A `UITextField` whose `tintColor` was set to nil under a green parent reported the parent's green, and `UIStandardTextCursorView`, UIKit's caret view, was drawn in its parent's tint with none of its own and in its own when given one:

```
no tint of its own, parent green: drawn rgb(0,128,0)
own tint red, parent green: drawn rgb(255,0,0)
```

The check that would settle it: on a simulator, focus each of the two controls above and compare the carets.

## Direction

Give a bare input its `caretColor` as the textarea has it: set it in `styleNativeField` outside the native branch, and have line 58 restore it where it now writes nil.

## Related

None found in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md` for `caret-color`. No app in this repository writes `caret-color` on an input.
