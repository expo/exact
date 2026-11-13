# The web placement omits a child's clip planes

**Status:** Closed
**Resolution:** The web placement writes the two clip rows into a 16-float output, and the GPU glue clips the placed DOM child to them with a clip-path polygon.
**Systems:** gpu, host/web
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

Web `placement` writes nine homography floats and a depth into a 10-float buffer and returns. Native `gpu_placement` writes `clip_depth` when the caller passes at least 16 floats. The web host allocates a 10-float array and applies `matrix3d` and `z-index`. It does not read clip rows.

Linux passes 16 floats and clips the software composite with those rows. A placed child with finite clip planes is clipped there and painted in full on the web.

## Where

`gpu/src/web.rs`, `placement`. `gpu/src/native.rs`, `gpu_placement`. `host/web/gpu-glue.js`, `placeChildren`.

## How to reproduce

Place a canvas child with finite near and far clip planes. On Linux the composite is clipped to those planes. On the web the child is transformed and drawn in full.

## Direction

When the output has room for 16 floats, write `clip_depth` as the native placement does, and clip the DOM child to those rows.
