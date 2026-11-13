# The LLP reader cannot open a bare filename from the current directory

**Status:** Open
**Systems:** LLP reader data source
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, `apps/llp/data/src/lib.rs:81–93` uses a file path's
parent as its corpus root. For a bare relative filename such as
`1000-valid.md`, Rust returns an empty parent path. The continuation
passes that empty string to `index::read()`, whose `read_dir("")` fails
(`index.rs:161–163`). The document itself exists, but the reader refuses
to open it because loading its corpus is a prerequisite.

The graphical reader passes the path field's text to this source
unchanged (`apps/llp/app.contract:110–113`). Entering `1000-valid.md`
there fails, whereas `./1000-valid.md` or its absolute path opens the
same file. The terminal's initial command-line argument is canonicalized
and avoids this particular case.

Reproduced through the actual `llp_data::Llp` source, using `answer()`,
its continuation, and `parse()`, linked against existing checkout
libraries. The probe's current directory contains `1000-valid.md`:

```text
asked=/tmp/.../1000-valid.md; returned=/tmp/.../1000-valid.md; failed=false
asked=1000-valid.md; returned=""; failed=true
message=": No such file or directory (os error 2)"
```

Source and output are `/tmp/exact-review-20261112/llp-paths/probe.rs`
and `result.txt`. No app was launched.

Normalize the empty parent of a relative filename to the current
directory before indexing it, keeping document and index path identities
consistent. Cover a bare filename, `./filename`, a file in a relative
subdirectory, and an absolute path to the same document.

Checked all open and closed filesystem issues and the full bodies of the
275-issue GitHub snapshot. No report covers the empty corpus root.
