# Windows instance handoff does not enforce its five-second deadline

**Status:** Closed
**Resolution:** A later launch measures its five seconds on a monotonic clock across its tries and pauses, and gives each handoff only the time left.
**Systems:** Windows launch, host/windows/src/instance.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Problem

A launch that finds an existing instance unresponsive can wait longer than
the stated five-second deadline before reporting that the app did not
answer. The retry loop accounts for its sleeps but excludes each blocking
handoff attempt.

Reviewed at `f6be99af`. `settle` in `host/windows/src/instance.rs:80-94`
increments `waited` only by `pause()`. `claim` calls `hand_over` during each
attempt, which calls `SendMessageTimeoutW` with the full five-second
`PATIENCE` again. After one attempt consumes that budget, the loop still
allows another hundred 50 ms pauses and handoff attempts.

This affects a second launch while the existing app's UI thread is busy or
unresponsive. It delays the launch's failure and any caller waiting for
the process to end. The existing instance remains protected against a
second session; the defect is the timeout accounting.

## Evidence

Extracted the production `Attempt`, `Settled` and `settle` definitions into
a standalone Rust probe. An injected attempt records five seconds of
simulated elapsed time before returning `Taken`; the injected pause records
and returns 50 ms. The unmodified loop performs 101 attempts and consumes
510,000 ms of simulated time despite a 5,000 ms budget.

That number demonstrates the accounting defect, not a Windows timing
measurement: `SMTO_ABORTIFHUNG` may shorten later calls. Even if only the
first call spends five seconds and all later attempts return immediately,
the same loop still adds five seconds of pauses after its deadline.

## Fix and validation

Measure a monotonic deadline across attempts and pauses. Pass the remaining
time into each blocking handoff and stop retrying when none remains.
Test a slow failed attempt, fast repeated failures, a successful handoff,
and an instance exiting while the caller waits.

## Duplicate check

Checked open and closed filesystem issues and all 275 cached GitHub issues.
GitHub #194 concerns Windows windowless operation and login launches; no
existing issue covers the handoff timeout accounting.
