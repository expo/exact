# The production starter's app.js is over its 45 KiB budget, so build.test.mjs fails on main

**Status:** Closed
**Resolution:** The starter's production app.js is 45,258 bytes brotli-11 (822 under): bound-exit and layout-transition probes (presence-bindings.js) link only for a plan that binds one, and uploads.js only for sources whose fetch is granted.
**Systems:** host/web-js (runtime linked into every app), scripts/metrics.mjs (`JS_TARGET_KIB`), host/web-js/build.test.mjs
**Severity:** P2
**Author:** Claude (found while landing issue fixes)
**Date:** 2026-11-12

## What is wrong

`host/web-js/build.test.mjs` "the production starter stays within its 45 KiB budget"
fails: the starter's `app.js`, brotli-11 as it ships, is 46,493 bytes at `5b6f3e85`
against 46,080 (`JS_TARGET_KIB.starter` in `scripts/metrics.mjs`). It was 46,392 at
`bf764cf5`, so it crossed the budget before that; `cf7e0cfa` (own-key reads) added
about 100 bytes. When this test fails first, the rest of the file's cases fail in a
millisecond each, which hides them.

## Direction

Find what crossed it (bisect the starter's size over the commits that touched
`host/web-js` since the budget last passed) and cut runtime code the starter ships
but does not use, by link-by-use as earlier cuts did (QUEUE's RealWorld line). The
budget is Charlie's (LLP 1071); raising it is not an agent's call.
