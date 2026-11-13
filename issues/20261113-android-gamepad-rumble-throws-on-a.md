# Android gamepad rumble throws on a small magnitude

**Status:** Open
**Systems:** packages/gamepad
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`playEffect` vibrates when the magnitude is greater than zero. `amplitude` truncates `m * 255` into 0..255. A positive magnitude below 1/255 becomes amplitude 0. `VibrationEffect.createOneShot` throws `IllegalArgumentException` for amplitude 0. That throw is on the main-thread start runnable.

A dual-rumble with one normal motor and one tiny motor hits it too. Apple's Core Haptics path accepts the same level.

## Where

`packages/gamepad/android/Gamepad.kt`, `play`, `amplitude`, and `vibrate`.

## How to reproduce

Call `playEffect` with `dual-rumble` and `weakMagnitude` of `0.001` on a pad with two vibrators. The start runnable throws and the process dies.

## Direction

When the magnitude is greater than zero, pass an amplitude coerced to 1..255, or skip that vibrator when the truncated amplitude is 0.
