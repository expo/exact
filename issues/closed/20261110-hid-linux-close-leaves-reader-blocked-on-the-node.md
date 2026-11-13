# On Linux, closing a HID device leaves its reader thread and the node open until the device's next report

**Status:** Closed
**Resolution:** The reader polls its node beside a socket whose other end the open holds, so a close wakes it and its thread and node end at once, and a stopped reader logs and announces nothing.
**Systems:** packages/hid/linux/lib.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, each open device has a thread that reads the hidraw node in a loop, `while !stop.load(…) { match (&*file).read(&mut buf) { … } }` (`packages/hid/linux/lib.rs:180`). The node is opened blocking (line 247), so `read` waits until the device sends an input report. When the last lease goes, `release` sets `stop` and drops the `Open` (lines 327 to 328), and `destroy` only sets `stop` (lines 401 to 405). Neither wakes the read, and the thread holds its own `Arc<File>`, so:

- the thread and the open node stay until the device next sends a report. For a device that sends none (an LED strip, a relay board, a display written with output or feature reports) they stay for the life of the process, one more of each for every open and close;
- when that report does arrive, the stopped reader still takes the `Ok(n)` arm: it logs the report and calls `notifier.changed("hid.inputReports")` for a connection the app closed, and only then sees `stop`. If the read fails instead (the device unplugged after the close), it marks its log `gone` and announces `hid.inputReports` and `hid.hidConnection`.

The Windows implementation of the same package does both things this one omits: its `release` cancels the pending read (`CancelIoEx`, `packages/hid/windows/lib.rs:366`), and its reader announces nothing once `stop` is set (line 218).

## The failure

`open(device=d)`, then `close(connection=c)` on a device that is idle. Observed after the close: the reader thread is alive and `/dev/hidrawN` is still open; the device's next report is logged and announced. Expected: the close ends the read, as on Windows, and nothing is announced for a closed device.

## Verification

Ran `read_loop` (lines 177 to 205) copied as written, with `Log`, `Collection`, `blocked` and `report_blocked`, a counter for `Notifier`, and one end of a Unix socket pair for the node, since hidraw does not exist on a Mac and both reads block until bytes arrive. The program started the reader, did what `release` does (`stop.store(true, …)`, then dropped the `Open`'s `Arc<File>`), waited a second, and wrote three bytes to the other end. It printed:

```
1 s after close: reader thread finished = false, the node still open = true
after the device's next report: reader thread finished = true, the node still open = false, announcements after close = 1, reports logged after close = 1
```

Not run on Linux against a real hidraw node.

## Direction

Wake the reader at the close (poll the node together with a stop pipe, or poll with a timeout that rechecks `stop`), and leave the loop before logging or announcing when `stop` is set.

## Related

QUEUE "USB, HID and serial (LLP 1133.037): what is left" names the Rust hosts' plug notifications and URB reaping, not the reader's end. `issues/closed/20261102-serial-blocked-write-prevents-close.md` is the serial package's blocked write.
