# Caltrain's Live Activity counts down to a fixed instant in August 2026, so on a phone it reads 0:00

**Status:** Closed
**Resolution:** Caltrain reads `exactTime()` and adds `epochAtZero - seededEpoch` to the departure's `at`, and states the alert's `updatedAt` as the real time of the last tick, so the system draws both on the real clock.
**Systems:** apps/caltrain (app.contract, activities.contract)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Caltrain's schedule is seeded on one day: `seededEpoch = 1787915400000` (2026-08-28 11:10 UTC), and the app's own clock is `seededEpoch + performanceNow()` (`apps/caltrain/app.contract:73,139` at `dd6f5dcd`). Inside the app that is consistent. The Live Activity is drawn by the system on the real clock, and the state the app starts it with carries a seeded instant (`app.contract:104`):

```
derive departureState = DepartureState(…, at=seededEpoch + 600000 + trainDelay, …)
```

`apps/caltrain/activities.contract:29,33,42` draws it as `time datetime=at format="countdown"`. The service alert does the same with `updatedAt=seededEpoch` and `format="relative"` (`app.contract:120,123`, `activities.contract:64,75`).

The app's two other surfaces on the real clock both translate the seeded day. The watch entry reads `exactTime()` and adds `clock.epochAtZero - seededEpoch` to each instant, "so a `time` node can count down to a real instant" (`apps/caltrain/watch.contract:21-23,37,69-70,98`). The widget's refresh repeats the seeded day on the day asked (`apps/caltrain/data/src/widget.rs:1-2,21`). The phone's activity has neither.

## The failure

On a phone, any day after 2026-08-28, tap "Follow train" (`app.contract:321-324`, shown where the host links activities).

- Observed, by reading: the activity's countdown targets 2026-08-28 11:20 UTC. A countdown to a past instant is the empty interval (`host/apple/Sources/ExactSnapshot/SnapshotLeaves.swift:127`, `.countdown(min(entryDate, date)...date)`, drawn at `:169` as `Text(timerInterval:countsDown:)`), so the lock screen and the island read 0:00. "Delay train" moves the target ten minutes and it still reads 0:00. The alert's time reads as months ago.
- Expected: ten minutes, counting down, as the app's board shows on its seeded clock.

## Verification

By reading; not run. The iOS simulator cannot run on the review machine, and a Live Activity's clock is the system's, which the agent's virtual clock does not drive. The arithmetic: 1787915400000 + 600000 ms is 2026-08-28T11:20:00Z. `apps/caltrain/tests/app.rs:409` asserts the seeded instant in the update's payload (`"at":1787916600000`), so the tests hold the behaviour in place.

## Direction

As `watch.contract` does: read `exactTime()` in the phone app and add `epochAtZero - seededEpoch` to `at` and `updatedAt` when building the activity states. The payload test changes with it.

## Related

None found in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md`.
