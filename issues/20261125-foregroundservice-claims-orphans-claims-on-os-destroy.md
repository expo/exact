# ForegroundService (Claims) orphans claims on OS destroy (permanently wedging future starts), races with stopSelf(), and passes type 0 on early release (API 34+ crash)

**Status:** Open
**Systems:** Android Canvas host (ForegroundService / Calls / MediaCapture)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by reading.

At `9c0431fd5`, `Claims` in `host/android/canvas/src/main/java/dev/exact/canvas/ForegroundService.kt` has three coupled foreground-service lifecycle defects:

1. **Orphaned claims & permanent lockout on `Claims.destroyed()` (`ForegroundService.kt:181-190, 328-334`):**
   `Claims.destroyed(service)` removes `service` from `services` and `activeServices` and notifies `Exact.executionChanged()`, but never removes remaining claims of `service.kind` from `Claims.held`, never sets `released = true`, and never invokes `claim.lost(...)` (contrast `promote()`'s catch block at lines 300-303). Because orphaned claims remain in `Claims.held`, callers never learn the service died (e.g., `MediaCapture.background` stays non-null, so `MediaCapture.foreground(false)` at `packages/media-capture/android/MediaCapture.kt:358` fails to mute when hidden), and every future `ForegroundServices.claim(kind, ...)` sees `val running = Claims.held.any { it.kind == kind && !it.agent } == true` (line 181), skips `application.startForegroundService(...)` (lines 186-190), calls `Claims.changed(claim)` -> `services[claim.kind]` is `null` (line 310, no-op), and returns a permanently dead claim whose `promoted`/`lost` callbacks never fire.

2. **Early-release crash on Android 14+ / API 34+ (`ForegroundService.kt:281-305`):**
   If all claims of a kind are released after `application.startForegroundService(...)` (line 192) but before `onStartCommand()` runs (line 39-40) — e.g., `Calls.kt:217` releasing on `addCall` failure, `Calls.kt:176` hanging up immediately, or `MediaCapture.kt:320` ending a track immediately — `Claims.release()` removes the claim from `held` and returns early because `services[claim.kind]` is still `null` (line 317). When `onStartCommand()` subsequently runs `Claims.started(service)` -> `promote(service)` (line 283), `types(service.kind)` is `emptySet()`, so line 291 calls `service.startForeground(id, notification, 0)`. Note that `0` is `ServiceInfo.FOREGROUND_SERVICE_TYPE_NONE` (not `FOREGROUND_SERVICE_TYPE_MANIFEST`, which is `-1`). Because `host/android/app/build.gradle.kts:79` targets `targetSdk = 36` (>= 34), Android 14+ unconditionally prohibits `FOREGROUND_SERVICE_TYPE_NONE` (`0`) and throws `InvalidForegroundServiceTypeException` (`"Starting FGS with type none has been prohibited"`), even for single-type services with all permissions granted (and passing `-1` / `FOREGROUND_SERVICE_TYPE_MANIFEST` would instead throw `SecurityException` whenever any manifest-declared type lacks runtime grant/consent, such as `phoneCall|microphone` on `ForegroundService$Call` without `RECORD_AUDIO` at `Calls.kt:185`, or `microphone|mediaProjection` on `ForegroundService$Capture` without consent at `app/build.gradle.kts:315-318`). `promote()` catches the exception (line 295) and calls `stop(service)` without ever succeeding `startForeground()`, causing Android to crash the process with `ForegroundServiceDidNotStartInTimeException`.

3. **Stopping-service race (`ForegroundService.kt:308-311, 321-326`):**
   `Claims.stop(service)` calls `stopForeground` and `stopSelf()` (with no `startId`) without removing `service` from `services`. Because `onDestroy()` runs asynchronously, `services[service.kind]` still points to the stopping instance until `onDestroy()` executes. If a new claim arrives before `onDestroy()`, `claim()` sees `running == false`, adds the new claim to `held`, and calls `startForegroundService()`; if that claim then calls `update()`/`types()` or a second claim joins (seeing `running == true`), `Claims.changed()` (line 310) calls `promote(stoppingService)` on the dying service, re-adding `service.kind` to `activeServices` and firing `promoted` right before `onDestroy()` tears it down.

### Triggering sequences
- **Defect 1:** While a `call` or `capture` claim is active (`packages/calls/android/Calls.kt:186` or `packages/media-capture/android/MediaCapture.kt:310`), Android destroys the foreground service (e.g., OS teardown / timeout / `stopService`), then the app attempts a new `claim()` of the same kind.
- **Defect 2:** On any Android 14+ (API 34+) device (`targetSdk = 36`), a `call` or `capture` claim calls `startForegroundService()` and calls `claim.release()` before `onStartCommand()` runs (e.g., rapid stop or `Telecom.addCall` exception in `Calls.kt:217`).
- **Defect 3:** Release the last claim of a kind (`Claims.stop(service)`), immediately make a new claim of that kind and call `update()`/`types()` (or make a second claim) before the previous service's async `onDestroy()` runs.

### Actual vs. expected
- **Actual:** OS service destruction permanently wedges future starts of that kind and leaves callers thinking their claim is active; releasing before `onStartCommand()` on API 34+ passes type `0` (`FOREGROUND_SERVICE_TYPE_NONE`), fails `startForeground()`, and crashes with `ForegroundServiceDidNotStartInTimeException`; and `stop()` leaves the stopping service in `services` where subsequent claims can spuriously promote it.
- **Expected:** Per Android's foreground service contract (every `startForegroundService()` must succeed `startForeground()` with a valid permitted non-zero type on API 34+), `ForegroundService.kt:296-304`'s `lost` contract, and sibling services (`PlaybackService.kt`, `AlarmRing.kt`), unexpected destruction must release/notify all held claims, `stop()` must immediately detach the service (and stop with `startId`), and early-release `onStartCommand()` must promote with a valid permitted type before stopping.

### Verification and siblings
Verified by reading `ForegroundService.kt`, `host/android/app/build.gradle.kts:79, 315-318`, `packages/calls/android/Calls.kt:176-220`, `packages/media-capture/android/MediaCapture.kt:308-360`, and `ForegroundStartTest.kt`.
Siblings checked:
- `host/android/app/src/playback/java/dev/exact/video/PlaybackService.kt:39, 61, 111-119, 129-137` avoids all three defects: `onDestroy()` calls `stopPlayers()` to clear `claims` and pause players; `refresh()` always passes `FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK` even when `claims` is empty; `finish()` clears `service = null` before `stopSelf()`, and `onStartCommand()` uses `stopSelf(startId)`.
- `packages/alarms/android/AlarmRing.kt:286-288, 317-325, 380-411` always passes `FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED` on API 34+ even when `ringing.isEmpty()`, and stops with `stopSelf(lastStart)`.

### Suggested fix and regression test
1. In `Claims.stop(service)` (`ForegroundService.kt:321-326`), immediately detach `services.remove(service.kind)` when `services[service.kind] === service` (before `stopSelf(startId)`), and track `lastStartId` on `ForegroundService` so `stopSelf(lastStartId)` does not kill a concurrent newer start.
2. In `Claims.destroyed(service)` (`ForegroundService.kt:328-334`), when `services[service.kind] === service` (unexpected teardown not initiated by `Claims.stop`), also remove all non-agent claims of `service.kind` from `held`, set `released = true`, and invoke `it.lost("service destroyed")`.
3. Carry the claim's permitted non-zero type bitmask on the start `Intent` (or cache the last started permitted mask per `kind`) so `Claims.started(service)` passes a valid non-zero type bitmask to `startForeground()` before stopping when `held` is empty at `onStartCommand()` time.

Add Robolectric tests in `host/android/canvas/src/test/java/dev/exact/canvas/ForegroundStartTest.kt` covering: (1) `service.onDestroy()` while a claim is held fires `lost`, marks `released`, clears `Claims.held`, and allows a subsequent `claim()` to start the service again; (2) releasing the sole claim before `Claims.started(service)` invokes `startForeground` with a non-zero permitted type mask (not `0`) and then stops; (3) a new claim + `update()` arriving after `release()` and before `onDestroy()` does not promote the stopping service.

### Siblings checked

PlaybackService.kt:39,61,111-119,129-137 (clears service before stopSelf, clears claims and notifies players on onDestroy, always passes FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK); AlarmRing.kt:286-288,317-325,380-411 (passes FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED even when empty, uses stopSelf(lastStart)).
