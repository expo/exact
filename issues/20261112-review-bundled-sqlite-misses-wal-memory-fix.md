# Bundled SQLite lacks the fix for reading memory through a crafted WAL

**Status:** Open
**Systems:** Cargo.lock, vendored Ibex SQLite provider, native SQLite consumers
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `Cargo.lock:3243` resolves `libsqlite3-sys` to 0.38.2.
`vendor/ibex/crates/ibex2-sqlite/Cargo.toml:12`, `js/Cargo.toml:52` and
the vault fixture select rusqlite's `bundled` feature. This compiles SQLite
3.53.2, affected by CVE-2026-119375. A database and its sibling WAL with
different page sizes can make queries return uninitialized page-cache
bytes. The fix is in SQLite 3.54.0, bundled by `libsqlite3-sys` 0.39.0
and rusqlite 0.41. See the
[RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2026-0459.html).

The provider opens existing files with `Connection::open_with_flags` at
`vendor/ibex/crates/ibex2-sqlite/src/lib.rs:129`. Its path checks,
defensive mode and SQL authorizer do not replace SQLite's WAL parser.
An app that imports or restores a less-trusted database together with its
WAL would expose the native process to this bug.

The precondition matters: this is not a claim that ordinary queries against
an app's own database disclose memory, or that Fieldnotes imports arbitrary
SQLite files. Fieldnotes imports validated JSON records. No exploitable
import path in a bundled sample app or memory-disclosure reproduction was
established. The finding is that the shipped storage dependency lacks a
published memory-safety fix relevant to consumers that open supplied files.

Validation: parsed the complete Cargo lockfile, checked the OSV result and
the primary advisory, and traced the provider's open path and bundled
feature declarations. The workspace's Rust 1.97 toolchain exceeds the
patched dependencies' stated minimum versions.

Update rusqlite and its resolved SQLite library together. Run the provider,
native-storage, migration and cross-library reservation tests against the
new version. Keep the upstream WAL regression covered when accepting
database files supplied outside the app.

Checked open and closed filesystem issues and all 275 cached GitHub issues
for libsqlite3-sys, the advisory identifiers and WAL memory disclosure. No
matching report was found.
