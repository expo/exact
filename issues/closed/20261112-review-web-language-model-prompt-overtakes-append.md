# A web language-model prompt can start before an earlier append finishes

**Status:** Closed
**Resolution:** Queued work that answers a promise, an append's, holds its session until that promise settles, so the next prompt, clone or append waits for it.
**Systems:** packages/language-model/web/index.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Failure

At `f6be99af`, `queued` holds a model session only when its entry has a
response object (`packages/language-model/web/index.js:213-225`). An append
has no response object. Its async callback adds the local turn and awaits
`platform.append` (lines 488-500), but the session remains available for
another prompt immediately.

Consequently the next prompt can reach the browser before the appended
instruction. The module's transcript then claims an order that the model
did not observe; a browser that rejects concurrent session operations can
instead reject the prompt or append.

## Reproduction

Ran the actual web implementation under Bun 1.4.2 with a browser-model
stand-in whose `append` waits on an explicitly released promise. Create a
session, start `appendLanguageModel`, then start `prompt` without awaiting
the append. The observed order was:

```text
while append held: append-start, prompt-start
after release:    append-start, prompt-start, append-finish
```

The prompt generated while append was held. No real browser model or
network was used. Probe:
`/tmp/exact-review-20261112/model-append-probe.mjs`.

## Direction and acceptance

Hold the session for the complete duration of every queued operation,
including async non-response work. Release it on success, failure and
cancellation before admitting the next operation. Test held append followed
by prompt, clone and another append, including destruction during the wait.

## Duplicate check

Checked open and closed filesystem issues and all GitHub issue bodies on
2026-11-12. QUEUE.md's “A web language-model prompt can overtake an append”
already describes the defect but has no issue; this file provides the
filesystem issue requested by the review. Existing abort and cache reports
concern separate transitions.
