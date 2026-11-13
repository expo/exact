# An atomic replace of a kept document forgets it when the keep set cannot be saved

**Status:** Closed
**Resolution:** `carry` returns the failed save of the keep set and the atomic write fails with it (from Rust and TypeScript) after writing the file; the session keeps the new identity, not the destroyed one, so writing again carries the entry and saves the set.
**Systems:** data documents
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

An atomic write of a kept file replaces the file, then `carry` records the new inode on the in-memory keep record and ignores a failed save of that set. The file on disk is the one just written. The keep set on disk still names the inode the replace destroyed.

`keep` and `forget` put the previous record back when the set cannot be saved. `carry` does not. The next launch's adopt refuses the inode mismatch and drops the record. The file is still in the folder. The app no longer lists it as a document it keeps.

## Where

- `data/src/documents/kept.rs` `carry`: assigns `r.identity` and then `let _ = save(store)`
- `data/host/src/native.rs`: `carry` runs after `AtomicWriteFile` has succeeded
- `keep` and `forget` in the same file roll the record back when `save` fails

## How to reproduce

Atomically replace a kept document, and fail the keep-set write (a full disk on that file, or a `save` that returns an error). Relaunch. `recentDocuments` no longer includes it, and the new file is an ordinary path.

## Direction

If `save` fails, put the previous identity back, as `forget` puts the record back, and surface the error from the write that already happened so the app can retry the binding.
