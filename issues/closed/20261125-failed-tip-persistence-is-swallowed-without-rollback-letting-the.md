# Failed tip persistence is swallowed without rollback, letting the retry pass permanently consume an unpersisted paid IAP tip

**Status:** Closed
**Resolution:** Deferred mutating in-memory tip count and tokens in Weatherlight deliverPurchases until writeJson resolves so failed writes cannot skip persistence and consume the tip on retry.
**Systems:** Weatherlight in-app purchases delivery (`deliverPurchases`)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Halfway failure / Swallowed errors)

Verification: verified by running.

### Location
At `9c0431fd5`:
- `apps/weatherlight/app.contract:154, 159, 279-282`: `task delivering when due != "" key=due` triggers `deliver` -> `deliverPurchases()` whenever the comma-joined unfinished purchased tokens (`due`) change — including while an earlier `deliverPurchases()` call is still running (e.g. after consuming the first of two tips, or when a second tip arrives).
- `apps/weatherlight/app.ts:175-208`:
  - Line 183 reads `kept` once from `app:/data/tips.json`.
  - Lines 190-194 mutate `kept.tips += 1` and `kept.tokens.push(p.purchaseToken)` *before* `await writeJson(storage, TIPS, kept)`.
  - Lines 199-201 catch and swallow the error from `writeJson` without rolling back `kept.tips` or `kept.tokens`.
  - Lines 184-203 repeat the loop when `askedAgain` was set by an overlapping call (lines 177-180); on the next pass, `kept.tokens.includes(p.purchaseToken)` is already `true` in memory, so `writeJson` is skipped and `await consume({ purchaseToken: p.purchaseToken })` runs at line 195.
  - Line 204 returns `{ tips: kept.tips }` reflecting the unpersisted increment even if `askedAgain` is `false`.

### Trigger
With two purchased `tip.small` items (`t2`, `t3`) in `unfinishedPurchases()` (or a single tip whose delivery overlaps with a `due` update setting `askedAgain = true`):
1. `deliverPurchases()` persists and consumes `t2` (which changes `due`, causing `task delivering` to invoke a second `deliverPurchases()` that sets `askedAgain = true`).
2. For `t3`, `deliverPurchases()` mutates `kept` in memory and calls `writeJson(storage, TIPS, kept)`, which fails (e.g., `ENOSPC` or transient storage failure).
3. `catch {}` swallows the error without rolling back `kept`; the `do ... while (askedAgain)` retry pass re-reads `unfinishedPurchases()`, sees `kept.tokens.includes('t3') === true`, skips `writeJson`, and calls `consume({ purchaseToken: 't3' })`.

### Actual vs. expected
- **Actual:** `t3` is permanently consumed at the store without ever being written to `app:/data/tips.json`, and `deliverPurchases` returns `{ tips: 9 }` while the persisted file still has `{ tips: 8, tokens: ['t1', 't2'] }`.
- **Expected:** Per `apps/weatherlight/app.ts:151-155` ("a tip is counted once by its token, in the same write as the count, then consumed. A finish that fails leaves the row in the queue"), a failed `writeJson` must not mutate `kept` or allow `consume()` to run until the updated count and token are actually persisted.

### Verification & siblings
Verified by running a Bun probe directly against `apps/weatherlight/app.ts` at `9c0431fd5` (mocking `@exact/purchases` and `@exact/geolocation` as in `apps/weatherlight/app.test.mjs:250-287`, with initial `{ tips: 7, tokens: ['t1'] }`, queue `['t2', 't3']`, failing the second `atomicWriteFile` with `ENOSPC` while a second `deliverPurchases()` call overlaps):
- Observed output: `{"r1":{"tips":9},"r2":{"tips":8},"consumed":["t2","t3"],"queue":[],"persisted":{"tips":8,"tokens":["t1","t2"]},"writeCalls":2}`.
- Sibling sites checked: `visits` (`apps/weatherlight/app.ts:212-220`, no `try/catch` or retry loop so failed writes reject cleanly), `reviewAsked` (`apps/weatherlight/app.ts:221-225`, writes `{ ...kept, asked: VERSION }` without mutating `kept`), `apps/shelf/app.ts:104-116` (clears in-memory `state = null` on write failure), `apps/messages/app.ts:14-19` and `apps/agents/app.ts:66-71` (no external irreversible `consume()` gated on unrolled-back state). No other app in `apps/` uses `@exact/purchases`.

### Suggested fix & regression test
1. Compute the candidate object without mutating `kept` until `writeJson` resolves:
   ```ts
   if (!kept.tokens.includes(p.purchaseToken)) {
     const next = { tips: kept.tips + 1, tokens: [...kept.tokens, p.purchaseToken] };
     await writeJson(storage, TIPS, next);
     kept.tips = next.tips;
     kept.tokens = next.tokens;
   }
   ```
2. Add a unit test in `apps/weatherlight/app.test.mjs` beside `overlapping deliveries count every tip once` where `atomicWriteFile` rejects for a tip while a second `deliverPurchases` overlaps, asserting the failed tip is not consumed and remains in the queue (and that the returned `tips` matches persisted storage).

### Siblings checked

visits and reviewAsked in apps/weatherlight/app.ts:212-225 (no swallow/retry or immutable copy), apps/shelf/app.ts:104-116 (resets in-memory state on write failure), apps/messages/app.ts:14-19 and apps/agents/app.ts:66-71 (no external consume() gated on in-memory state); no other app uses @exact/purchases.
