# A Windows short name can replace a stream head with put

**Status:** Open
**Systems:** filesystem
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`put` refuses a four-segment path whose first segment is `.exact` and whose leaf is `exact.json` or `.lock`, comparing ASCII case only. The Windows directory opens each segment with `NtCreateFile` and `OBJ_CASE_INSENSITIVE`. On a volume that still has 8.3 names, that lookup also matches the DOS short name. `exact.json` is not a legal 8.3 name, because the extension is four characters, so its short name is a different string, and `.exact` is generated the same way. `name()` allows those strings. A `put` of the short path is not the string the guard checks, and the rename lands on the head or the lock. `putHead`'s lock and digest check never run.

## Where

`filesystem/src/main.rs`, the `put` path check. `filesystem/src/directory_windows.rs`, `name` and `open_encoded`.

## How to reproduce

On a volume with 8.3 names enabled, `put` the short name of `.exact/<stream>/<cohort>/exact.json`. The head file is replaced.

## Direction

After the relative open, compare the file id with the canonical `exact.json` and `.lock`, and refuse `put` when it is either, before the rename.
