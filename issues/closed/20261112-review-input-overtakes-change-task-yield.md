# Input runs while its preceding clock advance still needs the host to apply receipts

**Status:** Closed
**Resolution:** An input that arrives behind a change action's yield is held until the work due by its arrival has run, after the host applies each receipt; a refused timer no longer stops that work, so the input observes its arrival; the JavaScript target's input wrapper resumes its advance the same way.
**Systems:** runner/src/runner/advance.rs, host/apple/src/host.rs, host/linux/src/host.rs, host/web-js/rt.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At reviewed commit `f6be99af`, `Runner::dispatch_at` runs the event immediately
after one `advance_timed` call (`advance.rs:56-69`). It does not inspect
`Advanced::needs_apply`. A change task deliberately yields with that flag
(`:927-935`), leaving further due work for another advance after the host applies
the receipts. The input therefore runs before that work and before the host
applies the preceding change task's receipt.

This reaches normal host entry points: Apple (`host/apple/src/host.rs:944-948`)
and Linux (`host/linux/src/host.rs:912-936`) call `runner.dispatch_at` directly,
then commit the combined receipts without checking `needs_apply`. An input can
observe state from before a timer that was already due, and the receipt boundary
that change tasks need for layout and host commands is skipped.

The JavaScript target has the same transition defect. Its input wrapper calls
`advance()` once and ignores its yield result before running the handler
(`host/web-js/rt.js:906`). If an ordinary timer precedes a due change task, the
first advance yields before that task, and the input overtakes it.

## Verification

A temporary Rust program compiled this Contract with the current compiler,
booted the current runner, set its live clock to return `100`, and dispatched
`Press` to `tap` at `100`:

```text
component App
  state seed = 1
  state changeSeen = -1
  state timerSeen = -1
  state eventSeen = -1
  action changed
    changeSeen = performanceNow()
  action timed
    timerSeen = performanceNow()
  action tapped
    eventSeen = timerSeen
  task changeSeed key=seed
    change(changed)
  task timer mount
    after(50, timed)
  view
    button "Tap" testId="tap" press=tapped
```

Observed: no error, `needs_apply=true`, `now_ms=100`, both `eventSeen` and
`timerSeen` remain `-1`, and the next due time remains `50`. Both returned
receipts are stamped `100`. Expected: apply the change task's receipt, finish
the work due by `100`, then deliver the input; it should observe `timerSeen=100`
on this live clock.

The same entry point also mishandles the time after a recoverable advance
failure. A one-shot timer at `10` that assigns `1 / 0` to a number state is
refused; an input arriving at `100` still runs, as documented, but its
`performanceNow()` is `10` while its receipt is stamped `100`. The event's time
must be established even when the earlier timer failed.

Probe source:
`/tmp/exact-review-20261112/queue-origin/src/bin/dispatch_clock.rs`; executable:
`/tmp/exact-review-20261112/dispatch-clock`. It was linked directly against this
checkout's compiled libraries. No native UI was started; host call sites were read.

A separate Bun probe extracted the production JavaScript `advance`, `fire`, and
`on` functions from `rt.js`, supplying a controlled clock and a small event-target
fixture. It scheduled an ordinary timer followed by a change task, both due at
zero, then delivered a click. The observed order was `["timer", "input"]`;
resuming the advance produced `["timer", "input", "change"]`. The expected order
is timer, change, input. This ran with the pinned Bun 1.4.2, without a browser:
`/tmp/exact-review-20261112/web-input-change-yield.mjs`.

## Direction and validation

Preserve the pending input across advance yields, apply each required receipt
batch, then resume advancing to the input's cutoff before dispatching it.
Give the handler its arrival time after a recoverable timer refusal. Cover a
change task followed by an overdue timer, multiple change tasks, a change task
that removes the input target, and a refused timer before an input. Check the
same ordering through the Apple and Linux host bridges and the JavaScript
target's input wrapper.

## Duplicate check

Searched open and closed filesystem issues and all 275 GitHub issues retrieved
on 2026-11-12. Closed GitHub #177 introduced keyed change tasks; it does not
report this input transition. Closed
`20261007-semantics-event-before-due-work.md` concerns an older semantics driver
that omitted the initial advance. Closed
`20261110-paused-input-overtakes-queued-events.md` concerns the separate game
simulation queue.
