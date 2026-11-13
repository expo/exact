# `affected.mjs` drops `include_str!("name")` when the file is beside the Rust file that includes it

**Status:** Closed
**Resolution:** `rustSource` marks an `include_str!` or `include_bytes!` path as starting at the including file's directory (`from: 'own'`), and `references` joins a bare name there, as rustc resolves it.
**Systems:** scripts/affected.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

The header says an `include_str!` is one of the forms read (`scripts/affected.mjs:12-13`), and
`references` says "what it includes (`include_str!`)" counts in any code (`:488-489`). rustc
resolves the macro's path from the directory of the file that holds it. `references` keeps a word
with no `/` only when it is joined onto the code's own directory, which it tells from
`literal.base` (`:541`), or onto the root, from `literal.receiver` (`:548`). `rustSource` never
sets `base`, and an `include_str!` has no receiver, so a bare file name passed to `include_str!`
or `include_bytes!` returns at `:548` and names nothing. A path with a `/` in it (`"../rt.js"`)
is read.

## The failure

`js/bake/src/lib.rs:1248`, library code of `exact-js-bake`:

```rust
const CANVAS_TYPES: &str = include_str!("canvas-types.d.ts");
```

```
echo '// x' >> js/bake/src/canvas-types.d.ts
bun scripts/affected.mjs
bun scripts/affected.mjs --why cargo:exact-bake
```

Observed:

```
1 changed files select 384 of 598 suites: Cargo packages 5 of 84, Bun files 296 of 431, …
cargo:exact-bake: not selected (no changed file is among its inputs)
```

The same edit to `js/bake/src/lib.rs` selects 433 suites, 40 of them Cargo packages, and
`cargo:exact-bake` through `pkg:exact-js-bake`. The 384 reach the declaration file only through
`scripts/app-shells.mjs`, which names the directory `js/bake`.

Expected: every suite that reaches `pkg:exact-js-bake` reads the file, since it is compiled into
that crate. It is the declaration file an app's `draw` module is type-checked against
(`js/bake/src/lib.rs:1078`).
The 49 suites that miss it (35 Cargo packages, among them `exact-bake`, `exact-js` and
`exact-linux`, and 14 Bun files) run for such a change only in the async lane's daily full run.

## Verification

Ran the commands above in a worktree at `dd6f5dcd`; the output is as shown. Of the 72 files that
library code embeds with `include_str!` or `include_bytes!`, resolved as rustc resolves them, this
is the one that `pkg:<name>` does not hold. Six others are bare names
(`js/bake/src/resident.rs:174-183`, `contract/cli/src/vocab/card.rs:15`, `concepts.rs:20`), and
are held only because another file names their path or directory in a string.

## Direction

For a literal whose call is `include_str!` or `include_bytes!`, join a path that says no base onto
the including file's directory, as `:551` already does first for a path with a `/`.

## Related

`QUEUE.md`, "`affected.mjs --run` stops at the first failing command", is about running. Not in
`issues/`.
