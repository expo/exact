# `@exact/oauth`'s `signOut` leaves the provider's browser session, so the next sign-in completes silently as the same account

**Status:** Open
**Systems:** packages/oauth/index.ts, packages/oauth/index.d.ts, docs/recipes/oauth.md, LLP 1146 D5
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`signOut` (`packages/oauth/index.ts:614-634`) forgets the kept session and
revokes its token at the revocation endpoint (RFC 7009), as LLP 1146 D5
specifies. It never ends the person's session at the provider. Discovery
(`:757-764`) reads `authorization_endpoint`, `token_endpoint` and
`revocation_endpoint` and does not read `end_session_endpoint` (OpenID Connect
RP-Initiated Logout 1.0 §2), and `Metadata` (`:184-191`) has no field for it.

The system browser still holds the provider's cookie. The next `signIn` opens
the authorization endpoint, the provider sees a signed-in person, and redirects
straight back: the person never sees a page and cannot choose another account.
LLP 1146 does not rule logout out; it does not mention it. `ephemeral: true`
avoids the shared cookie, but it is per provider and makes every sign-in ask
for credentials again.

## The failure

Seen on all hosts (exact2 `5434f432`): sign out, press Sign in, and no page is
shown; the app is signed in again as the same account.

The workaround that worked: the app calls `openAuthSession` itself with
`<end_session_endpoint>?redirect=<authCallback()>&state=<random>`. That relies
on the provider echoing `state` on the logout redirect, and on the app knowing
the provider's parameter name: `post_logout_redirect_uri` in the specification,
another name at some providers.

## Fix

Read `end_session_endpoint` in discovery (and accept `endpoints.endSession`),
keep it with the session, and offer RP-initiated logout as an option, on
`signOut(store, { endSession: true })` or in the config: open the endpoint
through `openAuthSession` with `id_token_hint`, `client_id`,
`post_logout_redirect_uri` (the callback) and `state` (RP-Initiated Logout §2),
with an option for a provider's other parameter name. Revoke as today. A
provider that does not redirect back should still leave the person signed out,
with the browser step reported in the result rather than failing the sign-out.
Record the choice in LLP 1146 D5.
