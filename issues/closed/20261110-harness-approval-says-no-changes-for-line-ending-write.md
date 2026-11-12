# A harness approval shows "(no changes)" for a write that changes line endings or the final newline

**Status:** Closed
**Resolution:** When a write or edit leaves every line's text the same, its approval and its entry say whether the line endings change (from which to which) or the final newline is added or removed, in place of "(no changes)".
**Systems:** apps/harness/data (tools.rs)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

The approval for `write_file` and `edit_file` shows `diff(before, after)` (`apps/harness/data/src/tools.rs:231-241` at `dd6f5dcd`). `diff` splits both texts with `str::lines` (`tools.rs:863-864`), which drops `\n` and `\r\n` alike and does not say whether the last line ends in a newline. Two texts that differ only there compare equal line by line, and the approval reads `(no changes)`. The write then runs and changes the file.

The closed issue `20261006-harness-file-approval-stale` added a line for one such case: "a byte change that reads as the same text says so in the preview" (`tools.rs:235-238`, for a file that is not UTF-8). That test compares the whole strings, so it does not catch this one: here the strings differ and only their `lines()` agree.

## The failure

A file holds `one\r\ntwo\r\n`. The model calls `write_file {"path": …, "content": "one\ntwo\n"}`.

- Observed: the approval's lines are `(no changes)`; after "Yes" the tool answers `Wrote …`, its entry shows `(no changes)`, and the file is `one\ntwo\n`. The same for `edit_file` on `one\ntwo\n` with `old: "two\n"`, `new: "two"`: the preview is `(no changes)` and the final newline goes.
- Expected: the approval says what the write does, as it does for the non-UTF-8 case.

## Verification

Ran a test in `tools.rs` that calls `preview` and `run_prepared` on a temporary file:

```
CRLF to LF preview: ["(no changes)"]
result: "Wrote …/note.txt" / ["(no changes)"]
bytes now: [111, 110, 101, 10, 116, 119, 111, 10]
drop final newline preview: ["(no changes)"]
```

## Direction

When `diff` finds no changed line and the bytes differ, say which it is: line endings changed, or the final newline was added or removed. The existing "bytes change" branch is the place.

## Related

`issues/closed/20261006-harness-file-approval-stale.md`: its fix covers the stale preview and the non-UTF-8 case.
