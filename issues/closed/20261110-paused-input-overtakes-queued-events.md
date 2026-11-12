# Input that arrives during a pause is applied before events still queued from before it

**Status:** Closed
**Resolution:** A device event that arrives while the game is paused first applies the device events still queued from before the pause, so it no longer overtakes them.
**Systems:** game/engine/src/sim/queue.rs, game/engine/src/sim.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

While a game is paused, `Sim::input` applies a device event to held state at once
(`game/engine/src/sim/queue.rs:13-17`) without looking at the queue. An event queued before
the pause with a host stamp later than the last frame is still there: the pause's flush
delivers only events that are already stamped or not later than the last host sample
(`flush_paused`, `queue.rs:160-166`, called from `bind_with` at `sim.rs:520-521`). The next
paused frame delivers it, after the events that arrived later. `game/engine/README.md`
promises stamp order for the queue (`:136-137`) and says paused input updates held state
directly (`:149-150`); the direct path overtaking the queue breaks the first.

So a key that goes down just before a pause and lifts before the next frame is applied as
up (no change) and then down, and stays held until it is pressed and released again.

The web host can produce this outside the agent: it stamps input with `event.timeStamp`,
which is later than the last frame's time, and binds without a stamp
(`host/web/gpu-glue.js:579`, `:975-978`). The first event, the pause and the later event
must all come before the next frame runs: within one frame interval, or longer when a frame
is late. The Apple host does not reach it: it advances canvases at the display link's target
time (`host/apple/Sources/ExactKit/Frames.swift:58`), so an event's stamp is not later than
the last frame's and the event is stamped on arrival.

## The failure

```rust
// Actions::new().button("fire", &["KeyF"]); the tick counts input.held("fire");
// Game::paused reads a `#[live] paused` argument.
sim.frame_period(1000. / 60.);
sim.advance(0., Clock::Live);
sim.advance(16.7, Clock::Live);
sim.input(key("KeyF", true, 19.7));            // 3 ms after the frame
sim.bind(&[Value::Bool(true)], None).unwrap(); // the app pauses
sim.input(key("KeyF", false, 25.));            // released before the next frame
sim.advance(33.4, Clock::Live);
sim.advance(50.1, Clock::Live);
sim.bind(&[Value::Bool(false)], None).unwrap(); // resume
for frame in 4..64 { sim.advance(frame as f64 * 16.7, Clock::Live); }
```

Observed: all 60 ticks after the resume read `fire` held, and `state` reports
`"held":["KeyF"],"forwarded":["KeyF"]`. Expected: none; the key is up.

## Verification

Ran the snippet as a temporary integration test of `exact-game`: the count was 60 and
`state` printed the line above. Three variants each gave 0: a paused frame between the bind
and the release; the key-down stamped 2 ms before the last frame (Apple's order); the pause
bind stamped after the key-down. Not driven through a browser; the glue's stamps were read.

## Direction

Keep arrival order while paused: before applying a paused device event, deliver the device
events still queued (as `flush_paused` does, without its stamp test), or have the pause's
own flush deliver every queued device event whatever its stamp.

## Related

`docs/game-pitfalls.md` has a held message during a pause (by design) and same-tick taps;
neither is this.
