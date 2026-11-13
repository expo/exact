# An exit tape drops box-shadows and 3D planes the recorder claimed

**Status:** Open
**Systems:** host/linux
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

An exit tape is recorded by a backend that reports every blurred shadow and every projective plane as drawn. The live painter stores a shadow op, or a plane, instead of the band fills and the software warp it uses when a backend declines.

Replay calls `blurred_shadow` and `plane_begin` and ignores a false result. CPU and GPU decline those draws. On the way out of a push or a sheet, the sliding route loses every box-shadow, and a 3D subtree is drawn in the plane's space without the warp.

## Where

`host/linux/src/paint/tape.rs`, `Recorder::blurred_shadow`, `Recorder::plane_begin`, and `Tape::replay`. `host/linux/src/paint.rs`, the live shadow fallback when `blurred_shadow` returns false.

## How to reproduce

Push a route whose boxes have a blurred box-shadow, or whose subtree is a 3D plane. Watch the exit transition on CPU or GPU. The shadow is missing, and the plane is not warped with the slide.

## Direction

Make the recorder report that it did not draw the shadow or the plane, so the live fallback is what gets stored. Or, on replay, use that fallback when the target backend declines.
