# On iOS a route under `navigationBar="transparent"` has no top inset it can read: `env(safe-area-inset-top)` is 0

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/IOS/ExactViewIOS.swift, host/apple/Sources/ExactKit/IOS/NavigationBarIOS.swift, LLP 1123 D9, docs/contract-for-agents.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261019-scroll-inset-under-transparent-bar.md

## What is wrong

`navigationBar="transparent"` sets the route's content scroller to
`contentInsetAdjustmentBehavior = .never` (`NavigationBarIOS.swift:963`), so the content starts at the screen's top edge, under the status bar
and the bar. The route then has no number to pad a control below the status
bar with:

- `env(safe-area-inset-*)` is the document's inset, and the document's inset is
  0 under the default `viewport-fit="auto"`, even when a navigation stack takes
  the whole screen (`ExactViewIOS.fit`, `var insets = cover ? safe : .zero`,
  `ExactViewIOS.swift:404`). This is deliberate (LLP 1123 D1, table row C1) and
  documented in `docs/contract-for-agents.md` and in `docs/agent-pitfalls.md:591`
  ("A banner at the root of a tabbed app stays under the status bar on iOS").
- The value an owner is meant to pad with, LLP 1123 D9's
  `var(--exact-safe-area-inset-top)`, is accepted but not built.

So the rule that `env()` stays 0 is deliberate, but under a transparent bar it
leaves the author nothing to read. The closed issue that added the value says
"the author pads with `env(safe-area-inset-top)` where content must clear it",
which holds only under `viewport-fit="cover"`, and the guide's paragraph on
`navigationBar="transparent"` (`docs/contract-for-agents.md:2151-2156`) does not mention the
inset at all.

## The failure

A route with `navigationBar="transparent"` drew a full-bleed picture from the
screen's top edge. The author wanted a control just below the status bar and
read `env(safe-area-inset-top)`: it was 0, so the control went under the status
bar and Dynamic Island, and the author used a hard-coded number. Seen on exact2
`9e53578f2`, on an iPhone with a Dynamic Island and in the iOS 27.2 simulator.
Other insets were not checked.

Checked by reading the code at `49ae6357`; not run on a simulator. Whether the
report's root said `viewport-fit="cover"` is not known; under `cover` the
inset is the device's, but `cover` also hands every other route's clearance to
the author (`routeEdges`, `NavigationCoversIOS.swift:71-79`), which is too high a price for one
screen.

## Fix

Build LLP 1123 D9's `var(--exact-safe-area-inset-top)` for a route's content
owner, at least for a route whose bar is transparent, where the host clears
nothing and the author must. Until then, say in the guide's
`navigationBar="transparent"` paragraph and in `contract vocab navigationBar`
that the route's top inset is not readable under `viewport-fit="auto"`, and
correct the closed issue's line.
