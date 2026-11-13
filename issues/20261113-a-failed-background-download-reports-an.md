# A failed background download reports an old saveTo file

**Status:** Open
**Systems:** packages/background-transfer
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`place` moves a finished record's body from the carrier path to `saveTo`. When the carrier path is missing it skips the move. It then returns `saveTo` whenever that file already exists and is not a folder.

An error or abort that never wrote a body still hands the app the old file's path. A download that was meant to replace that file, and then failed, is presented as if those old bytes were the response. A later registration read does the same.

## Where

`packages/background-transfer/apple/Transfers.swift`, `place` and `registration`.

## How to reproduce

Start a background download with `saveTo` pointing at a file that already exists. Fail or abort it before a body is stored on the carrier. Read the registration. `path` is the old file.

## Direction

Remember a path only after a move succeeds, and return that on later reads. If the carrier has no body, return nil even when `saveTo` already exists.
