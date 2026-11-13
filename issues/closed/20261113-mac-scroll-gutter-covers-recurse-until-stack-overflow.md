# On macOS, a scroll box whose legacy scrollers change whether they are needed reports its gutter on and off until the stack overflows

**Status:** Closed
**Resolution:** A scroll box reports a legacy gutter only on an axis that overflows with the other scroller hidden and keeps a gutter whose own layout made its content fit, and covers reported during a delivery are delivered after it returns, at most eight times in a row.
**Systems:** host/apple/Sources/ExactKit/Mac/ChainingScrollView.swift, host/apple/Sources/ExactKit/Mac/CoversMac.swift, host/apple/Sources/ExactKit/Mac/PresenterMac.swift, host/apple/Sources/ExactKit/Session.swift
**Severity:** P1
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-mac-page-zoom-scrollers-recurse-until-stack-overflow.md, issues/20261113-ios-overflowing-tablist-recurses-until-stack-overflow.md

## What is wrong

Two faults combine.

**The gutter does not converge.** A `scroll` node's `ChainingScrollView` has
both scrollers with `autohidesScrollers` (`NodeViewMac.swift:1160-1162`).
`tile()` reports the width of each visible legacy scroller to the kernel as a
cover, `.gutter(right:bottom:)` (`ChainingScrollView.swift:86-98`). When the
content is about as tall as the box, the vertical scroller shows, `tile`
reports a 17-point gutter, and the kernel lays the content out in the
narrower box, where it fits; AppKit hides the scroller, `tile` reports
`gutter(0, 0)`, and the content overflows again. Browsers settle this within
one layout pass. Both gutters are reported as 17 although only the vertical
axis overflows: the clip narrowed by the vertical scroller makes the content
overflow horizontally until the kernel lays it out again.

**Each report is delivered inside the previous one.** `HostCovers.report`
queues `deliver` with `presenter.afterBatch` (`CoversMac.swift:26-35`).
`deliver` calls `presenter.onCovers`, which applies `runtime.covers(...)`
(`Session.swift:788`); that batch's outermost `Presenter.apply` runs its
after-batch work synchronously in its `defer`
(`PresenterMac.swift:1079-1090`), where the next `tile` report is delivered.
Nothing returns between turns, so the alternation recurses until the main
thread's stack is exhausted.

## The failure

Two notes from two agents, on exact2 `e5ac601e1` and `4ad9f6687`, macOS. An
app crashed three times in a day with "Thread stack size exceeded due to
excessive recursion", the stack repeating `HostCovers.deliver` →
`Presenter.onCovers` → `ExactSession.wire` closure → `whenIdle` → `idle` →
`applyUnlessEmpty` → `ExactSession.apply` → `Presenter.apply` → its `defer` →
`HostCovers.deliver`; the first followed View ▸ Zoom In. The second agent
found a cause: "Show scroll bars" at Always (or Automatic with a mouse), and a
`scroll` box (`flex: 1`, `min-height: 0` in a column) whose content is about
as tall as the box. A log in `deliver` showed the node's gutter alternating
17 and 0 on both axes until the crash, minutes after launch with no input.
The agent worked around it with `scrollbar-width: none` on the app's
scrollers.

Confirmed at `49ae6357` by reading the code named above; not run.

## Fix

- Keep a gutter once claimed for the rest of a layout pass, as browsers do
  for `overflow: auto`, or let a box that overflowed only because of its own
  scroller keep the scroller, so the report converges.
- Report a gutter only on an axis the node scrolls and whose content
  overflows without the other scroller.
- Deliver covers after the batch without nesting, with a bound, as the same
  change should do for deferred geometry
  (`issues/20261113-mac-page-zoom-scrollers-recurse-until-stack-overflow.md`).
