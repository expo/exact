# @exact/payments (Web): mid-flight cancel() is lost across onpaymentauthorized retry and PaymentResponse.retry(), re-invoking onpaymentauthorized on a cancelled checkout

**Status:** Open
**Systems:** packages/payments (Web Payment Request API)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by running.

At `9c0431fd5`, `packages/payments/web/index.js:101, 120-132, 145-149` drops mid-flight `cancel(n)` calls across `onpaymentauthorized` retries and `PaymentResponse.retry()`, allowing a cancelled checkout to stay open and re-invoke `onpaymentauthorized`.

### Reachable trigger
On the web, `host/web-js/modules.js` invokes `impl.cancel(request)` immediately when a pending `showPaymentRequest` call is let go — either when a Contract send is superseded (`modules.js:251-252, 1019, 1036`) or when a data-code call's owning scope drops (`!c.kept()` in `modules.js:265, 595`). Unlike the native runner (`runner/src/runner/modules/bound.rs:345`), `modules.js` does not delay `cancel(n)` during a `final` callback and keeps `handlers.get("request:" + request)` registered until `showPaymentRequest`'s returned promise settles (`modules.js:604, 1038`).

Two reachable sequences trigger the defect:
- **Path A (cancelled while authorizing, handler returns `retry`):** `pr.show()` resolves, line 121 sets `entry.final = true`, and `call(n, args.onpaymentauthorized, ...)` is awaiting the server. `cancel(n)` runs, hits `if (!entry || entry.final) return;` (line 147), and discards the cancellation completely. When `onpaymentauthorized` resolves with `{ retry: { ... } }`, lines 126-129 clear `entry.final = false`, call `await r.retry(...)`, and loop back to invoke `args.onpaymentauthorized` again when the user resubmits.
- **Path B (cancelled while awaiting `r.retry(...)`):** `onpaymentauthorized` returned `{ retry: { ... } }`, so `entry.final` is `false` and the loop awaits `await r.retry(...)` (line 128). `cancel(n)` runs line 148: `entry.pr.abort().catch(() => {})`. Per the W3C Payment Request API, `PaymentRequest.prototype.abort()` always rejects with `InvalidStateError` once `show()` has resolved (the request's state is `"closed"`). The rejection is swallowed and no cancelled state is recorded, so when the user submits the retry sheet, `await r.retry(...)` resolves and the `for (;;)` loop invokes `args.onpaymentauthorized` again for the cancelled checkout.

### Actual vs. expected result
- **Actual:** `cancel(n)` is lost in both paths; `r.retry(...)` still prompts the user and a subsequent submission invokes `args.onpaymentauthorized` a second time and completes with `"success"`, risking charging an abandoned or superseded order.
- **Expected:** Per LLP 1133.034 D6 phases 2-3 ("A let-go that comes during [authorizing] is held and takes effect when it ends, so the request then becomes abandoned rather than returning to editing"; "except during a `retry`, when the W3C's `abort()` and `complete()` both reject until the person acts ... any authorization that arrives is answered as failed at once (`complete("fail")` after the web's retry resolves) without calling the app, so no payment reaches a server") and matching the substitutes (`runner/src/runner/modules/sheet.rs:507-511, 563-577`, `host/web-js/sheets.js:165-171, 251-255, 293-301`) and native modules, cancellation must be latched on `entry`. If cancelled during `onpaymentauthorized` and the handler returns `retry`, or if cancelled while awaiting `r.retry(...)`, the module must complete `r.complete('fail')` (and log closure after call end) without calling `r.retry(...)` or invoking `onpaymentauthorized` again.

### Verification and sibling sweep
- **Verification:** Verified by running `packages/payments/web/index.js` directly under `bun -e` with a W3C-compliant `PaymentRequest` stub (where `show()` transitions state to `"closed"` and `abort()` rejects with `InvalidStateError` once closed):
  - Path A (`cancel(1)` during `onpaymentauthorized` returning `{ retry: ... }`): `{ abortCalls: 0, retryCalls: 1, authCalls: 2, completeCalls: ["success"], result: "success" }`.
  - Path B (`cancel(1)` while awaiting `r.retry(...)`): `{ abortCalls: 1, retryCalls: 1, authCalls: 2, completeCalls: ["success"], result: "success" }`.
- **Siblings checked:**
  - `packages/payments/android/Payments.kt:233, 264, 302-308`: latches `s.letGo = true` and refuses any subsequent `changed` / `authorized` callbacks without calling the app handler.
  - `packages/payments/apple/Payments.swift:337, 349-353, 384-388`: latches `s.letGo = true` so a later `didAuthorizePayment` fails without calling `onpaymentauthorized` (minor related gap on Apple: if `s.letGo` is set while `s.authorizing` is true and `onpaymentauthorized` returns `retry`, lines 351-353 return retry errors instead of dismissing the sheet immediately).
  - `runner/src/runner/modules/sheet.rs:507-511` and `host/web-js/sheets.js:165-171, 251-255, 293-301`: both latch let-go across `retry` and refuse later authorizations.
  - Other web packages implementing `cancel(request)` (`clipboard`, `camera`, `language-model`, `nfc`, `screen-capture`, `translation`, `sign-in`): none have a `PaymentResponse.retry()` loop.

### Suggested fix and regression test
- Add `letGo: false` to `entry` at `packages/payments/web/index.js:101`.
- In `cancel(n)` (lines 145-149): always set `entry.letGo = true` before checking `if (entry.final) return;`, then call `entry.pr.abort().catch(() => {})` only when not already past `show()` (or always with `.catch(() => {})`).
- In the `for (;;)` loop (lines 120-139):
  - Check `if (entry.letGo)` before invoking `args.onpaymentauthorized` (so if cancelled during `r.retry()`, immediately `await r.complete('fail')` and return `{ error: 'AbortError' }` / log closure without calling `onpaymentauthorized`).
  - After `onpaymentauthorized` returns `answer?.retry`, if `entry.letGo` is true, do not call `r.retry(...)` — call `await r.complete('fail')` and exit instead.
  - Also ignore update callbacks (`updating` / `onpaymentmethodchange`) once `entry.letGo` is true.
- Add a Bun unit test under `packages/payments/test/` exercising both Path A and Path B against `packages/payments/web/index.js` and asserting `onpaymentauthorized` is called once (`authCalls === 1`), `r.complete('fail')` is called, and `r.retry` is not called in Path A.

### Siblings checked

packages/payments/android/Payments.kt:233,264,302-308 (latches letGo, refuses later authorize); packages/payments/apple/Payments.swift:337,349-353,384-388 (latches letGo, refuses later authorize, minor note that retry while letGo does not immediately dismiss); runner/src/runner/modules/sheet.rs:507-511 & host/web-js/sheets.js:165-171,251-255,293-301 (safe); other packages/*/web cancel() implementations have no retry loop
