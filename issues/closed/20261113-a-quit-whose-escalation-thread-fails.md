# A quit whose escalation thread fails to spawn leaves children past SIGTERM

**Status:** Closed
**Resolution:** When the quit's escalation thread cannot start, the caller waits out the groups' grace, sends SIGKILL and prunes their records itself.
**Systems:** host
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`quit` drains the live process groups, signals each with its kill signal, and counts an escalation the process exit waits for. The escalation is a thread that waits out each group's grace, then sends `SIGKILL`, then prunes the group records. When that thread cannot be spawned, `quit` only decrements the count.

The groups are already gone from `LIVE`, so nothing else escalates them. `wait_for_escalations` sees a zero count and returns. A child that survives the first signal is still running when the app exits, and its record is not pruned.

## Where

`host/apple/src/executor_process_groups.rs`, `quit`, `escalate`, and `wait_for_escalations`.

## How to reproduce

Start a child that ignores `SIGTERM`. Quit the app while `std::thread::Builder` fails to spawn `exact-child-quit` (the process is at its thread limit). The child is still alive after the app has exited, and no `SIGKILL` was sent.

## Direction

If the escalation thread cannot be spawned, put the groups back and run `escalate` on the caller before returning, or keep the escalation count above zero until `SIGKILL` has been sent. The exit wait is what bounds that work today, and it only waits while the count is positive.
