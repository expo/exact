# The executor three hosts share is source under `host/apple`, compiled by `#[path]` into each

**Status:** Closed
**Resolution:** The executor is the crate `exact-executor` (`host/executor`), which the Apple, Linux and render hosts depend on through Cargo; a host passes its `WorkerBudget` to `Core`'s constructors, the process budget is one static per process, and the tests run once, in that crate.
**Systems:** host/apple/src/executor_core.rs, host/linux/src/executor.rs, host/render/src/executor.rs, scripts/affected.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, the native request executor (grant-checked transport, the ordered and independent
lanes, streams, sockets, uploads, child processes) is 24 files and 10,741 lines named
`host/apple/src/executor_*.rs`. Three crates compile it and none depends on another for it. Each
mounts the root file:

```rust
#[path = "../../apple/src/executor_core.rs"]
mod core;
```

(`host/linux/src/executor.rs:8`, `host/render/src/executor.rs:8`; `host/apple/src/executor.rs:6`
mounts it from beside itself.) Cargo sees no edge between `exact-apple`, `exact-linux` and
`exact-render`. The code that serves the Linux, Windows and Android hosts and the render server is
found, named and changed under the Apple host.

## What it costs

- **Three compilations and three test runs.** rustc's dep-info lists the 24 files in each crate's
  library, and 15 test files (9,065 lines; 16 in `exact-apple`) in each crate's test build. The same 211 tests are in all
  three unit-test binaries: 211 of `exact-apple`'s 604, of `exact-linux`'s 1,767 and of
  `exact-render`'s 246. They take 5.1 s to run in each.
- **`scripts/affected.mjs` does not know `exact-render` compiles it.** With
  `host/apple/src/executor_stream.rs` changed, `--why cargo:exact-render` answers "not selected (no
  changed file is among its inputs)". `exact-linux` is selected, through an unrelated chain:
  `pkg:exact-apple`, `caltrain-apple`, `app:apps/caltrain`, which a Linux test names.
- **An interface nothing declares.** The shared file reads `super::WAIT_FOR_SLOT`
  (`executor_core.rs:340`), a constant each includer must define (`executor.rs:9`, `:14`, `:15` in
  the three). `host/apple/src/executor_workers.rs:79` reaches the core as
  `super::super::super::super::{Core, WorkerSlot, LIVE_WORKERS}`.
- **A process bound per includer.** `LIVE_WORKERS` (`executor_core.rs:21`), the count the worker
  bound is held against, is one static in each crate that mounts the file.

## Verification

Ran `cargo check -p exact-render --lib` and read `target/debug/deps/exact_render-*.d` (and
`exact_linux-*.d`, `exact_apple-*.d` after their test builds). Ran
`cargo test -p <crate> --lib -- --list` for the three crates and counted the `executor::core::`
names. Ran `cargo test -p exact-render --lib -- executor::core`: 195 passed, 16 ignored, 5.09 s.
Appended a comment to `executor_stream.rs` and ran
`bun scripts/affected.mjs --base HEAD --why cargo:exact-render`. Not run: a process that links two
includers.

## Direction

Make the 24 files a crate. They name `exact_runner` and `ibex2` and nothing of `crate::`, so the
move is mechanical; `Core` takes the wait-for-slot choice as a value. The three hosts depend on it
and keep their own `Executor` wrappers and their own ibex2 features. The tests then run once.

## Related

`issues/20261110-the-shared-painting-host-is-the-crate-named-exact-linux.md` is the same kind of
finding for the painting presenter. The other `#[path]` mounts that leave their crate in production
code are single files: `update/src/envelope.rs` (`plan/src/entry.rs`), `game/engine/src/asset.rs`
(`gpu/src/asset_name.rs`), `js/bake/src/lib.rs` (`js/package_tool.rs`) and
`semantics/vm-extract/src/lib.rs` (`runner/src/machine.rs`, which is that crate's purpose).
