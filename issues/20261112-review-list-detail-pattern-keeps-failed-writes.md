# The list-detail pattern keeps a failed write in its cache

**Status:** Open
**Systems:** docs/patterns/list-detail/app.ts, docs/patterns/list-detail/app.contract
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Problem

`save` replaces the module's `items` cache before awaiting `atomicWriteFile`
(`app.ts:13–17`). If storage refuses the write, the mutation fails but subsequent
reads and mutations use the unsaved items. A failed add then prevents its own
retry: `addItem` finds the cached title and returns the duplicate-title refusal.
That completed reply refreshes the list, showing the item even though storage
never accepted it. Relaunching loses it.

Failed removes and lock changes likewise leave the cache ahead of storage. An
unrelated successful mutation can later persist those previously failed changes.
Apps copied from this starter inherit the behavior.

## Reproduction

Verified at `f6be99af` by importing the actual data module with controlled storage:

1. Begin with an empty `items.json` and make its next atomic write throw `ENOSPC`.
2. Call `addItem("Alpha", "notes")`: it rejects with `ENOSPC`.
3. Restore successful writes and repeat the call: it returns `ok: false` with
   “There’s already an item called Alpha.”
4. Call `loadItems`: it returns Alpha, while the stored array is still empty.

## Fix

Publish the new cache only after the write succeeds. Serialize mutations that
share the cache so a concurrent mutation cannot read an intermediate version or
overwrite a successful one. Cover failed writes followed by retry and reload.

## Existing reports

No matching filesystem issue or GitHub issue was found in the review's all-state
snapshot. The closed update-store issue
`20261110-check-reports-staged-after-record-write-fails` concerns a different
cache and does not fix this starter.
