# The agent driver cannot send an app to the background and back on macOS or a real iPhone

**Status:** Closed
**Resolution:** Approved panel decision (LLP 1012 §3): no new agent operation; real OS backgrounding stays outside the drive because phone suspension freezes its reporting carrier and a Mac hide does not suspend; prefer visibility-state hidden asserts the background fact the runner treats as the app leaving, but does not freeze timers, replies, hatch callbacks or module announcements; see issues/20261113-a-substituted-background-does-not-behave.md.
**Systems:** scripts/agent.mjs, host/apple/Sources/ExactKit/Mac/AgentMac.swift, host/apple/Sources/ExactKit/IOS/AgentIOS.swift
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261019-agent-screenshots-system-alerts-visibility.md

## What is wrong

No driver op backgrounds an app and brings it back. The ops list in `scripts/agent.mjs`
has none. `prefer` can hide the page (`PageFacts.hidden`), which is the page's visibility
alone, not the app's lifecycle. The touch runner's `springboard` (`host/apple/touches.mjs`)
is the simulator's, and is used only inside the archive comparison.

So what a stream, a timer or a resource does across backgrounding cannot be checked under
the driver on macOS or a phone. On macOS nothing honest stands in: under the agent the
app is an accessory and never active (`ExactMac/main.swift:49-53`), so hiding it or switching
to another app does not change what it was told. On a phone the reporter launched Settings
with `devicectl` and then the app again, outside the drive.

## The failure

On Exact `9e53578f`, macOS and a real iPhone: an app's behaviour across backgrounding (a
byte stream, a timer, a resource refresh) could not be driven or observed with
`agent macos` or `agent ios --device`.

## Fix

A lifecycle op, as the reporter asked: `background <ms>` (or `background` and
`foreground`), whose reply says what the app was told (the lifecycle notifications sent,
`visibilitychange`, any suspension) and what ran meanwhile on the agent's clock. On iOS
the host can post the scene's background and foreground transitions; on a phone,
`devicectl` can launch another app and return. On macOS, hide and unhide the app or
occlude its window, and say which.

## Decision

The approved panel decision declines a new agent operation (LLP 1012 note draft
pending). Real OS backgrounding of a phone stays outside the drive: suspending
the phone process freezes the carrier that would report it. Hiding a Mac app
does not suspend its process. A drive asserts backgrounding with the `prefer`
name/value pair `visibility-state hidden`, and returns with `visibility-state
visible`. The runner already treats that hidden fact as the app leaving.

This substitute changes page visibility and the runner's background fact; it
does not yet model a frozen process. Missed repeating-timer ticks, direct replies,
executor completions, hatch frame/after callbacks and native module topic/event
announcements can still run. The remaining work is tracked in
[a substituted background does not behave as a frozen process](../20261113-a-substituted-background-does-not-behave.md).
