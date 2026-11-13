# A refused grouped reorder drop leaves the ghost

**Status:** Closed
**Resolution:** Not a defect as described: a refused drop ends the hold in the runner, and the reply's batch carries the session's reorder-state (phase settling), which group-glue.js's state() lands and finishes; a test now asserts both.
**Systems:** host/web
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A grouped `reorder-terminal` whose `drop_reorder` returns `Err` answers `accepted: false` and a batch, with no phase, ending, target, or row, and leaves `Active` set. `Ok(None)` cancels the session and the page can land the ghost. The error path does not.

`group-glue.js` leaves the gesture only for `holding`, `settling`, or `cancelling`. The pointer is already up, so the ghost stays. A new begin is stale until Escape.

## Where

`host/web/src/group_drag.rs`, op 17's `Err` arm. `host/web/group-glue.js`, `after` and `state`.

## How to reproduce

Drag a grouped row and drop it where the `reorderdrop` action fails. The ghost stays on screen. Another drag does not start. Escape clears it.

## Direction

On that error, cancel the session and include the group fields the other replies carry, so the page can land or cancel.
