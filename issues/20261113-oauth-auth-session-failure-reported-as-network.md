# `@exact/oauth` reports an auth session the system ended with an error as a network failure, and drops the system's reason

**Status:** Open
**Systems:** packages/oauth/index.ts, packages/oauth/index.d.ts, host/apple/Sources/ExactAuthSession/AuthSession.swift
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`openAuthSession` rejects with a `status` (LLP 1069.006 D1; the generated
`app.d.ts` lists 499, 403, 409, 428, 501 and 502). On Apple,
`AuthSession.swift:29-40` settles 499 for `canceledLogin` and 502 for every
other `ASWebAuthenticationSessionError`, with the system's
`localizedDescription` as the message. The host also answers 502 when the
session cannot start (`AuthSession.swift:57`, no window) and when its URL or
callback is unreadable (`:27`); the loopback receiver answers 502 when its port
cannot be bound or the default browser did not open
(`host/apple/Sources/ExactKit/AuthLoopback.swift:62-85`).

`openedInBrowser` in `packages/oauth/index.ts:382-392` maps 499, 428, 403 and
501, and folds everything else, 502 included, into
`result('network', 'Sign-in could not finish. Try again.', true)` with
`status` 0. The host's sentence is dropped. `failure()` (`:934-937`) does the
same for any rejection that is not an `OAuthError`. `OAuthResult.code`'s
documented list (`index.d.ts:23`) has no code for "the system's sign-in session
failed".

So neither the person nor the app can tell a system auth-session error from a
network failure. The runner's `auth N:` log line carries the reason, but an app
opened from Finder shows no log.

## The failure

Seen on macOS (exact2 `5434f432`) with a non-Safari default browser hosting the
session: the session ended about six seconds after it opened, while the
provider's page was still showing. The app got `code: 'network'`, `status: 0`
and the generic sentence. The same mapping applies on iOS.

Checked at `49ae6357`: the mapping in `openedInBrowser` and `failure()` is unchanged.
Not reproduced: the system error itself needs a native run.

## Fix

Give 502 its own result in `openedInBrowser`, for example `code:
'auth_session'`, `status: 502`, not retryable by default, and an `error`
sentence that includes the host's message (`Sign-in could not open: <message>`).
Add the code to `OAuthResult.code`'s list in `index.d.ts` and to the recipe's
note on `problem`.
