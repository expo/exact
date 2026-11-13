# Forward seeks to the start while the video's duration is still zero

**Status:** Open
**Systems:** apps/video-player
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`duration` starts at 0. `forward` sets `target` to `min(duration, position + 10)`. If Forward is pressed after `timeupdate` has moved `position` and before `durationchange`, that minimum is 0. `settle` then seeks to 0, or to 0.001 when `seekTo` is already 0. The playhead jumps to the start. Unknown length is stored as 0, the same value as an empty clip, so the clamp cannot tell them apart. The Forward button is enabled the whole time.

## Where

`apps/video-player/app.contract`, `forward`, `measured`, and the initial `duration`.

## How to reproduce

Press Forward after the first `timeupdate` and before `durationchange`. The video seeks to the start.

## Direction

Ignore Forward until `duration > 0`. Clamp only against a known length.
