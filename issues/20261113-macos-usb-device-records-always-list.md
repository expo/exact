# macOS USB device records always list no endpoints

**Status:** Open
**Systems:** packages/usb
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

macOS device records list each interface alternate with `endpoints` set to an empty array. The IORegistry walk reads interface properties and does not read the interface's endpoint children. Linux and the web include the endpoints the device published.

An app that chooses an endpoint from the device record finds none on macOS for a device that has them.

## Where

`packages/usb/apple/USB.swift`, `configurations`. `packages/usb/linux/lib.rs` and `packages/usb/web/index.js` list endpoints.

## How to reproduce

Read `getDevices` for a USB device with bulk or interrupt endpoints on macOS. Every alternate's `endpoints` is `[]`. The same device on Linux or the web lists them.

## Direction

Read each interface's endpoint children from the registry and fill `endpoints` the way the other hosts do.
