# An installed command expands dollar signs in the app's name

**Status:** Open
**Systems:** scripts/exact.mjs, macOS command shims
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `install` puts `JSON.stringify(target)` into a shell script's
`open -a` command (`scripts/exact.mjs:239-243`). JSON quoting is not shell
quoting: a double-quoted shell argument still performs parameter and
command substitution. `appleExecutableName` permits dollar signs and
backticks, and the home directory is part of the same argument.

For an app named `Budget $5`, installation creates `Budget $5.app`, but its
command tries to open `Budget .app` when invoked without a fifth argument.
The generated shim therefore does not name the bundle it installed.

**Verification.** `/tmp/exact-review-20261112/install-ownership-probe.mjs`
executes the current installer with temporary paths and build/service
substitutes. It feeds the generated shim to `/bin/sh`, replacing only
`/usr/bin/open` with an argument-printing command. The printed launch target
drops `$5` and differs from the installed path. The probe uses parameter
expansion only and launches no GUI. Bun 1.4.2 generated the shim.

Quote the path as a shell literal, handling embedded single quotes, instead
of using JSON serialization. Verify spaces, dollar signs, backticks and
single quotes in supported bundle and home-directory names, preserving
each document argument separately.

No matching filesystem or GitHub issue was found. The companion installation
ownership issue concerns overwriting existing paths; this issue concerns
how a correctly installed bundle is named when launched.
