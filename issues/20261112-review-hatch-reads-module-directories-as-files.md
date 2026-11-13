# Adding an Apple hatch fails when its module directory contains a subdirectory

**Status:** Open
**Systems:** scripts/exact.mjs, native module scaffolding
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `hatch` checks that `modules/apple` contains a Swift file,
then reads every directory entry while searching for the generated module
marker (`scripts/exact.mjs:933-934`). It does not filter that second scan to
Swift files or regular files. A subdirectory encountered before the matching
module causes `EISDIR`, and `exact hatch` cannot add the requested hatch.

An existing native module can reasonably contain supporting directories.
This is independent of which platforms the manifest declares.

**Verification.** `/tmp/exact-review-20261112/hatch-directory-probe.mjs`
executes the unchanged `hatch` function with the app resolver pointing at a
temporary app. The app declares iOS, has `modules/apple/Hatches.swift`, and
also has `modules/apple/0-support/`. `hatch(['avatar'])` throws
`EISDIR: illegal operation on a directory, read` on Bun 1.4.2. The real
filesystem is used; no native build is needed or launched.

Restrict module discovery to regular Swift files before reading contents.
Check an existing handwritten module and a generated module with adjacent
directories and non-source assets, preserving existing source files.

No matching filesystem issue was found. GitHub #125 concerns recognizing
platforms in apps made by `exact new`; it does not cover this directory scan.
