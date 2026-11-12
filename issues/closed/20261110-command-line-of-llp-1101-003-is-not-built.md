# DEFERRED.md says a terminal entry runs once from a shell; the command line of LLP 1101.003 is not built

**Status:** Closed
**Resolution:** The terminal host now has LLP 1101.003's command line: print mode and `--json`, argv as the launch location, done by D2's settle test, the head's `status` as the exit status, one termination record with its watchdogs, and the plan's `surface` marker.
**Systems:** rules/DEFERRED.md, LLP 1101.003, host/terminal (cli.rs), QUEUE.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`rules/DEFERRED.md:536-546` (at `dd6f5dcd`) records, under the terminal surface: "a terminal entry runs once from a shell. With `--print`, or when stdout is a pipe, it settles (LLP 1101.003 D2, the command-line column of LLP 1113 D5), writes its rows, and exits. The argument list is its URL, the active `head`'s `status` is its exit status, `--json` writes its settled `state`, and one termination record decides how it ends. Unblocks scripting the coding harness (`harness --print --prompt "…"`), with `apps/todo` as the fixture." Its "Still out" list names only later slices. LLP 1101.003 is Accepted (2026-10-08), "Implementer: a Claude (Opus 5.5) lane", and has no as-built note.

None of it is in the tree. `host/terminal/src/cli.rs` is the command line LLP 1101.003 §1 calls "today": it knows `--size`, `--inline`, `--fullscreen` and `--color` (`cli.rs:314-359`), and every other argument is a headless operation (`cli.rs:402-543`). There is no print mode, `--json`, `--drive`, `--columns`, `--timeout`, `--help`, launch location, settle test, exit status from `head` or termination record. The plan format is version 5 with no `surface` header field (D9), and `apps/todo/terminal.contract` has no routes (§3). No source file cites LLP 1101.003 and `QUEUE.md` has no entry for it. Of D6, the host has `--color`, `NO_COLOR` and the launch place and time.

## The failure

`exact-terminal` and `harness` share `cli.rs`. The headless form AGENTS.md documents (`--size 80x24 … print`) works in both.

| Command | Observed | LLP 1101.003 |
|---|---|---|
| `exact-terminal apps/todo/terminal.contract --print` | `unknown operation "--print"`, exit 1 | the document, exit 0 (D1) |
| `exact-terminal apps/todo/terminal.contract --json /done` | `unknown operation "--json"`, exit 1 | the settled state as JSON (D7) |
| `exact-terminal apps/todo/terminal.contract /active` | `unknown operation "/active"`, exit 1 | the location `/active` (D3) |
| `harness --print --prompt hi` | `unknown operation "--print"`, exit 1 | the reply, then exit (§5) |
| `harness --json`, `harness --drive print`, `harness --help` | `unknown operation`, exit 1 | D7, D3 |
| `exact-terminal apps/todo/terminal.contract \| wc -l` | 24: the 80×24 emulator screen, taken at once | the document's rows, once settled (D1, D2) |
| `exact-terminal apps/todo/terminal.contract layout tree runner-tree \| head -c 20` | panic "failed printing to stdout: Broken pipe", exit 101 | exit 141, "never a panic" (D8) |
| `exact-terminal bad.plan print`, the file not a plan | panic `a plan: BadMagic` (`cli.rs:284`), exit 101 | exit 1 (D8) |

A missing `.plan` panics at `cli.rs:283` in the same way.

## Verification

Ran each command above with `target/debug/exact-terminal` and `target/host-dev/harness` (stdin `/dev/null` for the harness); the "Observed" column is what they printed, with the exit status from zsh's `pipestatus`. `git grep 1101.003` finds only the DEFERRED paragraph.

## Direction

Either build the first slice (LLP 1101.003 §3 is the work list) or make the record true until it is: say in the DEFERRED paragraph that the command line is admitted and not built, and add the slice to `QUEUE.md`. The two panics are small on their own: treat `ErrorKind::BrokenPipe` on the headless writes as the end of output, and return exit 1 for an unreadable or undecodable plan, as `run_embedded` does (`cli.rs:298-304`).

## Related

`issues/20261110-broken-pipe-panics.md` is the same panic in the `contract` binary. `QUEUE.md`, "The terminal surface's next work", lists other terminal items and not this one.
