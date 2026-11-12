# A virtual list scroll write can erase another scroller's capture on iOS

**Status:** Closed
**Resolution:** The iOS presenter restores every captured scroll position before it applies any authored offset, so a batch applied from a list's scroll feedback during the restore finds no capture left to clear.
**Systems:** host/apple/Sources/ExactKit/Collection.swift, host/apple/Sources/ExactKit/IOS/PresenterIOS.swift, host/apple/Sources/ExactKit/IOS/ScrollViewIOS.swift
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`host/apple/Sources/ExactKit/IOS/PresenterIOS.swift:821–824` calls `collections.endBatch()` before restoring scroll positions. That drops `batchDepth` to zero (`Collection.swift:588`). An authored offset is then applied during the restore loop (`IOS/ScrollViewIOS.swift:255–264,483`). Its synchronous scroll callback calls `collections.changed(..., user: true)` (`IOS/NodeViewIOS.swift:792`). Outside the mounted window, `Collection.swift:691,908` synchronously sends feedback; `Session.swift:733–740` applies the resulting batch immediately.

That nested batch captures, restores and clears other scrollers' captures. When the outer loop reaches a plain follow-end scroller, `IOS/ScrollViewIOS.swift:386` treats its missing capture as an instruction to follow the end.

## The failure

Put a virtualized list and a plain `scrollFollowEnd=true` scroller in one session. Scroll the plain scroller above its end. In a batch, write the list's `scrollTop` far outside its mounted rows. If the restore loop visits the list first, its feedback batch consumes the plain scroller's capture, and the outer restore sends that scroller to the end. Expected: its reading position survives the unrelated list jump.

## Verification

Verified by reading; no iOS simulator or device is available. The direct feedback path bypasses the deferred-callback queue and is not guarded by `Presenter.applying`. A device check must use a real session, arrange or record a run where the list is restored first, write an offset outside its mounted window, and compare the plain scroller's offset before and after. A presenter-only test with no `onFeedback` would miss the nested batch. No device execution is claimed.

## Direction

Prevent collection feedback from applying a nested batch during scroll restoration, while preserving the list's required synchronous fill and the outer scrollers' captures.

## Related

[Follow-end scroller jumps after nested covers batch](closed/20261110-follow-end-scroller-jumps-after-nested-covers-batch.md) fixed callbacks after navigation layout by moving restoration earlier. This callback happens inside restoration itself. Reported in the p2-follow-end-scroller review.
