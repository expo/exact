# A reloaded Android canvas stays marked presented

**Status:** Closed
**Resolution:** A replaced host drops each Android window's buffer frame and marks it preparing, and the reader, finding a presented window preparing again, covers it with the retained image and observes its status until the new canvas presents.
**Systems:** host/linux, host/android
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A reload or an activated update destroys every canvas and numbers views from 1 again. The Android windows stay, by view, to be attached to the new canvases. `windows_replaced` only sets `attached` false. `PRESENTED` stays at revealed, submitted, or refused, and `FRAMES` keeps the destroyed ring's buffer.

A successful reattach does not write preparing. The reader has already latched ready on status 1 and stopped observing, so it does not put the retained image back, and a `SurfaceView` is cleared so the surface shows through before the new canvas has presented. A later refusal is status -1, which that reader no longer reads. A buffered view skips a new frame whose words still match the retained ones, and keeps drawing the old buffer.

## Where

`host/linux/src/surfaces/retire.rs`, `replaced`. `host/linux/src/surfaces/android.rs`, `windows_replaced`, `attach_windows`, `publish_buffers`, and the flush that records submitted only from preparing. `host/android/canvas/src/main/java/dev/exact/canvas/CanvasPresentation.kt`, `observe`. `host/android/canvas/src/main/java/dev/exact/canvas/Natives.kt`, the poll that stops once ready, and the record that clears a ready `SurfaceView`.

## How to reproduce

Show an Android GPU canvas, then reload the plan or activate an update that keeps the activity. The same view id is attached again. The retained cover does not return. The surface is visible before the new canvas's first frame. If the new canvas refuses the window, the reader does not return to buffers.

## Direction

In `windows_replaced`, drop each view's buffer frame and set `PRESENTED` to preparing. Leave that view there until the new canvas's own `gpu_seen` or first submit. Reset the reader's ready latch for those views when the host is replaced.
