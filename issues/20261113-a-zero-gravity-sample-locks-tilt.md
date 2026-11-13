# A zero gravity sample locks tilt and a save of it will not restore

**Status:** Open
**Systems:** game
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`apply_sample` stores the first Gravity triple in `first_gravity`, including a finite `[0, 0, 0]`. `gravity` returns none for a zero vector, so `tilt_neutral` stays empty and `tilt` stays at zero. A later sample does not replace a slot that is already `Some`. The stick stays at zero until the sensor ends, which is the only path that clears the slot. `validate` rejects that same zero, so a save written in this state fails restore.

## Where

`game/engine/src/input/sensor.rs`, `apply_sample`, `gravity`, `tilt_neutral`, `validate`.

## How to reproduce

Deliver one finite zero Gravity sample, then a real one. Tilt stays zero. Save, and the next restore is refused.

## Direction

Latch `first_gravity` only when `gravity` returns a vector. Leave the slot empty until a finite nonzero sample arrives.
