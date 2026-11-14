# An iOS absolute-url NFC write rejects bytes

**Status:** Closed
**Resolution:** The `absolute-url` arm of `encode` decodes its data through `NfcWire.data`, as `url` does, so bytes are written as a string is, and still requires a colon.
**Systems:** packages/nfc
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`NDEFRecordInit.data` is a string or bytes. Bytes cross the seam as `{base64}`. `NfcWire.data` accepts both. The `url` and `text` cases decode with that helper. `absolute-url` accepts only a `String`.

A write whose absolute URL is bytes fails `TypeError` on iOS and is written on Android.

## Where

`packages/nfc/apple/Nfc.swift`, `NfcWire.data` and the `absolute-url` arm of `encode`. `packages/nfc/android/Nfc.kt` accepts both.

## How to reproduce

Write an `absolute-url` record whose `data` is `{base64: ...}` of a URL that contains a colon. iOS returns `TypeError`. Android writes the record.

## Direction

Decode `absolute-url` data with `NfcWire.data`, as the `url` case does, and require the text to contain a colon.
