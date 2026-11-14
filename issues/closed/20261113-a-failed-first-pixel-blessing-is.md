# A failed first-pixel blessing is latched and the bundle can be quarantined

**Status:** Closed
**Resolution:** The store writes the blessing before it assigns memory, and the client latches it only after the write succeeds, so a failed write can be retried.
**Systems:** update
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`Client::boot_succeeded` sets `succeeded` before `store.boot_succeeded` returns. The store clears `failures` and copies `last_good` before `write_boot_mark`. A failed write leaves that memory in place and the disk mark unchanged. A retry in this process returns success without writing, because `succeeded` is already set. A current check does not flush the record when the sequence and digest already match.

The next launch reads the strike `boot_started` persisted. After one earlier crash, that single failed blessing is the second strike. Open demotes the bundle that reached first pixel and refuses to stage it again.

## Where

`update/src/client.rs`, `boot_succeeded`. `update/src/store.rs`, `boot_succeeded`, `write_boot_mark`, `adopt_record`, and the two-strike demotion in `open`. `adopt_record` writes the record before it assigns memory. The boot mark does the reverse.

## How to reproduce

Boot a selected bundle that already has one failure. Persist the boot strike, reach first pixel, and fail the blessing write. Launch again. The bundle is demoted and is not staged when the head still names it.

## Direction

Write the boot mark before assigning memory. Set `succeeded` only after the write returns success, and restore the previous `failures` and `last_good` on error so a retry can persist the blessing.
