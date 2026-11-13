# A horizontal scrollbar shortens when no vertical bar is drawn

**Status:** Open
**Systems:** host/linux
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A horizontal scrollbar shortens its track by the vertical bar's width whenever the content is taller than the frame. A vertical bar is drawn only for `overflow` auto or scroll. `overflow-y: hidden` or `clip` beside a horizontal scroller draws no vertical thumb, and `max.y` can still be greater than zero.

The horizontal track loses 12 px and the thumb stops short of the corner. The corner is empty.

## Where

`host/linux/src/paint/scrollbar.rs`, the track rectangle. The axis skip is the `Auto` or `Scroll` match just above it.

## How to reproduce

Make a box scroll horizontally, with `overflow-y: hidden` and content taller than the box. The horizontal thumb stops short of the right edge by the width of a vertical bar that is not there.

## Direction

Reserve that width only when the vertical axis will emit a bar.
