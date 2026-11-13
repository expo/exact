# A native drive refuses the build it just made when any source-like file changed during the build, read by it or not

**Status:** Open
**Systems:** scripts/agent-launch.mjs (`receiptChanges`, `notBuildInput`, `refuseStale`), scripts/agent-build.mjs, scripts/verify.mjs (`iosBuild`)
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261110-apple-freshness-counts-android-and-windows-crates.md

## What is wrong

`receiptChanges` (`scripts/agent-launch.mjs:1109`) decides whether an Apple build is older than
its sources. Besides the inputs the receipt records by digest, it walks the whole app directory
and counts every file newer than the build's start (`:1114`, `:1122`) that `notBuildInput`
(`:1042-1061`) does not exclude. That rule is by file name, not by what the build read: any
`.contract`, `.toml`, `.rs`, font or file under `assets/` counts (`BUILD_SOURCE`, `:960`).
Only TypeScript, JavaScript and JSON are checked against app.ts's import graph (`moduleGraph`,
`:1011`). A `.contract` file that no `use` reaches is not compiled (the compiler only warns
about it: `unreached`, `contract/cli/src/sources/uses.rs:572`), but saving one while a build
runs makes that build stale.

A build takes minutes, and a builder (or another agent) keeps editing meanwhile. When the build
ends, `agent` and `verify` find a file newer than its start and refuse to drive it
(`refuseStale`, `:760-766`, exit 3), naming the rebuild; there is no way to drive the build that
just finished. An app's `exact.mjs` then builds again, which may meet the same race.

A second gap is in what `verify ios` prints while it builds. A drive's build prints
`still building (N s)` every 30 s (`logged`, `scripts/agent-build.mjs:158-179`, since 4f5cc4ab),
but `verify`'s `iosBuild` (`scripts/verify.mjs:426-432`) prints only the lines that start with
`building for` or `waiting for`, so those progress lines are kept from the terminal and a long
rebuild under `verify ios` is silent until it ends.

## The failure

Five reports on macOS:

- "Adding an unimported help component while the baseline rebuilt made the completed build
  refuse its own drive as stale … ignore unused source files in freshness checks, or
  automatically refresh the plan" (a diary; about a minute lost each time).
- `agent ios` refused a build that went stale during the build when other edits landed, "with
  no flag to drive the installed app"; the builder filmed with `simctl recordVideo` instead
  (exit 3 after 388 s, at `22fddd513`).
- `verify ios` refused a build after a final spacing edit changed its source receipt; reran
  after all edits.
- Adding an icon while a native build ran invalidated its bake receipt; starting the web dev
  loop during `verify web` changed `.exact/backend-origin` and made that build stale. Both are
  real inputs, so those refusals were right; they show how often an edit lands during a build.
- "The first … drive needed a native rebuild and produced no terminal progress after the
  initial log path for several minutes" (the same diary as the first). Whether that drive ran
  through `logged`'s ticker could not be told; `verify ios` still drops it at `49ae6357`.

## Fix

- Count only files the build could have read: for `.contract`, the files reachable by `use`
  from `app.contract` (the compiler knows them; its build could record them), as TypeScript
  already is.
- When the files that changed are plan inputs only, the existing plan-only path
  (`scripts/agent-build.mjs:389-397`, "running the new plan on the existing build") could take
  them instead of refusing.
- Offer a flag to drive the build that finished anyway (`--stale-ok`), stated in the refusal.
- Let `verify`'s `iosBuild` print the `still building` lines.
