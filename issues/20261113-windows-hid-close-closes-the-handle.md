# Windows HID close closes the handle before the reader exits

**Status:** Open
**Systems:** packages/hid
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

The last Windows `release` stores `stop`, calls `CancelIoEx`, then `CloseHandle`, and does not wait for the reader. The reader checks `stop` only at the top of the loop. It can already have passed that check and then call `ReadFile` on the handle close just released.

If the process reuses the handle, the success path appends those bytes to this device's log and announces `inputReports`. Close is a normal call and runs on a different thread from `ReadFile`.

## Where

`packages/hid/windows/lib.rs`, `read_loop` and `release`.

## How to reproduce

Close the last connection while the reader is between the `stop` check and `ReadFile`, and have the process reuse that handle. A later `inputReports` can contain bytes that are not this device's reports.

## Direction

Keep the handle alive until the reader leaves `ReadFile`, and only then `CloseHandle`.
