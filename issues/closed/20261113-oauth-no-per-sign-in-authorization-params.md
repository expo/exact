# `@exact/oauth` takes authorization parameters only per provider, so a login hint or connection chosen at the press needs the app's own `authorize`

**Status:** Closed
**Resolution:** signIn accepts per-call params over provider defaults, refuses reserved fields and preserves synchronous prepared popup opening.
**Systems:** packages/oauth/index.ts, packages/oauth/index.d.ts, docs/contract-for-agents.md (Sign-in providers), contract/cli/src/typescript.rs
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`authorizationParams` is part of `OAuthConfig` (`packages/oauth/index.d.ts`,
`index.ts:326`), fixed when the provider is made at module level. `signIn`'s
options (`OAuthSignIn`) carry only `authorize`, which LLP 1146 D8 and
`docs/contract-for-agents.md` ("An app's own sign-in screens") describe for a
provider whose sign-in the app sends itself. There is no way to add a
parameter chosen at the press: a `login_hint`, an identity-provider connection
id, a `prompt=select_account` for "use another account".

What works is an `authorize` that opens the browser itself:

```ts
provider.signIn(store, {
  authorize: (req) => openAuthSession(req.url + '&connection_id=…',
    { callback: req.params.redirect_uri, state: req.params.state }),
});
```

But then the app must map `openAuthSession`'s rejections (499 cancelled, 428
popup blocked, 403 and 501 configuration) to `OAuthResult` itself, which the
package does in `openedInBrowser` (`index.ts:382-390`) and does not do for
`authorize`. On the web it also loses the package's guarantee that the popup
opens before the first `await`.

`openAuthSession` and `authCallback` are declared, with their statuses, in
the `app.d.ts` the TypeScript bake generates (`contract/cli/src/typescript.rs:147-152`),
so an app's own module does not need to declare them. No guide shows them as
public API, or says that a callback must carry exactly one `state`; the
reporting agent declared them by hand.

## The failure

Seen on all hosts (exact2 `5434f432`): an app that lets the person pick an
identity provider at the sign-in press found no documented option, and used the
`authorize` hook above.

## Fix

Accept `signIn(store, { params: { … } })`, refused for the names in `RESERVED`
(`index.ts:227`) as `authorizationParams` is, merged into the prepared
transaction's parameters before `openAuthSession` (no `await` between, so the
web popup still opens), and opened by the package's own path. In the
`contract-for-agents` sign-in section, say that `openAuthSession` and
`authCallback` are in the generated `app.d.ts` and are public for what the
package does not do.
