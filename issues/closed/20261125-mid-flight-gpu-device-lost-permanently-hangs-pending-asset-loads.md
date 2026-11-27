# Mid-flight GPU device_lost() permanently hangs pending asset loads in both model-viewer and shader

**Status:** Closed
**Resolution:** Re-request pending assets (deduplicated) in ModelViewer::device_lost and Shader::device_lost so in-flight loads resume after Module::lose_device clears outstanding requests.
**Systems:** @exact/model-viewer, @exact/shader (Rust GPU surfaces — all platforms)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by reading.

At `9c0431fd5`, `ModelViewer::device_lost()` (`packages/model-viewer/rust/src/lib.rs:251-256`) and `Shader::device_lost()` (`packages/shader/rust/src/lib.rs:354-366`) do not re-enqueue pending asset requests after `Module::lose_device()` clears `inst.outstanding` and `inst.answered` (`gpu/src/lib.rs:989-991`), permanently hanging in-flight loads.

### Triggering sequence
1. Bind a non-empty `src` on `<model-viewer>` (`ModelViewer::bind` -> `request()` at `packages/model-viewer/rust/src/loading.rs:114-126`) or `<shader>` (`Shader::bind` at `packages/shader/rust/src/lib.rs:153-164` and `state.rs:39-48`).
2. Host drains `Module::take_assets(id)` (`gpu/src/lib.rs:1128-1158`), which calls `std::mem::take` on `self.assets` / `self.changes`, records the asset in `inst.outstanding`, and starts the async fetch/file read (e.g., Web `host/web/gpu-assets.js:34-116` or Apple live-clock `host/apple/Sources/ExactSurfaces/CanvasSeams.swift:457-499`).
3. A GPU device loss triggers `Module::lose_device()` (`gpu/src/lib.rs:978-998`) before `Module::asset()` delivers the bytes, followed by device recovery (`gpu_recover`).

### Actual vs. expected
- **Actual:** `Module::lose_device()` invokes `inst.surface.device_lost()` and then clears both `inst.answered` and `inst.outstanding` (`gpu/src/lib.rs:989-991`). Neither `ModelViewer::device_lost()` nor `Shader::device_lost()` re-adds its pending asset to `self.assets.requests` / `self.changes.requests`. After recovery, `take_assets()` returns empty requests; even if the host re-calls `bind()` with the same inputs (as `host/web/gpu-glue.js:424` does via `entry.bound = null`), both `ModelViewer::bind` (`lib.rs:180`) and `Shader::bind` (`lib.rs:153`) skip requesting when `next.src == self.arguments.src`. Any late pre-loss delivery is dropped on Web (`host/web/gpu-assets.js:110`, because `recoverDevice()` replaced `entry` at `gpu-glue.js:406, 427`) or rejected by `Module::asset()` (`gpu/src/lib.rs:1171-1176`, since the name is no longer in `inst.outstanding` or `inst.answered`).
  - `ModelViewer` stays stuck in `Source::Pending(load)` (or transitions to `Source::Pending(load)` at `loading.rs:255-261` when a superseded parse finishes with `job.bytes == None`, after an earlier failed delivery cleared `job.bytes` at `loading.rs:151-153` and `request()` attached a new load to the running job at `loading.rs:120-124`). `preparing()` (`lib.rs:225-227`) remains `true` forever, causing `render()` (`lib.rs:484, 525`) to request another frame indefinitely while `frame_ready()` and `loaded` stay `false`.
  - `Shader` stays stuck in `File::Pending(path)` (`state.rs:46`) forever with `frame_ready() == false` (`state.rs:170-176`), leaving the canvas permanently blank or frozen on the previous shader until `src` changes.
- **Expected:** Established by `Module::lose_device` / `Module::asset` (`gpu/src/lib.rs:978-998, 1128-1176`), `gpu/tests/it/module.rs:1105-1163` (`answered_names_beyond_256_retire_and_device_loss_reopens_delivery`), and sibling `game/render/src/surface.rs:522-549`. Any asset still awaited by a surface when the device is lost must be re-requested via `assets()` after recovery so the host fetches and delivers it to the recovered module.

### Verification & siblings
- **Verification:** Verified by reading the exact control flow through `gpu/src/lib.rs`, `host/web/gpu-glue.js`, `host/web/gpu-assets.js`, `host/apple/Sources/ExactSurfaces/CanvasSeams.swift`, `packages/model-viewer/rust/src/{lib.rs,loading.rs}`, and `packages/shader/rust/src/{lib.rs,state.rs}` at `9c0431fd5`. Existing tests (`packages/model-viewer/rust/src/tests.rs:427, 538` and `packages/shader/rust/tests/surface.rs:570`) call `surface.device_lost()` directly rather than through `Module::lose_device()`, and the shader test explicitly re-binds `""` then `next` before calling `surface.asset()`, masking the hang.
- **Siblings checked:**
  - `game/render/src/surface.rs:522-549`: `device_lost()` properly clears `self.waiting`, invalidates device assets on `sim`, and sets `self.assets_dirty = true` so `assets()` re-requests them.
  - `bench/extra-heavy/exact-xheavy/gpu/src/lib.rs:128-254` (`Wave`, benchmark-only): implements `assets()` with `self.asked` and does not override `device_lost()`, so an in-flight photo fetch there also stalls on device loss.
  - All other `Surface` implementations (`apps/caltrain/gpu`, `apps/weatherlight/gpu`, `packages/fixtures/sky`, `bench/crypto-list`) do not implement `assets()`.

### Suggested fix & regression test
- **Fix:** In `ModelViewer::device_lost()`, if `self.source` is `Source::Pending(load)` or `Source::Parsing { load, job }` with `load.sequence != job` (awaiting fresh asset delivery), push `load.name.clone()` into `self.assets.requests` (deduplicated). In `Shader::device_lost()`, if `self.state` is `File::Pending(path)` (stripping `"assets/"`), push the asset name into `self.changes.requests`.
- **Regression test:** Drive both `ModelViewer` and `Shader` through `exact_gpu::Module`: `bind` a valid `src`, call `take_assets(id)`, call `lose_device()` before `asset()`, assert a subsequent `take_assets(id)` re-emits the pending asset name, deliver it via `asset()`, and assert the surface reaches `frame_ready()` / `loaded == true`.

### Siblings checked

Checked all Surface::assets() implementations: game/render/src/surface.rs:522-549 properly re-requests assets on device_lost(); bench/extra-heavy/exact-xheavy/gpu/src/lib.rs (Wave benchmark) also omits device_lost() clearing of self.asked; no other production Surface implements assets().
