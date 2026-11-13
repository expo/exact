# On iOS a cold launch sends no data request until about 0.4 s after its first frame

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/SessionActivation.swift, host/apple/src/activation.rs, host/executor/src/lib.rs
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-independent-http-workers-each-open-their-own-connections.md

## What is wrong

A launch's first request leaves only after a chain of steps that all start after first pixel, one after another (LLP 1027 D4: app logic loads after first pixel):

1. `firstDrawn` (`SessionActivation.swift:9`) posts to the main queue, which declares the app's notification categories (`declareNotificationCategories`, `NotifyActions.swift:430`) and, when they are declared, runs `natives.prepareAppModule()` there (`:24`).
2. `prepareData` (`:43`) posts to the owner thread, where the TypeScript module evaluates its top level (`Host::prepare_data`, `host/apple/src/activation.rs:20`). The comment above `prepareData` measures this at 325 ms for a 2 MB catalog on an iPhone 17 Pro Max.
3. Back on the main queue, `activateData` (`:60`) calls `runtime.dataReady()`, which asks the resources.
4. The first independent request starts the lane's workers, each of which makes its transport on its own thread (`Core::start_workers`, `host/executor/src/lib.rs:451`; `worker`, `:1050`).

None of these overlaps the first frame, and none overlaps the next.

## The failure

The port of a large React Native app (iOS device, exact2 `94b8671f`) saw the first frame, drawn from kept answers, at about 0.4 s after process start, and no data request leave until about 0.8 s. Cold start to a loaded first screen was still faster than the React Native app's (2.4 s against 3.7 s), so the 0.4 s matters most on a fast network.

Only the code path was checked; the timings are the reporter's, and how the 0.4 s divides among the steps above was not measured. `ExactLaunch` already records `modulePrepared` and `dataStep`, which would divide it.

## Fix

Measure first, with the launch trace's module and data steps. Then the likely moves: load and evaluate the module on the owner thread while the first frame is being drawn, committing only after first pixel, so step 2 overlaps the paint; and make the independent lane's transport when the module is prepared, not at the first request.
