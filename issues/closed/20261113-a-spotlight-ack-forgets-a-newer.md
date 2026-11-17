# A Spotlight ack forgets a newer content-index update

**Status:** Closed
**Resolution:** Each telling stamps its keys and an acknowledgement unmarks only keys still at its stamp; Spotlight is told only after the mirror is written.
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`publish` marks the changed keys pending, saves, and tells Spotlight. The success handler reloads the mirror and removes those keys from `pending`. A second set or delete of the same key, before the first acknowledgement, leaves two operations in flight. The older success unmarks the key. Pending is the crash-recovery mark between the mirror and Spotlight. After that unmark, a crash before the newer acknowledgement leaves Spotlight on the older index, or still showing a deleted entry, and the next launch sees an empty pending set. `save` uses `try?`, and `tell` still runs when the durable write fails, so the acknowledgement can clear a mark that was never stored.

## Where

`host/apple/Sources/ExactKit/ContentIndex.swift`, `publish`, `tell`, `save`, `finishPending`.

## How to reproduce

Update one content-index key twice before the first `indexSearchableItems` callback, then quit before the second callback. Relaunch. The key is not pending, and Spotlight can show the first value.

## Direction

Stamp each key with a generation at publish. On acknowledgement, unmark only that generation. Call Spotlight only after the mirror write succeeds.
