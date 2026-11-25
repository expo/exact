# WorldSurface format change rebuilds RendererWithAssets without invalidating device assets, permanently losing GPU textures

**Status:** Open
**Systems:** exact-game-render (surface, surface::render), exact-game (sim::assets)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Asymmetric pairs)

Verification: verified by reading.

### Location
At `9c0431fd5`:
- `game/engine/src/sim/assets.rs:267-272`: `Sim::take_textures()` moves CPU texture payloads out via `std::mem::take(&mut self.textures)` ("no CPU mip copy survives upload"), while `Sim::asset_prepared` (`sim/assets.rs:274-288`) records each `.tex` asset in `self.world.assets.prepared`.
- `game/render/src/surface.rs:502-521`: `WorldSurface::device_ready` only calls `sim.invalidate_device_assets()` when `family_changed || (!self.device && !self.device_assets_invalidated)`.
- `game/render/src/surface.rs:522-549` (`WorldSurface::device_lost`) and `game/render/src/surface/args.rs:36-57` (`WorldSurface::release_renderer`): establish the invariant that replacing a live `self.render` must call `sim.invalidate_device_assets()` (and clear `self.waiting`/`self.uploads`, setting `assets_dirty = true`) so `.tex` payloads are retired and redelivered by the host.
- `game/render/src/surface/render.rs:236-273` (`prepare_now`): when `self.format != Some(format)` on an already-initialized `WorldSurface` (`self.render.is_some()`), it drops the old `RendererWithAssets` (destroying all resident GPU textures in `renderer.models.textures`), calls `self.hooks.device_lost()`, and constructs a fresh empty `RendererWithAssets`, but never calls `sim.invalidate_device_assets()` or clears `self.uploads`.
- `game/render/src/surface/render.rs:508-534` (`render_frame`): duplicates the same `if self.format != Some(format)` replacement block without asset invalidation (note that in the current call order `render_frame:425` calls `prepare_assets` first, so for `ASSETS == true` with `self.render.is_some()`, `prepare_now:244-273` is the site that replaces the live renderer).

### Trigger
Any `ASSETS = true` `WorldSurface` whose target `wgpu::TextureFormat` changes on the same device without `device_lost()`:
1. **Windows composed window + readback (`G::SAMPLES == SceneSamples::One`):** `Module::present_window` (`gpu/src/window.rs:441` -> `gpu/src/frame.rs:26-28`) renders using hardcoded `wgpu::TextureFormat::Rgba8Unorm`, while `Module::readback_into` (`gpu/src/readback.rs:126-129`, `gpu/src/instance.rs:7-13`, `game/render/src/surface.rs:640-642`) on a headless/window-composed instance (`inst.config.is_none()`) uses `inst.offscreen_format() == Rgba8UnormSrgb`. Any readback/screenshot and return to window rendering flips `format` and drops `self.render`.
2. **Android / `Module::attach` after headless readback or `attach_buffers`:** Rendering first via `Module::readback_into` (`gpu/src/readback.rs:129, 167`) or `Module::attach_buffers` (`gpu/src/target.rs:157-182`, `gpu/src/frame.rs:386, 411`) uses `offscreen_format()` (`Rgba8Unorm` / `Rgba8UnormSrgb`). Calling `Module::attach` (`gpu/src/target.rs:100-144`) to attach a window whose `configure()` format differs (e.g., `Rgba16Float` when `G::HIGH_DYNAMIC_RANGE == true` at `target.rs:219-226`, or `Bgra8Unorm`/`Bgra8UnormSrgb`) calls `detach` (not `device_lost`) and `device_ready` (which skips invalidation because `self.device` is already `true` and the compression family is unchanged).

### Actual vs. expected
- **Actual:** `prepare_now` replaces `self.render` with a new `RendererWithAssets` whose `models.textures` is empty. Previously uploaded `.tex` payloads were already moved out of `sim.textures` by `Sim::take_textures()` and remain marked prepared in `sim.world.assets.prepared`, so `WorldSurface::assets()` never re-requests them. Sprites stay permanently invisible, `MaterialOverrides` textures never become ready (`game/render/src/world/assets.rs:91, 115`), and textured models permanently lose their textures.
- **Expected:** Replacing a live `self.render` on format change must invalidate device assets symmetrically with `WorldSurface::device_lost` (`surface.rs:522-549`) and `WorldSurface::release_renderer` (`surface/args.rs:36-57`) so the host redelivers `.tex` payloads and models/textures re-prepare into the new renderer.

### Verification & siblings
Verified by reading the full format-selection and renderer-lifecycle paths across `gpu/src/{target,readback,frame,window,instance}.rs`, `game/engine/src/sim/assets.rs`, and `game/render/src/{surface.rs,surface/render.rs,surface/args.rs,surface/look.rs,world/assets.rs}` at `9c0431fd5`. Sibling sites checked:
- `WorldSurface::release_renderer` (`game/render/src/surface/args.rs:36-57`): correctly routes through `Surface::device_lost(self)` (preserving `placed.children` textures and resetting `device_assets_invalidated`) when releasing a live renderer on the same device.
- `WorldSurface::draw_look` (`game/render/src/surface/look.rs:293-324, 394-418`): reuses `self.format` when `live` and refuses `ASSETS == true` when `!live`, so it cannot trigger this bug.
- `ModelViewer` (`packages/model-viewer/rust/src/lib.rs:488-507`): rebuilds `Renderer::new` on format mismatch directly from CPU-resident `model.scene`, so textures are not lost.

### Suggested fix & regression test
1. In `prepare_now` (`game/render/src/surface/render.rs:244`) — before `sim.take_textures()` at line 236, or via a shared helper modeled on `WorldSurface::release_renderer` (`surface/args.rs:36-57`) — when `self.render.is_some()` and `self.format != Some(format)`, release the old renderer through `release_renderer` (calling `sim.invalidate_device_assets()`, clearing `self.waiting` and `self.uploads`, preserving host-owned `self.placed.children` textures, and keeping `self.device = true`). Deduplicate the renderer-creation block between `prepare_now` and `render_frame`.
2. Add a GPU surface test in `game/render/src/surface.rs` (alongside `surface_lifecycle_rebuilds_textured_draws_after_prepared_retry_loss`) that prepares and renders a textured model under `Rgba8Unorm`, then calls `prepare_assets`/`render` with `Rgba8UnormSrgb` (or `Bgra8UnormSrgb`) on the same device without `device_lost()`, and asserts `surface.assets()` requests redelivery of the `.tex` asset and the subsequent frame renders identically after redelivery.

### Siblings checked

WorldSurface::release_renderer (game/render/src/surface/args.rs:36-57, correctly calls device_lost while keeping placed.children), WorldSurface::draw_look (game/render/src/surface/look.rs:293-324, 394-418, reuses self.format when live and refuses ASSETS when !live), ModelViewer (packages/model-viewer/rust/src/lib.rs:488-507, retains CPU textures in model.scene).
