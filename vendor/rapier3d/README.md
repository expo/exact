<p align="center">
  <img src="./assets/rapier-logo.jpg" alt="crates.io">
</p>
<p align="center">
    <a href="https://discord.gg/vt9DJSW">
        <img src="https://img.shields.io/discord/507548572338880513.svg?logo=discord&colorB=7289DA">
    </a>
    <a href="https://github.com/dimforge/rapier/actions">
        <img src="https://github.com/dimforge/rapier/workflows/Rapier%20CI%20build/badge.svg" alt="Build status">
    </a>
    <a href="https://crates.io/crates/rapier2d">
         <img src="https://img.shields.io/crates/v/rapier2d.svg?style=flat-square" alt="crates.io">
    </a>
    <a href="https://crates.io/crates/rapier3d">
         <img src="https://img.shields.io/crates/v/rapier3d.svg?style=flat-square" alt="crates.io">
    </a>
    <a href="https://opensource.org/licenses/Apache-2.0">
        <img src="https://img.shields.io/badge/License-Apache%202.0-blue.svg">
    </a>
</p>
<p align = "center">
    <strong>
        <a href="https://rapier.rs">Website</a> | <a href="https://rapier.rs/docs/">Documentation</a>
    </strong>
</p>

-----

<p align = "center">
<b>2D and 3D physics engines</b>
<i>for the Rust programming language.</i>
</p>

-----

## What is Rapier?

Rapier is a set of 2D and 3D physics engines for games, animation, and robotics. These crates
are `rapier2d`, `rapier3d`, `rapier2d-f64`, and `rapier3d-f64`. They are written with the Rust
programming language, by the [Dimforge](https://dimforge.com) organization. It is forever free
and open-source!

## Getting started

The easiest way to get started with Rapier is to:

1. Read the [user-guides](https://www.rapier.rs/docs/).
2. Play with the examples: `cargo run --release --bin all_examples2` and `cargo run --release --bin all_examples3`.
   Their source code are available on the `examples2d/` and `examples3d/` directory.
3. Don't hesitate to ask for help on [Discord](https://discord.gg/vt9DJSW), or by opening an issue on GitHub.

## Performance

SIMD-batched constraint solving and contact processing are always on: the
solver processes 4 contact manifolds per instruction, falling back to scalar
code on targets without SIMD support. For performance-sensitive applications,
also enable:

- **`parallel`** — multithreading of the whole physics step (broad phase,
  narrow phase, solver) through rayon. On CPUs with heterogeneous cores
  (Apple silicon, Intel hybrid), also call
  `PhysicsPipeline::set_dedicated_thread_pool(None)` to run the step on a
  pool sized to the performance cores only: the solver's barrier-paced stages
  otherwise run at the speed of the slowest (efficiency) core.

```toml
rapier3d = { version = "*", features = ["parallel"] }
```

## Python bindings

Python bindings are under development. They ship as four PyPI packages —
`rapier2d`, `rapier3d`, `rapier2d-f64`, `rapier3d-f64` — one per
(dimension, scalar) flavor. See [`python/README.md`](python/README.md) for how
to build the bindings, the docs, and the testbed from a checkout, and
[`python/docs/`](python/docs/) for the API documentation.

## AI coding disclaimer and policy

AI coding is extensively used for the implementation and maintenance of the following crates: `mjcf-rs`,
`rapier3d-mjcf`, as well as the Python bindings (`python/rapier-py*`), including their tests, examples, and docs.

We actively use AI assistance (with human reviews) for the following tasks:
- Documentation generation.
- Changelogs generation.
- Tests generation.
- CI configuration and scripts.

We accept contributions involving AI coding as long as:
- They are verified to work properly by a human.
- The code quality is up to human-written code standards.
- Include non-regression tests whenever applicable (which itself can be AI-generated).

## Resources and discussions

- [Dimforge](https://dimforge.com): See all the open-source projects we are working on! Follow our announcements
  on our [blog](https://www.dimforge.com/blog).
- [User guide](https://www.rapier.rs/docs/): Learn to use Rapier in your project by reading the official User Guides.
- [Discord](https://discord.gg/vt9DJSW): Come chat with us, get help, suggest features, on Discord!
- [NPM packages](https://www.npmjs.com/search?q=%40dimforge): Check out our NPM packages for Rapier, if you need to
  use it with JavaScript/Typescript.

Please make sure to familiarize yourself with our [Code of Conduct](CODE_OF_CONDUCT.md)
and our [Contribution Guidelines](CONTRIBUTING.md) before contributing or participating in
discussions with the community.

## Exact patches

Vendored from the crates.io rapier3d 0.35.3 archive (Apache-2.0) on 2026-09-18.
`game/physics/Cargo.toml` depends on this copy by path; the root workspace does
not depend on it. The list below is every change from the archive. In the
source, change 1 is marked `Exact:` and changes 2 to 6 `Exact2`.

1. **Pending optimization survives snapshots** (`src/geometry/broad_phase_bvh/mod.rs`).
   `deferred_optimize_pending` is serialized. The collision-only pipeline can
   leave it set between steps, and dropping it on load changes the next BVH
   update. `game/physics` wraps this representation in its versioned `EXPHYS`
   snapshot.
2. **Sleep on request** (`src/dynamics/rigid_body_components_more.rs`).
   `RigidBodyActivation::request_sleep` makes a body sleep-eligible at a given
   pose without flagging it asleep, so the engine can ask for sleep and let the
   island decide.
3. **Collision types filter pairs as groups do** (`src/geometry/collider.rs`,
   `src/geometry/broad_phase_bvh/update.rs`). `Collider::set_active_collision_types`
   marks the collider's groups changed when the types change, so the broad
   phase re-inserts its leaf and finds pairs the old types suppressed. A pair
   whose types now reject it (a sensor turned solid between two fixed colliders)
   ends as a separation does.
4. **Snapshots with holes** (`src/data/arena.rs`, `src/geometry/collider_set.rs`,
   `src/geometry/collider.rs`, `src/pipeline/physics_world.rs`; re-exported from
   `src/geometry/mod.rs` and `src/pipeline/mod.rs`). `PhysicsWorld::with_holes`
   serializes as the derived `Serialize` does, except that each collider the
   caller selects is written as a hole; slots, generations, the free list and
   the modification lists are kept. `FillHoles` reads it back, rebuilding each
   hole through the caller's function. `Collider::settled` clears pending change
   flags, so a rebuilt collider compares bit-exactly with one in a set.
   `game/physics` saves worlds in this format.
5. **Weight-bounded friction** (`src/dynamics/integration_parameters.rs`,
   `src/dynamics/solver/contact_constraint/contact_with_coulomb_friction.rs`,
   `src/dynamics/solver/staged_island_solver/worker.rs`).
   `IntegrationParameters::friction_gravity` bounds the friction of a touching
   contact point by its weight (cannon-es's model) instead of its normal
   impulse; below `friction_slip_speed` a point keeps the Coulomb bound. Both
   default to zero and are not serialized. With `friction_gravity` zero the
   solver is upstream's.
6. **A pair's touching state** (`src/geometry/contact_pair.rs`).
   `ContactPair::start_event_emitted` says whether the pair's last collision
   event was `Started`.
7. **Workspace boundary** (`Cargo.toml`). An empty `[workspace]` keeps the crate
   out of enclosing workspaces.

`LICENSE` (the Apache-2.0 text) is added; the archive has no license file.
`.cargo_vcs_info.json` and `Cargo.lock` are left out.

Three upstream files over 1,500 lines are split using same-module `include!`:
`rigid_body`, `rigid_body_components`, and `multibody`, each with a `_more.rs`
continuation. These splits change neither declarations nor visibility. Upstream
unsafe code is vendored unchanged; Exact adds no unsafe implementation.
