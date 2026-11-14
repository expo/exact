# OAuth sign-out can log out the wrong account and drops state-less logout returns

**Status:** Open
**Systems:** packages/oauth/index.ts, runner/src/auth.rs, host/listen/src/loopback.rs, host/web/auth-glue.js, host/android/canvas/src/main/java/dev/exact/canvas/Auth.kt
**Author:** Charlie Cheever
**Date:** 2026-11-14

<what is wrong, where, and how to reproduce it>

## What is wrong

`signOut(store, { endSession: true })` (93deacbe0) opens the provider's logout before any await, from the session it read then. Six rounds of blind review of a parallel implementation (branch `fs/oauth2` in this repository, commit `4c707dc7c`) found these defects in that shape, each reproduced with a failing test there:

- **Wrong account.** If another tab replaces Alice's session with Bob's before sign-out takes the store's lease, logout opens with Alice's ID token while forget and revoke act on Bob's session. Sign-out must take its place in the store's queue at the press, read a fresh snapshot under the lease, and bind the logout's `id_token_hint` and the revocation to that one login.
- **State-less logout returns.** Entra and Cognito redirect back from logout without `state`. The macOS loopback receiver (`host/listen/src/loopback.rs`) and the runner (`runner/src/auth.rs`) require `code` or `error`, so the browser gets a 400 and `signOut` waits until its timeout; the web drops the return once the provider severs `window.opener` (`host/web/auth-glue.js`); Android drops it (`Auth.kt`). A logout session needs its own, correlated return rule on each host: accept only a URL with neither `code` nor `error`, and complete only the session whose `state` matches.
- **A waiting logout must not take a sign-in's return.** With the rule above, a stale sign-in redirect, or a sign-in finishing while a logout waits, must still go to the sign-in.
- **Leases on the wasm web host.** Awaiting `exclusive()` there holds the secret lease while the logout waits for the browser; releasing it must publish the writes first, and a forgotten call holding a lease must still settle.

The branch has the fixes and their tests; it was not landed because this issue's feature landed first in a different shape. Its LLP drafts (LLP 1146 D5, LLP 1069.006 D1/D3/D4/D5a, LLP 1133.005 D9) are in the branch's history of this work.

## Fix

Rebase the branch's fixes onto 93deacbe0, keeping its API (`end_session` code, `logoutRedirectParam`), and amend LLP 1146 D5, 1069.006 and 1133.005 before landing.
