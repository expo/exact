# A queued synchronous answer runs its continuation as App

**Status:** Closed
**Resolution:** A waiting send keeps its commit's causes beside its origin and invocation, and `run_next` makes its `next` commit, arms its `then` and makes a refused ask's reconciling commit under all three.
**Systems:** runner/src/runner/queue.rs, runner/src/runner/modules/admit.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P1

At reviewed commit `f6be99af`, a queued mutation whose data source answers
synchronously arms its `then` outside the queued caller's context. A Tool or
Wake caller's continuation consequently runs as App, losing the origin and
invocation identity needed for admission and cancellation.

`run_next` restores the queued origin and invocation only around `next_inner`
(`runner/src/runner/queue.rs:229-232`). It then leaves that context before calling
`arm_then` at line 238. `note_then` reads the current origin, invocation id and
causes (`runner/src/runner/modules/admit.rs:535-548`), so it records the ambient
App context. The scheduled continuation restores those recorded facts at
`runner/src/runner/advance.rs:699-703`.

## Reproduction

An external Rust probe compiled the repository's current compiler, kernel and
runner through path dependencies; no host request was executed. Its Contract:

```text
component App
  mutation q as shape number queue then follow
  mutation work as shape number
  action first
    send q = save(1)
  action second
    send q = save(2)
  action follow
    send work = work()
  view
    text "queue origin"
```

The source answers `save(1)` with `Answer::Later`, `save(2)` with
`Answer::Now(Number(2))`, and `work()` with another `Answer::Later`. Call `first`
and `second` using `act_from(CallOrigin::Tool, ...)`, fulfill the first request
with a numeric answer, then call `land_then()`.

Observed outgoing request origins:

```text
first request origin: Tool
advance error: None
continuation work ticket 2 origin: Tool
continuation work ticket 3 origin: App
```

Both continuations must carry Tool origin. The first retains it through the
ordinary reply path; the queued synchronous answer loses it. This affects the
runner's origin admission boundary; no undeclared capability was requested or
executed by this probe. The invocation-id and cause loss follow from the same
`note_then` code; this probe directly observed the origin mismatch only.

Command run: `cargo run --offline --manifest-path
/tmp/exact-review-20261112/queue-origin/Cargo.toml`. The temporary probe is outside
the repository.

## Direction and validation

Arm the continuation within the queued caller's origin, invocation and causes,
keeping that context through any resulting reconciliation work. Cover Tool and
Wake callers whose queued sends answer both immediately and asynchronously;
their continuations must retain caller identity and cancellation ownership.

## Duplicate check

Searched open and closed filesystem issues and all 275 GitHub issues retrieved
on 2026-11-12. The closed `20261111-a-refused-wake-reply-releases-its.md` concerns
failed-reply reconciliation after leaving its context; this is the separate
successful queued-answer path. The closed
`20261102-kept-replies-lose-tool-origin.md` concerns retained requests.
