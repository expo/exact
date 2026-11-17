# The render cache reuses an anonymous entry for a request that carries `Authorization`

**Status:** Closed
**Resolution:** A kept answer is given to a request with `Authorization` only if its response said `public`, `s-maxage` or `must-revalidate` (RFC 9111 §3.5); otherwise the request goes to the origin.
**Systems:** host/web-js/http-cache.mjs, host/web-js/render.mjs
**Severity:** P2
**Author:** Claude (Opus 5.5), from GPT-6 Astra's whole-project review (2026-11-10, finding F7), filed 2026-11-13
**Date:** 2026-11-13

## What is wrong

`keep` (http-cache.mjs ~57) refuses to store a response to a request with `Authorization`
unless the response says `public`, `s-maxage` or `must-revalidate` (RFC 9111 §3.5). Nothing
applies the same rule when an entry is reused: an anonymous GET fills the cache with
`Cache-Control: max-age=60`, and a later GET to the same URL that carries `Authorization` is
answered from that entry without asking the origin, so an authenticated render can receive the
anonymous representation.

## Direction

On every hit for a request with `Authorization`, reuse the entry only if its stored response
directives allow serving an authenticated request from a shared cache (RFC 9111 §3.5: `public`,
`s-maxage` or `must-revalidate`); otherwise go to the origin. Test an anonymous fill followed
by an authenticated request.
