# An Android location watch hides a disabled provider

**Status:** Closed
**Resolution:** A disabled provider's POSITION_UNAVAILABLE replaces the fix a watch holds, so the re-read answers it; the registration survives, so the next fix after the provider returns answers the watch.
**Systems:** packages/geolocation
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`onProviderDisabled` delivers `POSITION_UNAVAILABLE`. `deliver` keeps `last` when the answer is an error, then announces the watch. After the first fix, `watchers` is empty, so the error is delivered to nobody. The re-read calls `watch`, which returns `last` and never starts a listener. `onProviderEnabled` is empty.

A watch that already has a fix stays on that position after the provider is turned off, and turning it back on does not resume updates.

## Where

`packages/geolocation/android/Geolocation.kt`, `watch`, `deliver`, and `Listener`.

## How to reproduce

Watch position until a fix arrives. Turn the location provider off. The next answer is still the old position, with no `POSITION_UNAVAILABLE`. Turn the provider on. Updates do not resume.

## Direction

On a disabled provider, clear `last` and deliver the error to the watch. When the provider is enabled again, request updates if a watch is still held.
