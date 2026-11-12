# Gzip decompression accepts an incorrect header checksum

**Status:** Closed
**Resolution:** `gzip_header` compares FHCRC with the low 16 bits of the CRC32 of the header bytes before it (RFC 1952 §2.3.1) and refuses a mismatch before any payload is inflated.
**Systems:** js/src/compression.rs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`gzip_header` skips the two FHCRC bytes without comparing them with the low 16 bits of the header's CRC32 (`js/src/compression.rs:230`). The decompressor validates the payload CRC and size, but accepts a corrupt header checksum. The existing all-fields fixture supplies `0xbbaa` (`js/src/compression.rs:500`) where its header requires `0x7179`.

## The failure

Start with header bytes `1f 8b 08 1e 00 00 00 00 00 03 03 00 61 62 63`, append `name\0comment\0`, then `aa bb`, raw deflate of `hello, header`, and that payload's valid CRC32 and size trailer. Both `write` and `finish` return `Ok(())`, producing `hello, header`. Expected: reject the wrong FHCRC; the same stream with `79 71` succeeds.

## Verification

A temporary integration test included the production `compression.rs` and called `Decompressor` with each checksum:

```
cargo test -p exact-js --test fix08_fhcrc fix08_ -- --nocapture
header checksum: 0x7179
FHCRC 0x7179: Ok(()); output "hello, header"
FHCRC 0xbbaa: Ok(()); output "hello, header"
wrong FHCRC must be refused
```

One test failed as expected. The temporary test was removed.

## Direction

Once the complete header is available, check FHCRC before passing the payload to the inflater. Give the split-header fixture a valid checksum and cover a mismatching one.

## Related

The deflate-raw-tail lane concerns trailing input, not header integrity. No matching issue or QUEUE entry was found.
