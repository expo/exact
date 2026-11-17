# The vendored Rapier's patch note lists two changes and says the rest is unchanged; the source marks six more places

**Status:** Closed
**Resolution:** The README's Exact section now lists all seven changes a diff against the crates.io 0.35.3 archive finds (the collision-type fix, the hole format, and the weight-bounded friction and pair touch state added since the issue among them) and says that `game/physics` selects the copy by path.
**Systems:** vendor/rapier3d
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`vendor/rapier3d` is the one vendored crate with no `EXACT-PATCHES.md`. Its inventory is a section appended to upstream's README (`vendor/rapier3d/README.md:107-122`, "Exact snapshot patch (2026-09-18)") at `dd6f5dcd`. It names two changes, the serialized `deferred_optimize_pending` and `RigidBodyActivation::request_sleep`, plus three file splits, and then says: "All other behavior and dependency versions are unchanged."

The source marks eight places, seven with `Exact2` and one with `Exact:`. Six are not in the note. They are two further changes, and the first alters what a simulation does:

| Marked at | Change | In the note |
|---|---|---|
| `src/geometry/broad_phase_bvh/mod.rs:101` (`Exact:`) | `deferred_optimize_pending` survives snapshots | yes |
| `src/dynamics/rigid_body_components_more.rs:445` | `request_sleep` | yes |
| `src/geometry/collider.rs:194` | `set_active_collision_types` inserts `ColliderChanges::GROUPS`, so the broad phase re-inserts the leaf | no |
| `src/geometry/broad_phase_bvh/update.rs:550` | a pair whose collision types now reject it ends like a separation | no |
| `src/geometry/collider.rs:67` | `Collider::settled` | no |
| `src/geometry/collider_set.rs:478` | colliders written as holes in a snapshot | no |
| `src/data/arena.rs:1193` | an arena serialized with selected values left out | no |
| `src/pipeline/physics_world.rs:108` | `PhysicsWorld` serialization with holes | no |

The note's last sentence is also wrong about how the crate is selected: "only game/Cargo.toml patches it in". The `[patch.crates-io]` table of `game/Cargo.toml` (lines 73-80) has no `rapier3d`; `game/physics/Cargo.toml:11` depends on it by path.

## Why it matters

Every other inventory under `vendor/` exists so that taking a new upstream release does not lose an Exact correction (`vendor/taffy/EXACT-PATCHES.md:13-15`). A refresh of Rapier guided by this note would restore two changes and drop the collision-type fix and the hole format that `game/physics` saves worlds in (`game/physics/README.md:66-68` names `PhysicsWorld::with_holes` "in the vendored Rapier … marked Exact2").

## Verification

`grep -rn "Exact2\|Exact:" vendor/rapier3d/src` prints the eight places above. Read each and the README section. Not done: a diff against upstream rapier3d 0.35.3, which is not in this machine's Cargo registry, so changes that carry no marker are not counted.

## Direction

Complete the note: list the collision-type change, the hole format and whatever a diff against the 0.35.3 archive adds, and correct the sentence about how the crate is selected. The other vendored crates keep this in an `EXACT-PATCHES.md`; moving the note there is the owner's choice.

## Related

`issues/closed/20260901-taffy-patch-provenance-stale.md` was the same drift in Taffy's inventory. No open issue or QUEUE entry names `vendor/rapier3d`.
