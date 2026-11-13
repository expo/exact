# A native build fails without advice when a cargo that is not rustup's is first on PATH

**Status:** Closed
**Resolution:** A native build or drive that fails runs `cargoProxyFault` and, when the cargo first on PATH is not rustup's and refuses the pin, says which directory to put first.
**Systems:** scripts/app-cli.mjs, scripts/app-shells.mjs, scripts/cargo-check.mjs (`cargoOnPath`), scripts/exact.mjs (`cargoProxyFault`, `platformRowNames`)
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

Homebrew's `rust` puts its own `cargo` and `rustc` on PATH, often ahead of `~/.cargo/bin`.
That cargo is not rustup's proxy, so it refuses a `+<toolchain>` argument and ignores
`rust-toolchain.toml`.

`cargoProxyFault` (`scripts/exact.mjs:126`) names this. `setup` and the Rust row of
`setup --check` call it (`:415`, `:555`), and so does `exact contract` when cargo exits 101
(`:720`, a11b8797). A native build does not:

- It runs plain `cargo` in the generated workspace and relies on its copied
  `rust-toolchain.toml` (`scripts/app-shells.mjs:296`). Homebrew's cargo builds with
  Homebrew's rustc, whose std has no iOS target, so the build fails with
  `error[E0463]: can't find crate for core`. The row for the target reads
  `rustup target list --installed --toolchain <pin>` (`scripts/exact.mjs:640`), which passes,
  because the pinned toolchain has the target; the cargo that runs is not that toolchain's.
- The setup rows are read before a build only the first time a platform is named in
  `app.json` (`scripts/app-cli.mjs:290-298`; `platformRowNames`, `scripts/exact.mjs:507`).
  After that no build reads them.
- `cargoOnPath` (`scripts/cargo-check.mjs:11`) adds `~/.cargo/bin` to PATH only when no
  `cargo` is found at all, so it leaves a Homebrew cargo first.

So a machine where `setup` once succeeded (PATH set differently in that shell), or where PATH
changed afterwards, fails every iOS build with an error that does not name the cause.

## The failure

On macOS at `94b8671f7`: `bun exact.mjs ios` failed with `can't find crate for core`
(aarch64-apple-ios-sim) because Homebrew's cargo and rustc came first; `setup --check`
reported Rust 1.98.1 against 1.97.0 without saying the cargo on PATH was not rustup's. Fixed
with `PATH=$HOME/.cargo/bin:$PATH`. The same builder's `contract` failures (`no such command:
+1.97.0`) are now named by a11b8797.

## Fix

Run `cargoProxyFault` when a native build's first cargo call fails, as `contract` does. Better,
put the directory of `rustup` first on the PATH of every cargo Exact starts, where
`cargoOnPath` now only adds `~/.cargo/bin` when no cargo is found; then a Homebrew cargo
earlier on PATH stops mattering for Exact's own commands.
