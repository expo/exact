# user-select all on a logical parent does not select as one unit

**Status:** Closed
**Resolution:** `selectAllElement` walks the authored parents `textSelectable` walks, and an element above a subtree shown in a view of its own answers with that subtree's root, the part of it the selection can reach.
**Systems:** host/apple
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`textSelectable` walks `eventParent`, then the superview. On macOS that reaches a dialog, a popover, or a block root's authored parent. On iOS it reaches a route controller's logical parent. `selectAllElement` walks the superview only.

`user-select: all` on that parent makes the text selectable, because the first walk sees `all`. The atomic element stays nil, because the second walk never reaches the parent. A drag then takes part of content that CSS UI 4 selects as one unit. Mac atomize and the iOS expand run only when `selectAllElement` is set.

## Where

`host/apple/Sources/ExactKit/TextSelectable.swift`, `textSelectable` and `selectAllElement`.

## How to reproduce

Put `user-select: all` on a parent that is an `eventParent` and not a superview (a dialog's content, a route). Select part of the text with a drag. The selection is the dragged range. The same style on a superview selects the whole element.

## Direction

Build `selectAllElement` with the same `eventParent` walk `textSelectable` uses, and keep the outermost `user-select: all` on that walk.
