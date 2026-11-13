# A crash record is deleted before the queue write sticks

**Status:** Open
**Systems:** modules/observe
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

Linux `ingest_crashes` unlinks each `pending-*.json` before it records the panic in memory and before `save` replaces `queue.json`. `save` ignores a failed write or rename. A kill before `save` returns leaves the panic in neither file. The next event reloads the old queue.

Apple deletes the signal file even when the SQLite insert fails. That file is the only copy of a fatal signal or panic.

## Where

`modules/observe/linux/launch.rs`, `ingest_crashes` and `save`. The Apple path is `ObserveService.swift` deleting the file from `ingestCrashes` and `ObserveStore` ignoring a failed insert.

## How to reproduce

Panic, then kill the next process after it has unlinked `pending-*.json` and before `queue.json` is renamed into place. The following launch has no `native.exception` for that panic.

## Direction

Commit the queue, including an fsync of the rename or a successful SQLite insert, first. Delete the pending file only after that commit. On failure, leave the file for the next launch.
