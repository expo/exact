# Module::texture_from_metal copies the already-overwritten Metal texture into previous, breaking Composite { previous: true } crossfades on iOS and tvOS

**Status:** Closed
**Resolution:** Added Module::prepare_texture_metal (gpu_prepare_texture_metal) to snapshot previous children and wait for GPU reads before Shadow.renderTexture overwrites the shared MTLTexture, and removed the post-overwrite copy from texture_from_metal.
**Systems:** gpu / Apple (iOS & tvOS Metal)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Asymmetric pairs)

Verification: verified by reading.

At `9c0431fd5`, on iOS and tvOS (`#if os(iOS) || os(tvOS)`), `CanvasesHost.capture` (`host/apple/Sources/ExactSurfaces/GpuIOS.swift:537-544`) runs the default zero-copy Metal path (`Capture.cpu` defaults to `false` at `host/apple/Sources/ExactKit/IOS/CaptureIOS.swift:31`). It first calls `m.sync()` (`gpu/src/lib.rs:1257-1269`), which only flushes and waits for the device to go idle without snapshotting previous children. It then calls `Shadow.shared.renderTexture(overlay, scale: scale)` (`host/apple/Sources/ExactKit/IOS/Shadow.swift:109-122`), which reuses the single `handed.texture` whenever `(w, h)` is unchanged, clears it to transparent and draws the new children into it (`Shadow.swift:196-212`), commits, and waits for completion before returning the same `MTLTexture` pointer. Finally, `GpuIOS.swift:544` calls `gpu_texture_metal` -> `Module::texture_from_metal` (`gpu/src/lib.rs:1285-1424`), where `same` at `gpu/src/lib.rs:1314` is `true` and lines `1407-1420` copy `texture` into `previous`.

**Triggering sequence.** On iOS or tvOS, run any surface requesting `ChildrenMode::Composite { previous: true }` such as `GlassSurface` in Caltrain (`apps/caltrain/gpu/src/glass.rs:89-91, 166-188`, `apps/caltrain/gpu/shaders/glass.wgsl:116-117`). After the initial children capture at size `(W, H)`, wait >= 100 ms (`CONTINUOUS_MS`) and trigger any same-size UI update (e.g., switching stations or content inside the glass canvas), which invokes `captureIfNeeded()` -> `capture()` -> `Shadow.shared.renderTexture` -> `gpu_texture_metal`.

**Actual vs. expected.** Because `Shadow.renderTexture` clears and overwrites `handed.texture` in place *before* `Module::texture_from_metal` runs, `texture_from_metal`'s `copy_texture_to_texture(texture -> previous)` at `gpu/src/lib.rs:1413-1419` copies the *already-overwritten* new frame into `previous`. Both `children` and `previous` hold identical pixels of the new frame, so `GlassSurface` crossfades from the new frame to itself (an instant pop instead of a 320 ms fade, while still doing a wasted full-screen GPU copy and 320 ms fade pass). Expected behavior is established by `gpu/src/lib.rs:528, 1278-1279` and the byte-upload path `Module::texture` (`gpu/src/children.rs:237-269`), which copies `texture -> previous` *before* `queue.write_texture` overwrites `texture`.

**Verification and siblings.** Verified by reading the full iOS/tvOS capture path (`GpuIOS.swift:510-566`, `CaptureIOS.swift:31`, `Shadow.swift:68-122, 196-212`, `gpu/src/lib.rs:1257-1424`, `apps/caltrain/gpu/src/glass.rs:89-188`). Siblings checked: `gpu/src/children.rs:237-269` (`Module::texture`, used by `GpuMac.swift:380-385`, `host/painted/src/surfaces/placed.rs:157-199`, and iOS CPU fallback `GpuIOS.swift:546-552`) correctly copies `texture -> previous` prior to writing new bytes; `host/web/gpu-glue.js:261-282` does not use `texture_from_metal` or `Composite` uploads.

**Suggested fix and regression test.** Snapshot `texture -> previous` *before* `Shadow.shared.renderTexture` overwrites `handed.texture` — e.g., have a pre-capture ABI entry point (or `gpu_sync`/a dedicated `gpu_prepare_texture_metal(id)` called right before `renderTexture`) enqueue the `texture -> previous` copy and wait for device idle, and remove the post-overwrite copy from the `same` branch of `Module::texture_from_metal` (or double-buffer `handed` in `Shadow.swift` while keeping stable views on the module side). Add a test in `gpu/tests/it/` that calls `texture_from_metal` twice with the same `MTLTexture` pointer after mutating its contents in between (matching `Shadow.renderTexture`) and asserts that `previous` still holds the first capture's pixels while `children` holds the second's.

### Siblings checked

Checked gpu/src/children.rs:237-269 (Module::texture copies texture -> previous before queue.write_texture, unaffected), host/apple/Sources/ExactSurfaces/GpuMac.swift:380-385 (uses m.texture, unaffected), host/apple/Sources/ExactSurfaces/GpuIOS.swift:546-552 (CPU fallback uses m.texture, unaffected), host/painted/src/surfaces/placed.rs:157-199 (uses gpu_texture, unaffected), and host/web/gpu-glue.js:261-282 (no texture_from_metal path, unaffected).
