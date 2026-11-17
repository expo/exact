# A canvas GPU failure drops the pixels it accumulated

**Status:** Closed
**Resolution:** When the GPU fails a replay, Core Graphics continues from a copy of the GPU's last pixels and the tracked state, applies the failed lists, and shows that bitmap.
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

While a canvas is on the GPU, a frame records fonts and replays. It does not `keep` the draw lists. If `replay` returns no surface, the GPU canvas is dropped and the function returns. The log says Core Graphics draws the next bitmap. Those lists are not in the kept tracker, so the next bitmap is built from whatever was kept before the GPU took the canvas.

A canvas that paints over its last picture loses the pixels the GPU frames drew. One device-loss or shader failure is enough.

## Where

`host/apple/Sources/ExactSurfaces/Canvas2DImage.swift`, the GPU branch of the replay job: `trackFonts`, `replay`, and the `surface == nil` path. `keep` runs only on the Core Graphics branch below that return.

## How to reproduce

Draw on a canvas that the GPU owns, accumulating strokes across frames. Force `replay` to return no surface (device loss, or a shader failure). Draw again. The bitmap Core Graphics produces does not contain the strokes from the GPU frames.

## Direction

Before dropping the GPU canvas, `keep` the lists that failed, or copy the last surface into the bitmap Core Graphics will draw. The next frame should show that picture.
