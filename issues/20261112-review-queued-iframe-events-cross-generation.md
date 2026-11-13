# Queued Apple iframe events can invoke a replacement generation's handlers

**Status:** Open
**Systems:** host/apple
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `WebViews.receivedEvent` defers partitioned iframe navigation,
new-window, and download events onto the main queue. These closures retain
only the session and numeric node ID; they do not check the originating
entry or session generation (`host/apple/Sources/ExactKit/WebModule.swift:575-599`).

A plan replacement between queueing and delivery rebuilds the runtime's
nodes and can reuse the same ID. The old frame's `navigatesuccess`,
`navigateerror`, or download event then runs the replacement frame's handler
with the old document's record. An app can navigate or update its state in
response to an event from a frame that no longer exists.

The teardown gate does not cover this interval: `ResetGate.resetHeld` sets
`runtime.held` only around the synchronous `presenter.reset`, then clears it
(`host/apple/Sources/ExactKit/ResetGate.swift:200-207`). Queued main-thread
callbacks run after that gate has reopened. Session generation increments
at `Session.swift:997`, but these callbacks never capture it. Fresh and
carrying boots both construct `Ids::default()` (`runner/src/runner.rs:836`);
`Ids::fresh` numbers new nodes from 1 (`runner/src/instance.rs:237-254`).
The event dispatch resolves the ID in the current tree and chooses its
current handler (`runner/src/runner/event.rs:1205-1214,1392-1401`).

A local Swift probe extracted the actual navigation-event branch, queued an
old `navigatesuccess`, replaced the entry and generation in the fixture,
then manually drained the queue:

```text
queued old iframe event delivered: generation=2 node=1 event=navigatesuccess
record={"navigationId":7,"url":"https://old.example/"}
```

The probe is `/tmp/exact-review-20261112/frame-event-generation.swift`.
The queue, session, entries, and runtime were controlled boundaries. The
replacement and runner dispatch paths above were checked in source;
no real WebKit frame, GUI, network, or full native reload ran.

Capture the originating generation and entry, and drop the closure unless
both still own that node when it runs. Apply the same guard to navigation
and guest-record events. Cover a queued event followed by a completed
reload, and a partition change or node replacement before delivery.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies for iframe event callbacks, generation
replacement, and `WebModule`/`frameNavigation`. The separate presence-reply
issue concerns biometric completions and its own call-ID registry; this
iframe event queue has no common callback guard. No matching report was found.
