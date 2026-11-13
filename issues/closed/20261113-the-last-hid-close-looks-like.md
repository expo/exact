# The last HID close looks like an unplug on Linux and Windows

**Status:** Closed
**Resolution:** On Linux and Windows each HID connection keeps its open's report log, so the last close leaves its last reports readable and the device connected, and only the reader seeing the device leave sets interrupted.
**Systems:** packages/hid
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

The last `close` removes the shared open and its report log. `hidConnection` treats a missing open as gone, so `connected` becomes false. `inputReports` then returns no reports and `interrupted` true.

Web and Apple keep the log, leave `connected` true, and set `interrupted` only when the device is removed. Closing the only connection is the normal end of a session. The app cannot tell a user close from the device leaving, and a poll that has not yet read the last reports loses them.

## Where

`packages/hid/linux/lib.rs` and `packages/hid/windows/lib.rs`, `release`, `reports`, and `hidConnection`. `packages/hid/web/index.js` and `packages/hid/apple/HID.swift` keep the log.

## How to reproduce

Open a device, read reports, and close the only connection while the device stays attached. The next `hidConnection` says `connected` false, and `inputReports` says `interrupted` true with an empty list.

## Direction

On a voluntary close, keep the log for later reads. Set `open` false without marking the device gone. Return `interrupted` only when the reader saw the device disappear.
