# OAuth sign-out can log out the wrong account and drops state-less logout returns

**Status:** Closed
**Resolution:** Sign-out revokes inside the store's lease and logs out the account it read there; a logout return needs no state and never completes a sign-in; on the web a state-less logout return is addressed by an identity in the popup's own sessionStorage; the wait ends on an elapsed bound.
**Systems:** packages/oauth/index.ts, runner/src/auth.rs, host/listen/src/loopback.rs, host/web/auth-glue.js, host/web/auth-callback.js, host/web/module-glue.js, host/web/module-worker.js, js/src/prelude.js, host/android/canvas/src/main/java/dev/exact/canvas/Auth.kt
**Author:** Charlie Cheever
**Date:** 2026-11-14

## What is wrong

Before this change, `signOut(store, { endSession: true })` opened the provider's
logout from a session read before it entered the secret's queue. Four defects
follow:

- **Wrong account.** If another tab replaces Alice's session with Bob's before sign-out takes the store's lease, logout opens with Alice's ID token while forget and revoke act on Bob's session. Sign-out must take its place in the store's queue at the press, read a fresh snapshot under the lease, and bind the logout's `id_token_hint` and the revocation to that one login.
- **State-less logout returns.** Entra and Cognito redirect back from logout without `state`. The macOS loopback receiver (`host/listen/src/loopback.rs`) and the runner (`runner/src/auth.rs`) require `code` or `error`, so the browser gets a 400 and `signOut` waits until its timeout; the web drops the return once the provider severs `window.opener` (`host/web/auth-glue.js`); Android drops it (`Auth.kt`). A logout session needs its own return rule on each host: accept only a URL with neither `code` nor `error`, match `state` when present, and never complete a sign-in. The runner and Android select their oldest live logout in one process. The web must address a severed return to the logout whose popup produced it, without choosing another tab's request.
- **A waiting logout must not take a sign-in's return.** With the rule above, a stale sign-in redirect, or a sign-in finishing while a logout waits, must still go to the sign-in.
- **Leases on the wasm web host.** The provider's browser wait must not hold
  the secret lease. An ordinary browser promise awaited inside `exclusive()`
  is not host-tracked work, so the answer fails as pending on nothing. A
  completed lease must keep its publication marker across results the realm
  consumes itself; forgetting before completion reaches the host must abandon
  the update rather than publish it outside the lock.

## Implemented

Sign-out enters the store queue at the press. One `exclusive` callback reads
one fresh session, binds the logout endpoint, ID-token hint, login identity and
revocable token to that snapshot, removes the active credential, and retains
only a provider logout that still needs to finish. It then revokes inside that
lease, newest token first. No reply, a timeout, 429 or 5xx leaves the token in
runtime-local bookkeeping for the next sign-out on that key; any other reply
drops it. Expected revocation failures do not turn local sign-out into an
interrupted secret update.

The callback publishes its writes and releases the lock before
`openAuthSession` begins. A successful provider return takes another lease and
removes only the matching retained logout. A second sign-out waits through
`store.exclusive`; no answer awaits another answer's request, and no untracked
browser promise is awaited inside the lease. Revocation is best effort: there
is no background retry and no revocation list in the secret. A reload, restart
or runtime replacement ends the in-memory retries.

`openAuthSession` carries a `logout` bit. A logout return is accepted only
when it has neither `code` nor `error`. Its one `state`, when present, must
match; it may omit state. Android and the runner complete the oldest live
logout at the callback endpoint. A loopback receiver belongs to one session
and port, so its state-less choice is unambiguous. On the web, each logout
writes a fresh random identity into its blank popup's own `sessionStorage`
immediately before navigating it. The callback reads and removes that value,
then posts the URL and identity to its opener first and to the channel. The
opener's tab accepts a state-less return directly from its own popup. After
severance, only the live logout holding that identity completes, synchronously
in its own tab. An unaddressed return completes nothing. No tab takes a lock,
writes shared storage or reads another tab's state. A state-less URL with
`code` or `error` completes no logout. A request still held after its
ten-minute bound answers 499 exactly once, whether a return or the bound timer
is handled first; a request the runner already let go ends silently. The bound
uses the page's monotonic clock, so a system-clock change cannot extend or
shorten it, and the bound timer's firing ends a request still held.

The wasm realm now keeps the lease-completion marker only in `settle`. A yield,
storage wait or own-storage delivery consumed inside the realm leaves it armed;
the next result the host sees is stamped unless it asks for another lease, which
clears the old marker. A parked answer therefore keeps its Web Lock until its
next result reaches `run`. Forgetting it first abandons the unfinished update:
the lock is released, no writes are published, and an armed recovery marker
deletes the secret. Once completion reaches `run`, the host persists the writes
and removes the completed turn from the answer's current-turn index before
runner delivery. It keeps the physical lock until delivery returns. A
continuation started by that delivery therefore inherits no completed turn; a
continuation that asks for the secret registers a fresh turn, which releasing
the old turn cannot remove.

`run` also fences storage after each wait which precedes persistence. If
`forget` released or replaced that turn while `turn.run()` was unresolved, its
late writes are discarded. This is independent of sign-out's ordering:
`signOut` makes only secret-store mutations and tracked token requests inside
the lease, and does no filesystem storage before the completed result reaches
`run`; provider navigation begins afterward.

Prepared logout windows now live in an id-keyed reservation map, separate from
the sign-in button's id-less window. A logout adopts only the reservation id it
carries and never the sign-in window. Cancellation, request refusal, agent
fulfilment without browser adoption, and a ten-minute bound close an unadopted
reservation. A logout with no reservation opens `about:blank` before the
provider URL, so every logout gets the same chance to seed its popup-local
identity. A failed seed still allows opener delivery and matching `state`; an
unaddressed severed return fails closed.

Recovery from an interrupted secret lease continues to delete the whole
secret. That fail-closed rule can also discard a retained provider-logout hint;
it does not need to preserve revocation debt because the debt is runtime-local.

## Coverage

The tests cover the Alice/Bob leased snapshot, serialized overlapping
sign-outs, revocation before provider navigation, runtime-only retry after a
503 and a thrown host timeout, host-delivered revocation timeout, and
task-delayed secret-store Web Locks in both wasm placements. They also cover matching,
wrong, repeated and absent logout state; code and error rejection; opener
ownership; a fresh popup identity written before navigation; exact two-tab
attribution in either return order; a stalled or retired tab; missing, foreign
and unavailable identities; task-delayed messages and timers; a late opener,
stateful or addressed return reporting the bound once; wall-clock steps in
either direction leaving the elapsed bound unchanged; callback endpoints;
window reservation and cleanup; a
second lease held through its own request;
lease completion across yield, storage parking and forgetting; and refusal of
an answer waiting on another answer's fetch. A 400 revocation reply is covered
as a completed attempt which is not retried. The storage suite directly covers
a forgotten worker turn resolving late with writes after another realm has
taken the lock. That worker is held after a successful result carrying writes,
another holder publishes a replacement, and the late result is then delivered.
Removing the post-worker fence makes the test overwrite the replacement with
`forgotten-complete`. A separate ordered worker reply proves a completed lease
is parked before it is forgotten; it no longer relies on page-side timing
checkpoints.

A completed lease followed by a zero-delay timer reproduces the publication
race in both main and worker placements. Delivery queues the timer continuation
in a promise reaction, as `host/web/wasm/commit.js` does. Before the fix both
placements refused the continuation with `the secret’s answer was let go`.
They now deliver its value and grant the next lease. Fetch, storage and auth
continuations enter the same generic `continue` branch: the completed turn is
removed before any runner delivery, independent of which operation later
caused that branch.

## Browser proof

The driver already put its auth fixture in the page URL as `?authFixture=` for
both web targets. The JS target read it directly. The wasm auth glue omitted it
from `exact_auth`'s arm request, while the Rust runner looked only for the native
process environment variable. The runner therefore redacted its held URL and
the driver correctly refused to authorize it. The wasm glue now forwards the
existing parameter, and the runner validates it as an agent-only loopback
origin before exposing the full fixture URL.

The wasm target now reaches the local provider in headless Chrome and uses the
browser's `navigator.locks`:

`bun host/web/build.mjs oauth-fixture-web --wasm`

`EXACT_WEB_DIST=host/web/dist bun scripts/agent.mjs web --app oauth-fixture --test apps/oauth-fixture/app.test.contract`

The drive reported `3 passed, 0 failed, 1 skipped`: sign-in and relaunch,
ordinary sign-out with revocation, and `endSession: true`. The fixture's spec
profile names no end-session endpoint, so the last case covers the option's
no-endpoint result, not a provider logout return. The fixture has no authored
zero-delay continuation after `exclusive`; the two-placement storage test
covers that exact production microtask shape without adding a driver primitive.

The headless browser test drives the real delayed package shape: the press
reserves a blank popup, a held same-origin request puts `arm` and the seed in a
later task, and the popup navigates from a `localhost` app to a `127.0.0.1`
provider with COOP, then back to the callback. Chromium 156.0.8078.4, Firefox
157.0 and WebKit 27.2 each preserved the identity with `opener` severed and
completed only the logout whose popup carried it. An independently opened
state-less callback completed no logout.

The remaining main-equivalent storage deadlock is tracked separately in
`issues/20261114-a-parked-wasm-answer-can-hold.md`.

## Limits

Each token request has its own 60-second host deadline. A backlog can therefore
delay sign-in, refresh and another sign-out for longer than 60 seconds in total,
and a custom transport that ignores `exactTimeout` defeats that bound. Runtime
loss can leave a provider token unrevoked. Another tab may send the same token
again; RFC 7009 treats that as success.
