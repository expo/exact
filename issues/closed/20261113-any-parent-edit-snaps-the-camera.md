# Any parent edit snaps the camera, the lights, and every socket

**Status:** Closed
**Resolution:** The scene and attachment feeds ask ParentChanges whether an entity's own chain changed (snap_in), so a parent edit elsewhere no longer snaps the camera, the lights, the pointer's children or the sockets.
**Systems:** game
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`snap` returns true for every entity when `parent_changed` is set. That flag is `next.parent != old.parent`, a world-wide revision. `Scene::feed` passes it into the camera, the sun, the fill, every selected light, and the pointer. `Owner::update` passes it into every link. Spawning or editing any `Parent` therefore cancels interpolation for the camera, the lights, and every socket in the same tick. A viewmodel whose own parent did not change still blends from the transform buffer, so it separates from the view for that frame. The mesh path asks `ParentChanges` for the entity.

## Where

`game/render/src/world/scene.rs`, `snap`, `History::update`, `Scene::feed`, `Owner::update`. `game/render/src/world.rs`, the `parent_changed` flag and the mesh override loop.

## How to reproduce

Move the camera or a light in the same tick as any other entity's parent edit. The camera and the lights sit on the tick-end pose for that frame, and a parented viewmodel does not.

## Direction

Snap an entity only when its chain changed or it is fresh, as the mesh override path already does.
