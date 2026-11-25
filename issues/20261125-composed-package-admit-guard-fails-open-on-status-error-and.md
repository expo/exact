# Composed package `admit` guard fails open on status error and turns mid-flight `cancel` into executing the cancelled send

**Status:** Open
**Systems:** host/painted (Linux and Windows composed native packages)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Mid-flight cancellation and teardown)

Verification: verified by reading.

At `9c0431fd5`, `Packages::dispatch` in `host/painted/src/presenter/requests/packages.rs:133-174` has two compounding defects in the `admit` pre-check path (LLP 1133.002 D12) and `cancel` path (LLP 1133.016 D15 item 2):

1. **Fail-open admission (`packages.rs:154-169`):** When a non-app request carries `"admit": {status, field, denied}`, `asked` reads `state` only from `Outcome::Response` and maps every other outcome (including `Outcome::Failed` when `Reply` is dropped at `runner/src/request.rs:1269-1282`) to `Json::Null`, while `Reply::error` (`host/painted/src/package.rs:109-115`) produces `{"error": ...}` whose `body[field]` is also `Json::Null`. Line 163 refuses **only** when `state == "prompt"`. Any failed, aborted, malformed, missing-field, or `"unknown"` status response fails open, strips `"admit"` (`again`, `L152-153`), pushes `(again, reply.0)` to `inbox`, and executes `package.later(name, &args, ...)` on the next turn.
2. **Mid-flight `cancel` lost or inverted into execution (`packages.rs:133-135, 145, 148-172`):** Line 145 sets `context.0.request` to the send's request number `n` before invoking `package.later(&status, ...)` (`L171`), and `cancel` (`L133-135`) forwards `package.cancel(n)` without recording `n` as cancelled. While `status` is in flight (or while `again` sits queued in `inbox` awaiting the next `modules_in` turn at `host/painted/src/presenter/requests.rs:653-687`):
   - If the package honors `cancel(n)` by failing or dropping `asked`, `state` becomes `Json::Null != "prompt"`, so the fail-open path enqueues `again` and **starts executing the cancelled send `name`**.
   - If the package ignores `cancel(n)` on the `status` read, a `"granted"` reply still enqueues `again` and starts `package.later(name, ...)` after the runner already sent `cancel: n` and removed ticket `n` from `self.extensions.sent` (`runner/src/runner/modules/bound.rs:340-362`), leaving the started send uncancellable.

### Reachable trigger
The runner attaches `"admit"` in `runner/src/runner/modules/bound.rs:257-272` for non-app calls (`origin != App`) where `self.asks(row, row.send) && sub["admission"] != "none" && sub["permission"]["status"].is_string()`, and attaches `"request": n` for `send`s (`bound.rs:279-282`). Reachable targets include:
- Built-in Linux/Windows packages attaching `admit`: `@exact/window-management` (`getScreenDetails`, `request: "when-prompt"`, no `origin: "app"`) and `@exact/local-network` (`browseServices`/`resolveService` when joining a running app reader via `runner/src/runner/modules/admit.rs:637`). (Note: `@exact/hid`, `@exact/serial`, and `@exact/usb` do not attach `admit` because their prompting names declare `origin: "app"` without `request`.)
- Any composed `rust-static` package (`LLP 1133.001`, `host/painted/src/package.rs:1`) declaring `permission.status` with a `request: true` `send` or `request: "when-prompt"` `source` (the contract exercised by `host/painted/src/presenter/requests/packages_tests.rs:292-316`), when its `status` probe fails/omits `field` or when `cancel_let_go` (`bound.rs:340-362`) cancels request `n` before `again` runs.

### Actual vs. expected
- **Actual:** A failing/aborted/malformed `status` probe allows the non-app call to run; a `cancel: n` arriving during `status` or while `again` is queued either triggers execution via the fail-open path or fails to prevent `again` from starting `package.later(name, ...)`.
- **Expected:** Per LLP 1133.002 D12 (tool/wake calls that could prompt must fail closed with `person-required` unless status proves permission is non-prompting) and LLP 1133.016 D15 item 2 (`cancel` stops let-go sends), failed/missing/`prompt`/`unknown` status replies must refuse with `person-required`, and a `cancel: n` arriving while admission is pending or queued must suppress executing `again` and abort/reply cleanly.

### Verification and siblings checked
Verified by reading `host/painted/src/presenter/requests/packages.rs:133-174`, `packages_tests.rs:291-316`, `host/painted/src/presenter/requests.rs:653-687`, `host/painted/src/package.rs:88-116`, `runner/src/request.rs:1269-1282`, and `runner/src/runner/modules/{bound.rs,admit.rs}`. Siblings checked — the same defect pattern exists on other hosts:
- `host/apple/modules/ExactExtensions.swift:284-300`: **shares both defects** — `guard answer[field] as? String != "prompt"` (`L296`) fails open on error/missing field (including default `ExactExtensionAdapter.ask` at `L57-59` returning `["error": "NotSupportedError"]`), and `cancel` (`L284-286`) does not track cancelled request IDs while `a.ask(status, [:])` is in flight; on Apple this affects live built-in sends (`@exact/geolocation`, `@exact/motion`, `@exact/nfc`, `@exact/pedometer`, `@exact/push`).
- `host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt:735-759`: fails closed on status error/missing field (`L742-744`), **but shares defect (2)** — `cancel` (`L754-759`) does not track cancelled request IDs while `extension.later(statusName, ...)` (`L740`) is in flight.
- `host/web-js/modules.js:587-595`: fails open if `impl[p.status]({})` resolves to an error object or omits `p.field` (`L590`), and registers `forwarded` only after `await impl[p.status]({})` (`L593-594`).

### Suggested fix and regression test
In `host/painted/src/presenter/requests/packages.rs` (and sibling `ExactExtensions.swift` / `Extensions.kt` / `modules.js`):
1. Fail closed in `asked`: require `Outcome::Response` with valid JSON containing no `"error"` key and a string `state` that is explicitly non-prompting (e.g., `"granted"`, `"limited"`, or non-retryable `"denied"` on desktop/Apple; refusing `Outcome::Failed`, error payloads, missing/non-string fields, `"prompt"`, and `"unknown"` with `reply.error(&denied, Some("person-required"))`).
2. Do not bind `context.0.request` to `n` during the `status` probe (use `None`), and track pending-admission request numbers (e.g., a shared `Arc<Mutex<...>>` or per-extension cancelled set checked both in `asked` and before dispatching `again` in `Packages::dispatch`) so `cancel: n` marks `n` cancelled, completes/drops `reply` with `AbortError`, and prevents `again` from ever calling `package.later(name, ...)`.
3. Add regression tests in `host/painted/src/presenter/requests/packages_tests.rs` verifying that (a) `status` returning `reply.error(...)`, dropping `reply`, or omitting `field` refuses with `person-required` without calling `name`, and (b) `cancel: n` arriving while an async `status` probe is in flight (or while `again` is queued in `inbox`) never invokes `package.later(name, ...)`.

### Siblings checked

Checked host/apple/modules/ExactExtensions.swift:284-300 (shares both fail-open and mid-flight cancel defects, affecting live Apple packages geolocation/motion/nfc/pedometer/push), host/android/canvas/src/main/java/dev/exact/canvas/Extensions.kt:735-759 (fails closed on status error, but shares the mid-flight cancel defect), and host/web-js/modules.js:587-595 (fails open on resolved error/missing-field status objects and registers forwarded cancel tracking only after awaiting status).
