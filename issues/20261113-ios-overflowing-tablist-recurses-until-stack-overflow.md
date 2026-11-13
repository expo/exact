# On iOS, a root tablist wider than the screen crashes the app at launch with nested applies through the navigation covers

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/IOS/NavigationCoversIOS.swift, host/apple/Sources/ExactKit/IOS/PresenterIOS.swift, host/apple/Sources/ExactKit/Session.swift
**Severity:** P1
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-mac-scroll-gutter-covers-recurse-until-stack-overflow.md, issues/20261113-mac-page-zoom-scrollers-recurse-until-stack-overflow.md

## What is wrong

An authored tab bar a few pixels wider than the screen makes the iOS host
nest applies on the main thread until its stack is exhausted; the crash
showed six, so either the covers never settle or each apply's frame is deep
enough that six suffice.

The nesting is the one the macOS covers loop has. `NavigationHost.reportCovers`
queues `deliverCovers` with `presenter.afterBatch`
(`NavigationCoversIOS.swift:262`); `deliverCovers` calls `presenter.onCovers`
(`:271-283`), which applies the kernel's answer (`Session.swift:788`, through
`whenIdle`, which runs it at once unless a fill is in flight); that batch's
outermost `Presenter.apply` runs its after-batch work synchronously in its
`defer` (`PresenterIOS.swift:825-842`), where the next report is delivered.
When the covers do not settle, nothing returns between turns.

Which cover alternates here is not known: the report gives the stack, not a
log of the covers.

## The failure

From one agent's diary (Rough), on exact2 `94b8671f7`, iOS. A root `tablist`
with `appearance: none`, `flex-direction: row`, `width: 100%` and `padding: 0
4px` (so, under `box-sizing: content-box`, 8 px wider than the screen) crashed
at launch: `EXC_BAD_ACCESS` (stack guard) in `NodeView.frame.getter` ←
`NodeView.textRasterGeometryChanged` (`TextRasterIOS.swift:453`) ←
`Presenter.frame` (`PixelGridIOS.swift:171`) ← `applyGeometry`, with
`ExactSession.apply` nested six deep through
`NavigationHost.reportCovers`/`deliverCovers`. `box-sizing: border-box` fixed
it; each of the properties alone was fine. The frames at the top are where the
stack ran out, not the cause.

Confirmed at `49ae6357` by reading the delivery path above; not run on a
simulator, so the alternating value is unconfirmed.

## Fix

- Reproduce on a simulator with the four properties above and log
  `deliverCovers`' changes to find the value that does not converge.
- Deliver covers after the batch without nesting, with a bound, on both
  Apple presenters: one fix for this and the two macOS loops.
