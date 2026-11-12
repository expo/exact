# Linux native content keeps the border box corner radii

**Status:** Closed
**Resolution:** A video, web view, native view or GPU canvas is now shown in its content box on the content edge's curve, each radius less the border and padding beside it, as a picture and a 2D canvas already were.
**Systems:** host/linux/src/paint/native.rs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`host/linux/src/paint/native.rs:108–111` constructs a native element's clip from the content rectangle and `outer.radii`. It does not subtract the adjacent border and padding from the corner radii.

## The failure

A 60×60 border-box video with a 6-point border and radius 8 has content rectangle `(6,6,48,48)`. The native backend receives radius 8 for every corner. Its content-edge radius should be 2, so the current clip removes too much video near the corners. A surface canvas takes the same path.

## Verification

Ran `CARGO_BUILD_JOBS=5 cargo test -p exact-linux --test fix08_paint fix08_ -- --nocapture`. The test compiled that video, booted the portable presenter, and painted through a backend recording the actual `native` clip. It printed `rect: (6.0, 6.0, 48.0, 48.0), radii: [(8.0, 8.0), (8.0, 8.0), (8.0, 8.0), (8.0, 8.0)]`. The assertion for four `(2,2)` radii failed. No Linux-only backend was run. The temporary test was removed.

## Direction

Derive the content-edge radii from the reduced border-box radii and the content insets before passing the shape to the native backend.

## Related

[Bordered rounded picture covers its border](20261110-bordered-rounded-picture-covers-its-border.md) concerns image and Canvas 2D painting. This is the separate native video/surface clip. Reported in the f-bordered-rounded-picture review.
