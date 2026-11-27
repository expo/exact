# Deleting a streaming conversation fails to cancel an in-flight `POST /turns` job and lets racing `pulse` generate titles for the deleted chat

**Status:** Closed
**Resolution:** Cleared streaming state and aborted active jobs in command('delete') and guarded start, startOnDevice, pulse, pulseOnDevice, and name with conversation membership checks so deleted chats cancel late turns and skip title generation.
**Systems:** Agents conversation lifecycle (`command`, `start`, `startOnDevice`, `pulse`, `pulseOnDevice`, `name`)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Mid-flight cancellation and teardown / Out of order)

Verification: verified by running.

At `9c0431fd5`, `command('delete')` in `apps/agents/app.ts:388-394` removes the conversation `c` from `conversations`, calls `await release(c)`, and only issues `DELETE /turns/${c.job}` when `c.job` is already populated. It never clears `c.streaming`, `c.reply`, or `c.job`, and `start()` (`apps/agents/app.ts:175-192`), `startOnDevice()` (`apps/agents/app.ts:230-246`), `pulseOnDevice()` (`apps/agents/app.ts:248-269`), `name()` (`apps/agents/app.ts:301-316`), and `pulse()` (`apps/agents/app.ts:318-338`) do not check `conversations.includes(c)`.

**Trigger.** From the chat view (`apps/agents/app.contract:234,362-366`), `testId="delete"` and `testId="delete-confirm"` remain enabled while streaming (and `changed` is a newest-wins mutation at `apps/agents/app.contract:102`, while `pulsed` is a separate mutation at `apps/agents/app.contract:103`).
1. User sends a prompt (`sendDraft` -> `command('send')`) and confirms Delete (`remove` -> `command('delete')`) while `start()` is awaiting `POST /turns` (`c.job === ''`).
2. Or, once `c.job` is set, user confirms Delete while `pulse()` (awaiting `GET /turns/${c.job}`) or `pulseOnDevice()` (awaiting `languageModelResponse`) is in flight.

**Actual vs. expected.**
- In case 1, `command('delete')` skips `DELETE` because `c.job === ''`. When `POST /turns` resolves at `apps/agents/app.ts:181`, `if (!c.streaming || c.reply !== reply.id)` at line 182 is `false` (unlike `stop`, which clears both at lines 378-380), so `DELETE /turns/${job.id}` is never sent. The orphaned job continues streaming in `apps/agents/service.ts:43-77,149-153` until completion or its 600 s timeout, consuming OpenRouter tokens and occupying one of the 8 active job slots.
- In case 2, `c.streaming` stays `true`, so when `GET /turns/${c.job}` resolves with `job.done === true` (including as a result of `delete`'s own `DELETE /turns/${c.job}` setting `job.done = true` at `apps/agents/service.ts:157`) or `languageModelResponse` resolves with a terminal state, `pulse()` (line 331) / `pulseOnDevice()` (line 268) still calls `name(c, storage)` — issuing a wasted `POST /title` OpenRouter call (`apps/agents/service.ts:100-111,143`) or creating a new on-device `createLanguageModel({})` + `prompt(...)` session (`apps/agents/app.ts:271-282`) for the deleted conversation, followed by an unnecessary `persist(storage)`. Similarly, `startOnDevice` lines 236 and 242 check `!c.streaming || c.reply !== reply.id` rather than membership.
- Expected: deleting a streaming conversation cancels any late-arriving turn job and suppresses title generation or persistence from racing polls, matching `apps/agents/app.test.ts:15-38` (late job cancellation on stop) and `apps/agents/app.test.ts:55-95` (resource release on delete).

**Verification & siblings.** Verified by running a read-only Bun probe directly against `apps/agents/app.ts` at `9c0431fd5`:
- Deleting while `POST /turns` was pending produced fetch calls `["POST http://127.0.0.1:4319/turns"]` with no `DELETE /turns/orphaned-job`.
- Deleting while `pulse()` was awaiting `GET /turns/j2` produced `["POST .../turns", "GET .../turns/j2", "DELETE .../turns/j2", "POST .../title", "GET .../health"]`.
- Deleting while `pulseOnDevice()` was awaiting `languageModelResponse` triggered a second `createLanguageModel` call (`1 -> 2`) to title the deleted chat.
Checked sibling `apps/messages/app.ts:63-139`: not affected — `delete-thread` refuses while `t.pending` (`app.ts:126`), `stop` deletes the WeakMap turn token (`app.ts:118`), and `start`/`pulse` check `turns.get(t) !== turn`.

**Suggested fix & regression test.** In `command('delete')` (`apps/agents/app.ts:388-394`), capture `const job = c.job;` and immediately clear `c.streaming = false; c.reply = ''; c.job = '';` before `await release(c)` and `DELETE /turns/${job}`. Also guard `start`, `startOnDevice`, `pulse`, `pulseOnDevice` (including `catch` paths), and `name` (before and after its `await`) with `conversations.includes(c)`. Add regression tests in `apps/agents/app.test.ts` covering `delete` during an in-flight `POST /turns` (asserting `DELETE /turns/<late>` is sent) and `delete` during an in-flight `pulse` / `pulseOnDevice` (asserting neither `POST /title` nor `titleOnDevice`'s `createLanguageModel` is invoked).

### Siblings checked

Checked apps/messages/app.ts:63-139 (guards delete-thread with !t.pending and tracks turn identity via WeakMap in start/pulse/stop; not affected).
