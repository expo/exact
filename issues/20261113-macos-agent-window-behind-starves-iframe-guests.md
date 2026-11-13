# Under the macOS agent an iframe guest runs no animation frames and throttles its timers, because the driven window sits behind every other window

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/Mac/AgentMac.swift (`NSWindow.Level.agentBehind`, `Agent.windowsBehind`), host/apple/Sources/ExactMac/main.swift, host/apple/Sources/ExactKit/WebModule.swift
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

A drive on the agent's clock puts its window at `NSWindow.Level.agentBehind`, one level
below every other app's windows (`AgentMac.swift:1440-1456`, set in `ExactMac/main.swift:267`).
The comment there gives the reason it is safe: "A drive on the agent's clock needs nothing
from being seen", since pictures come from the window alone and page visibility is the
driver's. A `WKWebView` in that window does not run on the agent's clock. WebKit treats a
covered window's web view as not visible: `requestAnimationFrame` callbacks stop, timers
are throttled to about once a second, and the view stops painting new content. So an
iframe guest under the driver behaves as a hidden tab whenever other windows cover the
driver's, which on a working Mac is nearly always.

The live frame recording already handles the same problem for display-link frames: it
lifts the window to `.normal` while it records (`FrameSampler.swift:808-827`). Nothing does
that for a guest.

## The failure

On Exact `5434f432`, macOS 26.6, an app with an iframe of a bundled page:

- A guest that answers the app's `postMessage` from inside a `requestAnimationFrame`
  callback never answered; the `message=` action never ran.
- After the guest's DOM changed, `screenshot <png> window` showed the frame's area blank.
- A guest's timers ran about once a second, so a paced typing benchmark in the guest could
  not be run.

With the driver window moved to an uncovered place, both the answer and the picture were
right.

## Fix

While a session holds a live web view, keep it rendering under the driver. One way is to
order the window in front while a guest is present, as the frame recording does. Another
is to keep the web view counted as visible while it is covered (WebKit's window occlusion
detection, if a development build may turn it off for its web views), and take guest pictures with
`WKWebView.takeSnapshot` after a forced layout. At the least, say in `docs/testing.md` that
a guest needs the driver window uncovered, and have `state` report a guest whose web view
is occluded.
