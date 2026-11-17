# An Android limited-library change does not revoke pictures

**Status:** Closed
**Resolution:** When the selection dialog answers, every drawn picture whose photo no longer opens is revoked, by the same check a resume runs.
**Systems:** packages/photos
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`resumed` withdraws pictures whose ids no longer open, and only when access is not `full`. `managePhotoSelection` asks for the read permissions again and then announces `photoLibraryStatus` and `photoAssets`. It does not revoke.

The selection dialog can return while the activity stayed resumed, so `resumed` does not run. Thumbnails already delivered for photos the person removed from the limited library stay on screen, and later `picture` reads can still be attempted for them.

## Where

`packages/photos/android/Photos.kt`, `resumed`, `manage`, and `picture`. `revokePicture` is `host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt`.

## How to reproduce

Grant limited photo access, show a picture, then change the selection through `managePhotoSelection` so that photo is no longer included, without the activity stopping. The picture stays. A later resume is what withdraws it.

## Direction

After the selection dialog answers, run the same `opens` check `resumed` uses and revoke every drawn id that no longer opens.
