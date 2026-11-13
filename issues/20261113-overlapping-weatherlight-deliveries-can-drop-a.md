# Overlapping Weatherlight deliveries can drop a counted tip

**Status:** Open
**Systems:** apps/weatherlight
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`deliverPurchases` reads `tips.json`, then for each purchased tip increments the count, writes the file, and consumes the purchase. The delivering task sends that answer again whenever `due` changes, and `due` changes when a consume updates the queue. A second call can read the file before the first write is issued.

Both calls then write their own snapshot. The later write replaces the file. A tip the first call already consumed is not in the second snapshot and is not offered again, so its count is gone. `readJson` refuses to invent a count over a failed read. Two successful reads of the same bytes are not serialized.

## Where

- `apps/weatherlight/app.ts` `deliverPurchases`
- `apps/weatherlight/app.contract` `deliver` and `task delivering when due != ""`

## How to reproduce

Leave two purchased `tip.small` entries in the unfinished queue and let the task fire a second `deliverPurchases` while the first is still in its loop (the first consume changes `due`). The file's `tips` value is short of the number of tokens consumed.

## Direction

Run one delivery at a time. Read the file again after each write, or ignore a `due` change until the in-flight answer has finished, and write a snapshot that still contains every token already consumed.
