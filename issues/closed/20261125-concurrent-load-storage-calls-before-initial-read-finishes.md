# Concurrent `load(storage)` calls before initial read finishes overwrite newly created conversations and reset `serial`

**Status:** Closed
**Resolution:** Re-checked if (loaded) return after awaiting storage.fs.readFile in Agents load(storage) so concurrent initial reads cannot overwrite newly created conversations or reset serial.
**Systems:** Agents persistence initial load (`load`, `snapshot`, `command`, `pulse`)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Out of order)

Verification: verified by running.

At `9c0431fd5`, `load(storage)` in `apps/agents/app.ts:50-65` checks `if (loaded) return;` only before `await storage.fs.readFile(FILE)`. It neither deduplicates concurrent loads nor re-checks `if (loaded) return;` after awaiting `readFile`.

**Trigger.** `load(storage)` is called by `snapshot` (`apps/agents/app.ts:415`), `command` (`apps/agents/app.ts:347`), `pulse` (`apps/agents/app.ts:319`), and `tools.findConversations` (`apps/agents/app.ts:287`). On a launch where `app:/data/conversations.json` already holds persisted conversations (`version: 1`):
1. Boot `resource data = snapshot(...)` (`apps/agents/app.contract:106`) enters `load()` (#1) and queues `readFile(FILE)`.
2. While #1 is pending, the user opens New Conversation (`testId="new"`, `apps/agents/app.contract:204`) and taps Start (`testId="new-start"`, `apps/agents/app.contract:323`, which is gated on `pending(changed)`, not `pending(data)`), entering `command('new')` -> `load()` (#2) and queuing a second `readFile(FILE)`.
3. Before #1 resolves, a third call enters `load()` (#3) — e.g., typing in Search (`testId="search"`, `apps/agents/app.contract:203`, re-asking `snapshot`) or `task health mount` (`apps/agents/app.contract:121-126`) firing `pulse` — queuing a third `readFile(FILE)` in storage's FIFO queue (`host/web-js/ts-data.js:311-355`, LLP 1097 D3) ahead of #2's `persist()`.

**Actual vs. expected.** When the FIFO operations resolve: #1 loads `{ serial: 5, conversations: [c1] }` and sets `loaded = true`; #2 resolves and creates `c6` (`++serial` -> 6), pushes `c6` into `conversations`, and queues `atomicWriteFile`; then #3 resolves with the stale initial bytes and unconditionally executes lines 58-59 (`conversations = data.conversations; serial = Number(data.serial) || 0;`), dropping `c6` from memory and resetting `serial` to `5`. `snapshot('c6', ...)` returns `found: false` ("Not Found"), the next `command('new')` collides with ID `c6`, and the next `persist()` overwrites `app:/data/conversations.json`, permanently erasing the earlier `c6`. Expected: concurrent initial `load()` calls must never clobber mutations committed after the first successful load (matching the persistence invariant in `apps/agents/app.ts:52-53`, `apps/agents/app.test.ts:42-53`, `apps/messages/app.ts:31`, and `apps/shelf/app.ts:53-83`).

**Verification & siblings.** Verified by running a read-only Bun probe against `apps/agents/app.ts` at `9c0431fd5` with a FIFO storage queue seeded with `{ version: 1, serial: 5, conversations: [c1] }`: queuing `snapshot`, `command('new')`, and a second `snapshot` before releasing reads produced `rNew.id = 'c6'`, followed by `snapshot('c6')` returning `found: false, rows: ['c1']`, and a subsequent `command('new')` colliding with `id = 'c6'` and overwriting disk.
Checked sibling apps using `storage.fs.readFile`:
- `apps/messages/app.ts:20-37`: guards `if (!loaded && ...)` at line 31 after awaiting `readFile`; not affected.
- `apps/shelf/app.ts:53-83`: shares a single-flight `loading` promise cleared on rejection; not affected.
- `apps/recorder/app.ts`, `apps/weatherlight/app.ts`, `apps/fieldnotes/app.ts`: no module-level `loaded` cache overwritten by concurrent loads.

**Suggested fix & regression test.** Immediately after `await storage.fs.readFile(FILE)` in `load(storage)` (`apps/agents/app.ts:54`), add `if (loaded) return;` (mirroring `apps/messages/app.ts:31` and avoiding cross-answer shared storage promises on the native Hermes executor, while keeping `loaded = false` on read errors so failed loads still retry). Add a regression test in `apps/agents/app.test.ts` simulating concurrent `snapshot` / `command('new')` / `snapshot` calls over a FIFO storage stub and asserting the created conversation remains present and `serial` does not regress.

### Siblings checked

Checked apps/messages/app.ts:20-37 (guards !loaded after await readFile), apps/shelf/app.ts:53-83 (shares single-flight loading promise), and apps/recorder/app.ts, apps/weatherlight/app.ts, apps/fieldnotes/app.ts (no clobbered loaded cache).
