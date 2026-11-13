# Contract build advice is printed in a shape Problems does not match

**Status:** Closed
**Resolution:** Resource-clock and surface findings print as `file:line:col warning [id] message`, as lowering warnings do, and the stale declaration note as `<newer source>:1:1 warning [surface-declaration-stale] …`, so the problem matcher takes each.
**Systems:** editors/vscode, contract cli
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`$exact-contract` matches a line that starts with `file:line:col`, with an optional `warning` immediately before `[id]`. Resource-clock findings and stale-surface findings are printed as `warning: ` plus the finding. The path is no longer at column 0, so the matcher drops them.

The build task uses that matcher and `"reveal": "silent"`, and these findings do not fail the build. The terminal stays hidden and Problems stays empty. `--json` reports the same findings as warnings. A lowering warning already prints `file:line:col warning [id] message`, which the matcher accepts. The `warning:` prefix is a second shape, and the stale-file line has no `[id]` at all.

## Where

- `contract/cli/src/build.rs` prints `warning: {finding}` for clocks
- `contract/cli/src/main.rs` `surface_warnings` prints `warning: {finding}` and a `warning: .shells/surfaces.json is older than …` line
- `editors/vscode/package.json` the `$exact-contract` regexp
- `.vscode/tasks.json` the build task (`problemMatcher`, `reveal: silent`)

## How to reproduce

Introduce an `analyze-resource-clock` finding and run the "contract: build this file" task. The finding is on stderr. Problems has no entry.

## Direction

Print these findings with `CompileError`'s display and no extra prefix, the line lowering warnings already use. Give the stale-file note the same `file:line:col warning [id]` shape, or a path the matcher can take.
