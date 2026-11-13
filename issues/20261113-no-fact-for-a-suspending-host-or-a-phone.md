# An app cannot tell whether its host suspends it when hidden, or whether it runs on a phone

**Status:** Open
**Systems:** runner reserved sources (`exactViewport`, `exactPage`, `exactSurface`), docs/contract-for-agents.md ("Platform facts")
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

Contract has no fact naming the kind of host or device (phone, tablet,
desktop, browser). That is deliberate. LLP 1023 D10 lands platform
divergence in order: the presenter first, then a branch on capability ("not
platform, where possible"), and per-platform sources never.
`docs/contract-for-agents.md` ("Use dimensions, media preferences, page
facts, and capability state rather than suffixing files by platform") says
the same, and a new host fact goes through `exactSurface(name)` before any
new reserved source (LLP 1101.002 §0 P11). The facts there today are the
viewport's size, `displayMode`, `pointer` and `hover`, and `exactPage`'s
`visibilityState`, `hasFocus`, `persisted` and the rest.

Two reports show a cost the capability facts do not cover:

- **Whether hiding suspends the app.** A live feed should close at once when
  a phone app is hidden, since the OS will suspend it, and stay open for
  minutes when a desktop window is hidden. `visibilityState` says the app is
  hidden, not whether the host is about to stop it. `pointer == "coarse"`
  stands in for "phone", and is wrong for an iPad with a trackpad, where it
  reads `fine`.
- **A navigation the host should choose.** An app wanted a phone's bar and
  sheets on iPhone and a side panel on the Mac, whatever the window's width.
  With no fact it wrote a flag into the source from a build script per
  target: the per-platform source D10 rules out, made by hand. For the
  common case the presenter already chooses: a root `tablist` is UIKit's tab
  bar on iOS and the window's sidebar on the Mac (`docs/contract-for-agents.md`,
  the tablist section). Whether that covered this app's structure was not
  checked.

## The failure

Both reports are on exact2 `9e53578f2`: one from an app holding a feed open
on macOS, one from an app built for iPhone and the Mac.

## Fix

Keep to capability. The first case is a capability: a page fact such as
`suspendsWhenHidden` (true on iOS, iPadOS, Android and watchOS; false on
desktop hosts and the web) answers it without naming a platform. For the
second, first check whether the root `tablist`'s presenter, or a route's
`navigationPresentation`, already gives the host's navigation, and say so in
the guide where it discusses adapting to the device. If a device-idiom fact
is still needed after that, publish it through `exactSurface` per LLP 1101.002
P11, and record the decision in LLP 1023 D10.
