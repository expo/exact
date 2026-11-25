# RegionCarrier CLMonitor tasks race out of order (leaking removed geofences) with cross-package key collision, and macOS WakeStore stale callbacks consume rescheduled wakes

**Status:** Open
**Systems:** ExactWakeCarriers / @exact/geofencing / background wakes (iOS, macOS)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Out of order)

Verification: verified by reading.

At `9c0431fd5`, `ExactWakeCarriers` has two out-of-order concurrency bugs and a cross-package key collision across `Regions.swift` and `Wakes.swift`:

1. **Unserialized `CLMonitor` mutations leak removed geofences (`Regions.swift:180-226`):**
   - `RegionCarrier.Monitor.add` (`host/apple/Sources/ExactWakeCarriers/Regions.swift:213-220`) spawns an unstructured `Task { @MainActor in let m = await starting!.value; await m.remove(r.name); await m.add(condition(r), identifier: r.name) }`.
   - `RegionCarrier.Monitor.remove` (`Regions.swift:222-225`) spawns a separate unstructured `Task { @MainActor in await starting.value.remove(name) }`.
   - Because `CLMonitor` is an actor on its own executor, `Monitor.add` suspends at `await m.remove(r.name)` before calling `await m.add(...)`. When `addGeofence("home", ...)` is followed immediately by `removeGeofence("home")` (or by a second `addGeofence("home", ...)` with new coordinates), `Monitor.remove`'s task can run `m.remove("home")` while `Monitor.add` is suspended; `Monitor.add` then resumes and executes `m.add(..., identifier: "home")`, permanently leaving the deleted geofence registered in `CLMonitor` (consuming one of Apple's hard 20-condition slots and waking the app for crossings of a deleted fence) or leaving stale coordinates monitored.
   - Similarly, `Monitor.start` (`Regions.swift:180-207`) snapshots `kept` by value before `await CLMonitor(name)` and registers every entry in `kept` (`:189`) without rechecking `RegionCarrier.regions`; if a region is removed and/or re-added while `CLMonitor(name)` is initializing, `start` transiently registers stale entries and concurrent `remove`/`add` tasks waiting on `starting.value` resume without ordering guarantees.
2. **Cross-package key collision in `RegionCarrier` (`Regions.swift:40, 68, 83-102`):**
   - `RegionCarrier.regions` (`:40`), `offering` (`:43`), `crossedWhileOffering` (`:45`), and the `CLMonitor` condition identifier (`:189, 217-218, 224`) are keyed only by bare `name` instead of `(package, name)` (unlike `WakeStore`, which keys by `package + " " + k` at `Wakes.swift:95`).
   - Although `list`, `pending`, and `delivered` (`Regions.swift:104, 110, 114`) enforce `r.package == package` and LLP 1133.021 D14 exposes `context.regions` to any package, `case "add"` (`:87`: `regions[name] = r`) silently overwrites another package's region and `case "remove"` (`:96`: `regions.removeValue(forKey: name)`) deletes any package's region without checking `r.package == package`.
3. **Stale macOS `NSBackgroundActivityScheduler` callback consumes rescheduled wakes (`Wakes.swift:277-297`):**
   - In `WakeStore.schedule()` on macOS (`host/apple/Sources/ExactWakeCarriers/Wakes.swift:277-297`), rescheduling or cancelling+re-adding a wake `id` invalidates the old scheduler `s1` (`:278`) and installs a new scheduler `s2` at `schedulers[id]` (`:279-295`).
   - Because `NSBackgroundActivityScheduler.schedule(_:)` runs its block on a background XPC queue and dispatches `DispatchQueue.main.async` (`:286-293`), if `s1` fires right before `s1.invalidate()`, its already-queued main-thread block runs after `s2` is installed, unconditionally sets `schedulers[id] = nil` (`:287`, orphaning `s2` without invalidating it), removes the **new** request via `requests.removeValue(forKey: id)` (`:288`), persists the deletion (`:289`), and fires `offer(a)` prematurely for the rescheduled wake.

### Trigger
- From `@exact/geofencing` (`packages/geofencing/apple/Geofencing.swift:44-48`), call `addGeofence(name: "home", ...)` followed immediately by `removeGeofence(name: "home")` (or update `"home"` twice in quick succession).
- Or have two packages call `context.regions.add`/`remove` with the same region `name`.
- Or on macOS, reschedule a periodic sync wake (`packages/background/apple/Background.swift:158-160`) right as its prior `NSBackgroundActivityScheduler` fires on its XPC queue.

### Actual vs. Expected
- **Actual:** A removed geofence can remain permanently monitored in `CLMonitor` while absent from `RegionCarrier.regions`; one package can overwrite or delete another package's regions; and on macOS a stale scheduler callback can wipe the active scheduler entry and prematurely consume/fire a rescheduled wake.
- **Expected:** Per LLP 1133.021 D10–D14 (`llp/1133.021-maps-geofencing-geocoding.rfc.md`) and LLP 1133.017 D8 (`llp/1133.017-background.rfc.md`), `CLMonitor` state always converges to the latest `RegionCarrier.regions` state, packages' regions are isolated by `(package, name)`, and a superseded macOS `NSBackgroundActivityScheduler` callback does nothing.

### Verification & Siblings Checked
- **Verified by reading** `host/apple/Sources/ExactWakeCarriers/Regions.swift`, `host/apple/Sources/ExactWakeCarriers/Wakes.swift`, `packages/geofencing/apple/Geofencing.swift`, `packages/background/apple/Background.swift`, and `host/apple/tests/ExactKitTests/RegionCarrierTests.swift`.
- **Siblings checked:**
  - Android `packages/geofencing/android/Geofencing.kt:189-210`: has a sibling add/remove race (`add` writes `store.put(region)` inside Play Services' async `addGeofences` callback while `remove` immediately removes from `store` and calls async `removeGeofences`, so a concurrent `remove` during `add` can be resurrected). `host/android/canvas/src/main/java/dev/exact/canvas/Regions.kt` is single-namespace (takes no `package`).
  - iOS `host/apple/Sources/ExactWakeCarriers/Wakes.swift:120-270`: registers `BGTaskScheduler` handlers directly on `.main` and filters by `at <= now()` in `take(suffix)`, so it does not have the macOS XPC-to-main stale-scheduler race.
  - Apple `host/apple/Sources/ExactWakeCarriers/Transfers.swift`: has the same cross-package unnamespaced key collision (covered in C121).

### Suggested Fix & Regression Test
- In `RegionCarrier.Monitor`, serialize mutations on `@MainActor` via a chained task / reconcile loop (or re-read current `RegionCarrier.regions[key]` after awaiting `starting!.value` and reconcile `m` to match `regions` rather than applying a stale snapshot), and namespace `regions`, `offering`, `crossedWhileOffering`, and `CLMonitor` identifiers by `(package, name)` (e.g., `"\(package)\0\(name)"` or encoded pair) while enforcing `package` ownership on `add`/`remove`.
- In macOS `WakeStore.schedule()` (`Wakes.swift:286-293`), guard `schedulers[id]?.0 === s` inside `DispatchQueue.main.async` before clearing `schedulers[id]` or removing/firing `requests[id]`.
- Add tests in `host/apple/tests/ExactKitTests/RegionCarrierTests.swift` (using a mock/reconciler hook for `Monitor` ordering and two-package `add`/`remove` isolation) and a macOS unit test in `ExactKitTests` verifying a stale scheduler callback after rescheduling does not clear or consume the newer request.

### Siblings checked

packages/geofencing/android/Geofencing.kt:189-210 (sibling async add/remove race where add's Play Services callback can resurrect a removed region in RegionStore); host/android/canvas/src/main/java/dev/exact/canvas/Regions.kt (single-namespace, no package field); host/apple/Sources/ExactWakeCarriers/Wakes.swift:120-270 (iOS BGTaskScheduler runs on .main and checks at <= now(), not affected); host/apple/Sources/ExactWakeCarriers/Transfers.swift (same cross-package key collision, covered in C121)
