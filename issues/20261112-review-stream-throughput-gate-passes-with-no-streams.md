# Stream throughput gate passes without receiving any streams

**Status:** Open
**Systems:** Stream fixture, performance validation
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `apps/stream-fixture/gate.mjs` reports every throughput
criterion as passing when the page renders normally but no stream opens,
receives a message, or decodes a frame. This can conceal a regression in
the fixture's start actions or resource startup: the declared three-stream
workload is never established.

Lines 110-117 reduce empty observations to zero failures, zero drops,
zero latency, and zero busy time. Lines 133-144 check the number of
configured runs and the page's presented frames, but never require the
expected number of active streams or any stream traffic. The page's
independent spinner can satisfy the frame check while all three resources
remain idle. A run with only one working stream also lacks a check that
the other two participated.

Verified by importing the actual gate with controlled agent and HTTP
adapters. The agent returned healthy page frame samples, idle resources,
and an empty stream timing list; the server returned no connections.
The gate completed all six runs and requested exit status 0:

```text
three iOS: 0 sent, 0 decoded, 0 dropped, 0 refused
two iOS and one Android: 0 sent, 0 decoded, 0 dropped, 0 refused
noOverflowOrDrop pass · decodeP95 pass · decodeP99 pass · busy pass · frames pass
```

The probe is `/tmp/exact-review-20261112/stream-gate-empty-probe.mjs`,
run with pinned Bun 1.4.2. It intercepts report writes and process exit;
it starts no server, browser, or native application.

Require the expected stream identities, live resources, nonzero frame
and decode samples, and traffic consistent with the measured duration
before evaluating latency and CPU budgets. Validate zero traffic, one
working stream out of three, missing decode instrumentation, and a
complete three-stream run. Keep the explicit no-stream baseline exempt.

Checked all open and closed filesystem issues and the full 275-issue
GitHub snapshot. The related `QUEUE.md` entry about replacing
`__exact_gate_decode` concerns the decoder API, not acceptance of an
unmeasured workload.
