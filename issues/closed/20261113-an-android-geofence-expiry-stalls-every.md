# An Android geofence expiry stalls every later crossing

**Status:** Closed
**Resolution:** A delivery pass skips a region whose wake did not complete and goes on to the next pending name, leaving the skipped one pending for the next arrival or launch.
**Systems:** packages/geofencing, host/android
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`deliver` raises one pending region at a time. It calls `deliver` again only when the wake outcome is `completed` or `unheard`. `expired`, `refused`, and `superseded` clear `delivering` and return. The head stays pending.

`pending()` is the whole store, alphabetical by name. The next arrival or launch retries that same head. A second fence that was already stored is not delivered until the head succeeds. Apple continues to the next name.

## Where

`packages/geofencing/android/Geofencing.kt`, `deliver`. `host/android/canvas/src/main/java/dev/exact/canvas/Regions.kt`, `pending`. `host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt`, `wake` outcomes.

## How to reproduce

Arm two geofences. Let the alphabetically first wake expire, with the second already pending. Cross the second fence. Its event stays in the store until the first wake completes or the process launches again and that head succeeds.

## Direction

On an outcome other than completed or unheard, leave that region pending and continue with the next pending name. Do not retry the same head in that pass.
