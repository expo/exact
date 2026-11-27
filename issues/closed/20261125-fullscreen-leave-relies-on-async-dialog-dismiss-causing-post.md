# FullScreen.leave() relies on async Dialog.dismiss(), causing post-release ExoPlayer calls and stale fullscreenchange events on pooled players

**Status:** Closed
**Resolution:** Made FullScreen.leave clear state and detach or notify synchronously before dismissing the dialog, guarded MediaPlayer.left against post-release calls, and cleared fullscreen on bind and release, with Robolectric regression tests.
**Systems:** Android Video host (FullScreen / MediaPlayer / Natives videoPool)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Out of order)

Verification: verified by reading.

At `9c0431fd5`, `FullScreen.leave()` (`host/android/video/src/main/java/dev/exact/video/FullScreen.kt:76`) only calls `dialog?.dismiss()` and relies exclusively on `shown.setOnDismissListener { if (dialog === shown) finish() }` (`FullScreen.kt:45,78-82`) to clear `dialog`/`view` and run `left(made)`. On Android, `Dialog.dismiss()` dispatches `OnDismissListener` asynchronously via `mListenersHandler` on a later main-looper turn, so `full.on` (`dialog != null`, `FullScreen.kt:31`) stays `true` and the fullscreen `PlayerView` remains attached to `exo` until that later turn.

This breaks three synchronous callers in `MediaPlayer`:
1. **Post-release calls on `ExoPlayer` (`host/android/video/src/main/java/dev/exact/video/MediaPlayer.kt:333-341`)**: `release()` sets `sink = null`, calls `full.leave()`, and immediately calls `exo.release()`. On the next looper turn, `OnDismissListener` runs `finish()` -> `left(made)` (`MediaPlayer.kt:395-401`), which has no released guard and calls `PlayerView.switchTargetView(exo, view, inline)` (reattaching `inline` to the released player) or `view.player = null; exo.setVideoTextureView(texture)`, followed by `layoutPicture()` (`MediaPlayer.kt:463-466`) which reads and writes `exo.trackSelectionParameters` on the already-released `ExoPlayer`.
2. **Cross-element state and event corruption on pooled players (`host/android/canvas/src/main/java/dev/exact/canvas/Natives.kt:947,1277,1283,1406-1411` & `MediaPlayer.kt:117,131-147,194,324-331,386-393`)**: A `<video>` with `controls=false` can enter fullscreen programmatically (`MediaPlayer.kt:194,386-393`, clearing `texture` from `exo`). When retired (`Natives.kt:1406-1411`), `player.stop()` calls async `full.leave()`, while `player.lendable` (`controls == null`, `MediaPlayer.kt:117`) is `true`, so `e.view` is immediately placed in `videoPool`. If another `<video>` borrows it via `obtain` -> `bind(newSink)` -> `update(...)` (`Natives.kt:947,1277,1283`) before `OnDismissListener` runs, `bind()` (`MediaPlayer.kt:131-147`) never cancels pending fullscreen teardown: `full.on` is still `true`, so `update()` reports `"fullscreen": true` to `newSink` (`MediaPlayer.kt:202,620`), drops any `fullscreen()` request on the new element (`MediaPlayer.kt:387`), and leaves a `controls=false` borrower without `texture` attached until the next looper turn — at which point the delayed `left(made)` runs against the new element and emits a spurious `fullscreenchange("false")` event and `pushState()` onto `newSink`.
3. **Out-of-order presenter switch (`MediaPlayer.kt:153-154,194,202,620`)**: Toggling `controls` while `full.on` calls `full.leave()` and immediately runs synchronous `presenter(...)` while `full.on` is still `true` and the fullscreen `PlayerView` still holds `exo`, reporting stale `"fullscreen": true` in `pushState()` (line 202), dropping a same-update `fullscreen()` request (line 194 -> line 387), and firing `left(made)` late on the next turn. (`hidden()` at line 302 similarly leaves `full.on == true` until the next turn.)

### Trigger
1. Unmount a `<video>` or stop/destroy the activity while fullscreen is open (`MediaPlayer.release()`).
2. Call `requestFullscreen()` on a `<video>` with `controls=false`, retire it into `videoPool` (`Natives.retire`), and mount another `<video>` in the same turn (`Natives.obtain` -> `MediaPlayer.bind`).
3. Toggle `controls` on a `<video>` while fullscreen is open (`MediaPlayer.update`).

### Actual vs. expected
- **Actual:** `full.on` remains `true` and `left(made)` fires asynchronously on a later looper turn against a released `ExoPlayer`, a newly bound pooled `<video>`, or a synchronously switched presenter.
- **Expected:** `FullScreen.leave()` clears `dialog`/`view` and finishes teardown synchronously while suppressing the subsequent `OnDismissListener` callback, and `release()` dismisses the fullscreen dialog without calling `left()`/`switchTargetView()`/`layoutPicture()` on the released `ExoPlayer` — preserving `MediaPlayerTest.kt:95-118` invariants (*"aBorrowedPlayerCarriesNothingOfTheNodeBefore"* and *"nothingIsReportedAfterRelease"*).

### Verification & sibling sweep
Verified by reading `FullScreen.kt`, `MediaPlayer.kt`, and `Natives.kt`. Sibling implementations checked:
- `host/android/canvas/src/main/java/dev/exact/canvas/Surfaces.kt:378-383`: `close(s)` sets `s.quiet = true` and clears `shown = null` synchronously before `s.dialog?.dismiss()`.
- `host/apple/videoarm/VideoArm.swift:1173,1180,1257,1337-1340,1369`: Apple's `VideoArm` clears `fullscreen?.left = nil` and `fullscreen = nil` synchronously before dismissing and guards `leftFullscreen()` with `guard !invalidated`.
- `host/android/canvas/src/main/java/dev/exact/canvas/Pickers.kt:198,249`: single-use picker dialogs without pooled reuse or player reattachment.

### Suggested fix & regression test
In `FullScreen`, make `leave(notify: Boolean = true)` capture and clear `dialog` and `view` synchronously before calling `shown.dismiss()` (so the later `OnDismissListener` check `dialog === shown` is false), detaching `made.player = null` when `!notify` (for `release()` / `bind()`) and calling `left(made)` synchronously when `notify` is true. Guard `MediaPlayer.left()` with a `released` flag as defense in depth, call `full.leave(notify = false)` in `release()` and `bind()`, and ensure `stop()` leaves fullscreen cleanly before pooling. Add Robolectric tests in `host/android/video/src/test/java/dev/exact/video/MediaPlayerTest.kt` covering: (1) `release()` while fullscreen followed by `Looper.idle()` emits nothing and makes no post-release calls; (2) retiring/re-binding a fullscreen `controls=false` player emits no `fullscreenchange("false")` and reports `fullscreen: false` to the new sink; (3) toggling `controls` while fullscreen reports `fullscreen: false` immediately in `pushState()`.

### Siblings checked

Checked Surfaces.kt:378-383 (clears shown and sets quiet synchronously before dialog.dismiss), Apple VideoArm.swift:1173,1180,1257,1337-1340,1369 (clears fullscreen/left before dismiss and guards invalidated), and Pickers.kt:198,249. None share this bug.
