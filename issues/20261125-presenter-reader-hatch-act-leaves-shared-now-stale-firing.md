# `Presenter::reader_hatch_act` leaves `Shared::now` stale, firing Android `ExtensionContext.after` timers immediately (aborting `@exact/recording` at 0 ms) and breaking Kotlin hatch `frames()` and `diagnostics().log()`

**Status:** Open
**Systems:** host/painted (Android `ExtensionContext.after` / `@exact/recording` and Android Kotlin hatches)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Units, clocks and coordinates)

Verification: verified by reading.

At `9c0431fd5`, `Shared::now` is only ever written inside `Session::call` (`host/painted/src/hatches/session.rs:542`). `Presenter::reader_hatch_act` (`host/painted/src/presenter/hatch.rs:34-52`) invokes `crate::hatches::reader::act(node, kind, text)` directly without refreshing `shared.now` first, and its Android JNI caller `CanvasHost::hatch_act` (`host/painted/src/canvas/requests.rs:79-81`) likewise does not pass the live `CanvasHost::now()` clock:

- `host/painted/src/presenter/hatch.rs:34-44`: `self.hatches.connect(...)` and `self.hatch_moments()` (which calls `Session::call` once for `app`/`window`) run only when `node == 0 && kind == 6 && self.hatches.install.is_none()`. On every subsequent call — or on the very first call whenever the app declares hatches or an `ExactModule` (`host/painted/src/presenter/requests/android.rs:216-218` populates `self.hatches.install` at launch) — `reader::act` runs with whatever stale timestamp `Session::call` last stored in `shared.now`.
- `host/painted/src/hatches/reader.rs:329-330, 341`: dispatches `kind == 5` to `shared.frames()`, `kind == 6` to `shared.after(ms)`, and `kind == 9` to `diagnostics().log(text)`.
- `host/painted/src/hatches/session.rs:287-309`: `Shared::frames` sets `clock.base = self.now.get()` and `Shared::after` schedules at `self.now.get() + ms`.
- `host/painted/src/hatches.rs:353-356` and `host/painted/src/hatches/diagnostics.rs:203-228`: `Diagnostics::log` reads `self.shared.now.get()` to bucket the 20 lines/second limit.

### Triggering sequence
1. **Shipping `@exact/recording` failure on Android:**
   - Start any Android recording with `maxDuration` (e.g., `5000` ms) via `packages/recording/android/Recording.kt:313` (or `VideoRecorder.kt:60` under the agent), which calls `ExtensionContext.after(ms)` (`host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt:324-328`) -> `HatchReader.register(0, 6, ...)` (`Hatches.kt:186-190`) -> `Exact.hatchAct` (`Owner.kt:934-937`) -> `Native_hatchAct` (`jni.rs:856-865`) -> `CanvasHost::hatch_act` (`requests.rs:79-81`) -> `Presenter::reader_hatch_act`.
   - Whenever either (a) the app already had `hatches.install` installed at launch (app hatches or `ExactModule`) and elapsed time since launch exceeds `maxDuration`, (b) the user pauses and resumes a recording (`Recording.kt:350-355` -> `arm()`) after more than `maxDuration - duration`, or (c) the user starts a second recording more than `maxDuration` after the first recording connected/fired: `shared.now` is stale at the old timestamp, so `shared.after(ms)` registers a deadline `stale_now + ms <= current_now`.
   - On the very next turn (`Owner.sync` immediately runs `produce()` -> `reader_clock` -> `hatch_timers`), `due <= now` is true and the timer fires immediately at 0 ms, aborting the recording with `"max-duration"`. (The existing test `a_readers_package_timer_runs_without_app_hatches` in `host/painted/src/presenter/hatch_tests.rs:1261-1282` only passed because it started with `install == None` and registered both timers at the same `clock = 1000.0` instant where `hatch_moments()` happened to update `shared.now`.)
2. **Kotlin hatch `frames()` and `diagnostics.log()`:**
   - Calling `ExactHatches.frames(...)` (`Hatches.kt:50`, `kind == 5`) after session time has advanced without an intervening `Session::call` sets `clock.base = stale_now`, so `Shared::frame_due` (`session.rs:370-379`) loops `(current_now - stale_now) / 16.67` times and treats a frame as immediately due.
   - Calling `ExactDiagnostics.log(...)` (`Hatches.kt:151`, `kind == 9`) from an Android callback keeps `now - bucket.from == 0.0 < 1000.0`, permanently dropping all log lines after the first 20 (`LINES_PER_SECOND`) until an unrelated `Session::call` runs.

### Actual vs. expected
- **Actual:** `ExtensionContext.after`, Kotlin hatch `after()`, `frames()`, and `diagnostics.log()` read a stale `Shared::now`, firing timers early/immediately (aborting Android recordings at 0 ms), misaligning/spinning `frames()` clocks, and permanently rate-limiting diagnostic logs past 20 lines.
- **Expected:** Per `Extensions.kt:323` (`"Run once on the session clock"`) and LLP 1075.003.000.001 §2.4 & §3.2, `after(ms)`, `frames()`, and `diagnostics().log()` must measure against the current session clock.

### Verification and siblings
Verified by reading `host/painted/src/presenter/hatch.rs`, `presenter/hatch_tests.rs`, `hatches/reader.rs`, `hatches/session.rs`, `hatches.rs`, `hatches/diagnostics.rs`, `canvas/requests.rs`, `canvas/clock.rs`, `Extensions.kt`, `Hatches.kt`, `Recording.kt`, and `VideoRecorder.kt`.
Siblings checked:
- `ExactModuleContext.after` (`host/android/canvas/src/main/java/dev/exact/canvas/ExactModule.kt:35` -> `presenter/requests/android.rs:222`) computes `Native.now() + ms` directly from `canvas::clock::now()` — not affected.
- Apple's `HatchClock.swift:43,49,62` reads live `natives?.session?.now()` directly — not affected.
- Rust hatches on Linux/Windows/Android can only reach `Context::after`/`frames`/`diagnostics().log` inside `Session::call`, which updates `shared.now` at `session.rs:542` — not affected.

### Suggested fix and regression test
1. In `Presenter::reader_hatch_act` (`host/painted/src/presenter/hatch.rs:34-44`), refresh `shared.now` before calling `crate::hatches::reader::act`:
   ```rust
   if let Some(shared) = self.hatches.shared() {
       shared.now.set(now); // `self.host.now()`, or live `now` passed from `CanvasHost::hatch_act`
   }
   ```
2. In `CanvasHost::hatch_act` (`host/painted/src/canvas/requests.rs:79-81`), read `let now = self.now();` (matching `CanvasHost::module_event` at lines 92-95) and pass `now` (or sync the host clock when live) so idle wall-clock callbacks from Android extensions/hatches also see live time rather than the previous turn's `host.now_ms`.
3. Add a unit test in `host/painted/src/presenter/hatch_tests.rs` that advances the clock after hatches are already connected (e.g., to `t = 10_000.0` after connecting at `t = 1000.0`), calls `p.reader_hatch_act(0, 6, Some("5000"))`, and asserts `p.timer_due_ms() == Some(15_000.0)` and that the timer does not fire at `t = 10_000.0`, plus checks `kind == 5` (`frames`) and `kind == 9` (`diagnostics().log` across a 1000 ms advance).

### Siblings checked

ExactModuleContext.after (ExactModule.kt:35 -> presenter/requests/android.rs:222, uses live Native.now()); Apple HatchClock.swift:43,49,62 (uses live session.now()); Rust hatches on Linux/Windows/Android (all run inside Session::call which sets shared.now at session.rs:542).
