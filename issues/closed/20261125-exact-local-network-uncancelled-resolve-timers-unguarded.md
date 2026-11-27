# @exact/local-network uncancelled resolve timers, unguarded platform callbacks, and Apple async-later/sync-release ordering abort active operations and leak browsers

**Status:** Closed
**Resolution:** Use per-waiter cancellable resolve timeouts, guard platform browse and resolve callbacks on active instance identity and generation, and dispatch LocalNetworkModule.later synchronously with a destroyed guard on Apple.
**Systems:** @exact/local-network (Apple iOS/macOS/tvOS, Android)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Out of order)

Verification: verified by reading.

At `9c0431fd5`, both the Apple (`packages/local-network/apple/LocalNetwork.swift`) and Android (`packages/local-network/android/LocalNetwork.kt`) `@exact/local-network` implementations have three ordering/lifecycle defects:

1. **Uncancelled shared resolve timeouts (`LocalNetwork.swift:248-252,311-316`; `LocalNetwork.kt:174,212-217`):**
   Both implementations cache one `LocalNetworkResolve` / `Resolve` per `(name, type)` (key omits `timeout`: `LocalNetwork.swift:235-237`, `LocalNetwork.kt:56`) and schedule an uncancelled timer on every `read`:
   - `LocalNetwork.swift:248-252`: `DispatchQueue.main.asyncAfter(deadline: .now() + timeout) { [weak r] in guard let r, r.info() == nil else { return }; r.finish(LocalNetworkModule.error("NotFoundError")) }`
   - `LocalNetwork.kt:174`: `main.postDelayed({ if (info == null) finish(error("NotFoundError")) }, timeout.toLong())`
   Because `finish(NotFoundError)` replies to **every** waiter in `waiting` and calls `stop()` while leaving `r` cached in `LocalNetworkResolve.all` / `resolves`, a shorter concurrent `timeout` aborts a longer concurrent caller early and tears down DNS-SD/`NsdManager`, and a stale timer from a failed, completed-then-backgrounded (`pause(false)` clears `record`/`addresses`/`info`), or retried resolve fires while `info` is `nil` during a newer attempt and immediately aborts the new attempt with `NotFoundError`.

2. **Unguarded platform browse/resolve callbacks after `stop()`/`pause()` (`LocalNetwork.swift:161-162`; `LocalNetwork.kt:120-123,183-190`):**
   Unlike the 1 s settle timers (`LocalNetwork.swift:168`, `LocalNetwork.kt:127`), which guard on `self.browser === b` / `listener === l`, the platform callbacks do not check whether they still belong to the active handle:
   - `LocalNetwork.swift:161-162`: `b.browseResultsChangedHandler` and `b.stateUpdateHandler` call `self?.found(results)` / `self?.moved(state)` unconditionally, and `stop()` (`:217-220`) cancels `browser` without clearing the handlers.
   - `LocalNetwork.kt:120-123`: `NsdManager.DiscoveryListener` posts `fail(...)` and `moved(info.serviceName, ±1)` to `main` without checking `listener === l`.
   - `LocalNetwork.kt:183-190`: `NsdManager.ServiceInfoCallback` (API 34+) and `Serial.ask` (API <34, which cannot be cancelled) post `finish(...)`, `took(...)`, and `info = null; finish(...)` to `main` without checking `callback === c` or a resolve generation counter.

3. **Apple async `later` vs. synchronous `release`/`destroy` (`LocalNetwork.swift:37-68`):**
   `LocalNetworkModule.later` defers `LocalNetworkBrowse.read(self, ...)` / `LocalNetworkResolve.read(self, ...)` via `DispatchQueue.main.async { [self] in ... }` (`:38`), whereas `release(_:)`, `release(_:args:)`, `foreground(_:)`, and `destroy(_:)` (`:54-68`) run synchronously via `ExactExtensions.later` (`host/apple/modules/ExactExtensions.swift:278-280`) and `ExactExtensions.destroy` (`ExactExtensions.swift:370`), with no released/destroyed guard.

### Triggering sequences
- **Stale/shared timeout:** Start `resolveService` for `(name, type)` with `timeout=500` and concurrently with `timeout=10000` (allowed by `packages/local-network/exact.json:106-127`, `timeout` 0..60000 ms), or start a `resolveService` with `timeout=5000`, background and foreground the app at t=4.8 s (`pause(true)` then `pause(false)` clears `info` and calls `changed()` to re-read), and observe the t=0 timer fire at t=5 s and abort the fresh attempt after 200 ms.
- **Stale callbacks:** Background (`pause(true)` -> `stop()`) and foreground (`pause(false)` -> `changed()` -> `start()`) while `NWBrowser` / `NsdManager` callbacks from the prior instance are queued on main; late `onServiceLost` decrements `names` counts for the new browse, late `onStartDiscoveryFailed` / `.failed` / `onServiceLost` cancels the new browse/resolve, and late `found` / `onServiceUpdated` writes stale state.
- **Async `later` / sync `release` or `destroy` on Apple:** Call `browseServices`/`resolveService` (which enters `LocalNetworkModule.later` via `NativeModule.swift:384` and re-enqueues on `DispatchQueue.main`), then close the session (`destroy`) or unmount the reader (`release_idle` in `runner/src/runner/modules/bound.rs:468-565`) before the inner main-queue block runs. `Self.leave(self)` finds nothing and completes; the deferred block then runs with a strong `[self]` capture, inserts the released/destroyed `LocalNetworkModule` into process-wide `LocalNetworkBrowse.all`/`LocalNetworkResolve.all`, and starts an `NWBrowser`/`DNSServiceResolve` that is never stopped.

### Actual vs. expected
- **Actual:** Concurrent callers with longer timeouts and retried/foregrounded resolves fail early with `NotFoundError`; stale callbacks from stopped instances corrupt `names` or kill newly started browsers/resolvers; on Apple, a `release` or `destroy` racing with `later` leaks the module instance and an active `NWBrowser`/`DNSServiceResolve` for the process lifetime.
- **Expected:** Each `resolveService` caller honors its own timeout without aborting unrelated waiters or later attempts; callbacks from stopped/replaced browsers and resolvers are ignored; `release` and `destroy` never allow an already-queued `later` to register or start a browser/resolve afterward. Established by `LocalNetwork.swift:167-170`, `LocalNetwork.kt:127`, `packages/local-network/exact.json:106-127`, `host/apple/modules/ExactPackage.swift:9,544-563`, and LLP 1133.036 D2–D3.

### Verification and siblings checked
Verified by reading `LocalNetwork.swift`, `LocalNetwork.kt`, `packages/local-network/exact.json`, `ExactExtensions.swift:278-280,354-378`, `NativeModule.swift:381-391`, and `runner/src/runner/modules/bound.rs:468-565`.

Siblings checked:
- `packages/local-network/linux/lib.rs:15-28` and `packages/local-network/web/index.js:11-39` return `NotSupportedError` for browse/resolve (unaffected).
- `LocalNetwork.kt:49-85` posts `later`, `release`, `foreground`, and `destroy` all onto `main`, so Android does not have sub-issue 3.
- Other Apple live-source packages with async `later` vs. sync `release`/`destroy` exhibit the same sub-issue 3 pattern: notably `packages/device/apple/Device.swift:308-326` (registers into process-wide `DeviceHub.shared`), as well as `packages/altimeter/apple/Altimeter.swift:70-81,160-182`, `packages/pedometer/apple/Pedometer.swift:62-73,195-225`, `packages/geolocation/apple/Geolocation.swift:91-102,116-121`, and `packages/motion/apple/Motion.swift:122-133` (unlike `packages/global-shortcuts/apple/GlobalShortcuts.swift:30,61,71,80`, which queues `release`/`destroy` onto `DispatchQueue.main.async` too).

### Suggested fix and regression test
1. Track each resolve waiter with its own cancellable timeout (`DispatchWorkItem` on Apple, `Runnable` removed with `main.removeCallbacks` on Android); on timeout, reply `NotFoundError` only to that waiter, remove it from `waiting`, and call `stop()` only if no waiters or live readers remain (cancelling all waiter timers when `finish` answers or `stop` runs).
2. Guard `NWBrowser` handlers on `self.browser === b` (and clear handlers in `stop()`), guard Android `DiscoveryListener` callbacks on `listener === l`, and guard `ServiceInfoCallback` / `Serial.ask` callbacks on `callback === c` / a monotonic resolve `generation` incremented in `stop()`.
3. Handle `LocalNetworkModule.later` synchronously on the main thread (as `ExactPackage` guarantees main-thread delivery) or guard against a `destroyed` flag and serialize `release`/`destroy` with `later`.
Add unit tests covering (a) concurrent 500 ms and 10 s `resolveService` callers and a retry after `pause(false)` before an earlier timeout expires, (b) late callbacks from a stopped browse/resolve after restart, and (c) `later("browseServices")` immediately followed by `destroy` on Apple.

### Siblings checked

Checked packages/local-network/linux/lib.rs and web/index.js (no browse/resolve); checked Android LocalNetwork.kt (FIFO-posted on main, no sub-issue 3); noted Apple async-later/sync-release siblings Device.swift, Altimeter.swift, Pedometer.swift, Geolocation.swift, Motion.swift.
