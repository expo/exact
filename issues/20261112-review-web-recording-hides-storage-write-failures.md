# Web recording reports a successful take when its audio file was not written

**Status:** Open
**Systems:** packages/recording/web/index.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, the recorder's `write` chain catches every storage error, sets the private `r.failed` flag, and logs it (`packages/recording/web/index.js:81-88`). Nothing reads that flag when completing the recording. `finish` always returns `RecordedMedia` and fills `r.result` after the now-fulfilled write chain (`:91-112`), leaving `reason: "requested"` and `error: null` for an ordinary stop.

When browser storage is full or a write otherwise fails, an app receives a successful path for a missing or incomplete recording. It can show the take as saved and discard its live capture even though the audio has been lost. An initial write failure produces a zero-size success; a later append failure silently omits that part of the recording.

Reproduced by importing the actual module with `agent: false`, a small `MediaRecorder` stand-in that delivers one nonempty audio blob at stop, and a storage adapter whose write throws `QuotaExceededError`. The track-table adapter recorded release. Result:

```text
writes: 1, releases: 1
stopMediaRecorder: {path: "app:/data/take.webm", duration: 1000, size: 0, mimeType: "audio/webm"}
mediaRecorderState: {state: "inactive", reason: "requested", error: null, result: <the same success>}
```

Probe: `/tmp/exact-review-20261112/recording-write-failure-probe.mjs`, run with Bun 1.4.2. It drives the real recorder bookkeeping and persistence chain; browser encoding and an actual quota exhaustion were not exercised. The synthetic/agent recording also uses this write chain.

Keep the first persistence error as part of the recorder's terminal result, release the track, and report failure through both the stop answer and the state. Do not expose an incomplete file as a successful take. Validate initial-write and later-append failures, repeated stops after failure, and successful recordings.

Checked all open and closed filesystem issues and the full bodies of all 275 cached GitHub issues for recording, recorder, transcript, file writes, and storage failures. The existing recorder issues concern rendering or level sampling, not lost audio accepted as saved.
