# Android device orientation asin is not clamped

**Status:** Open
**Systems:** packages/motion
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`orientation` passes rotation-matrix element `m[7]` straight to `asin`. Near ±90° that element rounds outside [-1, 1]. `asin` returns NaN. `JSONObject.put` rejects a non-finite double. The throw is on the sensor callback, which does not catch it.

Apple clamps that element to [-1, 1] before `asin`. Holding the phone upright is the pose where beta is ±90°.

## Where

`packages/motion/android/Motion.kt`, `orientation`. `packages/motion/apple/Motion.swift`, `orientation`.

## How to reproduce

Read `deviceOrientation` and hold the phone upright so the rotation-vector matrix element rounds past 1 or below -1. The sensor callback throws and the sample is lost. The process can die on that thread.

## Direction

Clamp `m[7]` to [-1, 1] before `asin`, as `Motion.swift` does.
