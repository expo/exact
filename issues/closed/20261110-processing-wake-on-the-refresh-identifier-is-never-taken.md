# A processing wake request scheduled on the refresh identifier is never handed to its package

**Status:** Closed
**Resolution:** `schedule()` and a task's arrival ask one predicate, `WakeStore.carries`, which task carries a request, so the refresh task takes the processing requests it was submitted for when the processing identifiers are not permitted; the Apple implementation declares `apple.task` and `apple.requiresExternalPower`.
**Systems:** host/apple/Sources/ExactKit/Wakes.swift, packages/background/apple
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

On iOS `WakeStore.schedule()` (`host/apple/Sources/ExactKit/Wakes.swift:221-234`) submits the refresh task for a `processing` request when Info.plist does not permit `<bundle id>.processing.network`:

```swift
submit("refresh", at: soonest { $0.kind == "refresh" || !permitted("processing.network") && $0.kind == "processing" }) { … }
```

When that task runs, `arrived` (`Wakes.swift:198`) takes only requests of kind `refresh`:

```swift
let due = suffix == "refresh" ? take(kind: "refresh", power: nil) : take(kind: "processing", power: suffix.hasSuffix("power"))
```

`take` (`Wakes.swift:132-138`) filters on `$0.value.kind == kind`, so the processing request stays in the store. The arrival has nothing due, `offer` finishes it as a success (`Wakes.swift:146`), and `finish` calls `schedule()`, which submits the refresh task again for the same request, now due in the past.

## The failure

A package asks `context.wakes.request(kind: "processing", …)` (`host/apple/modules/ExactPackage.swift:392`) in an app whose only background grant is `device.periodic-background-sync`, which permits `<bundle id>.refresh` alone (`runner/src/device.rs:425-430`).

Observed: every refresh wake takes nothing, the package is never woken for the request, and each wake ends by asking iOS for another refresh at once. Expected, from LLP 1133.017 (stage 2, as built): "at run time its request goes to the refresh identifier when the processing ones are not permitted."

No app reaches this through `@exact/background` today. Its one such request is a periodic tag's with `apple.task="processing"` (`packages/background/apple/Background.swift:84`, `:158`), and `packages/background/apple/exact.json` declares no words (`"options": {}`), so the compiler refuses the word and the TypeScript binding has no `apple` field. LLP 1133.017 D12 specifies `apple.task` and `apple.requiresExternalPower`; declaring them makes this defect live.

## Verification

Ran `schedule()`, `take` and the `arrived` line as the file declares them, over stand-ins for `BGTaskRequest` and a recording `submit`, with one processing request due a minute ago and only `refresh` permitted. Three rounds printed the same:

```
round 1: schedule() submits ["refresh (due 60 s ago)"]
  the refresh task arrives and takes []; the store still holds ["background periodicsync news"]
```

Compiled `registerPeriodicSync(tag="news", minInterval=900000, apple.task="processing")` against the package: refused, `contract-module-argument`, "no implementation of `@exact/background` declares words for `registerPeriodicSync` on `apple`; the platforms with words are none". The bake's TypeScript binding types the call's options as `{ tag: string; minInterval: number }`.

Not run: an iPhone or a simulator; BackgroundTasks launches need a device.

## Direction

Have the refresh arrival take what `schedule()` put on it: in `arrived`, when `processing.network` is not permitted, take the due `processing` requests with the `refresh` ones, through one predicate both functions share. Declare the two words in the Apple implementation, or delete the code that reads them.

## Related

QUEUE.md "Background work: what is left" lists the `android.` registration words as unbuilt and says nothing of Apple's. `issues/closed/20261021-background-wake-needs-app-owned-plumbing.md` is about the launch plumbing. `packages/background/test/apple.mjs` reads the store's requests in an app that permits all three identifiers; no test runs a refresh arrival.
