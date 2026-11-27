# Wasm host main-realm realm.message() overwrites and nulls active closure context mid-turn (and realm.forget() races with resettle())

**Status:** Closed
**Resolution:** Save and restore active closure context and prelude currentCall around stream messages, and cancel forgotten answers queued in resettle() or mid-turn.
**Systems:** host/web wasm data-module main realm (module-glue.js)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Out of order / Mid-flight cancellation and teardown)

Verification: verified by running.

At `9c0431fd5`, the wasm web host's main-realm runtime in `host/web/module-glue.js` runs two synchronous operations outside the `tail` turn queue that corrupt in-flight async turns:

1. **`realm.message(request)` clobbers active `context` (`host/web/module-glue.js:450-460`, `291-293`):** `realm.message()` runs synchronously via `moduleCall` (`host/web/glue.js:293` -> `module-glue.js:585`). Line 454 overwrites the shared closure variable `context` without saving its previous value, and `finish()` unconditionally clears `context = null` at line 292 (as does the `catch` at line 459). When a turn suspended on `tail` at `await checkpoint()` or `await storage.deliver(context.owner)` (`module-glue.js:303-316, 343-348, 361-370, 399-415`) resumes, `context` is `null`.
2. **`realm.forget(inFlight)` races with `resettle()` (`host/web/module-glue.js:335-351`, `463-482`):** `resettle()` drains `parked` into a local array (`const waiting = parked; parked = [];`, line 340) and awaits `proceed()` on each entry. Because `realm.forget()` runs synchronously outside `tail` (`module-glue.js:572`), a `forget` arriving while `resettle()` is suspended on one parked answer sees `parked === []`, releases forgotten entries' secret locks via `forgetSecretTurns` (line 465), and leaves forgotten entries in `resettle()`'s local `waiting` array to execute once the current entry finishes.

### Reachable trigger
On the wasm web host with a default `main`-placed TypeScript data module:
1. Open an `exactStream` (SSE/WebSocket/long-poll) on source A (`host/web/wasm/commit.js:487-524`), while source B (or a background round) runs a continuation on `tail` (`commit.js:452-456`) and suspends at `await checkpoint()` or `await storage.deliver(context.owner)`. When a stream message arrives on A, `requestHost.message` calls `safelyFulfill(..., 8, ...)` (`commit.js:522` -> `glue.js:640` -> `js/web/src/lib.rs:267-274` -> `module-glue.js:585`), executing `realm.message()` synchronously mid-turn.
2. Have two answers P1 and P2 parked behind an active storage operation; when that operation completes, `resettle()` moves `[P1, P2]` into `waiting` and suspends on P1's storage step. A runner commit dropping P2 invokes `BrowserData::forget` (`js/web/src/lib.rs:936-940` -> `module-glue.js:572`), calling `realm.forget([P1])` synchronously before `resettle()` reaches P2.

### Actual vs. expected result
- **Actual:**
  1. After `realm.message()` finishes, the resuming turn on `tail` sees `context === null`: any `__exact_host` call throws `Error('host call outside an answer')` (`module-glue.js:163`), `storage.deliver(context.owner)` / `finish()` throw `TypeError: null is not an object (evaluating 'context.reads')`, and the `catch` handler (`module-glue.js:348, 415`) sees `context === null` so it neither aborts `context.early` fetches nor retires `context.owner`.
  2. After `realm.forget([P1])` returns `{ ok: true }`, `resettle()` still settles and executes storage operations for the forgotten answer P2 — and if P2 held a secret lease, it runs after `forgetSecretTurns` already released its `navigator.locks` lock.
- **Expected:** `realm.message()` must preserve any suspended turn's `context` (matching `letGoHooks.deliver` at `host/web/module-glue.js:261-266` and LLP 1016.000 D1), and `realm.forget()` must serialize with `tail` or cancel entries still in `resettle()`'s `waiting` list before they execute (matching `host/web/module-worker.js:288` and LLP 1027.002 D2).

### Verification and sibling sites checked
Verified by running two Bun probes against `host/web/module-glue.js` through `host/web/module-storage-harness.mjs` (`placement: 'main', taskDelayedStorage: true`):
1. Opened `"stream"` (`exactStream`), suspended `"work"` at `storage.deliver`, delivered a stream message via `globalThis.exact.moduleRuntime.call({ op: "message", ... })`, then resumed `"work"`. `"work"` rejected with `"null is not an object (evaluating 'context.reads')"`.
2. Parked `"p1"` and `"p2"` behind `"blocker"`, let `resettle()` suspend on `"p1"`'s held read, called `r.forget(["p1"])`, then released `"p1"`. `resettle()` proceeded to execute `"p2"`'s storage read (`["start","app:/data/p2-ran"],["finish","app:/data/p2-ran"]`) and resolved `"p2"` with `"app:/data/p2-ran"` despite `"p2"` being forgotten.

Sibling sites checked:
- `host/web/module-glue.js:261-266` (`letGoHooks.deliver`): already saves/restores `const prev = context` in a `finally` block (safe).
- `host/web/module-glue.js:424,437,439,442` (`background`, `draw`, `overlay`, `retire`): do not assign `context` (safe). Note that in `js/src/prelude.js:1170,1222`, `__exact_message` also overwrites and nulls `currentCall` without saving/restoring it (unlike `__exact_overlay` at `js/src/prelude.js:1273-1301`), which should be saved/restored too.
- `host/web/module-worker.js:288`: serializes `forget` on `tail` and does not implement `message` (`module-glue.js:585`). However, `prepareWorker`'s `forget()` in `host/web/module-glue.js:525-529` calls `forgetSecretTurns(id, ...)` synchronously before the worker processes `{ op: 'forget' }` on its `tail`, creating a similar early secret-lock release window for in-flight worker turns.
- `host/web-js/ts-data.js` and `host/web-js/ts-stream.js`: the JS target tracks per-call context (`__exact_async_context` / `answering.mapper`) rather than a single mutable closure `context`, so it does not share this defect.

### Suggested fix and regression test
1. In `host/web/module-glue.js:450-460`, save `const prev = context;` before installing the mapper context and restore `context = prev;` in a `finally` block (and in `js/src/prelude.js:1170,1222`, save/restore `currentCall` in `__exact_message` as `__exact_overlay` does at `1273-1301`).
2. In `host/web/module-glue.js:463-482` (and `340-349`), either serialize `forget` onto `tail` (matching `host/web/module-worker.js:288`, keeping `forgetSecretTurns` aligned so secret locks are not released while an in-flight turn is still running) or track the active `waiting` slice alongside `parked` and mark forgotten entries cancelled before `resettle()` resumes them.
3. Add both probe scenarios as tests in `host/web/module-storage.test.mjs` using `moduleRealm(..., { placement: 'main', taskDelayedStorage: true })`: assert that a stream message delivered mid-storage-wait leaves the suspended answer able to read `store` and resolve cleanly, and that a forgotten parked answer in `resettle()` is rejected with `'the runner let this answer go'` and never starts its storage operation.

### Siblings checked

host/web/module-glue.js:261-266 (letGoHooks.deliver saves/restores context); host/web/module-glue.js:424,437,439,442 (background/draw/overlay/retire do not touch context); js/src/prelude.js:1170,1222 (__exact_message also does not save/restore currentCall, unlike __exact_overlay at 1273-1301); host/web/module-worker.js:288 (serializes forget on tail, no message op, though module-glue.js:526 calls forgetSecretTurns before worker processes forget); host/web-js/ts-data.js & ts-stream.js (per-call async context, unaffected)
