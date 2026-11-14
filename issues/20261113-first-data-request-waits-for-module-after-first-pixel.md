# On iOS a cold launch sends no data request until about 0.4 s after its first frame

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/SessionActivation.swift, host/apple/src/activation.rs, host/executor/src/lib.rs
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-independent-http-workers-each-open-their-own-connections.md

## What is wrong

A launch's first request leaves only after a chain of steps that all start after first pixel, one after another (LLP 1027 D4: app logic loads after first pixel):

1. `firstDrawn` (`SessionActivation.swift:9`) posts to the main queue, which declares the app's notification categories (`declareNotificationCategories`, `NotifyActions.swift:430`) and, when they are declared, runs `natives.prepareAppModule()` there (`:24`).
2. `prepareData` (`:43`) posts to the owner thread, where the TypeScript module evaluates its top level (`Host::prepare_data`, `host/apple/src/activation.rs:20`). The comment above `prepareData` measures this at 325 ms for a 2 MB catalog on an iPhone 17 Pro Max.
3. Back on the main queue, `activateData` (`:60`) calls `runtime.dataReady()`, which asks the resources.
4. The first independent request starts the lane's workers, each of which makes its transport on its own thread (`Core::start_workers`, `host/executor/src/lib.rs:451`; `worker`, `:1050`).

None of these overlaps the first frame, and none overlaps the next.

## The failure

The port of a large React Native app (iOS device, exact2 `94b8671f`) saw the first frame, drawn from kept answers, at about 0.4 s after process start, and no data request leave until about 0.8 s. Cold start to a loaded first screen was still faster than the React Native app's (2.4 s against 3.7 s), so the 0.4 s matters most on a fast network.

Only the code path was checked; the timings are the reporter's, and how the 0.4 s divides among the steps above was not measured. `ExactLaunch` already records `modulePrepared` and `dataStep`, which would divide it.

## Measured

Weatherlight (a 217-line TypeScript module, a GPU canvas that loads after first pixel) on an iPhone 16 Pro Max, iOS 27.0.1, main at `84f51d7f` with a stderr line at each step, cold launches by `devicectl`. Fifteen of a development build and ten of a release build.

| After the first frame | Release, median (range) | Development, median |
|---|---|---|
| First draw to the block `firstDrawn` posts to the main queue | 37.5 ms (36 to 117) | 33.5 ms |
| Notification categories read and written | 20.8 ms (4 to 100) | 11 to 60 ms |
| `prepareAppModule` | 4.3 ms (2.5 to 18) | 2.6 ms |
| The module's top level on the owner thread | 10.2 ms (8 to 16) | 8.3 ms |
| Back to the main queue | 7.6 ms (3.5 to 114) | 3.6 ms |
| `dataReady` to the first request in the executor | 2.0 ms | 1.6 ms |
| Workers started and their transports made | under 1 ms | 0.2 ms |
| **First request in the executor** | **127 ms (53 to 274)** | **104 ms** |

The first frame was 156 ms after process start at the median (release).

- **The wait is turns of the main queue, not the module.** In the four launches of the 25 whose first request left before the canvas's world started, the whole chain took 53 to 59 ms. In every other launch it took 93 ms or more: the world's start holds the main thread for about 45 ms, and the chain waits behind it at whichever of its three main-queue turns comes next. This is a correlation across launches, not a profile.
- **The module's evaluation is 8 to 16 ms here** and grows with the module; the comment above `prepareData` has 325 ms for a 2 MB one.
- **The notification step is newer than the report.** `declareNotificationCategories` entered the chain on 2026-11-12 (`ac741eca`); the reporter's build (`94b8671f`) ran `prepareAppModule` directly. A launch that declares no notification actions and keeps none no longer asks the notification centre.
- **Starting the workers costs nothing measurable.** The transport's own first use is inside the first exchange and was not measured.

Not measured: the reporter's app, a launch with a preload against the same launch without, and the time from an answer to its commit.

## Fix

The likely moves, in the order the measurement gives them: ask the resources without waiting for turns of the main queue that the first frame's own work is also waiting for; load and evaluate the module on the owner thread while the first frame is being drawn, committing only after first pixel, so step 2 overlaps the paint; and make the independent lane's transport when the module is prepared, not at the first request.

## Owner

LLP 1162.000 owns this: its stage 4 is LLP 1027.008's early evaluation, and its §7 paired comparison on a device is the measurement this issue needs. A design review on 2026-11-12 ruled for evaluating the module's top level before first pixel on a launch thread of its own, never on the main or owner thread. A preload alone does not remove the wait, because its reply is taken only when the module asks. The review recommended shipping only if, on an iPhone Release build, first pixel moves less than a frame at p50 and p90 and fresh content arrives at least 100 ms sooner. That reverses the 2026-10-23 decline, which is the maintainer's call. Close this issue in the commit that lands it, or say here why the measurement declined it.

## Part 1 landed

LLP 1027.008 Part 1 (D1 to D3) is built. For an owner-placed TypeScript module, `firstDrawn` posts the module's evaluation (`prepareData`) to the owner from the draw's own callback, with `preparedGeneration` and `dataPreparing` set first. When the evaluation has ended, the owner posts the rest to the main queue, in its old order and in one turn: the notification categories, `prepareAppModule`, autofocus, kept documents, native views and `activateData`. Step 2 above now overlaps the main thread's first turn, and one turn of the main queue is gone from the chain.

- A Rust source, a module placed on a worker and, on a Mac, a module that keeps documents take the old path. The runtime says which (`exact_evaluates_from_draw`, `Host::evaluates_from_draw`), read in the same owner job as `exact_svg_islands` after each boot and commit, so the draw makes no call to learn it.
- The launch report's `data` gains `evaluation` (its start and end on the owner) and `asked` (the end of the `dataReady` that activated, by which its requests are with the executor), in ms after activation's start.
- Tests: `DataFromFirstDrawTests` (the evaluation is posted from the draw; while the owner is held no main-thread turn waits for it, nothing of the rest runs, and `activationInFlight` is true from the draw on; the rest runs in the turn after the evaluation; a generation replaced during the evaluation applies nothing; a source the draw does not evaluate waits for the first turn), `NotificationActionTests.testRegistrationPrecedesTheAnswersOfAModuleEvaluatedFromTheDraw`, `activation_tests::only_a_module_on_the_runners_thread_is_evaluated_from_the_draw`, and `native_init` in exact-js (a synchronous native call from a module's top level is refused and reaches no module, so the evaluation may precede `prepareAppModule`).

Simulator check, a direction only: Weatherlight, a development build on an iPhone 18 Pro simulator (iOS 27.0), six warm launches under the agent per build, the same tree with the old `SessionActivation.swift` as the control. The first draw to the end of the activating `dataReady` took 98 to 117 ms (median 110) as before and 66 to 114 ms (median 75) with Part 1. The evaluation took 6 to 9 ms. Under the agent, location is substituted, so the forecast asked nothing; the requests timed were the native and storage ones.

Still open: Q1 (asking from the owner, which would take the remaining main-queue turn off the path), Part 2 (evaluation before the boot), and the iPhone measurement of Part 1 against the same build without it (LLP 1027.008 §8 step 2).

A limit on Part 1's gain: the main thread still makes synchronous calls into
the owner outside activation (a layout outside an owner callback reads
`session.view`, a queued collection flush sends its feedback, `drainFill` waits
for a slice). One made while the evaluation runs waits for it. Before Part 1 the
same call waited one turn later, so nothing waits longer than it did, but a large
module's overlap with the main thread's first turn is lost whenever that turn
makes such a call.
