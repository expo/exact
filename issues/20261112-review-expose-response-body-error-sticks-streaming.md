# Expose cannot send another assistant message after a response-body failure

**Status:** Open
**Systems:** apps/expose/app.ts
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, `ask` handles rejection of the initial `fetch` with the second argument to `.then`, but does not handle rejection from its asynchronous fulfillment callback (`apps/expose/app.ts:231-262`). Reading the stream (`:256`) or parsing the native response as JSON (`:235`) can reject after the response has arrived. That rejection bypasses `finish`, leaving the conversation's `streaming` flag true.

A connection that drops during a web response therefore leaves Expose permanently "still answering" for that app session. Every following `ask` refuses to start, and `clearThread` also refuses to clear a streaming conversation (`:265-272`). Reloading loses the in-memory thread; there is no recovery action for this state.

Reproduced with the actual app module, a mocked local configuration containing no key, an in-memory store with a dummy key, and `fetch` returning a response whose body read rejects. No network request was made. Results:

```text
first ask rejects: connection lost during response body
conversation: streaming=true, error=""
second ask: done=false, error="Expose is still answering."
after clearThread: streaming=true
```

Probe: `/tmp/exact-review-20261112/expose-stream-error-probe.ts`, run with Bun 1.4.2. The native JSON-decode branch has the same missing rejection handler but was not separately driven.

Handle failures from the entire fetch-and-read operation, settle the current turn, and preserve any partial response with an error that allows another send. Release or cancel the body reader when the turn ends. Validate a rejected fetch, an interrupted body, invalid native JSON, and an ordinary completed response.

Checked all open and closed filesystem issues and the full bodies of all 275 cached GitHub issues for Expose, response-body reads, and stuck streaming. The harness streaming-timeout issue concerns another app and another mechanism; no existing report covers this path.
