# A synchronous package completion ends a shared Apple wake too early

**Status:** Open
**Systems:** host/apple
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `WakeStore.offer` clears the arrival's waiting set, posts each
package's wake notification, then adds that package to the waiting set
(`host/apple/Sources/ExactKit/Wakes.swift:135-147`). A package may finish
synchronously while the notification is being posted. Its completion observes
an empty waiting set and calls `finish` immediately (lines 101-106), before
the rest of the packages have even been offered their work.

The module forwards completion synchronously
(`host/apple/modules/ExactExtensions.swift:120-127`), and synchronous completion
is supported: the default adapter's `due` calls `done(true)` directly at line
93. `ExactPackage`'s default `woke` does the same. This is therefore a valid
callback ordering, including a package with nothing left to do.

For a shared iOS background task, the first such completion can call
`setTaskCompleted` while another package is still running. Its later result is
ignored because the arrival has already been removed, and the system can
suspend the app before that work finishes. A synchronous completion after
another package was added can instead leave its own completed package in the
waiting set, preventing completion when the remaining package finishes.

A local Swift probe extracted the actual `offer`, `finish`, and completion
observer implementations and used Foundation's real notification delivery.
Two packages accepted one arrival; the first finished synchronously and the
second retained its completion. The arrival finished before that second
completion was sent:

```text
after offering two packages: offers=2, completed=1, remaining arrivals=0
after second package finishes: completed=1
```

The probe is `/tmp/exact-review-20261112/wake-offer-reentrancy.swift`.
Persistence, scheduling and logging were inert boundaries; it invoked no
BackgroundTasks APIs, app windows or live service.

Record all outstanding offers before calling package code and postpone the
empty-set completion decision until offering has finished. Account separately
for packages that decline an offer. Cover synchronous completion first and
last, mixed synchronous/asynchronous packages, and declined offers.

Duplicate check: searched open and closed filesystem issues and all 275 cached
GitHub issue titles and bodies for wake offer/completion ordering. The closed
processing-wake carrier issue and refused-wake origin issue concern different
paths; no matching report was found.
