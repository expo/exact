# Two markdown decorations share one shadow under earlier glyphs

**Status:** Open
**Systems:** host/linux
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

Every markdown decoration in one text uses that text node as its source. Shadows with the same source and the same shadow are merged, and the merged mask keeps the earliest run. The later span's shadow is therefore painted in the earlier span's step. That step then paints its glyphs on top.

An offset `text-shadow` on two strikes or underlines in one markdown text lets the first span's glyphs cover the second span's shadow where they overlap. Two elements would paint the later shadow after the earlier glyphs.

## Where

`host/linux/src/paint/text_decoration.rs`, the markdown mark's source and `shadows`. `host/linux/src/paint/text_shadow.rs`, the step that paints a mask only when `first_run` is that step's start.

## How to reproduce

In one markdown text, strike or underline two separate spans and set an offset `text-shadow`. The second span's shadow is painted under the first span's glyphs where the two overlap.

## Direction

Key each markdown decoration span on its own, so its shadow stays in the step that owns that span, after the earlier glyphs.
