# The Agents app abandons live on-device model sessions when changing models or retrying

**Status:** Closed
**Resolution:** Changing the model, retrying and deleting go through one release() that destroys the conversation's on-device session, and a session that finishes being made for a conversation that no longer wants it is destroyed rather than adopted.
**Systems:** Agents (`apps/agents/app.ts`), language-model session ownership
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, `command("model", ...)` clears `c.session` without calling `destroyLanguageModel` (`apps/agents/app.ts:373-375`). Retry does the same (`:356-357`). The old session remains owned by the language-model module, but the app has discarded the only handle it keeps. Deleting the conversation cannot release it because the delete path destroys only the current `c.session` (`:376-378`).

A normal trigger is to create an on-device conversation, switch its model to a hosted model, and delete it. Each repetition leaves another native/browser model session alive until the app session ends. Retrying a failed on-device response likewise replaces its model session without releasing the old one. These sessions retain conversation context and platform model resources; their count has no app-level bound.

Reproduced with the actual `apps/agents/app.ts` and actual `packages/language-model/web/index.js`, using its agent model and an in-memory storage adapter. The `@exact/language-model` import was connected to that implementation; network requests were disabled. For each of ten iterations:

1. Call `createLanguageModel`, then pass its returned ID through the app's normal `command("new", "", "device|none|<id>", 0)` path used after download.
2. Call `command("model", id, FEATURED[0].id, 0)` and `command("delete", id, "", 0)`.
3. Query each original ID with `languageModelSession`.

Result: zero persisted conversations, ten original model sessions still queryable. Probe: `/tmp/exact-review-20261112/agents-session-probe.ts`, run with Bun 1.4.2. This establishes app ownership loss without a browser or native model; platform allocation sizes were not measured. The retry path was inspected, not separately driven.

Release the owned session before discarding its ID when changing models or rebuilding history for a retry. Use one ownership helper so deletion, pending creation, and replacement cannot leave a session without an owner. Validate that repeated switch/delete and retry cycles return the module's live-session count to its expected value, including a late create completion after deletion.

Checked against all open and closed filesystem issues and the full bodies of all 275 cached GitHub issues. `issues/closed/20261102-stopping-a-send-before-the-job-id-arrives.md` covers late accepted response jobs, not these already-owned model sessions. `issues/20261112-review-language-model-streams-retained-after-destroy.md` concerns responses retained by the module after destruction; this issue concerns sessions the app never destroys.
