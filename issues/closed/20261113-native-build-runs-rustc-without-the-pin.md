# A macOS build fails "rustc did not report its host target" when rustup has no default toolchain

**Status:** Closed
**Resolution:** The host-target probe (`bakeTarget`, and the Apple bake tool through it) runs the pinned toolchain's rustc from exact2's root and reports rustc's error, and setup says it installed nothing when rustup is missing.
**Systems:** scripts/app.mjs (`bakeTarget`), scripts/exact.mjs (`setup`)
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`bakeTarget` (`scripts/app.mjs:930-938`) asks for the host triple with a bare
`spawnSync('rustc', ['-vV'])`: no `+<pin>`, no `RUSTUP_TOOLCHAIN`, and no `cwd`, so it runs in
whatever directory the command was started from. rustup then picks a toolchain from that
directory's `rust-toolchain.toml` or from the default. An app made by `exact new` has no
`rust-toolchain.toml` in its folder (the copy lives in the generated `.shells/` workspace,
`scripts/app-shells.mjs:296`), so with no rustup default, rustc exits with rustup's "no default
toolchain" error and the build throws `rustc did not report its host target`, dropping rustc's
stderr.

`setup` does not set a default on purpose ("Build scripts select the pinned stable/nightly
without changing rustup default", `scripts/exact.mjs:488-489`), and `setup --check` runs its
probes with `cwd: ROOT`, where exact2's `rust-toolchain.toml` answers, so it passes on the same
machine. `bakeTarget` is the one call that depends on the default. Its callers are the macOS build
(`host/apple/build.mjs:161`, `:654`), the Linux build (`scripts/build-linux.mjs:23`), deploy
(`scripts/deploy.mjs:671`), and the Windows build and drive (`host/windows/build.mjs:132`,
`scripts/agent-stdio.mjs:54`, `scripts/agent-build.mjs:422`). Reproduced on Linux at
`49ae6357`: with `RUSTUP_HOME` pointing at an empty directory and the working directory outside
exact2, `bakeTarget('linux')` throws `rustc did not report its host target`.

A second report from a fresh Mac: `exact setup` stopped at
`install rustup from https://rustup.rs, then rerun exact setup`, and the builder reported exit
code 0, so it took two runs to notice nothing had been installed. At `49ae6357`, with no
rustup, `bun scripts/exact.mjs setup` throws that message (`scripts/exact.mjs:411`), which
`:1485` prints as `exact: …` before exiting 1 (read, not run). The reporter's commit
(`bb8524503`) is not in this repository, so the exit code they saw could not be checked; the
stop itself is as reported.

## The failure

macOS 26.6, Xcode 27.2, M4, at `5434f4325`: rustup installed with
`--default-toolchain none`, then `bun exact.mjs mac` failed after 10 s with
`rustc did not report its host target`. Simulator builds, `agent`, `test` and `verify ios`
were clean on the same machine.

## Fix

Run the probe as setup's are run: `rustc +<pin> -vV` (`PINNED_RUST`, `scripts/app.mjs:739`), or
with `cwd: ROOT`, and include rustc's stderr when it fails. Setup could also install rustup
itself when it is missing (the rustup.rs script with `--default-toolchain none -y`), or at least
say that nothing was installed.
