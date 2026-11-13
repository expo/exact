# A crop drag does not count as an active photo gesture

**Status:** Open
**Systems:** apps/photo-editor
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A crop-handle drag never counts as an active gesture. On the Mac, a pan that starts on a handle returns before `track`, so `active` stays 0. On iOS, `dragHandle` never inserts its recognizer into `active`. `settle` runs whenever that set is empty. It reports the resting edit at once and animates the crop, the scale, and the offset.

A scroll-wheel zoom on the Mac arms a settle timer for 0.25 seconds and does not mark a gesture active. A pinch or rotation that ends does the same. Either one can run while the handle is still down. The animation and the drag both write `edit`, and the app hears the resting crop before the finger lets go.

## Where

`apps/photo-editor/modules/apple/PhotoEditorMac.swift`, `panned`, `track`, `settle`, and `scrollWheel`. `apps/photo-editor/modules/apple/PhotoEditorIOS.swift`, `dragHandle` and `settle`. `apps/photo-editor/modules/apple/PhotoEditor.swift`, `EditMotion`, which interpolates the crop.

## How to reproduce

On the Mac, zoom with the scroll wheel and, within a quarter second, drag a crop edge. The rectangle eases toward a resting crop while the pointer is still moving it. On either Apple host, pinch and drag a crop handle, then let the pinch go first. The same ease starts under the handle.

## Direction

Count a crop drag in `active` from began through ended, the way a move does, and let `settle` wait until that drag has ended.
