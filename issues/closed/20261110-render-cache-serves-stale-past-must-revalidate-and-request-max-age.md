# The render server's HTTP cache serves an answer staler than `must-revalidate` or the request's `max-age` allows

**Status:** Closed
**Resolution:** An entry whose response said `must-revalidate` or `proxy-revalidate` is never given stale, and a request's `max-age`, `min-fresh` and `max-stale` bound the age of the answer it is given, any it does not meet going to the origin; `s-maxage` with `stale-while-revalidate` is still served stale, as the header comment says.
**Systems:** host/web-js/http-cache.mjs
**Severity:** P3
**Author:** Claude (Opus 5.5), from GPT-6 Astra's whole-project review (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

`cachingFetch` is the shared RFC 9111 cache in front of every render's fetch
(`host/web-js/http-cache.mjs:1-14`, `host/web-js/render.mjs:88`). Two rules of reuse are
missing:

- **A response's `must-revalidate` or `proxy-revalidate`.** An entry keeps
  `stale-while-revalidate` as its grace (`http-cache.mjs:93`), and a lookup serves it
  stale while one refetch runs (`:122-129`) whatever else the response said. RFC 9111
  §5.2.2.2 and §4.2.4: a cache must not serve such a response stale without validating it
  first.
- **The request's own `max-age`, `min-fresh` and `max-stale`.** Only `max-age=0` sends a
  request past the cache (`:119`); any other request directive is ignored, so a request
  with `Cache-Control: max-age=1` is given an answer ten seconds old. RFC 9111 §5.2.1.

## To see it

Run against the module with a stub origin and clock (Bun):

- An origin answering `Cache-Control: max-age=1, must-revalidate, stale-while-revalidate=60`.
  Fetch, advance the clock 2 s, fetch: the second fetch returns the first body at once
  and refetches in the background.
- An origin answering `max-age=60`. Fetch, advance 10 s, fetch with
  `{ headers: { 'cache-control': 'max-age=1' } }`: the cached body, with no call to the
  origin.

## What a fix is

Store `must-revalidate` and `proxy-revalidate` on the entry and give it no stale grace
when either is present; read the request's `max-age`, `min-fresh` and `max-stale` and go
to the origin when the entry's age does not meet them. One decision: s-maxage also implies
`proxy-revalidate` for a shared cache (§5.2.2.10), which read strictly forbids the common
`s-maxage=1, stale-while-revalidate=59`; keep honouring that pair, and say so in the
header comment. Tests beside `host/web-js/http-cache.test.mjs:97` (the SWR case).
