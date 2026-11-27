# Unvalidated physics save handles and solver settings abort release builds after load

**Status:** Closed
**Resolution:** Validated saved physics entry uniqueness, handle/component correspondence, Rapier handle existence in Saved::decode, and gravity/solver settings in Physics::read.
**Systems:** exact-game-physics (state, step, lib, types)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Trust boundaries)

Verification: verified by reading.

At `9c0431fd5`, `Executor::read` -> `Saved::decode` (`game/physics/src/state.rs:138-180, 281-288`) and `Physics::read` (`game/physics/src/types.rs:102-122, 357-369`) accept malformed physics saves without validating saved handles, entry invariants, or solver settings, causing unchecked indexing and `assert!`/`unwrap()` panics—which abort release/web builds (`panic = "abort"` in `game/Cargo.toml:48, 66`)—after `World::load` returns `Ok(())`.

### Triggering sequence
Load a crafted `EXGAME v6` save via `World::load` (`game/engine/src/world.rs:1147-1168`) or `EXSIM v8` via `Sim::restore` containing a `Physics` resource with:
- A valid empty `EXPHYS\0\x03` Rapier blob plus an `Entry` with nonexistent `body_handle: Some([999, 0])` or `collider_handle: Some([999, 0])` (or `extra_handles: vec![[999, 0]]`), or `body_handle: Some(...)` with `body: None`; or
- Invalid `Physics` fields such as `solver_iterations: 0`, `gravity: Vec3::NAN`, `contact_frequency: 0.0`, or `friction_gravity: 1.0` with `coulomb_friction: false`.

`Saved::decode` (`state.rs:160-178`) only runs `FillHoles` on actual holes in the Rapier snapshot (none exist in an empty `PhysicsWorld`), silently drops duplicate entities in `BTreeMap` (`state.rs:148`), and hands unchecked `entries` into `Live::new` (`state.rs:84-109`), which populates `live.bodies` from `e.body_handle.is_some()` and `live.reverse` from `e.colliders()` without checking `rapier.bodies`/`rapier.colliders`.

### Actual vs. expected result
- **Actual:** `World::load` returns `Ok(())`. Subsequent calls panic and abort release builds:
  - `physics::mass(world, entity)` (`game/physics/src/lib.rs:56`): immediate out-of-bounds panic in `live.rapier.bodies[h]`.
  - `physics::step(world)`: panics on `assert!` for gravity/solver settings (`step.rs:589`, `507-520`), `live.entries[e].body.as_ref().unwrap()` (`step.rs:498`, `720`), `&mut r.bodies[entry.bh().unwrap()]` (`step.rs:262`, `719`), and `&mut r.colliders[...]` (`step.rs:360`, `378`, `649`).
  - `world.save()` / `world.hash()`: after a single `physics::step` that takes the `unchanged_asleep` fast path (e.g., an entity whose `Collider`/`Transform` matches its `Entry` carrying a bogus `collider_handle: Some([999, 0])`), `step.rs:591` sets `saved.dirty = true` without panicking, and the next `save()`/`hash()` calls `verify(live)` (`state.rs:312`), panicking on `&live.rapier.colliders[ColliderHandle::from_raw_parts(h[0], h[1])]`.
- **Expected:** `game/physics/src/state.rs:154-155, 454-455` explicitly establishes that a save is untrusted input and nothing during or after load may panic because release builds abort (`game/Cargo.toml:48`). `Saved::decode` and `Physics::read` must validate all saved handles, entry invariants, and solver parameters and return `Err(DataError)` during `World::load`/`Sim::restore`.

### Verification & siblings checked
Verified by reading `game/physics/src/state.rs:84-180, 281-320, 454-500`, `game/physics/src/step.rs:183-745`, `game/physics/src/lib.rs:51-57`, `game/physics/src/types.rs:76-125, 324-396`, and `game/Cargo.toml:43-69`.
Siblings checked:
- `game/physics/src/queries.rs` (`Scene::new`/`refresh`) rebuilds its query `PhysicsWorld` directly from world components rather than saved `Executor` handles, so it does not index saved `Entry` handles (note: invalid saved `Body`/`Collider`/`CapsuleController` component values in saves still trigger `assert!`/`panic!` in `step::sync`, `queries::Scene::refresh`, and `character::move_capsule` at `character.rs:22-35`, which is a closely related sibling gap on component validation).
- `Saved::decode` at `state.rs:139-141` also returns `Ok(())` when `self.bytes.is_empty()` even if `self.entries` is non-empty, silently ignoring stale entries.

### Suggested fix & regression test
1. In `Saved::decode` (`state.rs:138-180`): reject `!self.entries.is_empty()` when `self.bytes.is_empty()`; reject duplicate `entry.entity` or duplicate body/collider handles; verify every `entry.body_handle` exists in `rapier.bodies` and satisfies `entry.body_handle.is_some() == entry.body.is_some()`; verify every `entry.collider_handle` and `extra_handles` handle exists in `rapier.colliders` (and `collider_handle.is_some() == entry.collider.is_some()`, `extra_handles` length matches `collider.extra`).
2. Add validation in `Physics::read` (or a dedicated `Physics::validate` called at the end of `Data::read`) enforcing the exact same finite/range predicates checked by `step.rs:507-520, 589`.
3. Add regression tests in `game/physics/src/state.rs` feeding crafted `EXGAME v6` saves with dangling body/collider/extra handles, `body_handle: Some` + `body: None`, duplicate entries, and invalid `Physics` gravity/solver settings to `World::load`, asserting `Err(DataError)` for each.

### Siblings checked

game/physics/src/queries.rs (builds Scene from components, not saved handles), game/physics/src/character.rs:22-35 (asserts on invalid saved CapsuleController components at move time rather than load time), game/physics/src/state.rs:139-141 (empty bytes skips entry validation).
