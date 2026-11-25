# Passkeys on Android ignores cancel(request) and lets a stale conditional-get callback hang a replacement conditional get

**Status:** Open
**Systems:** @exact/passkeys (Android, Web), @exact/local-auth (Apple)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Out of order)

Verification: verified by reading.

At `9c0431fd5`, `packages/passkeys/android/Passkeys.kt` has two lifecycle/ordering bugs in ceremony cancellation and replacement, and two sibling implementations share the missing `cancel(request)` handler:

1. **Stale conditional-`get` callback hangs replacement conditional `get` (`packages/passkeys/android/Passkeys.kt:66,200-224`):**
   ```kotlin
   if (conditionally && Build.VERSION.SDK_INT >= 35) {
       val pending = PendingGetCredentialRequest(request) { result -> endConditional(null); answered(result) }
       conditional = pending to reply
       fields.forEach { it.pendingGetCredentialRequest = pending }
       return
   }
   ```
   `endConditional(why: String?)` (`:219-224`) unconditionally clears `conditional` and detaches `pendingGetCredentialRequest` from every `EditText` without checking whether the invoking `pending` is still `conditional?.first`. When `pending1` is replaced by `pending2` (via `endConditional("a newer ceremony replaced...")` at `:179`), an already-dispatched autofill callback for `pending1` invokes `endConditional(null)`, clearing `conditional` (`pending2`) and detaching `pending2` from every `EditText` without invoking `reply2`. Because `conditional` is now `null` and detached, `reply2` can never be settled by autofill, a subsequent ceremony, or `destroy()`.

2. **Missing `cancel(request: Int)` override (`packages/passkeys/android/Passkeys.kt:66,72,153,200-216`):**
   `packages/passkeys/exact.json:52-53,128-129` marks both `create` and `get` `"request": true`. For data-code calls (`"contract": false`), the runner numbers the send (`runner/src/runner/modules/letgo.rs:26-42`), records it in `extensions.sent` (`runner/src/runner/commit.rs:1073`, `runner/src/runner/modules.rs:1154-1163`), and when the call is let go (e.g., navigating away from a sign-in screen holding a conditional `get` or superseding a modal ceremony), `cancel_let_go()` (`runner/src/runner/modules/bound.rs:340-362`) sends `{"extension": "passkeys", "cancel": n}`. On Android, `Extensions.kt:754-759` calls `extension.cancel(number)`. `PasskeysModule` does not override `cancel(request: Int)` (inheriting the no-op in `Extensions.kt:56`) and does not track `cause.request` for `conditional` or `signals`.

### Triggering sequences
- **Stale callback:** On Android 15+, start conditional `get` #1 (`pending1`), trigger an autofill pick whose callback is in flight while conditional `get` #2 installs `pending2`; `pending1`'s callback executes `endConditional(null)`, silently detaching `pending2` and hanging `get` #2 permanently.
- **Ignored `cancel(request)`:** Start a conditional `get` on a sign-in screen (the pattern exercised in `packages/passkeys/test/drives.mjs:75`) or a modal `create`/`get`, then navigate away or supersede the calling request so the runner dispatches `cancel(request)`. On Android, `PendingGetCredentialRequest` stays bound to `fields` (and is even attached to newly mounted `webauthn` `EditText`s in `webauthnField` at `:91-94`) and modal `CancellationSignal`s are never cancelled.

### Actual vs. expected
- **Actual:** A late callback from a superseded `PendingGetCredentialRequest` clears the replacement without replying, hanging it indefinitely; `cancel(request)` is silently ignored, leaking active `PendingGetCredentialRequest` bindings and uncancelled `CancellationSignal`s.
- **Expected:** Stale `PendingGetCredentialRequest` callbacks must be ignored when `conditional?.first !== pending`, and `cancel(request)` must abort a matching conditional `get` or modal `create`/`get` ceremony. Established by LLP 1133.011 D9 ("Lifetimes", `llp/1133.011-passkeys.rfc.md:484-488`), `packages/passkeys/exact.json:52-53,128-129`, `runner/src/runner/modules/bound.rs:337-362`, `Extensions.kt:54-56,754-759`, and `packages/passkeys/apple/Passkeys.swift:87-89`.

### Verification and siblings checked
Verified by reading `Passkeys.kt`, `packages/passkeys/exact.json`, `runner/src/runner/modules/letgo.rs`, `runner/src/runner/modules/bound.rs`, `runner/src/runner/modules.rs`, `Extensions.kt`, `host/web-js/modules.js:251-266,593-606,1133`, and sibling modules; additionally confirmed with a quick `bun -e` import of `packages/passkeys/web/index.js` that `implementation(...)` exports no `cancel` function (`typeof impl.cancel === "undefined"`).

Siblings checked:
- `packages/passkeys/apple/Passkeys.swift:87-89,293,394`: implements `cancel(_ request: Int)` properly (records `context.request` on `Ceremony` and calls `abort()`).
- `packages/passkeys/web/index.js:104-173`: **same missing `cancel(request)` defect** — creates `AbortController` in `get` (`:149-152`), omits `AbortController` in `create` (`:135-144`), ignores the `told` parameter (`host/web-js/modules.js:601`), and exports no `cancel(request)` method, so `host/web-js/modules.js:252,265,594-596` never registers or cancels let-go ceremonies.
- `packages/local-auth/apple/LocalAuth.swift:40,78-82,207-221`: **same missing `cancel(_ request: Int)` defect** — `packages/local-auth/exact.json:50-51` marks `authenticate` `"request": true`, and `LocalAuthModule` tracks active `LAContext`s in `evaluating` without request IDs and does not override `cancel(_ request: Int)` (unlike `packages/local-auth/android/LocalAuth.kt:54,85-88`, which implements `cancel(request: Int)` and tests it in `packages/local-auth/test/PromptResumeTest.kt:102-111`).
- `packages/sign-in/apple/SignIn.swift:99-101` and `packages/sign-in/web/index.js:118`: both implement `cancel(request)`.

### Suggested fix and regression test
1. In `Passkeys.kt`, capture `lateinit var pending: PendingGetCredentialRequest` and guard the callback with `if (conditional?.first !== pending) return` before calling `endConditional(null)` and `answered(result)`.
2. In `Passkeys.kt`, store `cause.request` alongside `conditional` and in `signals: mutableMapOf<CancellationSignal, Int?>()`, and override `cancel(request: Int)` to cancel matching `CancellationSignal`s and call `endConditional("the call was let go")` when `conditional` matches `request`.
3. Add matching `cancel(request)` support to `packages/passkeys/web/index.js` (track an `AbortController` per `told.request` for both `create` and `get`) and `packages/local-auth/apple/LocalAuth.swift` (record `context.request` alongside each `LAContext` in `evaluating` and call `invalidate()` in `cancel(_ request: Int)`).
Add Robolectric/unit tests for (a) a late callback from a replaced `PendingGetCredentialRequest` leaving the replacement active and answerable, (b) `cancel(request)` detaching a conditional `get` and cancelling a modal `CancellationSignal` on Android, (c) `cancel(request)` aborting `get`/`create` on web, and (d) `cancel(request)` invalidating the matching `LAContext` on Apple.

### Siblings checked

Checked packages/passkeys/apple/Passkeys.swift (implements cancel), packages/passkeys/web/index.js (same missing cancel), packages/local-auth/apple/LocalAuth.swift (same missing cancel), packages/local-auth/android/LocalAuth.kt (implements cancel), packages/sign-in/apple/SignIn.swift and web/index.js (both implement cancel).
