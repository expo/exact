# Android geocoding reports a bad argument as unsupported

**Status:** Open
**Systems:** packages/geocoding, host/android
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

Below API 33, `getFromLocation` and `getFromLocationName` run on a worker. An `IllegalArgumentException` for a latitude, longitude, or `maxResults` outside the platform's range is answered as `TypeError`.

On API 33 the same call uses the listener form. That throw leaves `later` and the host's call catch answers `NotSupportedError`. A bad argument and a missing geocoder then look the same.

## Where

`packages/geocoding/android/Geocoding.kt`, `lookup` and `Later`. `host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt`, the call catch that replies `NotSupportedError`.

## How to reproduce

On API 33 or newer, reverse-geocode a latitude of 200 or pass `maxResults` of 0. The answer is `NotSupportedError`. The same call on API 32 is `TypeError`.

## Direction

Catch `IllegalArgumentException` around the API 33 calls and answer `TypeError`, as the worker does.
