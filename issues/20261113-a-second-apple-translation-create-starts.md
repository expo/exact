# A second Apple translation create starts another download sheet

**Status:** Open
**Systems:** packages/translation
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

The agent path queues one waiter list per language pair and starts one download. The real `download` replaces `downloads[pair]` with an empty list and starts a new hosted `prepareTranslation` sheet for every `create`.

A second create for the same pair, while the first sheet is up, presents another sheet. The first create's callback is not in the list that was replaced, so both sheets run.

## Where

`packages/translation/apple/Translation.swift`, `create` and `download`.

## How to reproduce

Call `create` twice for a pair whose status is `supported`, before the first download sheet finishes. Two sheets appear.

## Direction

If a download for that pair is already in progress, append the second create to its waiter list and present one sheet.
