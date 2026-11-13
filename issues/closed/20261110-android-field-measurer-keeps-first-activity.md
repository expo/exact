# The Android field measurer keeps the first activity, and what it drew, for the life of the process

**Status:** Closed
**Resolution:** `Fields.measure` makes its thread's measuring `EditText` in the application context with the activity's theme resource (`Fields.detached`), so the measurer kept for the life of exact2's thread holds no activity.
**Systems:** host/android/canvas/src/main/java/dev/exact/canvas/Fields.kt, host/android/canvas/src/main/java/dev/exact/canvas/ExactActivity.kt
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

At `dd6f5dcd`. `Fields.measure` (`Fields.kt:875`) measures a hosted field's text for exact2's layout in an `EditText` that is made once per thread and kept in a static `ThreadLocal` (`:788`):

```kotlin
private val measurer = ThreadLocal<EditText>()
…
val v = measurer.get() ?: bare(EditText(unadjusted(context))).also { … measurer.set(it) }
```

`context` is `Fields.context`, which `ExactActivity.read` sets to the activity (`ExactActivity.kt:250`), and `unadjusted` wraps it in a `ContextThemeWrapper` (`Fields.kt:905`). The thread is exact2's own (`Owner`'s `HandlerThread`, `Owner.kt:83`), which lives as long as the process. So the `EditText` holds the activity that was reading when the first field was measured, and nothing replaces it: `release` clears `Fields.context` (`ExactActivity.kt:535`) but not the measurer, which no other line touches.

The host outlives its activities by design (LLP 1115.001 D15.1: an activity made again adopts the host that runs). When the first activity is destroyed, the measurer still reaches it, and through its `view` field the last `ExactView` it made: every picture's `Bitmap` in `images`, and the kept rows' `RenderNode`s in `rows` and `groups`, none of which `onDetachedFromWindow` clears (`ExactView.kt:509`). Each reader makes or takes its own bitmaps (`ExactView.kt:1466`), so the next activity's reader holds a second copy.

**The failure.** An app with a text field (Shelf), and its first activity destroyed while the process lives: Back at the first screen on Android 10 or 11, which finishes it (Android 12 and later [move the task to the background](https://developer.android.com/about/versions/12/behavior-changes-all#back-press) instead), or on any version a configuration change the manifest does not declare. Then open the app again.

- observed (by reading): the first `ExactActivity` and its view's pictures are never collected;
- expected: a destroyed activity is unreachable. A destroyed `Activity` that is still referenced is the leak Android Studio's heap dump reports ([Show activity/fragment leaks](https://developer.android.com/studio/profile/capture-heap-dump)).

**Verification.** Read only: the reference chain above. Not run: no Android device or emulator was available, so no heap dump was taken. The check to make on one: with "Don't keep activities" on, open Shelf's form, press Home, open it again, capture a heap dump and select "Show activity/fragment leaks".

**Direction.** Make the measuring field in a context that does not hold the activity (the application context with the activity's theme resource applied), or drop the thread's measurer when the reader that made it lets go of the host, so the next reader's theme measures the next layout.

**Related.** None in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md`.
