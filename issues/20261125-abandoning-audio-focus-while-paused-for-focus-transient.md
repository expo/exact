# Abandoning audio focus while paused for "focus-transient" permanently bricks Android TTS and leaks audio focus

**Status:** Open
**Systems:** @exact/speech-synthesis (Android, Apple)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by reading.

At `9c0431fd5`, `packages/speech-synthesis/android/SpeechSynthesis.kt` permanently wedges Android TTS and leaks Android audio focus whenever the speech queue empties while paused for `"focus-transient"`:

- `SpeechSynthesis.kt:258-260`: transient focus loss (`AUDIOFOCUS_LOSS_TRANSIENT` / `AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK`) adds `"focus-transient"` to `reasons`, and **only** `heard("focus-gained")` (`AUDIOFOCUS_GAIN`) removes it.
- `SpeechSynthesis.kt:90`: `"resumeSpeech"` lifts only `listOf("requested", "interrupted", "focus", "route")`, never `"focus-transient"`.
- `SpeechSynthesis.kt:314` (`cancelAll()`) removes only `"requested"` from `reasons`, and `SpeechSynthesis.kt:203-210` (`end()`) removes nothing from `reasons`.
- `SpeechSynthesis.kt:240-252` (`reclaim()`):
  ```kotlin
  val holds = (current != null || queue.isNotEmpty()) && reasons.all { it == "focus-transient" }
  val want = if (holds) strongest() else "auto"
  if (want == claimed) return
  claim?.release(); claim = null; claimed = want
  if (want == "auto" || context.agent) return
  claim = context.audioFocus.claim(want) { heard(it) }
  ```
  When the queue becomes empty (`current == null && queue.isEmpty()`), `holds` becomes `false`, releasing `claim`. `AudioFocus.release` (`host/android/canvas/src/main/java/dev/exact/canvas/AudioFocus.kt:47-61`) calls `manager.abandonAudioFocusRequest(it)` and empties `claims`, so Android will never deliver `AUDIOFOCUS_GAIN` (`"focus-gained"`).
- `SpeechSynthesis.kt:194-200` (`proceed()`): requires `reasons.isEmpty()` before starting queued utterances, and then calls `reclaim()`.

### Triggering sequence
1. On Android, start speaking with `speak(text=...)` (e.g., from `apps/markdown/app.contract:128-142` or any app using `@exact/speech-synthesis`).
2. Another app takes transient audio focus (`AUDIOFOCUS_LOSS_TRANSIENT` or `AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK`), adding `"focus-transient"` to `reasons`.
3. While paused for `"focus-transient"`, empty the queue by any of:
   - calling `cancelSpeech()` -> `cancelAll()` (`SpeechSynthesis.kt:88,308-319`),
   - backgrounding the app while speaking a default `transient` or `transient-solo` utterance (`foreground(false)` -> `cancelAll()`, `SpeechSynthesis.kt:425`), or
   - dropping the sole utterance handle (`dispose()` -> `end()`, `SpeechSynthesis.kt:403-415,203-210`).
4. `reclaim()` releases the `AudioClaim` and abandons Android audio focus, leaving `reasons == ["focus-transient"]`.
5. Call `speak(text=...)` again (or `cancelSpeech()` followed by `speak(...)`, the exact Listen/restart flow in `apps/markdown/app.contract:123-143` and `llp/1133.012-speech.rfc.md:77-79`).

Additionally, on both Android (`SpeechSynthesis.kt:314`) and Apple (`packages/speech-synthesis/apple/SpeechSynthesis.swift:393`), if the queue was paused for `"interrupted"`, `"route"` (headphones removed), or `"focus"` (permanent loss/refusal) and the app calls `cancelSpeech()` then `speak()`, `cancelAll()` removes only `"requested"`, leaving those reasons in `reasons` so the new `speak()` stalls silently until `resumeSpeech()` is explicitly called.

### Actual vs. expected
- **Actual:** After step 4, `"focus-transient"` can never be lifted by `resumeSpeech()` or `cancelSpeech()`. At step 5, `proceed()` sees `reasons.isEmpty() == false` and leaves every new utterance stalled in `queue`, while `reclaim()` sees `queue.isNotEmpty() && reasons.all { it == "focus-transient" }` (`holds == true`) and calls `AudioFocus.claim(...)`. Because `AudioManager.requestAudioFocus` returns `AUDIOFOCUS_REQUEST_GRANTED` synchronously without invoking `onAudioFocusChange(AUDIOFOCUS_GAIN)`, the module now holds Android audio focus indefinitely without speaking while TTS stays wedged. On Apple and Android, `cancelAll()` also leaves `"interrupted"`, `"route"`, and `"focus"` stuck across a cancel-and-restart sequence.
- **Expected:** Abandoning audio focus when no active claim remains waiting for `AUDIOFOCUS_GAIN` must clear `"focus-transient"`, and `cancelAll()` emptying the queue must clear queue-bound pause reasons (`"requested"`, `"interrupted"`, `"focus"`, `"focus-transient"`, `"route"`) so subsequent `speak()` calls start normally and audio focus is not leaked. Established by `SpeechSynthesis.kt:11-16,238-262`, `AudioFocus.kt:22,56-60`, and LLP 1133.012 D4 & D6 (`llp/1133.012-speech.rfc.md:253-366`).

### Verification and siblings checked
Verified by reading `SpeechSynthesis.kt`, `AudioFocus.kt`, `SpeechSynthesis.swift`, `packages/speech-synthesis/web/index.js`, `packages/speech-synthesis/exact.json`, and `apps/markdown/app.contract:123-174`.

Siblings checked:
- `packages/speech-synthesis/apple/SpeechSynthesis.swift:387-398`: does not use `"focus-transient"`, but has the same `cancelAll()` bug at `:393` (`reasons.removeAll { $0 == "requested" }`), leaving `"interrupted"` and `"route"` stuck when cancelling and re-speaking (e.g., `apps/markdown/app.contract:128-142` after headphone unplug or Siri interruption).
- `packages/speech-synthesis/web/index.js:134-159,190`: only ever adds `"requested"` to `reasons`, so `reasons.delete('requested')` clears all possible reasons (unaffected).
- Other Android `AudioFocus` consumers: `host/android/video/src/main/java/dev/exact/video/MediaPlayer.kt:628-640` releases its claim on any loss without waiting for `"focus-gained"`; `host/android/canvas/src/main/java/dev/exact/canvas/SoundOutput.kt:123-135,240-245` resets `interrupted = false` on `foreground(true)`.

### Suggested fix and regression test
1. In `SpeechSynthesis.kt`: whenever `reclaim()` drops the claim (`want == "auto"`), remove `"focus-transient"` from `reasons` (since no focus claim remains to receive `AUDIOFOCUS_GAIN`), and in `cancelAll()` remove `listOf("requested", "interrupted", "focus", "focus-transient", "route")` from `reasons` (keeping `"background"` if currently backgrounded, since `foreground(true)` lifts it).
2. In `SpeechSynthesis.swift`: in `cancelAll()` (`:393`), remove `"requested"`, `"interrupted"`, `"focus"`, and `"route"` from `reasons` (keeping `"background"` if backgrounded).
Add unit tests (Robolectric on Android, XCTest on Apple) covering: (a) `speak` -> `AUDIOFOCUS_LOSS_TRANSIENT` -> `cancelSpeech` / `foreground(false)` / `dispose` -> `speak`, asserting the new utterance speaks immediately and `pauseReasons` is empty; (b) `speak` -> `"interrupted"` / `"route"` / `"focus"` -> `cancelSpeech` -> `speak`, asserting the new utterance begins without requiring `resumeSpeech`.

### Siblings checked

Checked SpeechSynthesis.swift (affected by cancelAll leaving interrupted/route in reasons), packages/speech-synthesis/web/index.js (only uses requested, unaffected), and Android AudioFocus consumers MediaPlayer.kt and SoundOutput.kt (unaffected).
