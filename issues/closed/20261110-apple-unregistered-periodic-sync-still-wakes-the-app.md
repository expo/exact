# On Apple, `unregisterPeriodicSync` leaves the tag's wake request with the host, so the app is still woken for it

**Status:** Closed
**Resolution:** `unregisterPeriodicSync` cancels the tag's `periodicsync <tag>` wake request with the host, which then drops it from its store and resubmits (or cancels) the platform task without it.
**Systems:** packages/background/apple/Background.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, each periodic tag asks the host for a wake under the key `periodicsync <tag>` (`ask`, `packages/background/apple/Background.swift:157` to `159`). The host keeps a request in its defaults, across launches, until it falls due, and then runs the app for it (`host/apple/Sources/ExactKit/Wakes.swift:89` to `100`, `132` to `138`).

`unregisterPeriodicSync` removes the tag from the module's own table and calls `keep`, which calls `ask` (lines 86 to 89). `ask` requests a wake for each tag that remains and cancels only the two `sync` keys (lines 153 to 154). Nothing cancels `periodicsync <tag>`.

So at the removed tag's due time iOS may start or resume the app in the background for a refresh or processing task (on the Mac the request's scheduler fires). `woke` finds the tag unregistered and reports the wake done with nothing raised (lines 220, 227). One background run is spent on a registration the app withdrew, and on iOS it comes out of the system's refresh budget.

Android cancels the work at the same call: `work.cancelUniqueWork(work("periodicsync", tag))` (`packages/background/android/Background.kt:99`).

## The failure

```
registerPeriodicSync(tag="digest", minInterval=900000)
unregisterPeriodicSync(tag="digest")
```

Observed: the host still holds `periodicsync digest`. Expected: no wake for a tag that is not registered; the call is `PeriodicSyncManager.unregister(tag)` (LLP 1133.017 D1; https://wicg.github.io/periodic-background-sync/#dom-periodicsyncmanager-unregister).

## Verification

Compiled `Background.swift` as written against `host/apple/modules/ExactPackage.swift` on a Mac, with a context outside the agent whose `ExactWakes` records each request and cancel as the host's store keeps them, and made the two calls above, then handed the module the wake for that key. It printed:

```
after registerPeriodicSync(digest): the host holds ["periodicsync digest"]
after unregisterPeriodicSync(digest): periodicSyncTags = [] ; asked of the host: [] ; the host holds ["periodicsync digest"]
  journal: background: a refresh wake: 
the wake for periodicsync digest: events raised 0, done(success: true)
```

Not run: iOS's scheduler launching the app for the stale request, which needs a device.

## Direction

Call `context.wakes.cancel(key: "periodicsync " + tag)` where the tag is removed.

## Related

QUEUE "Background work: what is left" lists unbuilt Android parts and device checks. `issues/20261110-processing-wake-on-the-refresh-identifier-is-never-taken.md` is the host's handling of a processing request, not a request left behind.
