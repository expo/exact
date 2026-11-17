# A retry with a growing wait has no direct form, and the refusal points to a task that wakes every frame

**Status:** Closed
**Resolution:** The literal-timer refusal recommends gated fixed-delay backoff steps; the guide explains their wake behavior.
**Systems:** contract/lower/src/timers.rs (`lower-timer-literal`), rules/DEFERRED.md (gated timers), docs/contract-for-agents.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261027-timers-cannot-wait-for-a-frame.md

## What is wrong

`after(ms, action)` and `every(ms, action)` take only a literal number of
milliseconds. `Lowerer::timers` in `contract/lower/src/timers.rs` refuses
anything else:

```
[lower-timer-literal] `after` needs a literal number of milliseconds (`after(1000, …)`). For a computed wait, gate an `every(frame, check)` task on waiting and have `check` compare `performanceNow()` with the wait; one that follows the tempo follows the transport (docs/contract-for-agents.md, "Following the transport")
```

The literal is deliberate: `rules/DEFERRED.md`'s gated-timers entry (LLP
1092, LLP 1147) keeps "computed intervals" out. The cost is in the advice.
For a retry that waits longer each time (250 ms, then 500 ms, then 1 s), the
message's form wakes the app every display frame for the whole wait, which is
what LLP 1147's wake work exists to avoid. A form the message does not name
is exact and wakes once per step: one gated task per step of a fixed ladder.
This compiles on main:

```contract
task r1 when waiting and attempt == 1
  after(250, again)
task r2 when waiting and attempt == 2
  after(500, again)
task r3 when waiting and attempt >= 3
  after(1000, again)
```

and `task retry when waiting key=attempt` with `after(delay, again)` is
refused as above.

## The failure

An app that follows a server stream wanted to follow it again after a drop,
waiting 250 ms, 500 ms, then 1 s (exact2 `9e53578f2`). It found only the
`every(frame, …)` form the message names. A port of an existing mobile app
(diary on `94b8671f7`) lists "`after()` needs a literal delay" among its
language gaps.

## Fix

Within the current rule: make the refusal name the stepped ladder for a
bounded backoff, and keep the frame task for a wait that must follow a
continuous value (the transport). A short recipe ("retry with backoff") would
carry the example.

Beyond it: a computed wait for `after` (a state value, bounded and checked at
arming) needs a take or waiver on the DEFERRED entry. These reports are its
consumers.
