# An error batch drops aborted fetch answers

**Status:** Open
**Systems:** host/web, runner
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

`Host::complete` takes aborted fetch answers into a list, and moves each ticket onto `explicit_aborts`, before it looks at the batch's error. If an error was already set, or one `fulfill` in that list returns `Err`, the loop drops the outcomes still in the list. The batch then emits `abort` for every ticket it took. The page cancels the transport. The runner never applies the aborted answer, and `module_aborts` is already empty, so a later collect finds nothing.

The resource stays pending. `clock settle` waits on that ticket.

## Where

`host/web/src/host/complete.rs`, `complete`. `runner/src/runner/admission.rs`, `take_aborted_answer`, which pushes `explicit_aborts` before it returns the outcome. `host/web/src/host.rs`, `batch_from`, which calls `complete` with the caller's error.

## How to reproduce

Abort a fetch, then in the same turn deliver a host call that completes with an error (a refused collection report, a bad fulfill, a refused viewport). The page shows the request aborted. The resource stays on its pending branch, and no later reply arrives.

## Direction

If an error is already set, leave the aborts queued. If one fulfill fails, requeue the outcomes not yet applied, and emit `abort` only for answers that were fulfilled.
