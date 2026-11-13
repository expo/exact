# Web gpu_unload drops surfaces while a frame still holds their drawables

**Status:** Open
**Systems:** gpu
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

Web `unload` clears `instances`, then destroys the device. `Module.open` can still hold those canvases' `SurfaceTexture`s. A render that failed after recording parks the drawable in that open frame. `Drop` for `Module` clears `open` before releasing surfaces, which is what native `unload` does by dropping the module. The web export clears the instances first, so the drawable's surface is already gone when the open frame is dropped. wgpu panics in `surface_texture_discard` in that order.

## Where

`gpu/src/web.rs`, `unload`. `gpu/src/release.rs`, `Drop` for `Module`. `gpu/src/frame.rs`, the open frame's textures. `host/web/gpu-glue.js` calls `gpu_unload` on the previous module after a replace, and on the new module when a render fails before `gpu_flush`.

## How to reproduce

`gpu_render` a canvas, then `gpu_unload` without `gpu_flush`, while the frame still holds a drawable. The process panics in the surface discard.

## Direction

Drop the module the way native unload does, or clear `open` before clearing instances and destroying the device.
