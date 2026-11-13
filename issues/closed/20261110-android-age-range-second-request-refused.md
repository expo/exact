# On Android, a second `requestAgeRange` while one is showing fails where the design and Apple make it wait

**Status:** Closed
**Resolution:** The Android module keeps each request's reply in a queue and asks Play for the next when the one in front answers, as the Apple module does, so a second request waits instead of failing.
**Systems:** packages/age-range/android/AgeRange.kt
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

LLP 1133.028 D10 gives one rule for two requests at once: "the implementation calls the platform for one request at a time, so a second waits for the first and is then asked of the platform, which answers from its cache without a sheet." The runner holds nothing back: the age range has no permission block, so it forwards each admitted request (D10).

The Apple module does that: requests go through a queue, and the next starts when the one before it has replied (`packages/age-range/apple/AgeRange.swift:49`, `70` to `85`, at `dd6f5dcd`).

The Android module refuses the second:

```kotlin
// One request at a time: Play's prompt is one activity.
if (asking) return reply(JSONObject().put("error", "InvalidStateError").put("message", "a request is already showing"))
```

(`packages/age-range/android/AgeRange.kt:62` to `63`). `asking` is set when a request starts (line 92) and cleared only when it answers (line 93).

## The failure

Two parts of one app each gate on the range and ask as they mount:

```ts
const [a, b] = await Promise.all([requestAgeRange({ ageGates: [13, 16] }), requestAgeRange({ ageGates: [18] })]);
```

On Android `a` is the range and `b` is `{ error: "InvalidStateError", message: "a request is already showing" }`. On iOS both are ranges: the second is asked after the first and answered from the platform's cache. Expected on Android: the same.

## Verification

Read, not run: Kotlin does not run on the review machine, and Play's Age Signals answers only a build Play installed. The branch is the two lines quoted, and nothing between the runner and the module queues the call. The device check that settles it: in a Play-installed build, send the two requests above in one turn and read `b`.

## Direction

Keep the reply while `asking` and start its request when the one in front answers, as the Apple module's `queue` and `next` do.

## Related

No issue or QUEUE entry names it.
