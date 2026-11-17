# An auth callback ends another tab's pending sign-in

**Status:** Closed
**Resolution:** The page's arm reply carries the session's `state`, and a tab takes a BroadcastChannel callback only when its one `state` is its own session's, leaving every other tab's popup, listener and deadline as they were.
**Systems:** web authentication, host/web/auth-glue.js, host/web-js/auth.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Problem

Two tabs on the same origin can start sign-in independently, but the first
callback ends both sessions. The other tab rejects the callback's state and
loses its pending sign-in. Its own callback can no longer complete it.

Reviewed at `f6be99af`. In `host/web/auth-glue.js:55-61`, `matches` compares
only the callback's origin and pathname, and `accept` immediately calls
`finish`. Every tab subscribes to `BroadcastChannel('exact-auth')` at lines
70-72. The callback page broadcasts to that shared name
(`host/web/auth-callback.js:9-10`). `finish` removes the listener, closes the
channel and forgets the session before the runner checks the state. The JS
target's `host/web-js/auth.js:164-167` turns the other tab's callback into a
502, rather than leaving that tab's session pending.

This does not bypass the state check or sign the other tab in as someone
else. It breaks concurrent sign-ins, including when a provider separates
the popup from its opener and the broadcast is the surviving channel.

## Evidence

Imported the actual `auth-glue.js` twice under distinct module URLs to give
each tab its own `live` state. A shared BroadcastChannel substitute delivered
one callback to the two listeners; popup and event functions were substitutes,
and no provider or network was contacted. Both sessions used
`https://app.example/.exact/auth/callback`, with distinct states A and B.
Broadcasting A's callback produced:

```text
session A, ticket 1: done(callbackState=A)
session B, ticket 2: done(callbackState=A)
```

Both listeners had been removed after the broadcast. The repository's state
check then rejects B's completion because A is not B. This is a module-level
reproduction, not a two-browser-tab drive against a provider.

## Fix and validation

Route a broadcast to its owning session, or check that it belongs to the
pending session before consuming that session. Keep the runner's full
callback validation. A callback for another session must leave the current
popup, listener and deadline intact.

Exercise two same-origin tabs with distinct states, complete them in either
order, and check that each receives only its own callback. Include the
BroadcastChannel-only path and an unsolicited or malformed broadcast.

## Duplicate check

Checked open and closed filesystem issues and all 275 GitHub issues fetched
on 2026-11-12. The closed
`20261102-oauth-refresh-lock-does-not-span-realms.md` concerns simultaneous
refresh-token redemption, not callback routing. GitHub #260 concerns the
missing Android auth host. Neither covers this failure.
