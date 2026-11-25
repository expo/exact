# Mid-flight stopSpeechRecognition permanently hangs and leaks the audio track lease on Apple (AnalyzerEngine) and Web

**Status:** Open
**Systems:** @exact/speech-recognition (Apple, Web)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by reading.

At `9c0431fd5`, mid-flight `stopSpeechRecognition` (and setup-time teardown) hangs and leaks the audio track lease in `packages/speech-recognition/apple/SpeechRecognition.swift:454-475, 545-588, 650-663` and `packages/speech-recognition/web/index.js:114-124, 218, 228-243, 247-271`.

### Triggering sequence
- **Apple (`AnalyzerEngine`, default on iOS/macOS 26):** Call `startSpeechRecognition` with `track=` (lines 185-242); `start()` subscribes to the track (taking its lease), sets `r.state = "listening"`, creates `AnalyzerEngine`, and replies immediately with `r.info()` while `AnalyzerEngine.init` (lines 549-578) runs an unstored async `Task` awaiting `Self.module`, `AssetInventory.status`, `bestAvailableAudioFormat`, `analyzer.setContext`, and `analyzer.start`. Call `stopSpeechRecognition` before that `Task` reaches `began()` (line 570). Similarly, call `abortSpeechRecognition`, `foreground(false)`, or `destroy()` while setup is in flight.
- **Web:** (1) Call `stopSpeechRecognition` twice concurrently on a live browser recognition; (2) call `stopSpeechRecognition` on a `track=` recognition before `media-capture` fires `live(source)` (e.g., while asynchronously reopening an idle track via `getUserMedia`); or (3) trigger `abortSpeechRecognition`, `foreground(false)`, `dispose`, or `track.ended()` while `r.state === 'stopping'`.

### Actual vs. expected
- **Apple actual:** `Recognition.stop()` (lines 454-467) appends `reply` to `waiting`, sets `stopping = true` and `state = "stopping"`, retains `r.engine`, and calls `AnalyzerEngine.stop()` (lines 650-655). Because `tap`, `input`, and `analyzer` are all still `nil` and `AnalyzerEngine` has no `stopped` flag, `stop()` is a complete no-op. When the setup `Task` reaches `began()` (lines 583-588), `aborted` is `false`, so it attaches the audio tap and streams indefinitely; `results(module)` never completes, `stopSpeechRecognition` never replies (retries and non-continuous `stop` at line 485 hit `guard !stopping` at line 456), and `r.subscription` leaks the audio track lease indefinitely. On `abort()`/`foreground(false)`/`destroy()` during setup, `r.finish()` drops `r.engine`, but the unstored `Task` is never cancelled and does not check `self != nil` before lines 550-569, so it still opens `AVAudioFile` / creates and starts `SpeechAnalyzer` in the background; if `abort()` lands while `began()` runs, `began()` finishes `input` without calling `cancelAndFinishNow()`.
- **Web actual:** `stopSpeechRecognition` (lines 258-261) overwrites `r.stopped` on a second concurrent call (hanging the first caller); `finish(r)` (lines 114-124) does not invoke `r.stopped` (so abort/background/dispose during `'stopping'` hangs stop callers if `onend` never fires); `track.ended()` (line 239) checks only `r.state === 'listening'` and ignores `'stopping'`; and stopping a `track=` recognition before `live(source)` calls `r.recognizer.stop()` on an unstarted recognizer (no `onend`), after which `live(s)` checks only `r.state === 'ended'` (line 233) and calls `recognizer.start(s)` after stop was requested, hanging `stopSpeechRecognition` and leaking the track lease.
- **Expected:** Established by `Recognition.whenDone`/`waiting` (`SpeechRecognition.swift:448-467`), Android `SpeechRecognition.kt:484-518` (shared `waiting` list drained by `finish()`, re-entrant `stopping` guard, `ended()` checking `state != "ended"`), and Web `finish(r)` (`web/index.js:113-124`). Every `stopSpeechRecognition` caller must settle once the recognition ends and release the track lease; stopping or aborting during setup must not start recognition afterward.

### Verification & siblings
- **Verification:** Apple `AnalyzerEngine` verified by reading `SpeechRecognition.swift` at `9c0431fd5`. All four Web defects were additionally reproduced directly via a `bun -e` probe against `packages/speech-recognition/web/index.js` (`p1Resolved: false, p2Resolved: true, stopCalls: 2`; stop-before-live `startedWith: [{ id: "fake-source" }], stopResolved: false, unsubCalled: false`; `track.ended()` during stopping left `state: "stopping", stopResolved: false`; `abortSpeechRecognition` during stopping left `state: "ended", stopResolved: false`).
- **Siblings checked:**
  - `packages/speech-recognition/apple/SpeechRecognition.swift:668-826` (`RecognizerEngine`): creates `request` and `task` synchronously in `init` -> `begin()`, so `stop()` before `live()` still ends the request and finishes.
  - `packages/speech-recognition/android/SpeechRecognition.kt:484-526, 598-603`: starts `SpeechRecognizer` synchronously in `listen()`, drains all `waiting` callbacks in `finish()`, guards re-entrant `stop()` with `stopping`, and checks `state == "ended"` in `ended()`.

### Suggested fix & regression test
- **Apple `AnalyzerEngine`:** Store the setup `Task` and cancel it in `abort()`; record `stopped = true` in `stop()`; check `!aborted` / `!Task.isCancelled` / `self != nil` across setup awaits before opening files or starting `SpeechAnalyzer`; in `began()` and `hold()`, if `aborted` cancel via `cancelAndFinishNow()`, and if `stopped` finish `input` and call `finalizeAndFinishThroughEndOfInput()` without attaching the tap.
- **Web `index.js`:** Keep a `waiting` list (or await `r.done` via a shared deferred resolved from `finish(r)`), guard re-entrant `stopSpeechRecognition` when `r.state === 'stopping'`, handle `r.state !== 'ended'` in `track.ended()`, and if `stopSpeechRecognition` runs before `live(source)` has started the recognizer (when `input === 'track'`), finish immediately and guard `live(s)` with `r.state !== 'listening'`.
- **Tests:** Add a Bun unit test under `packages/speech-recognition/test/` covering concurrent `stopSpeechRecognition`, stop-before-`live(source)`, `track.ended()` during `'stopping'`, and `abortSpeechRecognition` during `'stopping'`; add a Swift test covering `AnalyzerEngine` stop/abort before `began()` completes.

### Siblings checked

Checked Apple RecognizerEngine (SpeechRecognition.swift:668-826) and Android SpeechRecognition.kt:484-526, 598-603; neither has the setup-window or single-waiter/stopping-state defects.
