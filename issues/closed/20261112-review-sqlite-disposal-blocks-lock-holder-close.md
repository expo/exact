# Disposing a SQLite lock waiter blocks the holder's close until timeout

**Status:** Closed
**Resolution:** Disposing a client of the web SQLite worker ends its opens still waiting for a file lock (`lockPaths` takes a cancel signal), so its disposal settles at once and neither the lock holder's close nor another client's work waits for the 10-second bound.
**Systems:** web SQLite lifecycle, host/web/storage-worker.js, host/web/storage-sqlite.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Problem

Disposing one SQLite client can stall every other client sharing its worker,
including the close that would release the lock the disposed client awaits.
The wait lasts until the ten-second file-lock timeout breaks the cycle.

Reviewed at `f6be99af`. `storage-sqlite.js` shares one worker per app and
sends a `dispose` message when one client ends while another remains. In
`host/web/storage-worker.js:330-350`, disposal adds the ending client's
entire queue to the global `retiring` promise. Every later operation from
every client waits for that promise before dispatching.

The cycle is:

1. Client A has a database open and holds its file lock.
2. Client B asks to open that file; its queue waits in `lockPaths`.
3. B is disposed while its open is pending.
4. A closes its database. The close waits for B's disposal, which waits for
   B's open, which waits for A's close.

Unrelated operations arriving after step 3 also wait behind this cycle.
`host/web/storage-fs.js:64-108` eventually rejects B's acquisition after
10,000 ms. B has already gone, but its abandoned work delays active clients.

## Evidence

Executed the actual `onmessage` scheduling block from `storage-worker.js`
on Bun 1.4.2, with a substitute dispatcher that holds A's lock and lets B's
open wait. Sent B's dispose, A's close, and an unrelated C operation.
Neither A nor C reached the dispatcher. Injecting the lock timeout rejected
B's open; only then did A close and C execute.

The probe controls the lock boundary and advances pending microtasks without
wall-clock sleeps. It verifies the production queue dependencies, not
SQLite or browser Web Locks themselves.

## Fix and validation

Cancel pending acquisitions when their client is disposed. Scope cleanup
ordering to operations that need that client's resources, so unrelated
clients and lock-releasing closes can proceed. Keep the guarantee that a
replacement sees its predecessor's released locks.

Test two clients contending for one database, dispose the waiter, then close
the holder and run an operation on another file. Both live operations must
complete without waiting for the abandoned acquisition's timeout.

## Duplicate check

Checked open and closed filesystem issues and all 275 cached GitHub issues.
The closed `20261110-web-sqlite-failing-statement-changes-not-persisted.md`
concerns SQL persistence after a statement error. No existing report covers
this disposal/lock dependency cycle.
