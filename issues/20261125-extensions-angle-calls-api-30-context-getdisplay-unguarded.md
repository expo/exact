# Extensions.angle calls API 30 Context.getDisplay() unguarded, crashing every Android 10 (API 29) app in ExactActivity.onCreate

**Status:** Open
**Systems:** Android Canvas host (Extensions / ExactActivity startup on API 29)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Copy-paste drift)

Verification: verified by reading.

At `9c0431fd5`, `Extensions.angle(activity: Activity?)` in `host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt:619-624` calls `activity?.display?.rotation` without a `Build.VERSION.SDK_INT >= 30` check:

```kotlin
fun angle(activity: Activity?): Int = when (activity?.display?.rotation) {
```

`Context.getDisplay()` was introduced in Android 11 (API 30). `Extensions.angle` is called unconditionally during startup and on display/orientation reads:
- `host/android/canvas/src/main/java/dev/exact/canvas/ExactActivity.kt:165`: `ExactActivity.onCreate()` unconditionally calls `Extensions.bind(this)` before `setContentView(root)` (line 188).
- `host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt:645`: `Extensions.bind(context)` calls `told = angle(context as? Activity)` on first bind.
- `host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt:631`: `displays.onDisplayChanged` calls `angle(Exact.reader?.get())`.
- `host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt:341`: `ExtensionContext.orientation` calls `Extensions.angle(activity)`.

### Triggering sequence
Launch any Exact Android app built with `host.android.minimumOS: "10"` (`minSdk = 29`) on an Android 10 (API 29) device. `ExactActivity.onCreate()` reaches line 165 -> `Extensions.bind(this)` -> `Extensions.angle(this)` -> `Context.getDisplay()`.

### Actual vs. expected
- **Actual:** Every Android 10 (API 29) app crashes immediately at launch inside `ExactActivity.onCreate()` with `java.lang.NoSuchMethodError` on `Context.getDisplay()` before `setContentView`. Even if `Extensions.bind` were first invoked from `Headless` with an `Application` context, the first display change (`Extensions.kt:631`) or `ExtensionContext.orientation` read (`Extensions.kt:341`) throws `NoSuchMethodError`.
- **Expected:** `host/android/canvas/build.gradle.kts:20` (`minSdk = 29`), `host/android/app/build.gradle.kts:57-59` (`minimumOS = "10"` -> `minimumApi = 29`), `host/android/README.md:1313` (all post-API-29 calls must be behind `Build.VERSION.SDK_INT`), and `host/android/canvas/src/main/java/dev/exact/canvas/Legacy.kt:39-40` (`Legacy.display(activity)`) establish that API 30 calls must fall back to `Legacy.display(activity)` on API 29.

### Verification and siblings
Verified by reading `Extensions.kt`, `ExactActivity.kt`, `Legacy.kt`, `host/android/canvas/build.gradle.kts`, `host/android/app/build.gradle.kts`, and `host/android/README.md`.
Siblings checked:
- `ExactActivity.kt:630`, `Orientation.kt:92`, `Device.kt:78-82`, and `MainActivity.kt:31` all properly guard `Context.getDisplay()` with `if (Build.VERSION.SDK_INT >= 30) ... else Legacy.display(...)` (or `WindowManager.defaultDisplay`).
- `FrameTiming.kt:208` and `Presents.kt:62` call `view.display` (`View.getDisplay()`, API 17 — safe on API 29).
- Grepped `packages/*/android/` and `host/android/widgets/` — no other unguarded `Context.getDisplay()` calls exist.

### Suggested fix and regression test
Guard `Context.getDisplay()` in `Extensions.angle` (`host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt:619`):
```kotlin
fun angle(activity: Activity?): Int {
    val display = activity?.let { if (android.os.Build.VERSION.SDK_INT >= 30) it.display else Legacy.display(it) }
    return when (display?.rotation) {
        android.view.Surface.ROTATION_90 -> 90
        android.view.Surface.ROTATION_180 -> 180
        android.view.Surface.ROTATION_270 -> 270
        else -> 0
    }
}
```
Add a Robolectric unit test annotated `@Config(sdk = [29])` in `host/android/canvas/src/test/java/dev/exact/canvas/` exercising `Extensions.angle(activity)` and `Extensions.bind(activity)` on API 29.

### Siblings checked

ExactActivity.kt:630, Orientation.kt:92, Device.kt:78-82, MainActivity.kt:31 (all guard Context.getDisplay() with SDK_INT >= 30); FrameTiming.kt:208 and Presents.kt:62 (View.getDisplay(), API 17); checked packages/*/android/ and host/android/widgets/ with no other unguarded Context.getDisplay() calls.
