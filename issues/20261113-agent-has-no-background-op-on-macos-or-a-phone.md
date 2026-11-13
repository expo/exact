# The agent driver cannot send an app to the background and back on macOS or a real iPhone

**Status:** Open
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
