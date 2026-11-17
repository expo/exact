# On Linux a USB device's `usbVersion*` is always 0, and a native connection's `configuration` is always null

**Status:** Closed
**Resolution:** Linux reads the USB version from sysfs's `version` and both Linux and macOS answer `usbConnection`'s `configuration` with the device's active configuration, which macOS now reads from `kUSBCurrentConfiguration` (its device node has no `bConfigurationValue`, so the record's `configuration` was null there too).
**Systems:** packages/usb/linux/lib.rs, packages/usb/apple/USB.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Two fields of the native answers never carry the device's value, at `dd6f5dcd`.

**The USB version, on Linux.** `devices` in `packages/usb/linux/lib.rs:122` reads the
specification version from a sysfs file named `bcdUSB`:

```rust
let (usb, bcd) = (hex(&p.join("bcdUSB")), hex(&p.join("bcdDevice")));
```

sysfs publishes `bcdDevice`, but not `bcdUSB`: the specification version is the `version`
attribute, printed as ` 2.00`
([sysfs-bus-usb](https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-bus-usb);
`version_show` in `drivers/usb/core/sysfs.c` prints `bcdUSB` as `"%2x.%02x\n"`, and the file's
attribute list has no `bcdUSB`). `hex` answers 0 for a file that is not there (`:68-70`), so every
record reads `usbVersionMajor: 0, usbVersionMinor: 0, usbVersionSubminor: 0`. macOS reads the
IORegistry's `bcdUSB` (`packages/usb/apple/USB.swift:80`) and the web the browser's fields, so a
USB 2.0 device reads 2.0.0 there.

**The connection's configuration, on Linux and macOS.** `USBConnection` declares
`configuration: option<number>` (`packages/usb/exact.json:93-98`). Linux's `usbConnection`
answers `"configuration": null` whatever the device's state (`lib.rs:367`), though the record
beside it reads `bConfigurationValue` (`:123`, `:133`) and `selectConfiguration` sets it. macOS's
leaves the field out (`USB.swift:179-181`), though its record also reads `bConfigurationValue`
(`:87`). The web answers the device's `configurationValue` (`packages/usb/web/index.js:86`).
WebUSB's idiom for "configure once" reads this field:

```ts
const c = await usbConnection({ connection });
if (c.configuration == null) await selectConfiguration({ connection, configurationValue: 1 });
```

On Linux and macOS that selects the configuration on every pass, and a UI that shows the active
configuration shows none.

## Verification

Read, not run on Linux. The attribute list is the kernel's source. The helpers `text`, `hex` and
`dec` (`lib.rs:65-73`, unchanged) were run over a directory laid out as sysfs lays out a USB 2.0
device (`version` ` 2.00`, `bcdDevice` `0100`, `bConfigurationValue` `1`, no `bcdUSB`), read as
`devices` reads it:

```
usbVersionMajor 0 usbVersionMinor 0 usbVersionSubminor 0
deviceVersionMajor 1 deviceVersionMinor 0 deviceVersionSubminor 0
bConfigurationValue 1
```

The device check: on a Linux machine, `ls /sys/bus/usb/devices/usb1/` shows `version` and no
`bcdUSB`, and every record of `attachedDevices()` reads `usbVersionMajor` 0.

## Direction

Parse `version` (` 2.10` is major 2, minor 1, subminor 0) and answer the device's
`bConfigurationValue` in both implementations' `usbConnection`.

## Related

QUEUE "USB, HID and serial (LLP 1133.037): what is left" owes the hardware drive on Linux; neither
field is listed there.
