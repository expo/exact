# Untitled status items share one autosave name after a removal

**Status:** Open
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A status item with no `testId` and no author `id` is named `exact-status-<items.count>` when it is created. Count is how many items are already placed. Two such buttons become `exact-status-0` and `exact-status-1`. Removing the first leaves the survivor on `exact-status-1`. The next new button also sees a count of 1 and takes `exact-status-1`.

AppKit keeps the menu-bar position under `autosaveName`. Two live items with one name share one slot, and the removed item's saved position is abandoned.

## Where

`host/apple/Sources/ExactKit/Mac/StatusItemsMac.swift`, `place`.

## How to reproduce

Place two status buttons that set neither `testId` nor `id`, remove the first, then place another. The two that remain use one autosave name.

## Direction

Key the autosave name on the button's node id.
