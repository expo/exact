# An iOS app with widgets or Live Activities holds a file lock in its App Group container and is killed with 0xdead10cc when suspended

**Status:** Closed
**Resolution:** The Apple update store, whose owner and reader locks last the process, lives in the app's private Application Support, so an app with widgets or Live Activities holds no lock in its App Group (371201f6).
**Systems:** update/src/store.rs, host/apple/Sources/ExactUpdates/Updates.swift, host/apple/Sources/ExactCore/WidgetContainer.swift
**Severity:** P1
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261110-second-instance-boots-embedded-bundle.md

## What is wrong

`Store::open` takes an exclusive `flock` on `owner.lock` in the store's
directory and keeps it for the life of the process: the `Store` holds the
file in `_owner` (`update/src/store.rs:305-307`, the lock at `:373-386`). A
second process that finds the store owned opens it as a reader and holds a
shared lock on the entry it runs, also for its life (`_running_lock`,
`:311-312`, `:509-514`).

On Apple, `Updates.open` puts the store under `WidgetContainer.updateBase`
when there is one (`host/apple/Sources/ExactUpdates/Updates.swift:57`), which
is `Library/Application Support` inside the App Group container
(`WidgetContainer.swift:37`, under `root` at `:22`). Every iOS app with widgets or Live
Activities has that group (`host/apple/build.mjs:424` sets `ExactAppGroup` for
them), so its update store, and the lock, live in the shared container.

iOS terminates a suspended app that holds a file lock in a shared container,
with termination code `0xdead10cc` ("Understanding the exception types in a
crash report", Apple developer documentation: the app "held on to a file lock
or SQLite database lock during suspension"). Since the lock is held from launch
to exit, such an app is killed every time it is suspended, and the next open
is a cold launch.

## The failure

An agent building an iOS app with `live_activities`, on exact2 `95f015045`,
received two TestFlight crash reports: `Termination: RUNNINGBOARD 0xdead10cc`
after the app went to the background.

Confirmed at `49ae6357` by reading the store's open and the Apple store base;
not run on a device. The simulator does not enforce this termination.

## Fix

Do not hold a lock in the group container across a suspension. Either keep
the update store in the app's own container (the widget and activity
extensions read the published `widget-generations/`, not the update store;
check that nothing else needs it in the group), or hold `owner.lock` (and a reader's entry lock) only
while the store reads or writes and take it again for each operation, or
release it when the app resigns active and take it again on return. The
second-owner protection of LLP 1026 D11 must survive whichever is chosen.

## Implementation

The update store uses the app's private Application Support directory. Its
owner and reader locks keep their existing lifetime; extensions continue
reading immutable `widget-generations/` in the App Group (LLP 1126 D8).
All 10 WidgetContainerTests pass on iOS Simulator. A Caltrain drive opens the
station picker and searches for Palo; its updater's `owner.lock` is in the
private container, no updater lock is in the group, and the group's published
widget plan is readable. A signed iPhone or TestFlight
run must still confirm background suspension and resumption without
`0xdead10cc`; the simulator does not enforce this termination.
