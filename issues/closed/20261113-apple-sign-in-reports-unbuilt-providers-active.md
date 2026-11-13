# On Apple, `@exact/sign-in` reports Google active, but a Google request is not built and always fails

**Status:** Closed
**Resolution:** Apple capability replies report Google and combined Apple/passkey requests unavailable until implemented.
**Systems:** packages/sign-in/apple/SignIn.swift, docs/recipes/sign-in.md, apps/auth-fixture/app.contract
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`SignInModule.capabilities()` (`packages/sign-in/apple/SignIn.swift:33-38`)
answers `getIdentityCapabilities` with `google.active: true` on every Apple
build. The Google request is not built: `identity` goes to `Self.request`,
which takes only a request whose provider is Apple's and refuses any other
(`:96-105`) with

```
NotSupportedError: this implementation offers Apple identity requests alone
```

The guide says so (`docs/contract-for-agents.md:1567-1569`: "The existing
native capabilities still advertise Google active requests"), and the recipe
ends with "Google on Apple … not" (`docs/recipes/sign-in.md:54`). But the
capability is what an app reads to decide whether to show the button: the
recipe gates Apple's button on `ids.providers.apple.active`, and the auth
fixture, the app the recipe says to copy, gates Google's on
`identities.providers.google.active` (`apps/auth-fixture/app.contract:136`).
An app that follows either shows Google's button on iOS and macOS, and every
press fails.

## The failure

Two notes from one agent building an app's sign-in for iOS, on exact2
`94b8671f7`, macOS host. Both buttons showed; each press answered
`NotSupportedError`. The agent moved Google to `@exact/oauth` (the system
sheet with the iOS client), which works. Apple's request has since been built
(63dacc5a); Google's has not.

Confirmed at `49ae6357` by reading `SignIn.swift`, the recipe and the
fixture; not run on a simulator.

## Fix

Until Google's native request is built, `capabilities()` should report
`google.active: false` on Apple, so a condition on it hides the button, and
the recipe should say that Google works through `@exact/oauth` meanwhile.
Then build it: Google through `ASWebAuthenticationSession`, as `QUEUE.md`
("Sign-in's native requests and seams") describes.
