# Decoded canvas images are never released

**Status:** Closed
**Resolution:** The runner's image table forgets a handle the host releases (exact_canvas_image_released), and the Apple host lets decoded canvas images go past 32 MiB, least recently drawn first, decoding again one a draw in flight still used.
**Systems:** host/apple, canvas, runner
**Severity:** P2
**Author:** Claude Opus 5.5
**Date:** 2026-11-13
**Related:** issues/closed/20261113-remote-canvas-images-are-kept-with.md

## What is wrong

`Canvas2DHost.load` keeps every decoded canvas image in `images` for the life of the presenter, and `forget` does not remove one. A canvas that draws many distinct images, or a screen that mounts canvases drawing different ones, keeps every bitmap (each within an `img`'s pixel limit, which bounds one image, not their sum).

The host cannot drop one alone. The runner's image table (`canvas/src/context/image.rs`, `ImageTable`) marks a handle ready once the host answers it and never asks again, so a later `drawImage` of a dropped handle records a draw whose replay finds no image and draws nothing.

## Where

`host/apple/Sources/ExactSurfaces/Canvas2DImage.swift`, `images`, `load` and `forget`. `canvas/src/context/image.rs`, `Images` and `ImageTable`. `runner/src/runner/canvas2d.rs`, `canvas_image`.

## How to reproduce

From a canvas, `drawImage` many distinct remote or asset images, then remove the canvas. `images` still holds every decoded `CGImage`.

## Direction

Give the image table a way to forget a handle (it is requested again at its next use), and have the host release a handle past a byte budget, least recently drawn first, telling the runner as it does.
