# A delayed iframe crash recovery overwrites a newer navigation

**Status:** Open
**Systems:** host/apple
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, a partitioned iframe's content-process crash queues a reload of
its current destination after 250, 500 or 1,000 ms
(`host/apple/webarm/FrameBrowser.swift:455-479`). The task captures the old
URL and checks only that the browser still exists and is not closed.

If the person or app navigates the same frame to another URL before this task
runs, `command` starts the new load without canceling the recovery (lines
403-416). `started`, `committed`, and `finished` also leave it queued. At the
recovery deadline the old task loads the crashed URL, replacing the newer
navigation or its completed document. An app handling `navigateerror` by
choosing another page can reproduce the same ordering. `stopLoading` does
not cancel a pending recovery either: after a crash there is no `flight`, so
it returns at line 428 before doing anything.

A local Swift probe extracted the actual `GuestURL`, `command`, `reload`, and
`crashed` implementations. A manually drained queue retained the real
`DispatchWorkItem`; an inert web-view boundary recorded load requests. It
crashed on the old page, requested a new destination, then drained recovery:

```text
loads after a newer navigation and the recovery deadline:
["https://new.example/person-choice", "https://old.example/page"]
```

The probe is `/tmp/exact-review-20261112/frame-crash-navigation.swift`.
It invoked no WebKit content process, window, network request, or wall-clock
wait. The existing crash test at
`host/apple/tests/ExactKitTests/IframePartitionTests.swift:400-429` covers
successive recovery attempts and the retry cap, without an intervening
navigation.

Cancel or invalidate recovery when a newer navigation or stop command takes
ownership of the frame. Make each recovery task verify that its crash is
still the current recovery before issuing a load. Cover a new navigation,
a changed source, stop, another crash, and explicit reload while recovery is
queued.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies for `FrameBrowser`, iframe/WebKit crash
recovery, and recovery replacing navigation; no matching report was found.
