# A harness tool entry for `~/…` links a file under the working directory, not the one the tool reads

**Status:** Closed
**Resolution:** `file_link` resolves the call's path with `resolve`, the tools' own function, so a title's link names the file the tool reads, `~/` included.
**Systems:** apps/harness/data (tools.rs)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Two functions in `apps/harness/data/src/tools.rs` (at `dd6f5dcd`) turn a tool call's `path` into a file, and they disagree:

- `resolve` (`tools.rs:115-126`), which `read_file`, `list_dir`, `grep`, `write_file` and `edit_file` use, expands a leading `~/` to `$HOME` and reads an empty path as `.`.
- `file_link` (`tools.rs:89-108`), which makes the `file://` link on the entry's title (`agent.rs:391`), joins the path to the working directory unless it is absolute.

The model has reason to write such paths: the system prompt names the working directory relative to home, "in the directory ~/project" (`lib.rs:71-79,179`, `providers/mod.rs:452`).

## The failure

The model calls `read_file {"path": "~/notes.txt"}` in `/Users/me/project`.

- Observed: the tool reads `/Users/me/notes.txt`; the title `Read(~/notes.txt)` links `file:///Users/me/project/~/notes.txt`, which does not exist.
- Expected: the link names the file read.

## Verification

Ran a test in `tools.rs` that prints both for `~/notes.txt`:

```
reads:  $HOME/notes.txt
links:  file://<cwd>/~/notes.txt
```

## Direction

Have `file_link` call `resolve` and encode its result.

## Related

None found in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md`.
