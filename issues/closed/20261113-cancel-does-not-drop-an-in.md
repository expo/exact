# Cancel does not drop an in-flight takePhoto

**Status:** Closed
**Resolution:** Cancel, and back from the camera, assign `shot = none` so a photo still being taken is dropped, and `taken` uses a photo only while the camera route is on top.
**Systems:** apps/photo-editor
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

`shutter` sends `takePhoto` on `shot`, and `taken` runs when that send settles. `cancel` stops `track` and pops the camera route. It does not clear `shot`. `taken` still replaces `photo`, resets the edit, and calls `back`, which pops whatever route is now on top.

## Where

`apps/photo-editor/app.contract`, `shutter`, `taken`, and `cancel`. Leaving the print sheet already assigns `ordered = none` so its payment cannot land. The camera send does not.

## How to reproduce

Open the camera, press the shutter, then Cancel before the capture returns. When the photo arrives, it replaces the picture being edited and pops the route opened after Cancel.

## Direction

On cancel, set `shot` to none before popping. Make `taken` a no-op unless the camera route is still on top.
