# An interrupted browser secret lease after a safe wait keeps its tentative credential

**Status:** Open
**Systems:** host/web/module-glue.js, host/web/module-worker.js
**Severity:** P2
**Author:** Codex, from the parked secret lease ordering fix
**Date:** 2026-11-15
**Related:** issues/closed/20261114-a-parked-wasm-answer-can-hold.md

The wasm browser host arms interrupted-lease recovery only when a lease returns a non-safe request. A lease that writes a tentative credential and then waits for a GET or timer has no recovery marker. If the answer is forgotten at that wait, the following lease reads the tentative value.

`host/web/module-storage.test.mjs` records the current behavior in the page and worker placements with `an interrupted secret lease after a GET exposes today's half-written credential` and its timer counterpart. Extending marker coverage is separate from the storage-before-acquisition repair; panel P explicitly left it unchanged there.

A related base behavior occurs after a non-safe POST arms recovery. If the body writes a partial credential, queues unawaited storage, then throws, an outer catch makes the answer successful and publishes the partial credential. The test `today's rejected lease with queued storage publishes its partial credential after an outer catch` records that behavior in both placements. The uncaught form still invokes interrupted recovery. Treating a caught failed lease body as abandoned requires its own rule; the parked-lease ordering change does not add one.
