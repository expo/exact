# Retained content replay paints text outside its overflow clip

**Status:** Closed
**Resolution:** Replay pushes a node's overflow clip before its content, so a retained text's lines are drawn under it as the live walk draws them, and the node's leave pops it as before.
**Systems:** host/linux
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13
**Related:** issues/closed/20261111-linux-paints-nowrap-text-outside-its.md

## What is wrong

A text node in a retained content region is painted by replay. Replay draws the paragraph, then pushes the overflow clip. The glyphs are already outside the clip. The live walk pushes that clip before it draws the text node's lines.

Over-wide nowrap ink is not clipped by paragraph rasterization. Linux, Windows, and Android share this painter, so the replayed text draws past the box.

## Where

`host/linux/src/paint/region.rs`, the `Payload::Text` arm and the overflow clip pushed after it. `host/linux/src/paint.rs`, the live text node, which clips before `text_node`.

## How to reproduce

Put nowrap text in a box with `overflow: hidden`, on a path that replays a retained region (a scrolled or offscreen content picture). The glyphs extend past the box. The same text on the live walk is clipped.

## Direction

Push the overflow clip before the text draw, as the live path does, and pop it with the region's leave.
