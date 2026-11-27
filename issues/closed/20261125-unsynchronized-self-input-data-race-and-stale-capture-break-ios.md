# Unsynchronized self.input data race and stale capture break iOS camera switching and fast teardown (and Web applyConstraints camera move lacks a lease check)

**Status:** Closed
**Resolution:** Serialized Apple VideoCapture self.input mutations on the camera queue (and synchronized self.device), and guarded Web applyConstraints camera moves with a captured lease across await and error handling.
**Systems:** @exact/media-capture (Apple, Web)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Out of order)

Verification: verified by reading.

At `9c0431fd5`, `VideoCapture` (`packages/media-capture/apple/VideoCapture.swift:461, 525-574`) accesses `self.input` unsynchronized across the main thread and its serial camera `queue`, causing a Swift ARC data race and stale `old = input` captures that break iOS camera switching and teardown; on Web, `applyConstraints` camera moves (`packages/media-capture/web/video.js:257-273`) bump `t.lease` without capturing or checking it after `await media.getUserMedia(...)`.

### Triggering sequence
- **Apple (`VideoCapture.swift`):** Open a video track and subscribe a consumer (calling `VideoCapture.lease` -> `start()`). Before `start()`'s or a prior `switchTo()`'s `queue.async` block commits on `dev.exact.media-capture.camera`:
  1. On iOS (`VideoCapture.oneForProcess == true`), call `applyConstraints` to switch cameras (e.g., front/back flip -> `VideoTracks.applyConstraints` -> `capture.switchTo(next)` at line 154); or
  2. On iOS or macOS, release the last consumer / call `stopTrack` / background without an app consumer (`unlease()` -> `stop()` at line 501).
- **Web (`video.js`):** On a live video track, issue two overlapping `applyConstraints` camera moves (e.g., `deviceId: "cam-front"` followed by `deviceId: "cam-back"`, or `facingMode: "user"` followed by `deviceId: "cam-back"`), or trigger `deactivate`/`activate` (e.g. `foreground(false)`/`foreground(true)`) while a camera-move `getUserMedia` is in flight, and let the earlier move resolve or reject after the newer one.

### Actual vs. expected
- **Apple actual:** `self.input` (line 461) is written on `queue` in `start()` (line 535) and `switchTo()` (line 552), yet read on main in `switchTo()` (`let ... old = input`, line 545) and read/cleared on main in `stop()` (`let ... input = input; self.input = nil`, lines 563-564) — an unsynchronized cross-thread ARC race. When `switchTo()` captures `old = nil` (or a previously removed input) before the prior `queue.async` sets `self.input`, the active `AVCaptureDeviceInput` is not removed from `session`; on single-camera iOS `AVCaptureSession`, `session.canAddInput(input)` returns `false` and the new camera is silently dropped while `self.device`, `t.device`, `t.label`, and `t.settings` all report the new camera. Likewise, `stop()` capturing `nil`/stale leaves the newly added input attached to `session` (whereas `stop()` already queries `session.outputs` inside `queue.async` at line 570). (Related: `self.device` at line 458 is also written on main at lines 526/546 while read on `queue` via `VideoRecorder.swift:78, 90, 206, 210`.)
- **Web actual:** `applyConstraints` (line 259) executes `t.lease = (t.lease ?? 0) + 1` without saving `const lease = t.lease` or checking `t.lease !== lease` after `await media.getUserMedia({ video: next })`. When an earlier overlapping move resolves late, line 266 calls `stopSource(t)` (stopping the newer camera) and line 268 adopts the stale camera; when an earlier move rejects late, line 264 unconditionally calls `end(t, ...)`, permanently killing the live track and firing `ended()` to its consumers.
- **Expected:** Established by `VideoCapture.swift:8-10` (every `AVCaptureSession` configuration runs serialized on `queue`, matching `session.outputs` at line 570) and `packages/media-capture/web/video.js:116-129` (`activate()` captures `const lease = t.lease` and ignores stale resolutions and rejections when `t.lease !== lease`).

### Verification & siblings
- **Verification:** Apple `VideoCapture.swift` verified by reading at `9c0431fd5`. Web `video.js` additionally reproduced via a `bun -e` probe against `packages/media-capture/web/index.js`: after overlapping `deviceId` moves where move 2 resolved first (`cam-back`), move 1 resolving late stopped `cam-back` (`t2StateAfterS1: "ended"`) and overwrote the track with `cam-front`; when a stale move rejected after a newer move succeeded (`cam-front`), `mediaStreamTrack` transitioned to `readyState: "ended"`, stopped the live track (`t4State: "ended"`), and fired `endedCount: 1`.
- **Siblings checked:**
  - `packages/media-capture/web/index.js:84-98` (Web audio `activate`) and `packages/media-capture/web/video.js:116-129` (Web video `activate`): both capture `const lease` and check `t.lease !== lease` on resolve and reject.
  - `packages/media-capture/apple/MediaCapture.swift` (`ProcessCapture`): mutates `engine` on the main thread only (`AVAudioEngineConfigurationChange` dispatches `reopen()` to main at line 692).
  - `packages/media-capture/android/VideoCapture.kt` and `MediaCapture.kt`: mutate camera/audio session state on the main thread only.

### Suggested fix & regression test
- **Apple (`VideoCapture.swift`):** Read and write `self.input` exclusively inside `queue.async` (in `switchTo()`, remove `self.input` or all existing `session.inputs` inside the `queue.async` configuration block; in `stop()`, remove `self.input` / `session.inputs` and clear `self.input = nil` inside `queue.async`).
- **Web (`video.js`):** Save `const lease = t.lease = (t.lease ?? 0) + 1` in `applyConstraints`; in `catch`, return `failure(e)` without calling `end(t, ...)` when `t.lease !== lease`; after `await`, if `t.lease !== lease || t.readyState === 'ended'`, call `source.stop()` and return `shape(t)`.
- **Tests:** Extend `packages/media-capture/web/lease.test.mjs` with overlapping `applyConstraints` camera moves resolving and rejecting out of order (and a move rejecting after `foreground(false)`/`foreground(true)`), and add an Apple unit test verifying rapid `start()` -> `switchTo()` / `stop()` leaves only the requested input on the `AVCaptureSession`.

### Siblings checked

Checked web/index.js:84-98 and web/video.js:116-129 (activate() already guards lease on resolve and reject), apple/MediaCapture.swift (main-thread engine lifecycle), and android/VideoCapture.kt + MediaCapture.kt (main-thread CameraX/AudioRecord lifecycle).
