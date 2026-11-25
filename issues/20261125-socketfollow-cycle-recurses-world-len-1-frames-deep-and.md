# SocketFollow cycle recurses world.len() + 1 frames deep and overflows the call stack

**Status:** Open
**Systems:** exact-game (scene, animation::sockets), exact-game-render (world::scene::Attachments)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Quadratic work, stack depth and allocation)

Verification: verified by reading.

At `9c0431fd5`, resolving a `SocketFollow` cycle recurses `world.len() + 1` hops directly on the call stack with no visited-entity check and no constant depth bound, overflowing the stack on large worlds (and incurring quadratic row-lease scan overhead even below overflow).

### Location & triggering sequence
When `SocketFollow` is registered (`game/engine/src/animation/sockets.rs:185-190`), `world.attachments` is `Some(Attachments { pose: follower_pose })`. Thereafter, any `World::global(e)` (`game/engine/src/scene.rs:871-875`), `Globals::get(e)` (`game/engine/src/scene/globals.rs:31-33`), `World::current_global(e)` (used by `emitter::step`, `audio::step`, `World::drawn`, and picking), or `animation::socket_matrix(w, e, joint)` (`sockets.rs:113-118`) seeds `remaining = self.len() + 1` (`scene.rs:894`, `sockets.rs:117`).

Create (at runtime or via `World::load`/`Sim::restore`, since `World::validate_hierarchy` at `world.rs:1153` and `scene.rs:763-799` only validates `Parent` cycles) a world with ~20,000+ entities where two entities `A` and `B` have loaded `Mesh::Asset` models containing joint `j`, `A` has `SocketFollow::new(B, "j")`, and `B` has `SocketFollow::new(A, "j")`. Calling `w.global(A)` or `animation::socket_matrix(&w, A, "j")` enters the mutual recursion loop:
- `current_global_depth_at` (`scene.rs:910-915` or `:926-931`)
- -> `follower_pose` (`sockets.rs:192-209`, holding `Ref<SocketFollow>`)
- -> `compose_socket` (`sockets.rs:151-181`, holding `Ref<Pose>` and `Ref<Mesh>`)
- -> `current_global_depth(e, remaining - 1)` (`sockets.rs:177`) -> `current_global_depth_at`.

### Actual vs. expected result
- **Actual:** A 2-entity `SocketFollow` cycle recurses `world.len() + 1` hops (~80,000+ call frames at 20k entities), overflowing the native or 1 MiB `wasm32` call stack (`SIGSEGV`/abort/trap). Two compounding defects worsen it:
  1. Each hop holds `Ref<SocketFollow>`, `Ref<Pose>`, and `Ref<Mesh>` across the recursive call; `Holds::acquire` (`game/engine/src/storage/lease.rs:47-59`) linearly scans the live hold stack on every acquire, turning a single `global()` call into `O(world.len()^2)` work even on medium worlds below stack overflow.
  2. If the recursion does bottom out at `remaining == 0`, `compose_socket` returns `Err` -> `follower_pose` returns `None`, so `current_global_depth_at` (`scene.rs:910-916`) falls through to `e`'s `Transform` and returns `Some(...)`, causing every unwinding frame to multiply the joint and offset matrices `world.len()` times into a corrupted pose instead of failing or returning the fallback cleanly.
- **Expected:** Per `scene.rs:763-799, 918-938` (iterative `Parent` resolution and cycle rejection on `World::load`) and `sockets.rs:130-132, 152-154` (explicit intent to refuse follower cycles gracefully), `SocketFollow` resolution must detect cycles immediately via a visited set / constant depth bound (and validate cycles in `World::load`) without recursing `O(world.len())` frames on the call stack.

### Verification & siblings checked
Verified by reading `game/engine/src/scene.rs:763-799, 871-939`, `game/engine/src/scene/globals.rs:28-33`, `game/engine/src/animation/sockets.rs:113-209`, `game/engine/src/world.rs:1147-1168`, and `game/engine/src/storage/lease.rs:47-59`.

Siblings checked:
- **Confirmed sibling in renderer (`game/render/src/world/scene.rs:925-989`):** `Attachments::frame` seeds `remaining = self.items.len()` (`:952, 977`) and mutually recurses `Attachments::matrix` -> `owner_matrix` -> `matrix(i, alpha, remaining - 1)` with no visited-entity check and no constant depth cap. With many `SocketFollow` entities and a cycle, it similarly overflows the stack and performs `O(items.len()^2)` work per frame.
- `Parent`-only chain walks (`scene.rs:918-938` and `scene/hierarchy.rs`) are iterative and `validate_hierarchy` rejects `Parent` cycles on `World::load`.

### Suggested fix & regression test
1. Convert `current_global_depth_at` / `follower_pose` / `compose_socket` to an iterative loop (or pass a small visited list/set and a small fixed attachment-chain depth cap such as 64, dropping row leases before stepping to the target), and propagate cycle failure rather than falling through at the bottom of the chain.
2. Extend `World::validate_hierarchy` (or an attachment validation pass in `World::load`) to reject `SocketFollow` and mixed `Parent`+`SocketFollow` cycles.
3. Apply the same visited-set / constant depth cap to `Attachments::matrix` / `owner_matrix` in `game/render/src/world/scene.rs:925-989`.
4. Add unit tests with a 2-entity `SocketFollow` cycle inside a 25,000-entity world verifying `World::global`, `Globals::get`, `animation::socket_matrix`, `World::load`, and `Attachments::frame` terminate in `O(1)` hops without overflowing the stack.

### Siblings checked

game/render/src/world/scene.rs:925-989 (Attachments::matrix/owner_matrix/frame has the same unvisited mutual recursion bounded only by self.items.len()); game/engine/src/scene.rs:918-938 & scene/hierarchy.rs (Parent-only walks are iterative and validated on load).
