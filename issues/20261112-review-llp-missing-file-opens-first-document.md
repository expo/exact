# The LLP reader opens a different document when the requested file is missing

**Status:** Open
**Systems:** LLP reader data source
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `apps/llp/data/src/lib.rs:98–105` treats every path that
does not satisfy `is_file()` as a corpus directory. It then returns the
first document in the path's parent corpus. A missing file within a
valid corpus therefore opens a different document successfully, without
the reader's failure message or its fallback to the previously open
document.

This happens when the person mistypes a filename in the graphical
reader's path field, follows a broken local link, or selects an indexed
document that another process has deleted. The index is retained for the
reader's lifetime, so deletion does not remove that selectable row.
Both `document` and `page` use this path selection.

Reproduced through the actual `llp_data::Llp` source, using `answer()`,
its continuation, and `parse()`, linked against the checkout's existing
libraries. A temporary corpus contains only `1000-valid.md`:

```text
asked=.../1000-valid.md; returned=.../1000-valid.md; failed=false
asked=.../9999-missing.md; returned=.../1000-valid.md; failed=false
```

The missing request also returns an empty error message. Source and
output are `/tmp/exact-review-20261112/llp-paths/probe.rs` and
`result.txt`. No graphical or terminal app was launched.

Choose the first document only when the requested path is an existing
directory. Propagate a missing or inaccessible file as an ordinary open
failure, preserving the previous document where the reader promises to
do so. Cover a missing filename in a populated corpus, an indexed file
deleted after loading, and an existing directory that should still open
its first document.

Checked all open and closed filesystem issues and the full bodies of the
275-issue GitHub snapshot. The closed LLP citation and newest-order
reports concern different selection paths. No report covers this
missing-file fallback.
