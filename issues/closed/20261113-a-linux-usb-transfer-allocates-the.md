# A Linux USB transfer allocates the caller's length and wraps wLength

**Status:** Closed
**Resolution:** Linux refuses a control transfer longer than 65,535 bytes and a bulk transfer longer than usbfs takes with TypeError before allocating its buffer, so wLength no longer wraps.
**Systems:** packages/usb
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`controlTransferIn` and `transferIn` allocate `vec![0u8; length]` with no cap. A control transfer then sets `length: data.len() as u16`, so 65536 becomes a `wLength` of 0 and any larger value wraps. A non-negative ioctl is returned as success. A huge length panics or aborts the process while allocating.

macOS returns `TypeError` for a control length outside 0 to 65535.

## Where

`packages/usb/linux/lib.rs`, `control` and `bulk`. `packages/usb/apple/USB.swift`, the control length check.

## How to reproduce

On Linux, call `controlTransferIn` with length 65536. The device is asked with `wLength` 0 and the call can report success. Call `transferIn` with a multi-gigabyte length. The process aborts.

## Direction

Reject a control length outside 0 to 65535 before allocating, and cap a bulk length the same way. Narrow to `u16` only after that check.
