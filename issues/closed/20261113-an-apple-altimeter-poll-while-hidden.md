# An Apple altimeter poll while hidden under-counts dropped samples

**Status:** Closed
**Resolution:** A poll answered while the app is hidden withholds its samples by counting each of them as dropped, as Android does, so the cursor skips none uncounted.
**Systems:** packages/altimeter
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A poll that starts while the app is shown waits up to five seconds, then calls `answer`. If the app has been hidden by then, `answer` clears `samples` and forces `interrupted`, and leaves `dropped` as it computed it from the samples it just withheld.

`sequence` is still the newest sample. The runner stores that as the cursor. Samples still in the ring are neither returned nor added to `dropped`, so the history skips them. Android empties the batch too, but its kept list is empty while hidden, so those samples are counted in `dropped`.

## Where

`packages/altimeter/apple/Altimeter.swift`, the poll wait and `answer`. `packages/altimeter/android/Altimeter.kt`, `answer`.

## How to reproduce

Watch the altimeter with `history` greater than 0. Background the app while a poll is waiting and samples are in the ring. The answer has an empty `samples` list, `interrupted` true, and a `dropped` count that omits the withheld samples. The next poll's `after` is the newest sequence.

## Direction

When withholding the batch, add its length to `dropped` before clearing `samples`, so `sequence - after` equals `samples` plus `dropped`.
