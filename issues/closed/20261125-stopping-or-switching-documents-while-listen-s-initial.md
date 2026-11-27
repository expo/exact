# Stopping or switching documents while Listen's initial `cancelSpeech` is in flight still starts speaking block 0 with Stop hidden

**Status:** Closed
**Resolution:** Guarded Markdown queueFirst with listening and clearedRun == run and cleared cleared = none while incrementing run in stopListening so stopping or switching documents mid-cancel never starts speaking.
**Systems:** Markdown reader speech synthesis (`listen`, `stopListening`, `openFile`)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by reading.

At `9c0431fd5`, `apps/markdown/app.contract:105,123-137,171-173` declares `mutation cleared as shape SpeechSynthesisState then queueFirst` separately from `mutation held as shape SpeechSynthesisState` (line 109), and neither `queueFirst` (lines 129-130) nor `queueEven` (lines 131-137) checks `if listening` before calling `speak(...)` — unlike `keptEven` (line 148) and `keptOdd` (line 157), which both guard `if listening and evenRun == run` / `if listening and oddRun == run`.

**Trigger.**
1. User taps Listen (`testId="listen"`, `apps/markdown/app.contract:218`), running `action listen` (lines 123-128). This commits `listening = true` (immediately showing `testId="listen-bar"` with `testId="stop"`, lines 231-239) and sends `cleared = cancelSpeech()`.
2. Before `cleared` settles and its `then` handler `queueFirst` executes — `cancelSpeech()` is async across all platforms (`packages/speech-synthesis/web/index.js:189` via `host/web-js/modules.js:488,581-619`, which also lazy-imports the web implementation on first use; `packages/speech-synthesis/apple/SpeechSynthesis.swift:118` and `packages/speech-synthesis/android/SpeechSynthesis.kt:88` via async native `later` dispatch, as noted in `packages/speech-synthesis/test/drives.mjs:43-45`; and `then` runs in a subsequent commit per `host/web-js/rt.js:651-658`) — the user taps Stop (`testId="stop"`, line 238 -> `stopListening`) or opens another document (`testId="file-..."` / `testId="recent-..."` / `testId="open-file"` -> `openFile`, lines 119-121, which calls `stopListening()`).

**Actual vs. expected.** `stopListening` (lines 171-173) sets `listening = false` (hiding `listen-bar` and `testId="stop"`) and sends `held = cancelSpeech()`. Because `held` is a different mutation slot than `cleared`, `cleared` is neither superseded nor forgotten (`cleared = none` is never assigned). When `cleared` settles, `queueFirst` -> `queueEven(0)` unconditionally sends `queuedEven = speak(text=..., audioSessionType="playback")`. Block 0 starts speaking aloud while `listening == false` and the Stop/Pause controls are hidden; when `queuedEven` settles, `keptEven` sees `listening == false`, skips queueing block 1, and never cancels block 0, leaving block 0 speaking with no Stop button visible. Expected: `stopListening` and `openFile` must prevent any speech from starting once `listening` becomes `false` (consistent with the `if listening` guards already in `keptEven` and `keptOdd`).

**Verification & siblings.** Verified by reading `apps/markdown/app.contract:95-173,218-241`, `host/web-js/modules.js:485-620`, `host/web-js/rt.js:649-658`, `packages/speech-synthesis/{web/index.js:189,apple/SpeechSynthesis.swift:118,android/SpeechSynthesis.kt:88}`, and `packages/speech-synthesis/test/drives.mjs:43-45,88-96` (whose `stop` drive always waits for `clock data` + `+1000` before tapping `stop`).
Checked sibling apps: `apps/markdown` is the only app using `@exact/speech-synthesis`; checked sibling `mutation ... then` teardown handlers in `apps/photo-editor/app.contract:136-145,162-186` (`show` and `taken` guard on `top(nav).name == "camera"`, and `cancel`/`back` assign `shot = none` / `ordered = none`), `apps/shelf/app.contract:284-293` (`arm` guards on `current.name == "scan"`), and `apps/agents/app.contract:160-167` (`afterChange` guards on `top(nav).name == "new"`). None share this omission.

**Suggested fix & regression test.** In `apps/markdown/app.contract`, guard `queueFirst` with `if listening` (and optionally track `clearedRun = run` alongside `evenRun`/`oddRun`), and in `stopListening` assign `cleared = none` and bump `run = run + 1` so pending `cleared` or `queued*` completions cannot trigger speech. Add a drive case in `packages/speech-synthesis/test/drives.mjs` that taps `listen` and immediately taps `stop` (and another that switches files) without waiting for `queued(s)` in between, then settles and asserts `!voice.speaking && voice.queue.length === 0`.

### Siblings checked

Checked packages/speech-synthesis/test/fixture/app.contract and sibling mutation-then handlers in apps/photo-editor/app.contract:136-186, apps/shelf/app.contract:284-293, and apps/agents/app.contract:160-167 (all guard state or clear the mutation slot on cancel).
