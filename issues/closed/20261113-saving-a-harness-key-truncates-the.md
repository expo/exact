# Saving a harness key truncates the key file before the new text is written

**Status:** Closed
**Resolution:** `write_line` writes the new text to a 0600 file created beside the keys file, syncs it, renames it over the keys file and syncs the folder, so the old file stays whole until the new one replaces it.
**Systems:** harness keys
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`Keys::set` updates the in-memory map, then `write_line` opens `~/.config/exact-harness/keys` with `truncate(true)` and writes the rewritten file afterwards. A crash, a kill, or a short write after that open leaves an empty or partial file. The next launch has lost every key, while the process that failed still holds the new map in memory, so a later successful save can also persist a mix the disk no longer matches.

The file mode is set to `0600` between the truncate and the write, which is the same window.

## Where

`apps/harness/data/src/keys.rs`: `Keys::set`, then `write_line`'s `OpenOptions` (`write`, `create`, `truncate`).

## How to reproduce

Put two keys in the file. Stop the process after `open` returns and before `write_all` finishes (a kill, or a full disk). The file is empty. Restarting the harness loads no keys.

## Direction

Write the new text to a temporary file in the same directory, `fsync` it, and rename it over `keys`. Keep the `0600` mode on the temporary file.
