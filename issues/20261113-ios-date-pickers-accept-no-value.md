# iOS date pickers under a focus handler accept no value

**Status:** Open
**Systems:** host/apple UIKit presenter (date and time inputs), host/apple/tests/ExactKitTests
**Severity:** P2
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

Four tests of `DatesIOSTests` fail; in each the picker sends no value where one is expected:

- `testOpeningAnEmptyPickerAcceptsItsInitialValueAndCanRepeatAfterClearing`: `[]` is not `["2026-11-12"]`, and the field's text is `""`, not `2026-11-12`.
- `testAFocusHandlerDisablingOrMakingThePickerInertLeavesItEmpty`: `[]` is not `[2]`.
- `testAFocusHandlerReplacingThePickerDoesNotAcceptTheDetachedPickersValue`: `[]` is not `[1]`.
- `testAFocusHandlerGivingThePickerTheAuthorsLookLeavesItEmpty`: a `UIWindow` is still presented where the test expects none.

## How to reproduce

At 69668888a on the async lane's Mac (Xcode 27.2, an iPhone 18 Pro simulator on iOS 27.0), run at a load average near 40:

```
bun host/apple/build.mjs --test --ios --only DatesIOSTests
```

The same four failed in the async lane's run at afde7b25e. Not run on a second Mac.
