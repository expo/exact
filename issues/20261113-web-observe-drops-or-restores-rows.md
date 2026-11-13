# Web Observe drops or restores rows because appends skip the lock

**Status:** Open
**Systems:** modules/observe
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

Every tab shares one localStorage queue. `update` loads, changes, and saves with no lock. `drain` takes `navigator.locks` only around the send. Two appends can interleave `getItem` and `setItem`, so the later save omits the other tab's new row. An append that loaded before the sender's save writes the pre-send queue back and restores rows just removed, so they are sent again.

`getItem` and `setItem` are atomic per call, not across a read-modify-write. The closed double-send fix locked only the send path.

## Where

`modules/observe/web/service.js`, `update`, the removal inside `send`, and `exclusive`.

## How to reproduce

Log from two tabs at the same time. One tab's row can be missing from the queue. Send from one tab while the other appends: a row that was removed can reappear and be sent again.

## Direction

Hold the same Web Lock for every `update`, including appends, and do not release it until `setItem` returns.
