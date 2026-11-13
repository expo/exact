# A failed canvas shadow leaves its texture registered on the shared renderer

**Status:** Open
**Systems:** canvas vello GPU replay
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`Canvas::replay` blurs each shadow, then `override_image` registers that texture on the process-wide vello renderer, keyed by the image blob. The success path clears every override and destroys the textures. If a later `shadow()` returns `Err`, the function returns `Failed::Gpu` first.

Textures already registered stay in `image_overrides`. The renderer is created once per process and never torn down, so the entries and their textures remain for the life of the process. A later frame clears only the shadows of that frame. wgpu's `ErrorScopeGuard` does pop the validation scope on drop, so the scope itself is not what leaks.

## Where

`canvas/vello/src/gpu.rs` `replay`, the `Err(e) => return Err(Failed::Gpu(e))` arm inside the shadow loop. The clear and `destroy` run after the loop. `vendor/vello/src/lib.rs` `override_image` inserts into `engine.image_overrides`.

## How to reproduce

A frame with two shadow jobs where the second `shadow()` fails (device error or an allocation failure) returns `Failed::Gpu` with the first job's texture still in `image_overrides`.

## Direction

On that error, clear every override this replay installed, destroy the textures already made, and then return `Failed::Gpu`.
