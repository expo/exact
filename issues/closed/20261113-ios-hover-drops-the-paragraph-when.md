# iOS hover drops the paragraph when the pointer leaves an inline run

**Status:** Closed
**Resolution:** hovering enters the paragraph whenever the pointer is off an inline hover run and the paragraph is not already hovered, and ends, fails or cancels by clearing both the inline and the paragraph hover.
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`hovering` returns before the node-hover switch while the pointer is on an inline hover run. Sliding onto the rest of the paragraph only calls `hoverInline(nil)`. Entering on the run never calls `hover(self, true)`. The next `.changed` clears the inline target and does not enter the node, so neither target is hovered until the pointer leaves and comes back off the run. `.failed` over the run takes that same return, so the leave written for `.failed` does not run and the inline hover stays set.

## Where

`host/apple/Sources/ExactKit/IOS/AffordancesIOS.swift`, `hovering`.

## How to reproduce

Hover an inline run inside a paragraph, then slide onto the rest of that paragraph. The run's hover ends and the paragraph's does not begin.

## Direction

On `.changed` with no inline target, call `hover(self, true)` when this node is not already hovered. On `.failed`, clear the inline hover and run the same leave as `.ended`.
