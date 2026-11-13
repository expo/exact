# Four iOS XCTests fail on main: follow-end rest, render inventory, profiled coverage and a bar item's alert

**Status:** Open
**Systems:** host/apple UIKit presenter, host/apple/tests/ExactKitTests
**Severity:** P3
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

Four XCTests fail, each for its own reason as far as their messages show:

- `FollowEndAuthoredIOSTests.testWhereItRestsDecidesTheFollow`: "at the end, followed": the offset is 400, not 460.
- `UpdateRecorderIOSTests.testTheRenderInventoryNamesWhatCostsTheRenderServer`: "the scrolled blur at its content offset" is 500, not 1200.
- `ProfiledCoverageIOSTests.testIdenticalParagraphsShareOnlyLiveCoverageAcrossBatches`: "the worker carries its painted mask through settlement; publication does not repaint it": true, not false.
- `NavigationBarIOSTests.testABarItemShowingAnAlertByItsCommandAloneOpensIt`: the agent's tap is refused ("tap #224: UIView covers its middle") and Cancel does not dismiss the alert.

`NavigationBarIOSTests.testAHeadingsAuthoredTypographyIsTheBarsTitleFont` failed in the same run; it is QUEUE.md's entry of 2026-11-11.

## How to reproduce

At 69668888a on the async lane's Mac (Xcode 27.2, an iPhone 18 Pro simulator on iOS 27.0), run at a load average near 40:

```
bun host/apple/build.mjs --test --ios --only FollowEndAuthoredIOSTests,UpdateRecorderIOSTests,ProfiledCoverageIOSTests,NavigationBarIOSTests
```

The first three failed in the async lane's run at afde7b25e too; the bar item's alert test was not among its named failures. Not run on a second Mac.
