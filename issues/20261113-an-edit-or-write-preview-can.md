# An edit or write preview can block the harness turn on a pipe

**Status:** Open
**Systems:** harness tools
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`write_file` and `edit_file` build their approval preview by reading the whole target with `std::fs::read` before a tool entry exists and before the turn's cancel flag is consulted. A fifo, a device node, or anything else that never reaches EOF never returns.

Interrupt sets cancel and settles the entries that already exist. The turn stays the running one until that thread reaches `release()`, so the session stays busy and shutdown waits on the read. `read_file` and grep already reject a non-regular file, cap the bytes, and poll cancel. There is a test that a fifo can be read on the `read_file` path. Preview does not use it.

## Where

`apps/harness/data/src/tools.rs`: `contents` calls `std::fs::read`; `prepare` calls `contents` for `write_file` and `edit_file`; `preview` calls `prepare`. The bounded read is `read_file` further down the same file.

## How to reproduce

Ask the harness to edit a fifo (`mkfifo`). The approval preview blocks. Esc does not return the session to idle.

## Direction

Open the target, reject anything that is not a regular file, and read a bounded prefix with the same cancel checks as `read_file` before building the diff.
