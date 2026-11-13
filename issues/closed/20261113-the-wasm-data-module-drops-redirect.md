# The wasm data module drops redirect, cache, and long poll

**Status:** Closed
**Resolution:** The wasm data module copies a fetch's `redirect`, `cache` and `exactLongPoll` onto the request the page runs, and refuses a value no host runs.
**Systems:** js, host/web
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

The wasm module's `step` copies a fetch's URL, method, headers, and body, and refuses `exactIdleTimeout`. It never reads `redirect`, `cache`, or `long_poll` from the wire the prelude sends. `Request::get` leaves those at follow, unspecified, and no long poll.

The page writes `redirect`, `cache`, and `long_poll` when the request carries them. A `redirect: "manual"` or `"error"`, a cache mode, or a long poll is therefore run as an ordinary following GET.

## Where

`js/web/src/lib.rs`, `step`. `js/src/prelude.js`, the fetch wire. `runner/src/request.rs`, `Request::get`. `host/web/src/batch.rs`, the fields the page honors.

## How to reproduce

From a Rust source on the wasm web host, `fetch` with `redirect: "manual"`, or with a long poll. The request follows redirects, and the long poll is not sent. The same call on the JS target keeps both.

## Direction

Copy `redirect`, `cache`, and `long_poll` onto the request, and refuse a value this host cannot run.
