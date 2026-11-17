# The watch app starts its live session's boot before it configures the app group, so which store the session binds depends on timing

**Status:** Closed
**Resolution:** `ExactWatch.init` configures the app group before it starts the early boot, so the boot reads the group's variables and binds the group's stores on every launch.
**Systems:** host/apple/Sources/ExactWatch/App.swift, host/apple/Sources/ExactCore/WidgetContainer.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, `ExactWatch.init` starts the early boot and only then configures the app group (`host/apple/Sources/ExactWatch/App.swift:268-272`):

```swift
if Bundle.main.object(forInfoDictionaryKey: "ExactWatchPlan") != nil { EarlyBoot.start() }
WidgetContainer.configure()
```

`EarlyBoot.start()` boots the session on a global queue (`App.swift:117-125`). `WidgetContainer.configure()`, on the main thread, is what sets `EXACT_APP_GROUP_ROOT` and `EXACT_APP_GROUP_APP` (`ExactCore/WidgetContainer.swift:29-36`), and its comment says "Called before any session". ExactKit keeps that order (`ExactKit/Session.swift:127`, in `ExactApp.init`), and so does the widget extension (`host/apple/widget/Timeline.swift:80`). In the watch app the two run at once on two threads.

The boot reads those variables once. `exact_boot_plan` endows the stores before the runner boots (`host/apple/src/abi.rs:824`): `endow_for` binds the group's `kv` only when `group_root(app_id)` answers (`host/apple/src/store.rs:116`, `:828-835`), and `Prefs::of` picks the group's store or `UserDefaults` by the same test (`store.rs:55`). The session keeps both for its life. Every watch bundle carries `ExactAppGroup` and `ExactAppIdentifier` (`host/apple/watch.mjs:80`), so `configure()` always has something to set.

## The failure

An app whose watch entry (`app.json` `watch`) persists state. On a launch where the boot reaches `endow_for` before the main thread returns from `FileManager.containerURL(forSecurityApplicationGroupIdentifier:)` and `setenv`, the session binds the watch app's own stores: its default key-value store and its `UserDefaults`. On a launch where `configure()` wins, it binds the group's. Nothing orders the two.

Observed, by reading: state saved on one launch is absent on the next when the two bound different stores, and the complications, whose extension configures first, read a group store the app did not write. `picker::app_dirs` reads the variables on each call (`host/apple/src/picker.rs:42`), so in a launch the boot won, `app:/data` files go to the group while the key-value store does not.

Expected: the watch app binds the group's stores on every launch.

No watch entry in the tree persists state today (Caltrain's `watch.contract` declares none), and the watch build is the simulator's.

## Verification

Read only: watchOS cannot run on this machine, and the outcome depends on timing. The order in `init`, the boot on another thread and the single read at endowment are in the lines quoted. The boot's path to that read (`SessionFacts.prime`, then `exact_boot_plan`) makes no call that waits for the main thread, so `init` returning does not come first.

## Direction

Call `WidgetContainer.configure()` before `EarlyBoot.start()`. Its cost is the container lookup, which the launch pays either way.

## Related

QUEUE "Watch: two driver nits left from the stage 1a reviews" and "Watch stage 1b and 1c, after the live driver landed" name other early-boot gaps, not this order.
