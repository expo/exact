# An execFile drain waits on wall time

**Status:** Closed
**Resolution:** execFile records its child's exit on the supervision clock and waits for the drain's deadline there, so the agent's clock ends the drain.
**Systems:** host
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

After a real `execFile` child exits, leftover pipes close when wall time since that exit exceeds `drainTimeout`. The timeout, the abort deadline, and the `SIGKILL` grace use the supervision clock. The drain instant is not one of the deadlines passed to `wait_for`.

On the agent clock, `now()` moves only when the drive advances it, and `timeout` polls every 250 ms of wall time. The drain still completes after `drainTimeout` milliseconds of wall time (the default is 5 s, the maximum 600 s). `clock +N` does not finish it, and holding the clock does not hold it. A spawn's drain closes the same pipes with `clock.passed`.

## Where

`host/apple/src/executor_process_real.rs`, `exec`, the `exited_at` check. `host/apple/src/executor_process_supervise.rs` closes a spawn's pipes from the clock. `host/apple/src/executor_process_clock.rs`, `now` and `timeout`.

## How to reproduce

Under the agent, `execFile` a program that exits while a pipe stays open. Advance the clock past `drainTimeout`. The call is still waiting. It returns about `drainTimeout` milliseconds later on the wall clock.

## Direction

Record the exit with `clock.now()`, close the pipes when `clock.now()` reaches that time plus `drainTimeout`, and put that deadline in the `wait_for` set.
