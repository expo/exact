# RealWorld's old requests can change the next reader's session

**Status:** Open
**Systems:** apps/realworld/app.ts
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Failure

At `f6be99af`, a pending request carries no reader generation. A late
`currentUser` 401 unconditionally deletes `realworld.jwt`
(`apps/realworld/app.ts:164-168`), even when a successful login has replaced
that token. The previous reader's response signs the new reader out.

The same missing boundary affects favorites and follows. `forget()` clears
their maps but leaves `presses` intact (line 69). A pending `flip()` can
therefore pass its press-number check after logout or another login and
populate the maps again (lines 34-40). `joined()` then overlays the previous
reader's favorite on an anonymous or different reader's article, even when
the server returned `favorited: false` (lines 70-72).

Ignoring stale resource answers in the runner cannot prevent these writes:
they occur inside the data module before the answer is returned.

## Reproduction

Ran the actual exported `answer` under Bun 1.4.2 with an in-memory store and
held `fetch` responses; no real service or credentials were used:

1. Seed the dummy Alice token and start `answer('currentUser', ...)`, holding
   its `/user` response. Complete `answer('login', ...)` with Bob's token.
   Release Alice's response as HTTP 401. The store's token becomes absent.
2. Start Alice's `answer('favorite', ['story', true], ...)`, holding its
   response. Complete `answer('logout', ...)`, then release the favorite
   response as `{article: {favorited: true, favoritesCount: 1}}`.
3. Ask for the article anonymously. Make its server response say
   `favorited: false, favoritesCount: 0`. The answer instead contains
   `favorited: true, favoritesCount: 1`.

The temporary probe is
`/tmp/exact-review-20261112/realworld-session-probe.ts`. These checks exercise
the module's side effects directly; a browser drive was not run.

## Direction and acceptance

Give authentication transitions a generation. Capture it when starting
account-specific work, and verify it before clearing or replacing a token
or updating favorites/follows. Invalidating pending presses on a reader
change also prevents their late results from repopulating the maps. A 401
must only invalidate the credential it actually used.

Cover pending reads and favorite/follow mutations across logout and a
successful login. New credentials and the new reader's displayed values
must survive replies from the previous generation.

## Duplicate check

Checked open and closed filesystem issues and all GitHub issue bodies on
2026-11-12 for RealWorld, account changes, `currentUser`, and favorite/logout
races. GitHub #136 concerns declarative route guards. Existing RealWorld
filesystem reports concern presentation, budgets and build behavior; none
covers these stale replies. The OAuth package retry issue from this review
is a separate implementation: RealWorld does not use that package.
