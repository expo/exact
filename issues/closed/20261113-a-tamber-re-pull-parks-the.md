# A Tamber re-pull parks the light mode when the kept context changes

**Status:** Closed
**Resolution:** A pull remembers a light mode's id under every context it stands for, its twins in `also` included, so a later export that keeps another of them reuses that mode, default and values, with nothing parked or removed.
**Systems:** tools/figma-tamber
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

In the one-collection layout, one mode stands for every light context that differs only by an axis under `scheme=dark`. Its id is remembered only under the context that was kept. Twins live on `also` and are omitted from `contextIds`. A later export that keeps a different context for that same slot still has the same context strings. The old mode id does not match the new kept context.

If that mode is the collection default it is renamed `unused:…` and left as the default. Otherwise it is removed. A new `light` mode is created. `contextsMoved` treats the kept context and its twins as one set, so the UI does not ask. Bindings follow mode id. After the pull the default is the stranded unused mode, still holding the previous light values, or the old light mode is gone. The new mode has the design's values and is not the default.

## Where

`tools/figma-tamber/src/mapping.ts`, `pull`, `lightTwins`, and `contextsMoved`. `tools/figma-tamber/src/code.ts`, the reuse of `prior[kept context]`, the park of an unused default, and the `contextIds` write.

## How to reproduce

Pull a design whose light contexts share a mode, then export again with a different light context first in that slot and pull again. The previous light mode is parked or deleted, a new `light` mode appears, and the collection default is not that new mode. No token value had to change.

## Direction

Remember the light mode under every context it stands for, the kept context and `also`. On pull, reuse that mode when any of them is the new kept context, and do not park or delete it.
