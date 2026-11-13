# A replaced Android activity is not told its orientation lock

**Status:** Open
**Systems:** host/android, host/linux
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A reader made again is supposed to be told the orientation lock anew. `requests_reset` clears Back, the keyboard, the wake lock, and pad input so the next tell sends them. It leaves `orientation` and `orientation_epoch` set.

`orientation_moved` returns nothing while the kernel epoch still matches, and after a later epoch only when the session differs from that cache. The new Android activity starts at the manifest orientation. A landscape or current lock stays wrong until the lock value itself changes.

## Where

`host/linux/src/presenter/requests.rs`, `requests_reset` and `orientation_moved`. `host/linux/src/presenter/device.rs`, `reader_replaced`. `host/android/canvas/src/main/java/dev/exact/canvas/Orientation.kt`, the comment that the lock is told anew to a reader made again.

## How to reproduce

Lock orientation to landscape or current, then recreate the activity (rotate a configuration this activity does not declare, or any replacement that keeps the process). The new activity uses the manifest orientation. `state.orientationlock` still shows the lock. The activity is not asked again.

## Direction

In `requests_reset`, clear `orientation` and `orientation_epoch`, as the wake lock is cleared, so the next tell queues the lock even when the session is unchanged.
