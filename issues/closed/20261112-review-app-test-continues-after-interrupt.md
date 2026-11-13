# Interrupting an app test starts the next test file

**Status:** Closed
**Resolution:** `bun exact.mjs test` returns the signal's exit code once a signal reaches a test file's drive, starting no later file; a failing file still lets the rest run.
**Systems:** scripts/app-cli.mjs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `bun exact.mjs test` forwards SIGINT, SIGTERM and SIGHUP to
the active driver and records the signal in `stopped`. The loop over test
files never checks that flag: it treats the interrupted child as an
ordinary failed test and launches the next file (`app-cli.mjs:482–486`).

Ctrl-C or a supervisor's termination request therefore stops only the
current file. Remaining tests can launch more drivers, builds and app
sessions before the command finally exits. Continuing after an assertion
failure is useful; continuing after cancellation defeats the caller's
request to stop the run.

## Verification

Ran the actual exported `run` under Bun 1.4.2 with two explicit test files
and a temporary SDK adapter. The first driver printed `FIRST_READY` and
waited. After that marker, the probe sent one signal to the launcher PID
it had created. The second driver printed `SECOND_STARTED` and exited.

| Signal to launcher | Next file started | Final exit |
| --- | --- | --- |
| SIGINT | yes | 130 |
| SIGTERM | yes | 143 |

The final exit code preserves the signal, but only after the new work
finishes. The probe used a no-server `withAppServer` adapter and no GUI,
device or build. It is
`/tmp/exact-review-20261112/app-test-interrupt.mjs`.

Stop scheduling test files once cancellation is recorded, await cleanup
of the active child and return the signal's exit code. Cover termination
during the first of several files and preserve running all files after
an ordinary test failure.

Searched open and closed filesystem issues and all 275 GitHub issues in
the review snapshot. The closed
`20261110-verify-survives-interrupt-while-contract-verify-runs.md` fixes a
different signal listener inside `verify`; it does not fix this test loop.
