# A failed child waiter leaves execFile without an exit

**Status:** Open
**Systems:** host
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

The child waiter, the pipe readers, and the stdin writer ignore a failed `thread::Builder::spawn`. The waiter is the only `wait` on the pid and the only `Seen::Exit`. Without it, `exec` leaves `exit` unset, so the loop that waits for an exit and both end-of-file marks never finishes.

The child is already in the process-group table. Nothing reaps it, so it stays a zombie, and the `execFile` or the spawn incarnation does not end. A ready probe reports a failure when its thread does not spawn. These threads do not.

## Where

`host/apple/src/executor_process_real.rs`, `spawn`, the waiter and `read` spawns, and the `exit.is_some()` break in `exec`.

## How to reproduce

Start `execFile` while the process is at its thread limit, so `exact-child-<pid>` fails to spawn. The child can exit and the call still does not return. `wait` does not reap it.

## Direction

If the waiter fails to spawn, reap on this thread and send `Seen::Exit`, or fail the call and signal the group. Do the same for a pipe thread that cannot start: that call has to end.
