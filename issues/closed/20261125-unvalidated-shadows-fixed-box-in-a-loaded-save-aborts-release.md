# Unvalidated Shadows::fixed_box in a loaded save aborts release builds on render

**Status:** Closed
**Resolution:** Validated ShadowBox extents and finiteness during Data::read and set_field, and made Cascades::reach return None for invalid fixed_box values.
**Systems:** exact-game (environment), exact-game-render (shadows, renderer::draw)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Trust boundaries)

Verification: verified by reading.

### Location
At `9c0431fd5`:
- `game/engine/src/world.rs:321` and `game/engine/src/scene/register.rs:7`: `Shadows` is automatically registered as a resource on every `World`.
- `game/engine/src/environment.rs:403-411, 427-475`: `ShadowBox` derives `Data` and `Shadows` uses `opt_in_record!` (`game/engine/src/data/mod.rs:393-446`), decoding arbitrary `f32` values (`game/engine/src/data/bin.rs:286-293`) — including `0.0`, negative numbers, `NaN`, and `±inf` — with no domain validation.
- `game/engine/src/world.rs:1147-1168, 1293-1308` (`World::load`) and `game/engine/src/sim/snapshot.rs:91-171` (`Sim::restore` / `Sim::restore_bound`, invoked by `WorldSurface::restore` at `game/render/src/surface.rs:491-497`): accept the decoded `Shadows` resource without validating `fixed_box`.
- `game/render/src/world/scene.rs:433-438`: copies `w.try_resource::<Shadows>()` verbatim into `FrameInput.sun.shadows`.
- `game/render/src/renderer/draw.rs:763-771`: `prepare_effects` checks only `sun.illuminance != 0.0` and `Cascades::reach(frame, *s).is_some()`, then calls `Cascades::new(frame, s)`.
- `game/render/src/shadows.rs:28-33`: `Cascades::reach` inspects only camera `near`/`far` against `settings.distance`, ignoring `settings.fixed_box`.
- `game/render/src/shadows.rs:67-73`: `Cascades::new` hits `assert!(fixed.center.is_finite() && w.is_finite() && w > 0. && d.is_finite() && d > 0., "sun box must be finite with positive extents")`.
- `game/Cargo.toml:48`: `[profile.release]` sets `panic = "abort"` (inherited by `[profile.web]` and `[profile.host-dev]`).

### Trigger
Load an `EXGAME v6` save via `World::load` or an `EXSIM v8` save via `Sim::restore` / `WorldSurface::restore` containing a `Shadows` resource with `fixed_box: Some(ShadowBox { center: Vec3::ZERO, half_width: 0.0, half_depth: 10.0 })` (or negative / `NaN` / `inf` extents or non-finite `center`) into any game that has a shadowed `DirectionalLight` (`illuminance != 0.0`, `shadows: true`), then render one frame. (A game inserting the same `Shadows` resource directly reaches the same abort.)

### Actual vs. expected
- **Actual:** `World::load` and `Sim::restore` succeed, and on the next rendered frame `Cascades::new` panics at `game/render/src/shadows.rs:70-73` inside `gpu_render`, aborting release builds (`panic = "abort"`).
- **Expected:** Untrusted saves must be rejected with `DataError` during decode/restore rather than panicking in release builds (per `game/engine/src/sim/snapshot.rs:183-184` and `game/physics/src/state.rs:154-155`), and the renderer must not panic on degenerate `Shadows` settings (matching the fix for `Shadows::distance <= camera.near` in `Cascades::reach`, `game/render/src/shadows.rs:28-33` and `issues/closed/20261110-camera-near-past-shadow-distance-panics.md`).

### Verification & siblings
Verified by reading the exact decode and render path at `9c0431fd5` (`World::load` / `Sim::restore` -> `Scene::frame` -> `Renderer::prepare_effects` -> `Cascades::reach` -> `Cascades::new`). Checked sibling scene components and resources:
- `Camera` (`game/engine/src/scene/camera.rs:24-35`, `game/render/src/world/scene.rs:191-195`): filtered by `Camera::valid()` before use; invalid saved cameras are ignored without panicking.
- `PointLight`, `SpotLight`, `LocalShadows` (`game/render/src/world/scene.rs:110-125, 334-340`, `game/render/src/local_shadows.rs:32-48`): skip non-finite/non-positive intensity, range, or position and clamp angles/resolution safely.
- `Mesh` primitives (`game/render/src/world.rs:1053-1057`, `game/render/src/quads/primitives.rs:72-80`): call `mesh.validate().map_err(RenderError::scene)?` before invoking `shapes::*` geometry builders.
- `Water`, `Fog`, `Bloom`, `AmbientOcclusion` (`game/render/src/water.rs:49-133`, `game/render/src/renderer/draw.rs:794-808`): clamp or guard values without render-time panics.
- Other `Shadows` fields (`distance`, `cascades`, `resolution`, `softness`) are guarded by `Cascades::reach`, `.clamp`, `.max(0.0)`, and `outline()`'s `is_finite()` check. `Shadows::fixed_box` is the remaining unguarded `assert!` in `Cascades::new`.

### Suggested fix & regression test
1. Validate `ShadowBox` in `Data::read` (or a `ShadowBox::valid` helper checked in `Shadows::read` and `Cascades::reach`), rejecting non-finite `center` or non-finite/`<= 0.0` `half_width`/`half_depth` with `DataError` on load, AND make `Cascades::reach` return `None` (or ignore an invalid `fixed_box`) when `settings.fixed_box` is invalid so direct resource writes also cannot abort release builds.
2. Add regression tests in `game/engine` verifying `World::load` / `Sim::restore` reject a save carrying `Shadows { fixed_box: Some(ShadowBox { half_width: 0.0 | -1.0 | f32::NAN, .. }) }` with `DataError`, and in `game/render/src/shadows.rs` verifying `Cascades::reach` returns `None` (no panic) for invalid `fixed_box`.

### Siblings checked

Camera (Camera::valid), PointLight/SpotLight/LocalShadows (world/scene.rs, local_shadows.rs), Mesh primitives (world.rs, quads/primitives.rs), Water/Fog/Bloom/AmbientOcclusion (water.rs, renderer/draw.rs), and other Shadows fields (shadows.rs): all validated, clamped, or skipped without panicking on render; Shadows::fixed_box is the lone remaining unguarded assert! in Cascades::new.
