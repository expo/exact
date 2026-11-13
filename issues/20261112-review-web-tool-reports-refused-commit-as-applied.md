# JavaScript tools report a refused action as successfully applied

**Status:** Open
**Systems:** host/web-js/calls.js, host/web-js/rt.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, tool dispatch calls its action and catches thrown exceptions,
but ignores the action's return value (`host/web-js/calls.js:119`). Runtime
actions return the result of `commit` (`rt.js:268`). A refused commit catches
its failure, restores the previous state, journals the refusal and returns
`false` (`rt.js:197-225`); it does not throw to the caller.

The tool therefore continues to completion after its action was rolled back.
With no owned work or an explicit `fails when` condition, it reports
`outcome: "ok", certainty: "applied"` and evaluates `returns` over the
unchanged state (`calls.js:123-129`). The WebMCP adapter then resolves with
that value as a successful result. A tool whose action produces an invalid
state write or hits a runtime refusal can tell its caller that the requested
change happened when it did not.

A controlled local probe extracted the production `commit`, `act`, and
`tools` implementations. An action threw a runtime `Refusal`; the actual
commit caught it and returned `false`. Dispatching the same action as a tool
produced:

```text
actualCommitReturn: false
result: { outcome: "ok", certainty: "applied", value: 1 }
journal: refused action: controlled invalid action
         tool fail ok applied
```

The probe is `/tmp/exact-review-20261112/web-tool-refused-commit.mjs`, run
with the pinned Bun 1.4.2. It exercises those production functions with inert
DOM, storage and scheduler boundaries; it is not a browser integration test.

Consume the commit's completion result and end a refused action with the
appropriate failure/refusal envelope instead of running successful completion.
Account for deferred tree application as well as synchronous commits. Cover
an invalid state write, an action argument refusal, a refused tree update,
and a successful tool with no asynchronous work, through both the agent and
WebMCP doors.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies. Open GitHub #329 covers WebMCP
registration, cancellation, schemas and activation; it does not report a
rolled-back action being reported as applied.
