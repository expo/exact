# An exclusive device open is not reserved across the check

**Status:** Open
**Systems:** runner
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`held_open` locks the shared open map, reads it, and drops the lock. `opened` locks again and sets the key without reading it. On Android, `granted == false` returns a permission hold before any key is set. `device_answered` of `allow` calls `opened` with no second check.

Two opens of an exclusive serial, HID, or USB device can both succeed. `closed` and `Drop` then remove the shared key when this session's own connections are gone, so the other session's open no longer blocks anyone. The module states that an exclusive open anywhere in the app fails `InvalidStateError` first.

## Where

`runner/src/runner/modules/devices.rs`, `device_open`, `held_open`, `opened`, and the `allow` arm of `device_answered`.

## How to reproduce

From two sessions, open the same exclusive device. On Android, deny it in the fixture so both calls hold for permission, then allow both. Both connections open. Closing one clears the claim while the other is still open.

## Direction

Hold the `AppState` lock from the check through the insert. Run that same check on the Android allow path before creating the connection. If the key is already true, answer `InvalidStateError` and do not open.
