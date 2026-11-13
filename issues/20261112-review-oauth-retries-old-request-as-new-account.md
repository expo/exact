# An OAuth request awaiting a 401 can be retried as a different signed-in account

**Status:** Open
**Systems:** packages/oauth/index.ts
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P1

## Failure

At `f6be99af`, `authorizedFetch` retains the original URL, method and body
while it waits for the API. If the person signs out and signs in as another
account before a 401 arrives, `renew` sees different stored tokens and
returns the new session (`packages/oauth/index.ts:463-467`). The retry at
lines 578-606 sends the first account's pending operation with the second
account's access token.

This includes POSTs and DELETEs. A delayed request to an account-relative
endpoint can change the newly signed-in person's data without that person
having requested the operation. Reusing a token another caller refreshed
within the same session is valid; reusing another login's session is not.

## Reproduction

Ran the real `oauth()` implementation with Bun 1.4.2, a memory store and the
documented custom `fetch` transport. The transport holds the first response
behind a promise and records each request's authorization header:

1. Seed a valid kept session for Alice.
2. Start `provider.fetch(store, 'https://api.example/delete-my-files',
   { method: 'POST', body: '{}' })`; hold its response.
3. Complete `provider.signOut(store)`, then store Bob's new signed-in
   session under the same key.
4. Resolve Alice's request with status 401.

Observed outbound authorization headers:

```text
[ "Bearer alice", "Bearer bob" ]
```

Both calls have the original URL and POST body. No token refresh request is
made. The probe used dummy tokens and no external service; it exercises the
package's account-switch decision directly.

## Direction and acceptance

Give a successful sign-in a session identity that stays stable through
refreshes and changes on each new login. Before retrying an authorized
request, require that its original session is still current. End the old
request with a session-changed/signed-out error otherwise. This must also
work for OAuth providers without an ID-token subject.

Test a held 401 across sign-out and a new login, for both reads and writes;
the new account must receive no retry. Preserve coalescing when concurrent
requests merely refresh the same session's token.

## Duplicate check

Checked open and closed filesystem issues and the GitHub issue export on
2026-11-12 for OAuth, account switches, sign-out and refresh. The closed
cross-realm refresh-lock issue concerns spending a rotating token twice;
the GitHub OAuth issues concern host support, storage and socket token
access. None covers a retry crossing account ownership.
