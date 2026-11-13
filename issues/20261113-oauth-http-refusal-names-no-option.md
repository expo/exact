# `@exact/oauth`'s refusal of a plain-http origin does not name the option, and no guide says how to sign in on a device against a development identity server

**Status:** Open
**Systems:** packages/oauth/index.ts, docs/contract-for-agents.md (Sign-in providers), docs/recipes/oauth.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`oauth()` checks `issuer`, `endpoints.*` and every entry of `apis` when the
provider is made (`packages/oauth/index.ts:247-249`), through `secure(url,
'oauth')` (`:984-989`). An origin that is neither HTTPS nor loopback throws:

```
oauth: http://<host>:<port> is not HTTPS; tokens go only over HTTPS (a loopback server excepted)
```

The rule is deliberate: LLP 1146 D5 sends tokens and reads discovery only over
HTTPS except on loopback. Two things around it cost an app time:

1. The message says `oauth:` and the origin, not which option held it
   (`issuer`, `endpoints.token`, `apis[2]`). Providers are made at module
   level, so the throw fails the module's load, and a native bake reports it as
   `exact-js: the module did not load: …` (`js/src/lib.rs:758`). A build that
   never uses the provider fails the same way.
2. An iPhone build that reaches a development stack on the developer's Mac
   over the local network has a non-loopback `http:` origin. No guide says what
   to do: serve the development identity server and API over HTTPS on the LAN
   (a locally trusted certificate) or a tunnel, or keep them on loopback in the
   simulator.

## The failure

Seen on an iPhone build (exact2 `9e53578f`) against a development stack on the
Mac: the bake failed. The agent reported seeing only "the module did not load"
in the build's output. At `49ae6357`, `js/src/shim.cc:326-327` passes the JS error's
message on and `lib.rs:758` appends it, so the sentence above should follow;
whether `host/apple/build.mjs` shows it whole was not checked (no native
build).

## Fix

Pass the option's name to `secure` (`oauth: apis[0], http://…, is not HTTPS; …`).
Add a line to the sign-in providers section on testing on a device against a
development server: HTTPS with a certificate the device trusts, or a tunnel;
the HTTPS rule is not relaxed for development builds. If the bake's output
truncates the load error, show it whole.
