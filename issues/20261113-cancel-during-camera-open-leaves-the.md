# Cancel during camera open leaves the photo session running

**Status:** Open
**Systems:** apps/photo-editor
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`takePicture` pushes the camera sheet and sends `getUserMedia` without waiting. `track` is assigned only in `show`, which runs when that send settles. `cancel` calls `stopTrack` on `track` immediately. A cancel during open stops `""`, and the later `show` still stores the stream, so `holding` becomes true and the camera stays on after the sheet is gone.

`flip` writes `facing` in the same turn as `applyConstraints`, including when that call fails and including `NotFoundError` while `track` is still empty. The open uses the facing from the original send. Later flips target the other camera.

## Where

`apps/photo-editor/app.contract`, `takePicture`, `show`, `cancel`, and `flip`.

## How to reproduce

Take a photo and cancel before the stream arrives. The camera indicator stays on. Flip before the stream arrives, or while `applyConstraints` fails: the stored facing and the open camera disagree.

## Direction

Ignore `show` when the sheet is already gone, and stop the track it just received. Change `facing` only after `applyConstraints` succeeds.
