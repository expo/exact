# Refreshing Convex live drops in-flight mutations

**Status:** Open
**Systems:** packages/convex
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`live()` increments `epoch`, drops `socket` without closing it, and ignores every event from the previous stream, including close. `Sync.connecting` does not settle `requests`. `closed` is what fails or completes them.

A refresh, which the API documents as connecting again, leaves mutations and actions from the old socket pending forever. The old close is ignored, and the new connection does not know those request ids. Queries can recover from the next transition. Requests cannot.

## Where

`packages/convex/index.ts`, `live`. `packages/convex/sync.ts`, `connecting` and `closed`.

## How to reproduce

Start a mutation, then call `live()` again before the mutation's result frame arrives. The old socket's close is ignored. The mutation promise never settles.

## Direction

Before ignoring the old stream, close that socket and call `sync.closed` once so in-flight requests settle.
