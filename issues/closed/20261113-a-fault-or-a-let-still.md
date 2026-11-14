# A fault or a let still spends Jev's return

**Status:** Closed
**Resolution:** When a point, a fault or a let leaves the ball dead, the tick clears both players' planned contact, so Jev neither commits nor swings at a return the point already ended.
**Systems:** game/tennis
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A serve that lands in the court but outside the service box, or a let, enters `Dead` and leaves the far player's `contact` set. `far` commits the return when the tick reaches that contact, with no phase check. `brain::commit` reveals the shot and counts it on time or late. The hit itself then requires `Rally`, so the swing is not played. The HUD and the on-time and late totals describe a return the point already ended. The next toss asks again, so the match continues.

## Where

`game/games/tennis/logic/src/lib.rs`, `far`, `fault`, and the serve bounce. `game/games/tennis/logic/src/brain.rs`, `commit`. `enter` only stores the phase.

## How to reproduce

Serve a kick that lands in the court and outside the service box, with the planned contact after the bounce and inside the fault pause. The return is counted and not hit.

## Direction

On entering `Dead` or `Over`, clear both players' `contact`. Call `commit` only in `Rally`, or in `Toss` for the server's own serve.
