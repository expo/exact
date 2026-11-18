# An interrupted browser secret lease after a safe wait keeps its tentative credential

**Status:** Closed
**Resolution:** Every host arms recovery at a lease's first write or request with effects, and an armed lease that fails, caught or not, or is let go loses the secret; an unarmed one keeps it.
**Systems:** host/web/module-glue.js, host/web/module-worker.js
**Severity:** P2
**Author:** Codex, from the parked secret lease ordering fix
**Date:** 2026-11-15
**Related:** issues/closed/20261114-a-parked-wasm-answer-can-hold.md

The wasm browser host arms interrupted-lease recovery only when a lease returns a non-safe request. A lease that writes a tentative credential and then waits for a GET or timer has no recovery marker. If the answer is forgotten at that wait, the following lease reads the tentative value.

`host/web/module-storage.test.mjs` records the current behavior in the page and worker placements with `an interrupted secret lease after a GET exposes today's half-written credential` and its timer counterpart. Extending marker coverage is separate from the storage-before-acquisition repair; panel P explicitly left it unchanged there.

A related base behavior occurs after a non-safe POST arms recovery. If the body writes a partial credential, queues unawaited storage, then throws, an outer catch makes the answer successful and publishes the partial credential. The test `today's rejected lease with queued storage publishes its partial credential after an outer catch` records that behavior in both placements. The uncaught form still invokes interrupted recovery. Treating a caught failed lease body as abandoned requires its own rule; the parked-lease ordering change does not add one.

A native lease arms its poison marker before its first write as well as its
first request with effects, and a lease so armed that fails or is let go loses
the secret at the next lease, whether or not the answer catches the failure.
The wasm web host now matches: the glue arms the marker before persisting a
lease write, a failed callback reports `secretFailed`, and the glue then
forgets every armed name, in localStorage and in the runner. What the answer
does after the callback settles is not the lease's: a request or a write then
does not arm it, and an answer that fails after a completed lease keeps the
lease's writes. The JS web target arms the same way (the store's write, or a request
with effects from the answer holding the lease), where it had armed every
lease at its start and forgot the secret on any failure; a lease that fails
unarmed now keeps it on every host. Every host arms as the request is made,
so a POST a callback does not await arms it though the callback ends first,
and a write of the value the name holds arms nothing. The GET, timer and caught partial-write
tests assert the recovered state in both placements, `host/web-js/store.test.js`
the JS target's, and `js/tests/it/secrets.rs` the native side's.
