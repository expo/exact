# Stale `saveSettings`/`login`/`register` response writes JWT back to store after `logout` or newer sign-in

**Status:** Open
**Systems:** RealWorld authentication and session store (`signIn`, `logout`, `SettingsPage`, `AuthPage`)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Out of order)

Verification: verified by running.

### Location
At `9c0431fd5`:
- `apps/realworld/app.ts:219-226`: `signIn(store, path, body, method)` awaits `api(store, path, method, { user: body })` and then unconditionally writes `if (u?.token) store.set('realworld.jwt', String(u.token)); forget();` with no auth generation guard.
- `apps/realworld/app.ts:245-249`: `login`, `register`, and `saveSettings` all call `signIn`; `logout` executes synchronously (`store.forget('realworld.jwt'); forget(); return { ok: true, errors: [] };`).
- `apps/realworld/app.contract:127, 160-167, 702, 792-808`: `mutation auth as shape Auth refreshes me, follows then signedIn` backs all four actions; neither `save-settings`, `logout` (`SettingsPage`), nor `submit` (`AuthPage`) is disabled while `pending(auth)`.
- `host/web-js/resources.js:847, 868-906`, `host/web-js/ts-data.js:632, 653-688`, and `host/web-js/admission.js:338-344`: on the web host, sending `logout` (or a second `login`/`register`) supersedes the prior `auth` ticket in Contract without aborting the in-flight `fetch`, and `createSecretFacade.set` permits the superseded continuation to write `realworld.jwt` into `localStorage`. (On native hosts, `Runner::fulfill_now` at `runner/src/runner/reply.rs:136-142` drops a superseded single-fetch ticket before resuming JS, so this triggers live on the web host as well as in direct module calls.)

### Trigger
1. **Settings update + Logout:** On `/settings` (web host), a signed-in user clicks "Update Settings" (`testId="save-settings"`, starting `PUT /user`), then before the HTTP response returns clicks "Or click here to logout." (`testId="logout"`, which completes synchronously, clears `realworld.jwt`, and navigates home signed out). When the in-flight `PUT /user` reply arrives, `signIn`'s continuation writes the logged-out user's JWT back into `realworld.jwt`.
2. **Overlapping sign-ins:** On `/login` or `/register`, two submissions overlap and resolve out of order; the older response overwrites the newer account's `realworld.jwt` and calls `forget()`, wiping the newer session's `favs`/`follows`/`presses` and cache.

### Actual vs. expected
- **Actual:** A superseded `saveSettings`, `login`, or `register` response unconditionally executes `store.set('realworld.jwt', String(u.token))` and `forget()`, silently re-authenticating a logged-out user on the next reload/`currentUser` refresh or replacing the active user's JWT with a previous account's token.
- **Expected:** Auth transitions must respect session boundaries so superseded replies cannot mutate `realworld.jwt` or clear a newer session's state — consistent with the existing stale-reply guards in `apps/realworld/app.ts:69-71, 170-175` and `apps/realworld/app.test.mjs:134-166` (`test('replies from a previous reader leave the next reader alone')`).

### Verification & siblings
Verified by running a Bun probe directly against `apps/realworld/app.ts` at `9c0431fd5` with an in-memory `store` and held `fetch` responses (mirroring `apps/realworld/app.test.mjs:134-166`):
- Observed output: `{"out":{"ok":true,"errors":[]},"afterLogout":null,"afterLateSave":"alice-new","afterLoginB":"bob-jwt","afterLateLoginA":"alice-jwt"}`.
- Sibling sites checked:
  - `currentUser` (`apps/realworld/app.ts:166-178`): already guards 401 `store.forget('realworld.jwt')` with `store.get('realworld.jwt') === token`.
  - `flip` (`favorite`/`follow`, `apps/realworld/app.ts:34-42, 71`): already guards map writes with `presses`, cleared by `forget()`.
  - `apps/auth-fixture/app.ts:167-208, 237-286, 352-391, 410-415`: test fixture app whose async sign-in flows (`signIn`, `createPasskey`, `passkeySignIn`, `signInWith`, `offerSignIn`) also write `store.set('fixture.session', ...)` after awaits without a generation check against `signOut`.
  - `apps/expose/app.ts:306-314`: writes `store.set` synchronously with no `await`.

### Suggested fix & regression test
1. Introduce a module-level `let authEpoch = 0;` counter in `apps/realworld/app.ts`, incremented at the start of `signIn` and inside `logout` (and when `currentUser` forgets an expired token). Inside `signIn`, capture `const epoch = ++authEpoch;` before `await api(...)`, and only execute `store.set('realworld.jwt', ...)` and `forget()` when `epoch === authEpoch`.
2. Extend `test('replies from a previous reader leave the next reader alone')` in `apps/realworld/app.test.mjs` to cover (a) `saveSettings` held across `logout`, asserting `realworld.jwt` remains absent when `saveSettings` resolves, and (b) two overlapping `login` calls resolving out of order, asserting the newer token survives.

### Siblings checked

currentUser (apps/realworld/app.ts:166-178, guarded by token equality on 401), flip/favorite/follow (apps/realworld/app.ts:34-42, guarded by presses cleared in forget()), apps/expose/app.ts:306-314 (synchronous store.set), apps/auth-fixture/app.ts:167-208, 237-286, 352-391, 410-415 (test fixture with similar unguarded async store.set('fixture.session', ...)).
