# A paranoid rebuild writes origin marks back into the world's messages

**Status:** Closed
**Resolution:** The paranoid rebuild keeps the pending messages as the world holds them, so only the host-facing take_messages marks them, once.
**Systems:** game
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`paranoid_rebuild` saves, then calls `take_messages`. That prefixes every pending emit with the world's origin mark, then restores the save and assigns the marked vector back onto `world.messages`. The next `take_messages` marks again. `World::hash` does not cover messages, so the hash assert still passes. With an origin other than `app` and any pending emit, the canvas receives the origin mark inside the text, and a later save persists that string.

## Where

`game/engine/src/sim/paranoid.rs`, `paranoid_rebuild`. `game/engine/src/sim.rs`, `take_messages`. `game/engine/src/sim/origin.rs`, `mark`.

## How to reproduce

Run a world whose origin is `tool`, `wake`, or `device`, with paranoid rebuild on and a pending emit. The message text that leaves the sim contains the origin mark.

## Direction

Keep the outbox with `mem::take` on `world.messages` and do not mark it. Mark only in the host-facing `take_messages` after the rebuild has restored the unmarked strings.
