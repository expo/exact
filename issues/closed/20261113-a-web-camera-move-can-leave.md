# A web camera move can leave the browser track capturing

**Status:** Closed
**Resolution:** A camera move takes a new lease number, so an activation's open already in flight is dropped, and `adopt` stops any browser track it replaces.
**Systems:** packages/media-capture
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

Moving a camera calls `getUserMedia`, then `adopt`s the new track. The move does not bump `t.lease`. `adopt` stores the track and does not stop the one it replaces. `stopSource` before that `adopt` only stops a track already stored on `t`.

An `activate` still waiting on its own `getUserMedia` passes `t.lease !== lease` because the move never changed the lease. It then `adopt`s. The track the move stored is left running. The camera indicator stays on after the lease ends.

## Where

`packages/media-capture/web/video.js`, `applyConstraints`, `adopt`, and `activate`.

## How to reproduce

Move a live video track to another camera. While that `getUserMedia` is in flight, subscribe a viewfinder so `activate` opens the previous camera. When both opens finish, one `MediaStreamTrack` is no longer `t.source` and was not stopped.

## Direction

Bump `t.lease` for the move, as `deactivate` does, and stop whatever track `adopt` replaces. Drop an activate whose lease no longer matches.
