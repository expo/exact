# A tool queued behind another call ignores its five-second wait limit

**Status:** Closed
**Resolution:** Both tool controllers refuse each waiting call `busy` once it has waited five seconds, whatever is running, schedule the earlier of that and the running call's limit, and so never dispatch a call past its wait.
**Systems:** runner/src/runner/modules/calls.rs, host/web-js/calls.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, both tool controllers promise to refuse a queued call as
`busy` after five seconds waiting for another invocation. They enforce that
deadline while a wake runs, but not while another tool call runs.

The Rust controller stops at the current call while it owns work, before
inspecting any waiting deadline (`runner/src/runner/modules/calls.rs:132-141`).
The waiting deadline is checked only in the wake/data-readiness branch
(lines 147-156). Its next scheduled deadline is also the current call's
ten-second execution limit, ignoring queued callers (lines 392-400). The
JavaScript controller follows the same structure (`host/web-js/calls.js:136-157`).
When the earlier call finishes, both dispatch the next call without checking
how long it already waited.

As a result, a slow tool can hold the next caller for ten seconds instead
of five. Several slow calls can postpone later calls much longer, and those
queued writes are still executed after their admission deadlines.

A local probe ran the production JavaScript `tools` implementation with a
controlled clock and ticket set. The first call retained an owned ticket;
the second arrived at the same time. At 6,000 ms the second still reported
`waiting`. Releasing the first ticket then dispatched the second action:

```text
after 6000ms behind active call: {"call":2,"state":"waiting"}
after first settles at 6000ms: secondDispatched=1,
  outcome="ok", certainty="applied", at=6000
```

Probe: `/tmp/exact-review-20261112/web-tool-wait-bound.mjs`, run with the
pinned Bun 1.4.2. No browser, network or wall-clock wait is involved. The
matching Rust controller was checked in source, not executed by this probe.

Expire waiting calls independently of the running invocation and schedule
the earliest execution or admission deadline. Check the admission deadline
again before dispatch. Cover a running call ending before and after five
seconds, a running call that times out, several queued calls, and wake waits
in both runtimes.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies. Closed
`20261102-web-tool-deadline-waits-on-stale-clock.md` fixed sampling the live
clock for a running call's ten-second limit; this defect persists with an
explicitly advanced clock and affects queued admission. Open GitHub #329
covers caller cancellation, not expiration of this wait limit.
