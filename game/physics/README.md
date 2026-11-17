# exact-game-physics

Vendored Rapier 0.35.3 behind Exact's saved components, in metres, kilograms and seconds.
Call `register` during setup, `physics::capsule(w, "player").step(velocity)` after
controls, and `physics::step` per tick. The capsule component is `CapsuleController`;
it can coexist in one world with the core flat-ground `Character` on a different
entity; combining both controllers on one entity is refused. The step result includes
the actual `displacement` and `grounded` state.

- Entity-ordered insertion maps `Body`, `Collider` and world `Transform` to Rapier;
  ordered handle maps and last-write comparisons detect edits, teleports and removal.
  Synchronization visits the rows written since the previous step (`World::changed`),
  every Body and every parented collider; the first step after setup, restore or clone
  visits every row. A static collider without a Body costs nothing per tick.
  Dynamic poses, velocities and sleep return to components; kinematics use next pose.
- One step uses `world.dt()`. A collision-only pass after moving kinematics supplies
  same-tick sensor transitions. Events are sorted by entity pair, preserving each
  pair's transition order within the step; `Announce` journals that order.
  Sleeping bodies still step; any awake dynamic body reports busy to Sim, even
  when its pose is unchanged. Support edits wake all.
  Two fixed solids (colliders without a Body, or with a Static one) never pair, so a
  static world carries no contact state; a sensor pairs with anything, fixed or not.
  So a static collider the game teleports or carries under a parent (it has no Body)
  emits no Touch, and journals no Announce, against static walls or terrain; give it
  a Kinematic Body, or make it a sensor, to keep those transitions.
- Density defaults to 1000 kg/m³; explicit mass is kg, and `physics::mass` reads the
  mass a step gave a body, derived or not. `Body::inertia` authors the principal
  inertia along the body's axes (all zero derives it). `Body::force` is a world-space
  force at the centre of mass that persists until written back to zero; it is saved
  in the Rapier snapshot and replays exactly after a restore. Friction combines
  geometrically. Restitution combines by `Collider::bounce_combine`: `Max` (the
  default), `Multiply`, `Min` or `Average`; a pair whose colliders differ takes the
  later of those (Rapier's priority), so `Average` needs both. Bodies and colliders
  write every field, including defaults, so decoding a save into an existing value
  replaces every field. A patch keeps fields it omits. Contact slop is 0.1 mm;
  other integration defaults are Rapier's. The Physics resource sets the solver:
  `solver_iterations` (substeps, 4), `contact_frequency` and `static_contact_frequency` (the contact springs
  between moving bodies and against a fixed one, 30 and 60 Hz),
  `contact_damping_ratio` (both springs, 10) and `coulomb_friction` (one friction
  constraint per contact point instead of Rapier's grouped friction with a twist
  constraint). They are applied each step, live in the saved snapshot, and enter
  the resource's encoding only when they differ from those defaults.
  `friction_gravity` (with `coulomb_friction`) bounds friction by weight, as
  cannon-es does: a point touching when a step starts may take friction ×
  `friction_gravity` × the pair's reduced mass × the substep's duration in each
  substep, whatever its normal load, so a contact lifting off keeps its friction
  for that step; below `friction_slip_speed` a point keeps the Coulomb bound, so
  resting and rolling contacts settle flat. Contacts with multibodies keep Coulomb
  friction. The vendored Rapier carries the bound (marked Exact2 in
  `contact_with_coulomb_friction.rs`); its two integration parameters are not in
  the snapshot, since the resource holds them, so EXPHYS stays v3. At zero, the
  default, every contact keeps Rapier's Coulomb bound exactly.
- `Collider::extra` lists `ExtraShape`s (shape, offset, friction, bounce and its
  combine rule, layer, mask) added to the collider's Body with no mass; they are
  Rapier colliders on that body, rebuilt with the primary and saved with the world
  (`Entry::extra_handles`, written only when present). An entity with extras
  reports one begin and one end per entity pair, derived from Rapier's per-collider
  touch state before and after the step (a pair that begins and ends within the
  step is reported begin then end), so a touch with two of its shapes is one
  touch. For now queries and capsules see the primary shape only. Extras need a
  Body and cannot be meshes or heightfields.
- Shapes: sphere, box, Y capsule/cylinder (total height), static mesh and heightfield
  (row-major, rows Z/columns X), and a convex hull of 4 to 255 points that any body
  may use, optionally rounded (its mass and inertia are the rounded solid's). Curved shapes and
  rounded hulls need uniform positive scale; a flat hull is refused.
  Bodies are roots; parented boxes must not acquire shear. Sweeps translate convex shapes.
- A fast dynamic body is swept against static colliders each step. `Body::ccd` also
  sweeps it against kinematic and dynamic bodies, except other `ccd` bodies.
- `let q = physics::queries(world); q.raycast(..); q.sweep(..);` shares a lazy
  query scene retained in the world's physics executor, outside Data and hashes.
  Drop the scope before structural edits or `step`; the next scope reuses it.
  Each operation updates only the colliders whose Body/Collider/Transform/Parent rows
  were written since the last one, plus parented colliders (their pose follows their
  ancestors), so same-tick edits are visible. A write to a collider-free entity (a
  camera, a tracer) costs a membership test; loading or another world rebuilds.
  Capsule rays (also a capsule under an offset) use the closed form: Parry 0.30's
  support-map capsule raycast misses rays that pass straight through (64 of 16,000
  in RIVALS). Other shapes use Parry's ray cast.
  The free query functions are thin one-shot calls through this same cache.
  The capsule handle queries a BVH of only the colliders it can reach that step,
  built in entity order, so its result never depends on the scene's edit history. Capsules use Rapier's steps/slopes/snap, saved-pose platform transport and an
  80 kg default push budget. A default 1 m³ crate is 1,000 kg and cannot be pushed
  by that controller; author `Body { mass: 10., ..Default::default() }` for a light crate.
  Movement and push share the layer-mask/sensor/self filter;
  the character's rigid collider is a sensor, but other controllers are solid to its
  movement: characters block, slide around and push out of each other in call
  order. A character on a layer outside the mover's mask passes through.

EXPHYS v3 writes each static collider (no Body) whose rebuild from its entry, the
Collider and pose it was built from, is byte-identical to Rapier's copy as a hole,
and rebuilds it on restore: a static collider costs its entry (about its components)
plus its broad-phase leaf, about 280 bytes for an offset cylinder. The broad phase,
whose shape orders pairs, stays saved. V2 persisted
`BroadPhaseBvh::deferred_optimize_pending`, which V1 omitted. Both are refused by
name before replacing the destination (including inside EXSIM). Start a new world.
Rapier is consumed by path from `vendor/rapier3d`; it stays outside the root
workspace's dependency graph.

Saved state is opaque bincode/serde for bodies, colliders, islands, broad/narrow phase,
joints and integration parameters, plus entity/handle maps and last writes. Restore validates and decodes live state atomically; pipeline/CCD workspaces are scratch under Rapier's serialization contract.
The hole format is `PhysicsWorld::with_holes`/`FillHoles` in the vendored Rapier
(`pipeline/physics_world.rs`, marked Exact2), which keeps slots, generations and
the free list.
`Data::write(&self)` refreshes dirty bytes for save, hash and JSON; `refresh_snapshot`
measures the same operation. Stepping does not serialize. JSON summarizes the opaque bytes by length/hash.
A hash (`Writer::digests`) reads a digest instead of the content: the snapshot
bytes' hash and each entry's hash in entity order, an entry rehashed only after
sync or writeback touches it. Both are functions of the saved content, recomputed
identically after a load. At 100k static trees a hash after a step costs about
10 ms of snapshot encoding plus 12 ms, against 11 + 39 ms reading every entry.
Malformed or obsolete Rapier payloads fail during `World::load`, before replacement.

Continuation tests preserve the complete authoritative snapshot; `Executor::clone`
refreshes and decodes that same representation. There is no parallel field-by-field
Rapier clone. See [state.rs](src/state.rs), [continuation tests](tests/paranoid.rs)
and [the pile diagnostic](examples/pile.rs).

The physics cards in [tests/pins.json](tests/pins.json) cover minimal-120 and
pile-600. Native arm64 macOS and the web-profile `minimal` Wasm under Bun agree
in Off/Save/FreshGame, including the pile's restored continuation. Native x86-64
Linux has EXPHYS v2 fixture parity evidence; the current pin values have not been
rerun there. The living controller rests at tick 547 with Off/Save/FreshGame
agreement.

`tests/continuation/mod.rs` reaches a non-initial
`deferred_optimize_pending = true` boundary at tick 6 through two bulk
collision-only refreshes after a regular step. Off/Save/FreshGame and restored
continuation agree at tick 12; clearing only the serialized pending flag changes
the subsequent hash. Both continuations have consumed the flag by then: the
divergence is subsequent executor state, not a retained flag bit. This proves
the saved executor boundary, not that every kinematic scene reaches it.

Reproduce from `game/` with `EXACT_UPDATE_TRUST=development`:
```
cargo test -p exact-game-physics --no-fail-fast -- --nocapture
cargo test -p exact-game
cargo clippy -p exact-game-physics --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p exact-game-physics --release --example pile -- 1000 2000 5000
cargo run -p exact-game-physics --release --example pile -- --verify
cargo test -p exact-game-physics --release --test scale -- --ignored --nocapture
cargo build -p exact-game-physics --profile web --target wasm32-unknown-unknown --example minimal
```

`Collider::of(&mesh)` supports cube, cuboid, sphere, capsule, cylinder and plane.
A plane makes a static 1 cm slab with its top face at Y=0. Ring, torus, cone and
assets require explicit collision geometry.

The copyable controller composition is [tests/living.rs](tests/living.rs):
`Move`/`Jump`/`Gravity` feed an upright unit-scale `CapsuleController` over a heightfield
slope, step and ledge; a rotated/scaled animated rig is its `Ambient` visual child.
Animation root motion is discarded, never applied after collision movement. The
explicit 10 kg crate is pushed, falls and sleeps. The sun turns, the child animates and a
derived HUD counts down throughout. Off/Save/FreshGame pin the same rest at tick 547:
player `(2.2726393, 1.5475401, -0.0044510923)`, crate
`(4.3423862, -1.5000682, 0.061061338)`. Ordinary time continues to night. The fixture
builds its two-node model in memory, so the physics test invokes neither the baker nor
the network. The imported Fox remains covered by the bake, renderer and skinned-game
fixtures; this test adds no game.
