# A crash before Observe starts is filed under the next launch

**Status:** Open
**Systems:** modules/observe
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

The panic hook writes `session`, `sessionStart`, and the panic text. It does not write the app version, build, or environment. The session row is inserted when startup runs. A panic before that has no stored metadata.

The next launch, finding no row for the crashed session, inserts one whose `meta` is this process's. After an update, the crash is counted against the build that opened next. `INSERT OR IGNORE` keeps an old row only if that process had already saved one.

## Where

`modules/observe/linux/launch.rs`, the panic hook and `ingest_crashes`. The session row is written from `after_startup`.

## How to reproduce

Panic during launch, before startup saves the session, then open a build with a different version. The ingested crash's session metadata is the new build's.

## Direction

Stamp app version, build, and environment into the crash record when the handler is installed, and store that metadata for the crashed session.
