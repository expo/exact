# A live module swap clears the web Canvas 2D roster

**Status:** Closed
**Resolution:** The web module's replacement carries the Canvas 2D roster, so Placed snapshots it unchanged across a swap.
**Systems:** js, data
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`replacement` builds a module with `Self::new`, which sets `canvas_surfaces` to empty, then copies only `id` and `placement`. The bake receipt's surface list is not read. `Placed::replacement` calls `with_spawn`, which copies `canvas_surfaces()` from that new module over the roster captured at construction.

A canvas whose name is in the roster is 2D. Every other surface update goes to the GPU side. After a dev-loop or update swap, those canvases are no longer 2D.

## Where

`js/web/src/lib.rs`, `Module::new`, `replacement`, and `canvas_surfaces`. `data/src/placed.rs`, `with_spawn` and `Placed::replacement`.

## How to reproduce

Open a page whose Rust module names a Canvas 2D surface, then swap the module. The next frame of that canvas is sent to the GPU path. The 2D roster is empty.

## Direction

Copy `canvas_surfaces` onto the replacement before `Placed` snapshots it.
