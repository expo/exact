# Concurrent Observe queue appends can overwrite another tab's event

**Status:** Open
**Systems:** modules/observe/web/service.js, modules/observe/tests/tabs.test.mjs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, `update` reads and rewrites the shared
localStorage queue without a lock (`modules/observe/web/service.js:94`).
The Web Lock at lines 200-206 surrounds dispatch only; recording an event
or metric calls `update` directly (lines 100-101). Two tabs can read the
same queue before either writes, then the last writer drops the other
append. A concurrent append can also overwrite a sender's removal and
restore an already processed row.

The send lock fixes simultaneous senders, but it does not protect the
queue mutation against producers. This loses custom events and metrics,
or sends them again, during overlapping activity in tabs of one app.

A deterministic probe imported the unchanged service, started two sessions
against shared storage, and selected this interleaving:

1. Session A reads the empty queue for `first`.
2. Session B reads the empty queue and stores `second`.
3. Session A stores its earlier snapshot plus `first`.

The result was `{"queue":["first"],"lockRequests":0}`; both event calls
completed. The probe is
`/tmp/exact-review-20261112/observe-interleave.mjs`, run with pinned Bun
1.4.2. It uses an instrumented storage adapter to control the interleaving;
this is not a measurement of race frequency in a browser. No requests were
sent. The existing browser test appends its three events serially and only
dispatches concurrently (`tabs.test.mjs:56-66`), so it does not test this
ordering.

Serialize every shared queue read/modify/write, including appends, session
metadata changes and acknowledgements. Use a separate short queue lock
alongside the send lock, or a transactional store; do not hold the queue
lock during network requests. Preserve event ordering when converting the
synchronous producer to queued writes. Validate simultaneous appends and
an append overlapping acknowledgement, then check exactly-once retention
and delivery within the existing retry model.

The closed `20261110-observe-web-tabs-send-rows-twice` covers long-lived
per-tab snapshots and duplicate senders. Its fix rereads the shared queue
and locks dispatch, but leaves this read/modify/write race. This is a
follow-up to that resolution. Searched all open and closed filesystem
issues and all 275 cached GitHub issue bodies; no open report covers the
remaining race.
