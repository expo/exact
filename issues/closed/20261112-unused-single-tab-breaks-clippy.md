# An unused copy of single_tab breaks the required Clippy check

**Status:** Closed
**Resolution:** Commit `3bf6c302` removed the unused duplicate from `roles.rs`; the active diagnostic remains in `tabs.rs`.
**Systems:** contract/lower/src/warnings/roles.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, the required command
`cargo clippy --all-targets --keep-going -- -D warnings` exits 101:

```text
error: function `single_tab` is never used
   --> contract/lower/src/warnings/roles.rs:213:15
    = note: `-D dead-code` implied by `-D warnings`
```

The review's baseline run failed both `contract-lower` library and library
test targets on this error. The complete output is retained at
`/tmp/exact-review-20261112/cargo-clippy.log`; the diagnostic starts at
line 171. It was run with incremental compilation disabled and dev/test
debug information disabled to fit the available disk. Those settings do
not alter this warning.

The warning feature is still wired: `warnings.rs:125` calls
`tabs::single_tab`, whose implementation starts at
`contract/lower/src/warnings/tabs.rs:12`. The copy in `roles.rs:213-244`
has no caller. Remove that obsolete copy. Keep the active single-tab
diagnostic and its existing regression coverage, then rerun the required
Clippy command with warnings denied.

Checked open and closed filesystem issues and all 275 cached GitHub
issue bodies. The closed dead-code report covers Tamber and verification
scripts; no report covers this Rust duplicate or its current check failure.
