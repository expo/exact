# On macOS a session's crash-breadcrumb slot is never freed, so the ninth session with hatches runs without one

**Status:** Closed
**Resolution:** `NativeViews.destroyModule`, which every session's end runs, frees the session's breadcrumb slot on every platform, in place of the UIKit-only `deinit`.
**Systems:** host/apple/Sources/ExactKit/NativeModule.swift, HatchBreadcrumb.swift, NativeHatches.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

LLP 1075.003.000.001 §4.4: "A session takes a free slot at start, and clears and frees only its own slot at its end … A ninth concurrent session runs without a breadcrumb, and the journal says so once."

The slot is taken in `NativeViews.takeBreadcrumb` (`host/apple/Sources/ExactKit/NativeHatches.swift:484`, commit `dd6f5dcd`) on both platforms. The only call of `HatchBreadcrumb.free` is in `NativeViews.deinit`, and that `deinit` is inside the UIKit-only block that also declares the parked-instance pool (`NativeModule.swift:442`):

```swift
#if os(iOS) || os(tvOS)
/// Parked instances by tag, oldest first.
fileprivate var parked: [String: [NativeEntry]] = [:]
fileprivate var observers: [NSObjectProtocol] = []
deinit {
    for o in observers { NotificationCenter.default.removeObserver(o) }
    if let crumbSlot { HatchBreadcrumb.shared?.free(crumbSlot) }
}
#endif
```

On macOS `NativeViews` has no `deinit`, and `destroyModule()` (line 794) does not free the slot either. Each session whose module connects hatches keeps its slot for the life of the process.

## The failure

A Mac app with a native module that has hatches, where each window is a session (`ExactMac/main.swift:510` destroys the session when its window closes). Open and close eight windows, then open a ninth. `HatchBreadcrumb.take` finds every slot taken (`HatchBreadcrumb.swift:91`) and returns nil, so the ninth session's journal says "hatch: 8 sessions hold the crash breadcrumb's slots; this one runs without one" although one session is live, and a crash inside one of its hatch calls leaves no record for the next launch. The eight stale slots keep `used = 1` with depth 0, so they add no false lines.

```swift
for i in 1...9 {
    autoreleasepool {
        let session = ExactApp.shared.makeSession(label: "crumb-\(i)")
        session.natives.openHatches({ _, _ in }, module)   // as NativeViews.module does when the table has hatches
        slots.append(session.natives.crumbSlot)
        session.destroy()
    }                                                      // the session and its NativeViews are released here
}
```

Observed: `slots` is `[0, 1, 2, 3, 4, 5, 6, 7, nil]`, and each of the nine `NativeViews` was released before the next session was made. Expected: each takes slot 0, which the one before gave back.

## Verification

Run as a temporary Mac XCTest in a process of its own (`bun host/apple/build.mjs --test --only <Class>`), with `CFFIXED_USER_HOME` set before the first use of `HatchBreadcrumb.shared` so that the process's breadcrumb file was made under a scratch home directory. The sessions were not booted and `openHatches` was called directly with a connect function that does nothing, so the journal line is read from `NativeHatches.swift:485`, not observed. `grep -rn 'free(' host/apple/Sources` finds no other caller of `HatchBreadcrumb.free`.

## Direction

Move the `deinit` out of the `#if` (keeping the observers' removal inside it), or free the slot in `destroyModule()` on both platforms.

## Related

QUEUE.md's "LLP 1075.003.000.001, access hatches on every platform" lists the breadcrumb as done on Apple and names no owed part of it. No issue covers it.
