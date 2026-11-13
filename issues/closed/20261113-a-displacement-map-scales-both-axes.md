# A displacement map scales both axes by the object width

**Status:** Closed
**Resolution:** Not a defect: Chrome, the drawing oracle, scales both axes by the box width (Blink's FEDisplacementMap::CreateImageFilter passes only ApplyHorizontalScale(scale_), under a FIXME), which is what the kernel does; a comment there now says so.
**Systems:** kernel, host/svg-raster
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

When `primitiveUnits` is `objectBoundingBox`, `kx` and `ky` are the box width and height. Blur and offset scale each axis. `feDisplacementMap` stores `scale * kx` only. The rasterizer multiplies that one value by each pixel scale.

On a non-square object the map shifts y by the width fraction, not the height. The default `userSpaceOnUse` path, where `kx` and `ky` are both 1, is unaffected.

## Where

`kernel/src/svg/scene/filter.rs`, the `kx`/`ky` pair and the `feDisplacementMap` arm. `host/svg-raster/src/filter.rs`, the displacement sample.

## How to reproduce

Filter a wide shape with `primitiveUnits="objectBoundingBox"` and `feDisplacementMap` at a non-zero scale. The vertical shift matches the width fraction. A square shape of the same scale matches the spec.

## Direction

Store separate x and y scales, `scale * kx` and `scale * ky`, and sample with both.
