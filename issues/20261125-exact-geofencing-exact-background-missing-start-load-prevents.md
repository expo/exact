# @exact/geofencing & @exact/background: missing "start": "load" prevents launch initialization, silently dropping Android geofence/periodic-sync wakes and offline sync drains

**Status:** Open
**Systems:** packages/geofencing (manifest + Android), packages/background (manifest + Android & Apple)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Dropped plumbing)

Verification: verified by reading.

At `9c0431fd5`, `js/bake/src/typescript.mjs:1295` (`appleImplementations`) and `js/bake/src/typescript.mjs:1314` (`androidImplementations`) populate launch-time instantiation (`exactStartAtLoad` in `host/apple/compose.mjs:119` and `startAtLoad` in `scripts/agent-android.mjs:1307`) strictly from `i.declaration?.start === 'load'`. Neither `packages/geofencing/exact.json` (lines 1–142) nor `packages/background/exact.json` (lines 1–109) declares `"start": "load"`.

### Triggering sequence
An app importing `@exact/geofencing` or `@exact/background` launches onto a home/default screen that binds root wake handlers (`geofenceenter`/`geofenceleave` or `sync`/`periodicsync`) but does not query a status/list source (`geofencingStatus`, `geofences`, `backgroundSyncStatus`, `syncTags`) or invoke a send on startup (e.g., geofences or sync tags were registered from a settings or offline screen in a previous session). On foreground launch, `runner/src/runner/modules/bound.rs:78` does not send `"foreground"` (`in_background()` is false), and on Android `Extensions.firstFrame()` (`host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt:813-815`, called from `ExactActivity.kt:513` and `Headless.kt:33`) and on Apple `ExactExtensions.created(_:)` (`host/apple/modules/ExactExtensions.swift:152-157`) only instantiate extensions listed in `startAtLoad` / `exactStartAtLoad`.

### Actual vs. expected result
- **`@exact/geofencing` (Android):** `GeofencingModule.init` (`packages/geofencing/android/Geofencing.kt:85-93`) never runs at launch or during `Headless.session()`. Consequently, `ExactRegions.listen(delivery)` is never registered—causing `GeofenceReceiver` broadcasts while the app is open to find `listener?.get() == null` in `host/android/canvas/src/main/java/dev/exact/canvas/Regions.kt:33` and drop live delivery—stored crossings recorded while closed (`Receivers.kt:38-40`) are never delivered via `post(::deliver)`, and fences dropped by Play Services after location was toggled off are never re-armed (`Geofencing.kt:90`).
- **`@exact/background` (Android):** `BackgroundModule.init` (`packages/background/android/Background.kt:76-82`) never runs, so `BackgroundModule.live` remains `null` and `ConnectivityManager.registerDefaultNetworkCallback(network)` is never registered. When WorkManager runs `BackgroundWorker.doWork()` (`Background.kt:275-284`) while the app is open, `BackgroundModule.live?.get()` is `null` (`Background.kt:278`), silently dropping `periodicsync` wakes with `Result.success()` and spinning `sync` work in `Result.retry()`, while queued offline `sync` tags never drain when connectivity returns.
- **`@exact/background` (Apple):** `BackgroundModule.init` (`packages/background/apple/Background.swift:53-67`) never runs at launch, so `NWPathMonitor` is not started and `ask()` is not called. Queued offline `sync` tags are not drained at launch or on foreground network recovery (on macOS, `Background.swift:147-157` only requests `WakeStore` wakes for `sync` under `#if os(iOS)`, so `NWPathMonitor` is the only drain path for `sync`). (Apple `@exact/geofencing` is unaffected because `RegionCarrier` in `host/apple/Sources/ExactWakeCarriers/Regions.swift` lives in the host and triggers `exact.wakes.due` -> `ExactExtensions.adapter`.)

Expected behavior is established by LLP 1133.015 D16 item 1 (`llp/1133.015-purchases.rfc.md:888-897`, which explicitly names geofencing as using `"start": "load"`), LLP 1133.021 D14 (`Geofencing.kt:88-91`, "Every launch re-arms the store's fences ... and delivers what the store kept undelivered"), `Background.kt:12-15, 270-273`, and `Background.swift:14-16` ("fires at whichever comes first, or at the next launch"). Existing conformance fixtures (`apps/geofence-fixture/app.contract:10-11`, `packages/background/test/fixture/app.contract:17-18`) masked this because their single root screen eagerly reads `geofencingStatus()` / `backgroundSyncStatus()`.

### Verification & siblings checked
Verified by reading the manifest bake (`js/bake/src/typescript.mjs:1295, 1314`), Apple and Android extension loaders (`ExactExtensions.swift:150-157`, `compose.mjs:119`, `Extensions.kt:688, 811-815, 839`, `Headless.kt:33`, `agent-android.mjs:1307`), runner foreground dispatch (`runner/src/runner/modules/bound.rs:78, 438-450`, `runner/src/runner/page.rs:60-62`), and the package implementations.

Siblings checked:
- `packages/push/exact.json:5`, `packages/purchases/exact.json:6`, `packages/background-transfer/exact.json:6`, and `packages/fixtures/invites/exact.json:6` all properly declare `"start": "load"`.
- **Sibling gap 1 (`@exact/notifications` Android):** `packages/notifications/exact.json` omits `"start": "load"`, even though `packages/notifications/android/Notifications.kt:139-142` runs `Later.rearm(app)` in `init` with the comment `"A force stop cancels the app's alarms, and its next launch makes this module."`
- **Sibling gap 2 (Web `start: "load"`):** `js/bake/src/typescript.mjs:1320-1327` (`webImplementations`) and `host/web-js/modules.js:498-503` do not load `"start": "load"` extensions automatically (only `gesture` and `tap.choices`), so `packages/background/web/index.js:79-80` likewise will not drain `localStorage` `sync` tags on reload if the opening screen does not call `@exact/background`.
- `packages/alarms/android/Alarms.kt` and `packages/calls/android/Calls.kt` do not require launch-time `init` for correctness (`AlarmReceiver`/`AlarmService` operate directly on `AlarmRecords`; `CallsModule` is woken by runner `exact:listed`).

### Suggested fix & regression test
Add `"start": "load"` to `packages/geofencing/exact.json` and `packages/background/exact.json` (and consider `packages/notifications/exact.json` plus wiring `start === 'load'` in `host/web-js/modules.js`). Add unit/build assertions in `packages/geofencing/test/android.mjs` and `packages/background/test/android.mjs` checking that generated `ExactExtensions.kt` includes `"geofencing"` / `"background"` in `startAtLoad`, and an Apple composition check verifying `exactStartAtLoad` includes `"background"`.

### Siblings checked

packages/push/exact.json, packages/purchases/exact.json, packages/background-transfer/exact.json, packages/fixtures/invites/exact.json (all set "start": "load"); packages/notifications/exact.json + Notifications.kt:139-142 (same missing "start": "load" for post-force-stop Later.rearm); host/web-js/modules.js:498-503 + packages/background/web/index.js:79-80 (web does not eager-load "start": "load"); packages/alarms/android/Alarms.kt & packages/calls/android/Calls.kt (no launch-init drop).
