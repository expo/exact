# The harness /image command reads the whole file to parse a header

**Status:** Open
**Systems:** apps/harness
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`/image` asks for a file's pixel size from its header. `image_size` then reads the whole file with `std::fs::read` before looking at the first bytes. PNG and GIF sizes sit in the first 24 and 10 bytes. `submit` calls this on the data-source thread.

A large PNG, GIF, or JPEG stalls the harness until the read finishes, and the whole file is held in memory. The command already rejects a path that is not a regular file.

## Where

`apps/harness/data/src/art.rs`, `image_size`. `apps/harness/data/src/commands.rs`, the `/image` submit that calls it.

## How to reproduce

Run `/image` on a large PNG or JPEG. The turn waits while the whole file is read. The size it returns is in the header.

## Direction

Read only the header bytes needed to identify PNG, GIF, or JPEG, with a small cap. If the header is incomplete, return the fallback size.
