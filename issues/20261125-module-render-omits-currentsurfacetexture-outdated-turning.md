# Module::render omits CurrentSurfaceTexture::Outdated, turning recoverable Vulkan swapchain invalidation into a permanent surface error

**Status:** Open
**Systems:** gpu / Vulkan (Android, Linux, Windows)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Two sources of truth)

Verification: verified by reading.

At `9c0431fd5`, `Module::render` (`gpu/src/frame.rs:257-278`) matches `wgpu::CurrentSurfaceTexture` for `Success`, `Suboptimal`, `Timeout | Occluded`, and `Lost`, but omits `Outdated`, letting it fall into `other => { self.error = format!("surface: {other:?}"); return None; }`. On Vulkan, `vendor/wgpu-hal/src/vulkan/swapchain/native.rs:514-516, 549` returns `hal::SurfaceError::Outdated` on `VK_NOT_READY`, `VK_ERROR_OUT_OF_DATE_KHR`, or Intel invalid image indices, which `wgpu-core` (`present.rs:270`) and `wgpu` (`api/surface.rs:156`) surface as `CurrentSurfaceTexture::Outdated` (and `gpu/src/acquire.rs:326` forwards unchanged from the off-thread acquire worker).

**Triggering sequence.** On a Vulkan host presenting through `Module::render` (such as Android `SurfaceView`/`TextureView` canvases via `host/painted/src/surfaces/android.rs:550-559`), invalidate the swapchain without changing its pixel dimensions `(w, h)` (for example, a 180° orientation/transform change on Android or compositor surface invalidation returning `VK_ERROR_OUT_OF_DATE_KHR` or `VK_NOT_READY` from `vkAcquireNextImageKHR`).

**Actual vs. expected.** `Module::render` records `"surface: Outdated"` and returns `None` without reconfiguring the surface. Because the device is not lost (`has_device(id)` remains `true`), `gpu/src/native.rs:497-502` returns error code `2` (not `3`). In `host/painted/src/surfaces/android.rs:555-559`, code `2` records `self.surfaces.error`, leaves `wants = false`, and does not trigger recovery. Moreover, because `gpu/src/frame.rs:223-231` only calls `target.configure` when `config.width != w || config.height != h`, even subsequent renders at unchanged dimensions never reconfigure the swapchain and fail with `Outdated` permanently. Expected behavior (per wgpu's `CurrentSurfaceTexture::Outdated` contract and sibling handlers) is to reconfigure the surface and request another frame (`Some(true)`), just as `Current::Suboptimal` already does at `gpu/src/frame.rs:259-267`.

**Verification and siblings.** Verified by reading `gpu/src/frame.rs:223-278`, `gpu/src/acquire.rs:130-138, 326`, `gpu/src/native.rs:485-503`, `host/painted/src/surfaces/android.rs:550-579`, `vendor/wgpu-hal/src/vulkan/swapchain/native.rs:511-550`, and `wgpu-30.0.2` (note: browser WebGPU in `wgpu-30.0.2` does not emit `Outdated`, so this affects native Vulkan targets). Siblings checked: `gpu/src/window.rs:485-492`, `host/painted/src/gpu/present.rs:114-123`, and `host/ink/src/device/wgpu/present.rs:236-248` all explicitly handle `CurrentSurfaceTexture::Outdated` by reconfiguring/recreating the surface; sibling `host/ink/src/device/wgpu/scanout.rs:250-254` (Linux DRM/KMS direct Vulkan scanout) also omits `Outdated` and returns `Error::Device` instead of reconfiguring.

**Suggested fix and regression test.** Handle `Current::Outdated` alongside `Current::Suboptimal` in `gpu/src/frame.rs:259-267` (drain off-thread acquire via `inst.acquire.reconfigure()`, call `crate::swapchains(|| target.configure(&gpu.device, config))`, and return `Some(true)`). Add a unit test in `gpu/tests/it/frame.rs` covering `CurrentSurfaceTexture::Outdated` recovery (or factor the `CurrentSurfaceTexture` disposition match into a testable helper asserting `Outdated` triggers reconfigure + `Some(true)` rather than an error).

### Siblings checked

Checked all get_current_texture() sites in the repo: gpu/src/window.rs:485-492, host/painted/src/gpu/present.rs:114-123, and host/ink/src/device/wgpu/present.rs:236-248 all handle Outdated properly; host/ink/src/device/wgpu/scanout.rs:250-254 (Linux DRM Vulkan scanout) shares the omission and maps Outdated to Error::Device. Also checked wgpu-30.0.2 webgpu backend (does not return Outdated).
