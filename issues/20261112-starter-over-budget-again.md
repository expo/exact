# The production starter's app.js is over its 45 KiB budget again

**Status:** Open
**Systems:** host/web-js (runtime linked into every app), scripts/metrics.mjs (`JS_TARGET_KIB`), host/web-js/build.test.mjs
**Severity:** P2
**Author:** Claude (found while landing issue fixes)
**Date:** 2026-11-12
**Related:** issues/closed/20261112-starter-over-its-45-kib-budget.md

## What is wrong

`host/web-js/build.test.mjs` "the production starter stays within its 45 KiB budget" fails at
`5c6f9658`: 46,622 bytes brotli-11 against 46,080. `d37891e7` (the cuts that closed the earlier
issue) left it at 45,258, so about 1,364 bytes arrived in commits to `host/web-js` since,
the same number at `5c6f9658~1`.

## Direction

Bisect the starter's size over the `host/web-js` commits since `d37891e7`, then cut code the
starter ships but does not use, by link-by-use (LLP 1047.002). The budget is Charlie's; raising it
is not an agent's call.
