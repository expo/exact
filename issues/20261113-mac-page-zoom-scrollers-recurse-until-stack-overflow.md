# On macOS, a page zoom that leaves the viewport a fraction of a pixel too small shows and hides the page's scrollers until the stack overflows

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/Mac/PresenterMac.swift, host/apple/Sources/ExactKit/Mac/ExactViewMac.swift, host/apple/Sources/ExactKit/Mac/ZoomMac.swift
**Severity:** P1
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-mac-scroll-gutter-covers-recurse-until-stack-overflow.md, issues/20261113-ios-overflowing-tablist-recurses-until-stack-overflow.md

## What is wrong

Two faults combine.

**The size does not converge.** The kernel lays out in
`Presenter.viewportSize`, the page scroll view's `contentSize` over its
`magnification` (`PresenterMac.swift:586-589`). `setPageZoom` takes any
factor in range (`ZoomMac.swift:38-43`, `:55-61`); only the menu's steps compare with a
1e-9 tolerance. At a factor of `0.9999999999999997`, a 1512-point view gives
`1512 / 0.9999999999999997 = 1512.0000000000005` CSS px. A root that fills
the viewport then overflows the clip by under a pixel. The page scroll view
has both scrollers with `autohidesScrollers` (`PresenterMac.swift:176-178`),
so under legacy scrollers it shows them; the viewport becomes 1495 × 932, the
document fits, the scrollers hide, and the viewport is 1512 × 949 again.

**Each turn runs inside the previous one.** `ExactView.fit()` and
`syncInsets()` hand a closure to `Presenter.deferGeometry` while a batch is
being applied (`ExactViewMac.swift:263`, `:412`). The outermost `apply`'s
`defer` sets `applying = false` and then calls that closure synchronously
(`PresenterMac.swift:1079-1085`). `fit()` calls `ExactSession.resize`, whose
batch is applied by a new `Presenter.apply`; a `NodeView.setFrameSize` in it
calls `syncInsets`, which defers `fit()` again, and that apply's `defer` runs
it. Nothing returns between turns, so a size that never settles recurses
until the main thread's stack is exhausted.

## The failure

Two notes from one agent, on exact2 `e5ac601e1`, macOS. An app open and idle
crashed with `SIGSEGV` (stack guard), its stack repeating `Presenter.apply` /
`$defer #3 in Presenter.apply` / `closure #1 in ExactView.syncInsets` /
`ExactView.fit` / `ExactSession.resize` / `ExactSession.apply`. The agent then
found the trigger: the app steps its zoom by 0.1 up and back, reaching
`setPageZoom(0.9999999999999997)`, with System Settings' "Show scroll bars"
at Always (or Automatic with a mouse attached). A log in `fit()` showed the
size alternating 1512×949 and 1495×932; the overflow came about 8 s after
launch. The terminal that ran the app's `mac --run` showed exit 0
(`issues/20261113-mac-run-reports-exit-0-when-the-app-crashes.md`). The app
worked around it by rounding its zoom to a tenth.

Confirmed at `49ae6357` by reading the code named above and computing the
viewport width; not run.

## Fix

- Snap `pageZoom` to a level within 1e-9 of it, as `PageZoom.next` and
  `previous` already compare, or round `viewportSize` to the device pixel
  grid, so a factor of one does not make the viewport larger than the view.
- Run the deferred geometry after the batch without nesting: loop over
  `pendingGeometry` in the outermost `apply` until it is empty, or post it to
  the next main-queue turn, and bound the iterations so a size that does not
  converge logs once and stops instead of recursing.

Browsers settle a scrollbar that changes whether it is needed within one
layout; the page's own scrollers are AppKit's here, so the second item is the
general guard.
