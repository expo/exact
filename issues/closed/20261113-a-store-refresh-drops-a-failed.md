# A store refresh drops a failed local write and reverts the slot

**Status:** Closed
**Resolution:** The Apple host records what the disk held when a write first failed; a refresh that still reads those bytes keeps the owed value in the snapshot it adopts and keeps the write queued, and only a changed name drops it as a peer's.
**Systems:** host/apple
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

After a non-durable store write fails, the new value stays in memory and in `pending_store_writes`, and the disk keeps the old bytes. A later refresh snapshots the disk over that memory. `store_changed` restores persisted slots from the snapshot, so the shown value reverts. `store_read_finished` then keeps a pending write only when the snapshot bytes equal the owed bytes. The failed write does not match, so the retry is dropped.

A widget app bumps `widget-store-generation` on any successful write, and every live session refreshes. One failed name is discarded when any other name commits.

## Where

`host/apple/src/activation.rs`, `refresh_store_after` and `store_read_finished`. `host/apple/src/persistence.rs`, the failed-write retry. `runner/src/runner/kept.rs`, `store_changed` and `restore_store`.

## How to reproduce

Fail a persisted write (a full store, or a permissions error on that name), then commit a different name so the session refreshes. The failed name shows the old value and is not retried.

## Direction

Drop a pending write only when the snapshot shows a peer change since the failed attempt. If the name is unchanged on disk, keep the owed bytes and do not copy the stale value over the committed slot.
