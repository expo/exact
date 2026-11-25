# Shaders mutates a singleton Matrix and FloatArray concurrently across the main thread and async row-recording worker thread

**Status:** Open
**Systems:** Android Canvas host (Player / Shaders / async row recording)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Wrong scope)

Verification: verified by reading.

At `9c0431fd5`, `Shaders` (`host/android/canvas/src/main/java/dev/exact/canvas/Shaders.kt:14-43`) is a process-wide Kotlin `object` holding two unsynchronized mutable scratch fields:

- `private val local = Matrix()` (`Shaders.kt:15`)
- `private val values = FloatArray(9)` (`Shaders.kt:16`)

When `Player.replay()` processes op `43` (`SHADER`, emitted for CSS linear/radial/conic gradients and SVG gradients in `host/painted/src/canvas/gradient.rs:19,26-99,107-120`), `host/android/canvas/src/main/java/dev/exact/canvas/Player.kt:1102` calls `Shaders.read(b, i)`, which writes `values[0..8]`, calls `local.setValues(values)`, and passes `local` to `shader.setLocalMatrix(local)` (`Shaders.kt:37-41`).

However, `ExactView` replays op streams on two threads concurrently whenever `async` is enabled (the default at `host/android/canvas/src/main/java/dev/exact/canvas/ExactView.kt:139`):
- On the main thread via `main: Player` (`ExactView.kt:126,425,594,914,1243,1296`), `relayer: Player` (`ExactView.kt:102,129`), `slots`'s `Player` (`ExactView.kt:135`), and `Pane.player` (`host/android/canvas/src/main/java/dev/exact/canvas/Surfaces.kt:39,109`).
- On `worker` (`Executors.newSingleThreadExecutor()`, `ExactView.kt:137`) via `background: Player` (`ExactView.kt:130`), scheduled from `makeRow` (`ExactView.kt:1285`) into `record` -> `worker.execute { ... background.replay(...) }` (`ExactView.kt:1425-1436`).

### Trigger
Scroll a list whose rows (or page chrome/layers) contain CSS gradients (`linear-gradient`, `radial-gradient`, `conic-gradient`) or SVG gradients so `worker` records offscreen rows with `background.replay()` while the main thread concurrently records onscreen rows, layers, or the page with `main.replay()` / `relayer.replay()`.

### Actual vs. expected
- **Actual:** Concurrent `Shaders.read()` calls from the main thread and `worker` race on the singleton `values` array and the native C++ `SkMatrix` wrapped by `local`. One thread's matrix coefficients overwrite the other's before `local.setValues(values)` / `shader.setLocalMatrix(local)` completes (and concurrently mutate/read the underlying `SkMatrix` and its type mask), baking corrupted gradient transforms into persistent `RenderNode` display lists and causing undefined concurrent native access to `SkMatrix`.
- **Expected:** Each `Player` owns its own scratch state (`Player.kt:62-63`: *"Its scratch objects are its own: one Player a thread"*), or `Shaders.read` uses caller-owned / per-call scratch storage rather than shared mutable singleton fields (as `Shadows.kt:45` explicitly documents: *"Rows record on the worker as well as the main thread: nothing here is shared but the cache"*).

### Verification & sibling sweep
Verified by reading `Shaders.kt`, `Player.kt`, and `ExactView.kt`. Sibling helpers called from `Player.replay()` checked:
- `host/android/canvas/src/main/java/dev/exact/canvas/Shadows.kt:20-87`: allocates local `r`, `src`, `dst` per call and synchronizes `masks`.
- `host/android/canvas/src/main/java/dev/exact/canvas/LegacyGlyphs.kt:21-75`: synchronizes `faces`, `glyphs`, and `images` caches and creates per-call `Path`/`Matrix`.
- `host/android/canvas/src/main/java/dev/exact/canvas/Player.kt:21-35,676-716`: `Icons.map` is a `ConcurrentHashMap`; `iconBounds`/`iconValues`/`iconMatrix` are per-`Player` fields.
- `host/android/canvas/src/main/java/dev/exact/canvas/PaintedOrigins.kt:32-91`: all mutating/reading methods are `@Synchronized`.
- `host/android/canvas/src/main/java/dev/exact/canvas/Legacy.kt:141-144`: allocates fresh `FloatArray` per call.

### Suggested fix & regression test
Pass per-`Player` scratch `Matrix` and `FloatArray(9)` (already available on `Player`, or dedicated `shaderMatrix`/`shaderValues` fields on `Player`) into `Shaders.read(b, i, matrix, values)` — or allocate a local `Matrix()` and `FloatArray(9)` inside `Shaders.read` — and remove the singleton `local` and `values` fields from `Shaders`. Add a unit test under `host/android/canvas/src/test/` that runs two threads calling `Shaders.read` in a tight loop with distinct gradient matrices and asserts every returned shader's `getLocalMatrix` matches its input matrix.

### Siblings checked

Checked Shadows.kt:20-87 (per-call locals + synchronized cache), LegacyGlyphs.kt:21-75 (per-call Path/Matrix + synchronized caches), Player.kt Icons/Polylines (ConcurrentHashMap + per-Player scratch), PaintedOrigins.kt (@Synchronized), and Legacy.radial (no shared state). None share this bug.
