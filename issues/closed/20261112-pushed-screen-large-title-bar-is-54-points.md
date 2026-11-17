# On iOS 27 a pushed screen's large-title bar is 54 points tall; PushedTitleIOSTests fails 6 of 13

**Status:** Closed
**Resolution:** A header lifted in a batch now holds every stack change until the runtime's answer to its cover is applied, however many syncs the batch runs, and a route scroller's clearance no longer counts the top cover it is giving up, so all 13 PushedTitleIOSTests pass on iOS 27.0.
**Systems:** host/apple UIKit presenter (navigation bar, large titles), host/apple/tests/ExactKitTests/PushedTitleIOSTests.swift
**Severity:** P3
**Author:** Claude (found while verifying docs/agent-pitfalls.md)
**Date:** 2026-11-12

## What is wrong

`bun host/apple/build.mjs --test --ios --only PushedTitleIOSTests` on an iPhone 17
simulator running iOS 27.0 fails 6 of its 13 tests at `a8922c5f`: a pushed
screen's navigation bar is 54 points tall where the tests expect 106, so its
large title is collapsed when the screen opens. docs/agent-pitfalls.md tells app
authors the title opens collapsed, which matches what runs and not what the tests
and LLP 1115.002 expect.

## Direction

Find whether iOS 27 changed when a pushed `UINavigationItem`'s
`largeTitleDisplayMode` expands (compare a hand-built UIKit push on the same
simulator, per AGENTS.md's tell test), or whether a change on main collapsed it.
Fix the presenter, then drop the pitfall.

## Found (2026-11-12)

Not iOS 27: a hand-built UIKit push of a `.always` controller arrives at 106 points
on the same simulator (iPhone 17 Pro, iOS 27.0), with and without a window scene, a
tab controller, animation or a scroll view. `8556bd73` moved the navigation sync
inside the batch, so the cover of a header the bar lifts reached the runtime after
UIKit had been given the stack: as the push was laid out the route's scroller was
still its header's height below the controller's top (UIKit's inset 67 points,
where 168 opens the title), the failure LLP 1115.000's row for D11.1 measured.
Putting the sync and the scroll restore back after the batch, as before `8556bd73`,
made all 13 tests pass, and would undo that commit's fix.

`sync` now holds a stack change whose header was lifted in the batch being applied
until the runtime's answer to the cover has been applied (`syncAfterBatch`), and 12
of the 13 pass. What is left: in
`testEveryKindOfLevelOneRouteArrivesExpandedAndALevelTwoRouteIsInline`, the `plain`
route (a plain list whose rows are built in slices) still arrives at 54 points.
Waiting for the session to be idle (`Session.whenIdle`, which holds the covers'
answer while a slice is in flight) before the held sync did not change it.

## Fixed (2026-11-13)

Two causes, both Exact's; the tests were right (a hand-built push arrives at
106 points, above).

- The `plain` route: its batch synced twice. The first sync lifted the
  header and held the push; the second found the header already lifted and
  gave UIKit the stack with the list still 49 points down (UIKit's inset
  67). A lift in a batch now holds every stack change (`liftHeld`) until
  just before the runtime's answer to its cover is applied, in the
  session's order of answers (a slice in flight first), so the answer's
  sync pushes; one the answer did not sync is pushed after it
  (`liftAnswered`). Pushing from an empty sync after the answer instead
  left a pushed route's bottom cover unreported
  (`NavigationBasicsIOSTests`).
- `testATabSwitchAContentSwapAndAReplaceKeepIt` (168 where 0), since
  `53d567920`: when the reason above `swap-first` went, the scroller's
  clearance (`clearanceExtra`) was measured while it still sat below the
  route's top cover, so it added 168 points to UIKit's own 168. Clearance now
  leaves out the top cover last reported.
