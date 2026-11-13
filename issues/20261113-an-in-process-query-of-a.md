# An in-process query of a stream leaves the module outstanding

**Status:** Open
**Systems:** js
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`Module::query` of a stream or a socket returns `Unavailable` after deleting only `self.parked`. `park` stored that call in `self.streams`, and the prelude call is not forgotten. `outstanding` stays true because `streams` is not empty.

A later answer parked `WAITING`, not ready and with no due timer, is never asked again. `wake` returns `None` while anything is outstanding.

## Where

`js/src/source.rs`, the non-timer `Later` arm of `query`. `js/src/resume.rs`, `park`. `js/src/turns.rs`, `outstanding` and `wake`.

## How to reproduce

In-process, query a source that opens a stream, then a source that waits with no timer. The stream query fails as unavailable. The waiting source is never asked again.

## Direction

On that arm, forget the call and remove both the parked rows and the stream rows for that key before returning `Unavailable`.
