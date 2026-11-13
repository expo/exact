# Remote canvas images are kept with no bound

**Status:** Closed
**Resolution:** A remote canvas image is fetched as an img's is, spooled to a file and stopped past its 64 MB byte limit, and an image over the img pixel limit is refused before it is decoded; releasing decoded images needs the runner to forget a handle and is issues/20261113-decoded-canvas-images-are-never-released.md.
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`load` accepts an `http` or `https` canvas image and reads the whole `URLSession` body before decoding. The request has a 20 s timeout and no byte cap. The decoded `CGImage` is stored in `images` for the process lifetime. `forget` drops the canvas's replayer, layer, and GPU canvas. It does not remove the image.

`drawImage` of a large URL, or of many distinct URLs, retains every decoded bitmap. One oversized body is allocated in full before the thumbnail options run.

## Where

`host/apple/Sources/ExactSurfaces/Canvas2DImage.swift`, `load` and `forget`.

## How to reproduce

From a canvas, `drawImage` a large remote URL, then another, and remove the canvas. The decoded images stay in `images`. A body larger than the process can hold is downloaded to completion before decode refuses it.

## Direction

Stop the task past a byte budget, refuse an image over the pixel budget, and drop decoded handles when the canvas that asked is forgotten or the cache exceeds its budget.
