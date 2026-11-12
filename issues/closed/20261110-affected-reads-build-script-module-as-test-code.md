# `affected.mjs` reads a build script's module as test code when a test also includes it

**Status:** Closed
**Resolution:** `rustRole` gives `build` to the modules a build script declares (resolved beside it, as a target root's are) and no library root reaches, before test code claims them, so each selects what its `build.rs` selects.
**Systems:** scripts/affected.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

A package built as a dependency contributes its Rust "without its test modules"
(`scripts/affected.mjs:7-8`, `:396-397`). `#testFilesOf` decides which files those are: what test
code reaches, less what the library reaches from its targets' roots (`:311-317`). The library's
roots leave out the build script (`:313` filters `custom-build`), and `rustRole` calls only
`build.rs` itself `build` (`:291`). So a file the build script declares with `mod name;`, which an
integration test also includes by `#[path]`, is reached by test code and by no library root: it is
`test`, and a dependent's suite does not read it. The file is compiled into the build script that
every dependent runs.

Three files at this commit:

- `js/engine_os.rs`: `js/build.rs:7` declares it, `js/tests/it/pinned_engines.rs:5` includes it.
  It holds `ENGINE_OS`, the operating systems whose builds link Hermes.
- `vendor/ibex/crates/hermes-lean-sys/build_support.rs` and `receipt_schema.rs`: `build.rs:3`
  declares the first, which declares the second; `tests/build_support.rs:2` includes the first.

## The failure

```
echo '// x' >> js/engine_os.rs
bun scripts/affected.mjs
bun scripts/affected.mjs --why cargo:exact-apple
```

Observed:

```
1 changed files select 5 of 598 suites: Cargo packages 1 of 84, Bun files 4 of 431, …
cargo:exact-apple: not selected (no changed file is among its inputs)
```

The one package is `exact-js`; the four Bun files read the whole checkout. The same edit to
`js/build.rs` selects 432 suites, 40 of them Cargo packages. Expected: each module selects what
its `build.rs` selects.

| file changed | suites selected | its `build.rs` |
|---|---|---|
| `js/engine_os.rs` | 5 | 432 |
| `hermes-lean-sys/receipt_schema.rs` | 4 | 434 |
| `hermes-lean-sys/build_support.rs` | 384 | 434 |

`build_support.rs` is selected at all only because other code names its path in a string; the 50
suites it misses include `cargo:exact-linux` and `cargo:exact-runner`.

The async lane's `build` step runs on every commit whatever is selected
(`scripts/async.mjs:192`), so a module that stops compiling is still caught. What a miss delays,
to the lane's next daily full run, is a dependent's test that fails on what the build script now
does.

## Verification

Ran the commands above in a worktree at `dd6f5dcd`, and the same for each file in the table; the
output is as shown. Of 173 build scripts, 7 module files are declared by one; `rustRole` gives
these 3 `test` and the other 4 `library`.

## Direction

In `#testFilesOf`, count the build script among the library's roots and resolve its `mod name;`
beside it, as a crate root's are: `declared` (`:302`) does that only for `lib.rs`, `main.rs` and
`mod.rs`. With both changes the three files select what their `build.rs` selects.

## Related

`QUEUE.md`, "`affected.mjs --run` stops at the first failing command", is about running. Nothing
in `issues/` covers the selection's roles.
