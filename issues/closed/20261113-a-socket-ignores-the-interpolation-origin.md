# A socket ignores the interpolation origin of teleport_from

**Status:** Closed
**Resolution:** An owner chain's snapped link takes its previous local pose from the `teleport_from` landing, as the transform buffer does, so a socketed prop and an owner override blend from the landing with the mesh.
**Systems:** game
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`History::update` writes `interpolation_origin` into the previous pose after a snap. `Owner::update` sets each link's previous local pose to the current local and never reads that origin. The socket matrix is the owner chain. `teleport_from` is a landing and motion in one tick. A root rig's mesh still blends from the transform buffer, which stores the origin, while the socket is already at the destination. A parented rig emits an owner override, so the skinned mesh pops while a sibling that is not skinned still blends.

## Where

`game/render/src/world/scene.rs`, `History::update` and `Owner::update`. The transform buffer stores the origin in `game/render/src/world.rs`.

## How to reproduce

`teleport_from` a rig that has a socketed prop. The prop is at the destination on the first frame and the mesh is still blending, or the whole skinned mesh pops.

## Direction

On a fresh link, set the previous local from `interpolation_origin` before composing the socket.
