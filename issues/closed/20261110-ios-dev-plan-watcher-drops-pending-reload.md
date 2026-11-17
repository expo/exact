# The iOS dev-plan watcher drops a reload that was refused as pending; the Mac's asks again

**Status:** Closed
**Resolution:** The file form's timer is `ExactDevelopmentPlan.watch` in ExactKit, with the Mac's rule that a candidate refused as pending stays unseen, and both adapters call it.
**Systems:** host/apple/Sources/ExactIOS/main.swift, host/apple/Sources/ExactMac/main.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, the file form of `EXACT_DEV_PLAN` is watched by the same 100 ms timer in two adapters, and the two disagree on a refused candidate.

The Mac keeps the revision unseen while the app says the candidate is pending (`host/apple/Sources/ExactMac/main.swift:814-818`):

```swift
let revision = candidate.revision
guard revision != last else { return }
if candidate.apply(to: exact) || !exact.generationPending { last = revision }
```

iOS records the revision before it knows the outcome (`host/apple/Sources/ExactIOS/main.swift:121-126`):

```swift
let revision = candidate.revision
guard revision != last else { return }
last = revision
candidate.apply(to: exact)
```

A candidate with a module pair goes through `applyGeneration`, which answers false with `generationPending` set when a session's module is still preparing (`{"ops":[],"pending":true}`, `host/apple/src/abi.rs:702`) or a request is in flight (`refuse_pending`, `abi.rs:669`, `:896`; `ExactKit/Session.swift:249`). That refusal means "ask again": `ExactSession.prepare` leaves it unlogged for that reason (`Session.swift:944`), and both the update composition (`ExactUpdates/Updates.swift:238-243`) and the URL form of this loop (`ExactKit/PlanURL.swift:756-758`) retry it every 50 ms.

## The failure

A development build of an app with a module (`app.ts`, or a Rust module) on the iOS simulator, launched with `EXACT_DEV_PLAN=<path to app.plan>`, which `host/apple/build.mjs:1380` passes through. Edit the app while one of its requests is in flight, or while the new module is still preparing.

Observed, by reading: iOS calls `apply` once, the candidate is refused as pending, and the edit does not land until a watched file changes again. Nothing is printed. On the Mac the same edit lands on a later tick. The first tick after launch is exposed the same way: `last` starts empty for a module pair, so that tick is what applies it.

Expected: the two adapters treat a pending candidate alike, and the edit lands once the requests settle.

## Verification

Read only: the iOS simulator cannot run on this machine, and neither `main.swift` is in the Mac test target. The two loops are quoted above, and `apply(to:)` returns `applyGeneration`'s result unchanged (`ExactKit/PlanURL.swift:107-112`).

## Direction

Move the timer into ExactKit beside `ExactDevelopmentPlan`, with the Mac's rule, and have both adapters call it. The loop is duplicated only because each `main.swift` wrote its own.

## Related

Closed `20260904-linux-dev-plan-loses-to-store` is the Linux host's selection order. Closed `20260904-ios-device-lane-has-no-dev` is the device launch's missing environment. Nothing in `QUEUE.md` names this loop.
