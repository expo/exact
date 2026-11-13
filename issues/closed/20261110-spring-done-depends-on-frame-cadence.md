# A spring's value at a clock time depends on which earlier times were sampled

**Status:** Closed
**Resolution:** A running spring keeps its settle time from its start and is done once that much time has elapsed, so a seek, any frame rate and the lowered frames end it at one instant with the same bits.
**Systems:** motion/src/transition.rs, motion/src/engine.rs, motion/src/spring.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`Engine::advance` promises a seek: "the result depends on `now`, never on how many calls it
took to get there" (`motion/src/engine.rs:661` at `dd6f5dcd`; the crate doc says the same,
`motion/src/lib.rs:33`). LLP 1003 §7 repeats it: "the value at `t` does not depend on the path
to `t`". For an underdamped spring it does not hold.

`Running::sample` calls a spring done when every component is at rest at the sampled instant
(`motion/src/transition.rs:382`), and `advance` then drops the curve and presents the target
(`engine.rs:676`). Rest is `|x| < 1e-3 ∧ |v| < 1e-3` (`motion/src/spring.rs:76`). An
underdamped spring enters and leaves that state many times: near each turning point the speed
is small, and a quarter cycle later it is not. So the first sampled instant that falls in a
rest window ends the spring, and which instant that is depends on the frame times.

LLP 1003 §4 names one end: "`settle_time` is the first at-rest sample on the 240 Hz grid,
capped at 10 s, after which the engine snaps to the target". `settle_time` (`spring.rs:251`)
and the web's lowered frames (`transition.rs:406`) follow that rule; sampling does not.

## The failure

```rust
// opacity 0 -> 1 under `transition: opacity -exact-spring(100, 10, 1)`, the default spring
a.advance(k / 60.0) for k = 1, 2, …   // done at 1.4500: presents 1, quiescent
a.advance(1.4667)                     // 1
b.advance(1.4667)                     // one seek: 0.9993016848890162, not quiescent
```

Expected: the same bits from both engines at 1.4667, and both done, since `settle_time()` is
1.4417. Frames at 120 or 240 Hz end the spring at 1.4417, at 50 Hz at 1.4400, at 60 Hz at
1.4500 and at 30 Hz at 1.7667.

## Verification

A temporary test of `exact-motion` printed the lines above. From the first rest at 1.4417 to
the last instant not at rest, 1.7458, 6 of 74 instants on the 240 Hz grid are at rest. With
`-exact-spring(100, 2, 1)` the window runs from 6.9375 to 9.0542. A critically damped spring
(damping 20) stays at rest once it is there and shows no difference. The gap is below the
rest threshold, so nothing visible moves; what
changes with the cadence is when the curve ends, when `quiescent()` turns true, and the bits a
test or the agent's `clock` reads.

## Direction

Decide `done` from time alone: a spring is done once `elapsed` reaches the settle time
`end_time` already computes, kept on the curve when it starts. Then a seek, sixty frames and
the lowered frames all end at one instant.

## Related

`issues/closed/20260901-clock-seek-unbounded-work.md` is about the cost of a large seek, not
its result. `motion/tests/it/engine.rs:95` (`the_clock_is_a_seek`) tests an easing only.
