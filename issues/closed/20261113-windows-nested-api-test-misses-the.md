# Windows nested api-test misses the server owner

**Status:** Closed
**Resolution:** withAppServer reads every process's parent from one table, CIM's Win32_Process on Windows and ps elsewhere, so a run nested two levels under the server's owner finds it.
**Systems:** scripts
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`withAppServer` reuses a server an ancestor already started. It learns ancestors from `ps -o ppid=`. `ps` has no Windows build, so the walk stops after the immediate parent. `commandLine` already reads the parent chain through `Win32_Process` on Windows. The owner check does not.

`exact api-test` starts the server, then `exact test`, then `agent.mjs --test` two hops down. On Windows that grandchild does not see the owner pid. It tries to start a second server. The port is already owned, and `withAppServer` returns 1.

## Where

`scripts/verify.mjs`, `ancestors` and `withAppServer`. `scripts/app-cli.mjs` runs `test` under `withAppServer`. `scripts/agent.mjs` starts another `withAppServer` for `--test`.

## How to reproduce

On Windows, run `exact api-test` for an app that has a server. The nested `agent.mjs --test` does not reuse the owner and the run exits 1.

## Direction

Read parent pids with the same `Win32_Process` query `commandLine` uses, and keep the `ps` walk on other platforms.
