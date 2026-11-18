# Recipe: web sign-in tokens and sessions

Use a **server-set HttpOnly, Secure cookie** for web refresh and session tokens.
Web secrets are origin-readable; presence locks refuse. HttpOnly prevents token
reads for replay elsewhere. XSS can still send requests and read responses.

Use `__Host-` with `HttpOnly; Secure; Path=/`, no `Domain`.
Choose SameSite: `Strict` or `Lax` for same-site use, `None; Secure` for cross-site.
Prefer same-origin; third-party cookies may be blocked.

Grant `net.fetch https://api.example.com`. The JS target preserves
`credentials` (default `same-origin`). Cross-origin sign-in, refresh, API and
logout need `credentials: 'include'` to send and accept cookies.
Scripts cannot set `Cookie` or read `Set-Cookie`.
**The wasm target, including workers, ignores `credentials`: cookie sessions
currently require same-origin endpoints.**

`--render js` leaves visitor-dependent reads pending until browser adoption.

CORS: explicit `Access-Control-Allow-Origin`, never `*`;
`Access-Control-Allow-Credentials: true`; `Vary: Origin` when origins vary.
Allow unauthenticated OPTIONS, methods and CSRF headers.
[Fetch](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials).

In answers, `exactHost()` returns `'web'` (workers too) or `'native'`, whose
HTTP has no cookie jar. Grant `secret.keep session`:

```ts
import type { Store } from './app.contract.d.ts';
function sessionTransport(store: Store) {
  const web = exactHost() === 'web';
  return {
    path: web ? '/session/web' : '/session/native',
    options(init: RequestInit = {}): RequestInit {
      const headers = new Headers(init.headers);
      if (!web) {
        const token = store.get('session');
        if (token) headers.set('authorization', `Bearer ${token}`);
      }
      return { ...init, headers, credentials: web ? 'include' : 'omit' };
    },
    remember(reply: { token?: string }) {
      if (!web) {
        if (!reply.token) throw new Error('Missing session token');
        store.set('session', reply.token);
      }
    },
    forget() { if (!web) store.forget('session'); },
  };
}
```

Create the adapter per source. Share sign-in, refresh and logout:
fetch `API + session.path + operation` with `session.options(init)`.
Validate before `session.remember(reply)`. Return no token to Contract or web
replies; native endpoints never exchange cookies for tokens.

Serialize refresh across tabs. Rotate atomically, revoke predecessors and detect
reuse on the server. Retry auth rejection once after refresh. Logout revokes,
expires the cookie at its name/path/domain scope, calls `session.forget()` and
clears account data. Failed logout is not revocation.

For sign-in, refresh, logout and writes, check Origin and require a session-bound
CSRF header (pre-session at sign-in). Return it in the body; keep in memory,
renew at sign-in, pass in `init.headers`. SameSite/CORS are insufficient.
[CSRF](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html).
