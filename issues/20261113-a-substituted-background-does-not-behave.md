# a substituted background does not behave as a frozen process

**Status:** Open
**Systems:** runner/src/runner/advance.rs, host/executor/src/pump.rs, host/apple/src/host_replies.rs, host/painted/src/host_replies.rs, host/apple/Sources/ExactKit/HatchClock.swift, host/apple/Sources/ExactKit/Agent.swift, host/painted/src/presenter/hatch.rs
**Author:** Codex, following the approved panel decision
**Date:** 2026-11-13
**Related:** issues/closed/20261113-agent-has-no-background-op-on-macos-or-a-phone.md

## Resolution (2026-11-15)

Main now has the separate `exactPage().frozen` fact (`9aa713c4b`, completed
for Apple by `1c129fe5d`). A drive substitutes it with `prefer frozen true`;
this is not `visibility-state hidden`. The frozen transition stops ordinary
execution that has a transition defined for the fact and releases resources
gated on it. It does not suspend the driven process.

The painted-host test `driven_page_facts_leave_painted_host_work_running`
starts an executor request and a hatch ticket, then sets both
`visibility-state hidden` and `frozen`. It changes a native module topic,
releases the reply and steps the clock. The test observes four repeating-timer
periods, the reply's committed action, a hatch frame and `after` callback, and
the module resource changing from `prompt` to `granted`; the announcement pump
returns no error. Those are the current behavior, not defects of either fact.
The commit titled “Driven page facts are documented without promising process
suspension” records that ruling and lets this issue close when it lands. Process
suspension, delivery coalescing and waking remain real-host qualifications
outside a drive.

## What was wrong

Under `prefer visibility-state hidden` on a suspending host, work keeps running
that a suspended process would not run:

- Every missed repeating-timer tick fires on `clock +N`; a device delivers one
  coalesced tick on return.
- Direct replies and executor completions can reach the runner while hidden.
- Apple's hatch frame and `after` callbacks keep firing on the agent clock.
- Android's painted hatch clock keeps delivering callbacks and queued actions
  (`host/painted/src/presenter/hatch.rs:589`, `:659`).
- Native module topic and event announcements bypass the executor outcome pump
  and can update resources or run Contract actions while hidden. The fourth-round
  review located these paths at `host/apple/src/host_replies.rs:59` and
  `host/painted/src/host_replies.rs:42` in the attempt below.

The approved panel decision keeps real OS backgrounding outside the drive and
adds no agent operation. This issue tracks the suspension model of the existing
substitute, not a phone lifecycle operation.

## Defects found beside the attempt

The named timer, reply, hatch and announcement behavior is outside the facts'
scope by the ruling above. Four independent defects remain in `QUEUE.md`:
`apply_announced` drops later topics after a refusal, `frame_tasks` ignores
`wake_stops_timers`, the announcement vector has no bound, and an executor
`Vec<Message>` allocation is not charged. Each needs its own change; none calls
for process suspension.

## Prior attempt and review cases

A four-round attempt is on branch `fs/agent2` in this repository, ending at
commit `6c3483582`. Its reviews are at
`/private/tmp/claude-501/-Users-ccheever-projects-exact/ed723a6a-9a04-4319-86f9-b8aa4fae4c31/scratchpad/reviews/agent2-r1`
through `agent2-r4`. Cases to preserve in a new design:

- **Transition and resume ordering.** An overdue data-source continuation could
  rewind the clock and replay all missed ticks. Resuming through `prefer` did not
  initially land the overdue work. Running it before applying the page-change
  receipt then let `change(action)` read old geometry or a missing newly mounted
  node, contrary to LLP 1092 D14. Apply the preceding commit before callbacks and
  honor every `needs_apply` yield. Land work due at the suspension instant before
  freezing; a suspended frame must not move past it. A refused resume must not
  leave later foreground ticks subject to resume coalescing.
- **Bounded reply drain.** Moving executor outcomes into a runner queue bypassed
  bounded buffering and the 32-completion/4-ms pump budget. Limits must count
  retained response, storage and capture payloads as well as stream messages.
  Drain incrementally on return; discard and overflow paths must dispose native
  handles, including a reply arriving after a queued terminal outcome.
- **Wake re-arming.** `begin_pump()` could continuously re-arm a wake for queued
  completions that suspension prevented it from consuming. Direct replies could
  instead remain stranded after resume or a budget-limited drain. Suppress wakes
  while held, wake explicitly on return, re-arm while work remains, and release
  the hold even when a resumed callback refuses.

## Superseded design direction

The attempted design would have gated every delivery path in one place rather than adding a separate guard to
each path. The boundary must cover runner timers, direct replies, executor
completions, hatch clocks and actions, and native module topic/event
announcements. Preserve commit application order, bounded buffering, bounded
drain work and wake ownership across suspension and return. Executor-only gates
and counter-only timer tests miss the hatch and announcement paths above. Main's
`frozen` fact instead gives each operation a defined transition rule and leaves
host-wide process suspension to the platform.
