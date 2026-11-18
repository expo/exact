# Recipe: OAuth sign-in and sign-out

`@exact/oauth` handles PKCE, tokens, refresh and sign-out. Install:
`EXACT_APP_DIR=$PWD bun <exact2>/scripts/exact.mjs add oauth`.
Provider setup: `bun exact.mjs docs sign-in providers`.

Web tokens are origin-readable. [Cookie sessions](web-sessions.md) need your server.

```ts
import { oauth } from '@exact/oauth';
// Grant origins, without paths.
export const grants = ['net.fetch https://login.example.com', 'net.fetch https://api.example.com',
  'auth.session https://login.example.com', 'auth.callback com.example.app:/oauth',
  'secret.keep example.session'].join('\n');
// Keep the issuer's path and trailing slash.
const provider = oauth({ issuer: 'https://login.example.com/oauth2/default', clientId: 'abc123',
  scopes: ['openid', 'profile', 'offline_access'], key: 'example.session',
  apis: ['https://api.example.com'] }); // Only these APIs receive the token.
const sources: Sources = {
  session: (_a, store) => provider.session(store),
  signIn: (_a, store) => provider.signIn(store),
  signOut: (_a, store) => provider.signOut(store),
  me: async ([signedIn], store) => signedIn
    ? { name: String(((await provider.json(store, 'https://api.example.com/me')) as { name?: string }).name ?? '') }
    : { name: '' },
};
```

```text
use OAuthSession, OAuthResult from "@exact/oauth"

shape Me
  name: string

component Account
  resource session = session() as shape OAuthSession
  resource me = me(session.signedIn) as shape Me else empty(name="")
  mutation signedIn as shape OAuthResult refreshes session, me
  mutation signedOut as shape OAuthResult refreshes session, me
  derive problem = match signedIn { case some(r) => (r.error != "" ? r.error : session.error), case none => session.error }
  action signIn
    send signedIn = signIn()
  action signOut
    send signedOut = signOut()
  action reload
    refresh me
```

- Web popup closure is unknowable under COOP. Cancel with `signedIn = none`
  to release a pending sign-in for retry.
- Show data only while `session.signedIn`; show `problem` when it is not `""`.
  `auth_session` (502) keeps the host's reason and is not retryable.
- Let `fetch` and `json` throw `OAuthError`: the resource keeps what it showed.
  A failed read is not a sign-in problem.
- `refresh me` reads again; sending a mutation instead asks the API twice.
- Keep `session` a resource the sign-in screen reads; it readies the next press.
- Provider logout: pass `{ endSession: true }` from a press; register its
  callback. `bun exact.mjs docs oauth endSession`.
- A provider-ended session leaves signed-in state and sets `session.error`; a
  network failure, 429 or 5xx keeps it.

With `openid`, `signIn(store, { nonce })` sends and checks an app nonce.
[Grant and host the callback](../contract-for-agents.md#web-sign-in-callback) before deploying.

For an HTTPS-only backend, declare `auth.callback https://app.example/auth/return`.
Apple needs `webcredentials`; Android uses Auth Tab or a verified App Link.
Setup: [HTTPS callbacks](../contract-for-agents.md#https-callbacks).
