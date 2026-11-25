# Apple audio Recorder.completed() swallows open, backpressure, write, and recovery failures as stopMediaRecorder success (and Android PartWriter.finish() ignores failed mux())

**Status:** Open
**Systems:** @exact/recording (Apple, Android)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Swallowed errors)

Verification: verified by reading.

At `9c0431fd5`, Apple audio `Recorder.completed(_:_:failure:)` (`packages/recording/apple/Recording.swift:360-369`) swallows open, backpressure, write, recovery, and agent-write errors as `stopMediaRecorder` success (and Opus recovery at `Recording.swift:558` hardcodes `.recovered.m4a`), while Android `PartWriter.finish()` (`packages/recording/android/Parts.kt:91-95`) ignores a `false` return from `mux(part, file)`.

### Triggering sequence
- **Apple audio (`Recording.swift`):** Start an audio recording via `startMediaRecorder` and hit any of: (1) `Writer.init` throws (`live()`, line 303); (2) the writer falls >2 s behind (`live()` tap, line 309); (3) WAV `file.write(from:)` throws (`Writer.append`, line 477); (4) `AVAssetWriter` fails and `recover()` also throws (`Writer.finish`, lines 535-545 — including any `audio/mp4` Opus take with `apple.formatID: "opus"`, because `recover()` at line 558 writes to `.recovered.m4a`, which Core Audio rejects); or (5) agent WAV writing throws (`try?` at lines 351-352). Then call (or await) `stopMediaRecorder`.
- **Android background audio (`Parts.kt`):** With `device.microphone-background` declared (`outlives == true`), start an `audio/mp4` recording and call `stopMediaRecorder` before any full AAC frame is emitted (`Parts.kt:112-115`), or when `tmp.renameTo(file)` returns `false` (`Parts.kt:144`).

### Actual vs. expected
- **Apple actual:** `Recorder.completed()` (lines 360-369) logs `failure` only as text, never checks `failure` or `self.error`, builds a success `RecordedMedia` dictionary `["path": path, "mimeType": type, "duration": duration, "size": size]` (`size: 0` when no file exists), stores it in `self.result`, and delivers it to every waiting `stopMediaRecorder` callback (and subsequent calls via `if let result` at line 338). For `Writer` failures (lines 477, 535-545), `self.reason` remains `"requested"` and `self.error` remains `nil`, hiding the error from `mediaRecorderState` as well. In `Writer.recover()` (line 558), `.recovered.m4a` fails with `kAudioFileUnsupportedDataFormatError` (`1718449215` / `'fmt?'`) for Opus even though line 435-436 explicitly notes `.m4a` rejects Opus and uses `.mp4`.
- **Android actual:** `PartWriter.finish()` (`Parts.kt:91-95`) ignores `mux(part, file) == false`, unconditionally deletes `file.path` from the crash-recovery ledger (discarding recovery if `part` still exists after a failed `tmp.renameTo(file)`), and returns normally; `Recording.kt:438-449` therefore answers `stopMediaRecorder` with a success `RecordedMedia` (`size: 0` or stale file) and stores it in `result`.
- **Expected:** Established by all sibling recorders: `packages/recording/web/index.js:127, 135-141` (`// A take whose file was not all written is no result: the stop answers the failure, and the state says it`), `packages/recording/apple/VideoRecorder.swift:144-151, 216-224`, `packages/recording/android/VideoRecorder.kt:167-177, 186-194`, and `packages/recording/android/Recording.kt:434-449`. A take that failed to open, write, keep up, recover, or mux must set `reason = "error"` and `error`, leave `result` unset, and answer `stopMediaRecorder` with `{"error": "UnknownError", "message": ...}`.

### Verification & siblings
- **Verification:** Apple `Recorder.completed()` and Android `PartWriter.finish()` verified by reading at `9c0431fd5`. Additionally ran a `swift -e` probe against `AVAudioFile(forWriting:settings:commonFormat:interleaved:)` with `kAudioFormatOpus`: `.recovered.m4a` threw `Error Domain=com.apple.coreaudio.avfaudio Code=1718449215` (`'fmt?'`), while `.recovered.mp4` succeeded (`OK`).
- **Siblings checked:**
  - `packages/recording/web/index.js:127, 135-141`, `packages/recording/apple/VideoRecorder.swift:216-224`, and `packages/recording/android/VideoRecorder.kt:186-194` all properly propagate failures as `stopMediaRecorder` errors and leave `result` unset.
  - Note two related sibling gaps on the non-`PartWriter` audio paths: in `packages/recording/android/Recording.kt:586, 593`, stopping before `INFO_OUTPUT_FORMAT_CHANGED` (`trackIndex < 0`) skips `muxer?.stop()` and catches/ignores `muxer?.release()`'s `IllegalStateException`, returning success on an unplayable file; and stopping before `live()` creates `encoder`/`writer` (`Recording.kt:438`, `Recording.swift:355`) also returns success with `size: 0` (unlike Web's `!r.size` check at `web/index.js:127` and both `VideoTake` classes).

### Suggested fix & regression test
- **Apple (`Recording.swift`):** Capture agent write errors instead of `try?` (lines 351-352); in `completed()`, combine `self.error` and `failure` (and fail if no file was written), set `reason = "error"` and `self.error`, leave `self.result = nil`, cache a separate `answer` dictionary (`["error": "UnknownError", "message": ...]`) for waiting and later `stopMediaRecorder` callers; in `Writer.recover()` (line 558), use `.recovered.mp4` when `(encoding[AVFormatIDKey] as? AudioFormatID) == kAudioFormatOpus`.
- **Android (`Parts.kt`):** In `PartWriter.finish()`, check `val ok = mux(part, file)`; only remove `file.path` from `ledger` when `ok || !part.exists()`, and throw an `IOException` when `!ok` so `Recording.kt:438-445` reports the failure.
- **Tests:** Add a Robolectric test in `packages/recording/test/PartsTest.kt` calling `PartWriter.finish()` with zero frames and with a failing rename (asserting throw and ledger retention when `part` survives), and a Swift unit test exercising `Recorder.completed()` on open/write/recovery failure and Opus recovery.

### Siblings checked

Checked recording/web/index.js:127-141, apple/VideoRecorder.swift:216-224, android/VideoRecorder.kt:186-194 (all properly fail stopMediaRecorder on error), and android/Recording.kt:438-449, 586-593 (noting non-PartWriter trackIndex < 0 and pre-live stop also return size-0 success).
