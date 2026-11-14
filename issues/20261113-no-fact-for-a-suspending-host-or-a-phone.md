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

## Partial implementation

`exactPage().frozen` exposes execution eligibility, with web Page Lifecycle,
Android Activity/service and Apple audio/lifecycle producers. iOS and tvOS freeze in
the background without active eligible audio; a configured playback category
alone grants nothing. macOS stays false. Apple forwards the freeze bit,
stops sequences and releases gated streams in that commit, and accepts
`prefer frozen`. A failed capture-engine restart releases its background claim;
a new capture lease reacquires it. A UIKit background task covers the freeze
commit's socket-close wait: URLSession confirms the peer's close, portable sockets
the close-frame write, within a shared 250 ms bound before aborting unfinished I/O.

Each host composes its execution sources in one function. Apple's
`AudioSession.backgroundExecutionEligible()` requires the background audio mode
and counts system-activated calls, activated background captures, playback speech,
and successfully activated sound and audible video owners under the
effective playback or play-and-record category. Canvas audio releases on the
background edge and does not keep execution running. Video counts only while
`timeControlStatus` is playing. Android's `Exact.executionEligible()` counts a
started Activity or a live claim on a successfully promoted microphone, call or
screen-capture foreground service. Pending, failed and agent-only service claims
grant no device execution. Both hosts update the page when the last eligible
owner ends while hidden.

The watch bundle links `ExactCore` and `ExactSnapshot`, without `PageFacts`. Its
live driver pauses timers in the background but does not publish `frozen`;
that lifecycle producer and its watch qualification remain to be implemented.

The issue remains open for that watch work and a signed-phone run: confirm the server sees the
socket close before suspension and a fresh socket on return; repeat with
active background playback and with playback ending while hidden. The
navigation consumer's tree review is also owed. No device-idiom field is added.
