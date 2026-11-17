# A grouped swipe and a native swipe can stay open together

**Status:** Open
**Systems:** host/apple
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

One swipe row is supposed to stay open. Opening a grouped list row closes painted rows only. `closeNative` calls `setEditing(false)` on each row's table. A grouped row has no table, so that call does not collapse a `UICollectionView` list swipe. Opening a native or painted row therefore leaves a revealed grouped swipe open, and opening a grouped swipe leaves a projected native row open.

UIKit coordinates swipes inside one table or one collection view. These rows are in different views, so both action sets stay open on one screen.

## Where

`host/apple/Sources/ExactKit/IOS/SwipeActionsIOS.swift`, `closeNative`, `configuration`, and `willBeginEditing`. `host/apple/Sources/ExactKit/IOS/PaintedSwipesIOS.swift`, the painted drag that calls `closeNative`.

## How to reproduce

On iOS, reveal a grouped list row's swipe actions, then swipe a projected native row on the same screen. Both stay open. Reveal the native row first, then the grouped row. Both stay open.

## Direction

When a grouped swipe is offered, close native rows as well as painted ones. When a native or painted row opens, dismiss a revealed grouped-list swipe. `setEditing` on a nil table does not do that.
